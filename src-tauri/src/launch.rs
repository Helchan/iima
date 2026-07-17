use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Url};

use crate::commands::{
    emit_player_state_for_session, open_dropped_media_paths_in_window, open_media_paths_in_window,
    open_new_player_window, reusable_idle_player_session_label, set_player_window_fullscreen,
    should_open_in_new_player_for_menu_action, toggle_music_mode_window_for_session,
    toggle_picture_in_picture_for_session,
};
use crate::menu;
use crate::player::PlayerCommand;
use crate::plugins::{self, PluginInstallNotification};
use crate::state::AppState;

const PLUGIN_PACKAGE_EVENT: &str = "iima-plugin-package";
const OPEN_FILE_REPEAT_TIME: Duration = Duration::from_millis(200);
static PENDING_OPENED_FILE_URLS: OnceLock<Mutex<Vec<Url>>> = OnceLock::new();
static OPENED_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
struct LaunchMediaRequest {
    target: String,
    new_window: bool,
    enqueue: bool,
    full_screen: bool,
    pip: bool,
    music_mode: bool,
    mpv_options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandLineLaunchRequest {
    targets: Vec<String>,
    mpv_options: Vec<(String, String)>,
    stdin: bool,
    separate_windows: bool,
    music_mode: bool,
    pip: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaunchPresentation {
    Fullscreen,
    MusicMode,
    PictureInPicture,
    None,
}

pub(crate) fn command_line_request_from_args(
    args: impl IntoIterator<Item = String>,
) -> Result<Option<CommandLineLaunchRequest>, String> {
    let mut request = CommandLineLaunchRequest {
        targets: Vec::new(),
        mpv_options: Vec::new(),
        stdin: false,
        separate_windows: false,
        music_mode: false,
        pip: false,
    };
    let mut drop_next_argument = false;

    for argument in args {
        if drop_next_argument {
            drop_next_argument = false;
            continue;
        }
        if argument == "-" {
            request.stdin = true;
            continue;
        }
        if !argument.starts_with('-') {
            request.targets.push(argument);
            continue;
        }
        let Some(option) = argument.strip_prefix("--") else {
            drop_next_argument = true;
            continue;
        };
        let (name, value) = option.split_once('=').unwrap_or((option, "yes"));
        if let Some(mpv_name) = name.strip_prefix("mpv-") {
            if mpv_name == "-" {
                request.stdin = true;
            } else if !mpv_name.is_empty() {
                request
                    .mpv_options
                    .push((mpv_name.to_string(), value.to_string()));
            }
            continue;
        }
        match name {
            "stdin" => request.stdin = true,
            "no-stdin" => request.stdin = false,
            "separate-windows" => request.separate_windows = true,
            "music-mode" => request.music_mode = true,
            "pip" => request.pip = true,
            _ => {}
        }
    }

    if request.music_mode && request.pip {
        return Err("cannot specify both --music-mode and --pip".to_string());
    }
    if request.targets.is_empty() && !request.stdin {
        Ok(None)
    } else {
        Ok(Some(request))
    }
}

pub(crate) fn handle_command_line_launch(
    app: &AppHandle,
    request: CommandLineLaunchRequest,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    state.note_external_open_request();
    let groups = command_line_media_groups(&request);

    let mut last_player_label = None;
    for targets in groups {
        // Use the same app-aware, creation-ordered idle-core selection as open_new_player_window.
        // The legacy state-only lookup iterates a HashMap and can choose a plugin-owned or later
        // idle core, leaving command-line options queued on a different PlayerCore than loadfile.
        let selected_label = reusable_idle_player_session_label(app, state.inner())?
            .map(Ok)
            .unwrap_or_else(|| state.create_player_session().map(|(label, _)| label))?;
        apply_command_line_mpv_options(
            state.inner(),
            &selected_label,
            request.mpv_options.clone(),
        )?;
        if targets.is_empty() {
            // Non-separate IINA launches still allocate a PlayerCore, apply mpv arguments, and
            // call openURLs([]) when every supplied filename is invalid. The empty call is a
            // no-op, so keep the selected idle session without manufacturing a player window.
            last_player_label = Some(selected_label);
            continue;
        }
        let (opened_label, _) = open_new_player_window(app, state.inner(), targets, Vec::new())?;
        if opened_label != selected_label {
            return Err(format!(
                "command-line player changed from {selected_label} to {opened_label} before open"
            ));
        }
        last_player_label = Some(opened_label);
    }

    if let Some(label) = last_player_label {
        if request.music_mode {
            toggle_music_mode_window_for_session(app, state.inner(), &label)?;
        } else if request.pip {
            toggle_picture_in_picture_for_session(app, state.inner(), &label)?;
        }
    }
    Ok(())
}

fn command_line_media_groups(request: &CommandLineLaunchRequest) -> Vec<Vec<String>> {
    if request.stdin {
        vec![vec!["-".to_string()]]
    } else {
        let targets = request
            .targets
            .iter()
            .filter(|target| is_command_line_media_target(target))
            .cloned()
            .collect::<Vec<_>>();
        if request.separate_windows {
            targets
                .into_iter()
                .map(|target| vec![target])
                .collect::<Vec<_>>()
        } else {
            vec![targets]
        }
    }
}

fn is_command_line_media_target(target: &str) -> bool {
    Path::new(target).exists() || Url::parse(target).is_ok()
}

pub fn handle_opened_urls(app: &AppHandle, urls: Vec<Url>) {
    app.state::<AppState>().note_external_open_request();
    let (file_urls, direct_urls): (Vec<_>, Vec<_>) =
        urls.into_iter().partition(|url| url.scheme() == "file");
    if !direct_urls.is_empty() {
        handle_opened_urls_batch(app, direct_urls);
    }
    if file_urls.is_empty() {
        return;
    }
    let Ok(mut pending) = pending_opened_file_urls().lock() else {
        handle_opened_urls_batch(app, file_urls);
        return;
    };
    pending.extend(file_urls);
    drop(pending);
    let sequence = OPENED_FILE_SEQUENCE.fetch_add(1, Ordering::AcqRel) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(OPEN_FILE_REPEAT_TIME);
        let urls = take_debounced_opened_file_urls(sequence);
        if urls.is_empty() {
            return;
        }
        let dispatch_app = app.clone();
        let _ = app.run_on_main_thread(move || handle_opened_urls_batch(&dispatch_app, urls));
    });
}

fn pending_opened_file_urls() -> &'static Mutex<Vec<Url>> {
    PENDING_OPENED_FILE_URLS.get_or_init(|| Mutex::new(Vec::new()))
}

fn take_debounced_opened_file_urls(sequence: u64) -> Vec<Url> {
    if OPENED_FILE_SEQUENCE.load(Ordering::Acquire) != sequence {
        return Vec::new();
    }
    pending_opened_file_urls()
        .lock()
        .map(|mut pending| std::mem::take(&mut *pending))
        .unwrap_or_default()
}

fn handle_opened_urls_batch(app: &AppHandle, urls: Vec<Url>) {
    if let Some(package) = urls
        .iter()
        .filter_map(|url| url.to_file_path().ok())
        .find(|path| is_plugin_package_path(path))
    {
        install_plugin_package(app, package);
        return;
    }
    let requests = launch_requests_from_urls(&urls);
    if requests.is_empty() {
        return;
    }

    let target_window_label = menu::active_player_window_label(app);
    let state = app.state::<AppState>();
    let Ok(target_session) = state.player_session_for_window(&target_window_label) else {
        return;
    };
    let targets = requests
        .iter()
        .map(|request| request.target.clone())
        .collect::<Vec<_>>();
    let all_enqueue = requests.iter().all(|request| request.enqueue);
    let last_active_label = state
        .last_active_player_session_label()
        .unwrap_or_else(|_| "main".to_string());
    let Ok(last_active_session) = state.player_session_for_window(&last_active_label) else {
        return;
    };
    let last_active_has_playlist = last_active_session
        .player()
        .lock()
        .map(|player| !player.playlist.is_empty())
        .unwrap_or(false);
    let explicit_new_window = requests.iter().any(|request| request.new_window);
    let preference_new_window =
        should_open_in_new_player_for_menu_action(state.inner(), false).unwrap_or(false);
    let use_new_player = explicit_new_window || preference_new_window;
    let mpv_options = requests
        .iter()
        .flat_map(|request| request.mpv_options.iter().cloned())
        .collect::<Vec<_>>();
    if all_enqueue && last_active_has_playlist {
        let Ok(selected_label) =
            select_launch_player_label(state.inner(), target_session.label(), use_new_player)
        else {
            return;
        };
        let enqueue_snapshot = {
            let Ok(mut player) = last_active_session.player().lock() else {
                return;
            };
            player.enqueue_media(targets);
            player.clone()
        };
        let _ = last_active_session.sync_mpv_executor_from_player();
        emit_player_state_for_session(app, last_active_session.label(), &enqueue_snapshot);
        let selected_session = match state.player_session_for_window(&selected_label) {
            Ok(session) => session,
            Err(_) => return,
        };
        let selected_snapshot = {
            let Ok(player) = selected_session.player().lock() else {
                return;
            };
            player.clone()
        };
        let selected_snapshot = apply_launch_presentation(
            app,
            state.inner(),
            &selected_label,
            selected_snapshot,
            &requests,
        );
        let selected_snapshot =
            apply_launch_mpv_options(state.inner(), &selected_label, mpv_options)
                .unwrap_or(selected_snapshot);
        emit_player_state_for_session(app, &selected_label, &selected_snapshot);
        return;
    }

    let (label, snapshot) = if use_new_player {
        let Ok(result) = open_new_player_window(app, state.inner(), targets, mpv_options) else {
            return;
        };
        result
    } else {
        let Ok(snapshot) = open_media_paths_in_window(
            app,
            state.inner(),
            &target_window_label,
            targets,
            mpv_options,
        ) else {
            return;
        };
        (target_session.label().to_string(), snapshot)
    };
    let snapshot = apply_launch_presentation(app, state.inner(), &label, snapshot, &requests);
    emit_player_state_for_session(app, &label, &snapshot);
}

fn select_launch_player_label(
    state: &AppState,
    active_label: &str,
    use_new_player: bool,
) -> Result<String, String> {
    if !use_new_player {
        return Ok(state
            .player_session_for_window(active_label)?
            .label()
            .to_string());
    }
    if let Some(label) = state.idle_player_session_label()? {
        return Ok(label);
    }
    state.create_player_session().map(|(label, _)| label)
}

fn apply_launch_presentation<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    label: &str,
    snapshot: crate::player::PlayerState,
    requests: &[LaunchMediaRequest],
) -> crate::player::PlayerState {
    match launch_presentation(requests) {
        LaunchPresentation::Fullscreen => {
            let snapshot = apply_launch_mpv_options(
                state,
                label,
                vec![("fullscreen".to_string(), "yes".to_string())],
            )
            .unwrap_or(snapshot);
            if let Some(window) = app.get_webview_window(label) {
                let _ = set_player_window_fullscreen(app, state, &window, true);
            }
            snapshot
        }
        LaunchPresentation::MusicMode => {
            toggle_music_mode_window_for_session(app, state, label).unwrap_or(snapshot)
        }
        LaunchPresentation::PictureInPicture => {
            toggle_picture_in_picture_for_session(app, state, label).unwrap_or(snapshot)
        }
        LaunchPresentation::None => snapshot,
    }
}

fn launch_presentation(requests: &[LaunchMediaRequest]) -> LaunchPresentation {
    if requests.iter().any(|request| request.full_screen) {
        LaunchPresentation::Fullscreen
    } else if requests.iter().any(|request| request.music_mode) {
        LaunchPresentation::MusicMode
    } else if requests.iter().any(|request| request.pip) {
        LaunchPresentation::PictureInPicture
    } else {
        LaunchPresentation::None
    }
}

fn apply_launch_mpv_options(
    state: &AppState,
    label: &str,
    options: Vec<(String, String)>,
) -> Result<crate::player::PlayerState, String> {
    let session = state.player_session_for_window(label)?;
    if options.is_empty() {
        return session
            .player()
            .lock()
            .map(|player| player.clone())
            .map_err(|error| error.to_string());
    }
    let snapshot = {
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        for (property, value) in options {
            player.apply(PlayerCommand::PluginMpvSet { property, value });
        }
        player.clone()
    };
    session.sync_mpv_executor_from_player()?;
    Ok(snapshot)
}

fn apply_command_line_mpv_options(
    state: &AppState,
    label: &str,
    options: Vec<(String, String)>,
) -> Result<crate::player::PlayerState, String> {
    let session = state.player_session_for_window(label)?;
    let snapshot = {
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        for (property, value) in options {
            if property == "shuffle" && value == "yes" {
                // IINA does not set a persistent `shuffle=yes` property. It arms a one-shot
                // before-start-file hook whose two commands run after this batch is assembled.
                player.arm_command_line_shuffle_once();
            } else {
                player.apply(PlayerCommand::PluginMpvSet { property, value });
            }
        }
        player.clone()
    };
    // Command-line options belong before the first loadfile, but IINA does not initialize video
    // output while its player window is still hidden. The open path consumes this ordered log only
    // after the native video host is visible and ready; an all-invalid open keeps it armed for the
    // next real media request on the same PlayerCore.
    Ok(snapshot)
}

pub fn handle_dropped_paths(app: &AppHandle, window_label: &str, paths: Vec<PathBuf>) {
    if let Some(package) = paths.iter().find(|path| is_plugin_package_path(path)) {
        install_plugin_package(app, package.clone());
        return;
    }
    let targets = dropped_path_targets(paths);
    if targets.is_empty() {
        return;
    }

    let state = app.state::<AppState>();
    let _ = open_dropped_media_paths_in_window(app, state.inner(), window_label, targets);
}

fn install_plugin_package(app: &AppHandle, package: PathBuf) {
    let notification = match plugins::install_package(app, &package) {
        Ok(result) => PluginInstallNotification {
            result: Some(result),
            error: None,
        },
        Err(error) => PluginInstallNotification {
            result: None,
            error: Some(error),
        },
    };
    if plugins::enqueue_install_notification(notification).is_err() {
        return;
    }
    if app.get_webview_window("main").is_some() {
        let _ = app.emit_to("main", PLUGIN_PACKAGE_EVENT, ());
    }
}

fn is_plugin_package_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("iinaplgz"))
}

fn launch_requests_from_urls(urls: &[Url]) -> Vec<LaunchMediaRequest> {
    urls.iter().filter_map(launch_request_from_url).collect()
}

fn dropped_path_targets(paths: Vec<PathBuf>) -> Vec<String> {
    paths
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .filter(|path| !path.is_empty())
        .collect()
}

fn launch_request_from_url(url: &Url) -> Option<LaunchMediaRequest> {
    match url.scheme() {
        "file" => url
            .to_file_path()
            .ok()
            .map(|path| LaunchMediaRequest::simple(path.to_string_lossy().into_owned())),
        "iina" => launch_request_from_iina_url(url),
        _ => Some(LaunchMediaRequest::simple(url.as_str().to_string())),
    }
}

fn launch_request_from_iina_url(url: &Url) -> Option<LaunchMediaRequest> {
    let host = url.host_str()?;
    if host != "open" && host != "weblink" {
        return None;
    }

    let query = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    let query_map = query.iter().cloned().collect::<HashMap<_, _>>();
    let target = query_map.get("url").filter(|value| !value.is_empty())?;

    Some(LaunchMediaRequest {
        target: normalize_media_target(target),
        new_window: is_enabled(&query_map, "new_window"),
        enqueue: is_enabled(&query_map, "enqueue"),
        full_screen: is_enabled(&query_map, "full_screen"),
        pip: is_enabled(&query_map, "pip"),
        music_mode: is_enabled(&query_map, "music_mode"),
        mpv_options: query
            .into_iter()
            .filter_map(|(key, value)| {
                key.strip_prefix("mpv_")
                    .map(|option| (option.to_string(), value))
            })
            .collect(),
    })
}

fn normalize_media_target(target: &str) -> String {
    Url::parse(target)
        .ok()
        .filter(|url| url.scheme() == "file")
        .and_then(|url| url.to_file_path().ok())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| target.to_string())
}

fn is_enabled(query: &HashMap<String, String>, key: &str) -> bool {
    query.get(key).is_some_and(|value| value == "1")
}

impl LaunchMediaRequest {
    fn simple(target: String) -> Self {
        Self {
            target,
            new_window: false,
            enqueue: false,
            full_screen: false,
            pip: false,
            music_mode: false,
            mpv_options: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mpv::{MpvClientOperation, MpvFormat};

    #[test]
    fn parses_file_open_event_as_local_path() {
        let url = Url::parse("file:///tmp/IINA%20fixture.mp4").unwrap();

        assert_eq!(
            launch_request_from_url(&url),
            Some(LaunchMediaRequest::simple(
                "/tmp/IINA fixture.mp4".to_string()
            ))
        );
    }

    #[test]
    fn parses_iina_open_url_with_options() {
        let url = Url::parse(
            "iina://open?url=https%3A%2F%2Fexample.com%2Fvideo.mp4&new_window=1&enqueue=1&full_screen=1&pip=1&music_mode=0&mpv_volume=20",
        )
        .unwrap();

        assert_eq!(
            launch_request_from_url(&url),
            Some(LaunchMediaRequest {
                target: "https://example.com/video.mp4".to_string(),
                new_window: true,
                enqueue: true,
                full_screen: true,
                pip: true,
                music_mode: false,
                mpv_options: vec![("volume".to_string(), "20".to_string())],
            })
        );
    }

    #[test]
    fn keeps_music_mode_on_an_independent_player_request() {
        let url = Url::parse(
            "iina://open?url=https%3A%2F%2Fexample.com%2Faudio.flac&new_window=1&music_mode=1",
        )
        .unwrap();

        assert_eq!(
            launch_request_from_url(&url),
            Some(LaunchMediaRequest {
                target: "https://example.com/audio.flac".to_string(),
                new_window: true,
                enqueue: false,
                full_screen: false,
                pip: false,
                music_mode: true,
                mpv_options: Vec::new(),
            })
        );
    }

    #[test]
    fn parses_iina_weblink_url() {
        let url =
            Url::parse("iina://weblink?url=https%3A%2F%2Fexample.com%2Fwatch%3Fv%3D1").unwrap();

        assert_eq!(
            launch_request_from_url(&url).map(|request| request.target),
            Some("https://example.com/watch?v=1".to_string())
        );
    }

    #[test]
    fn normalizes_file_url_parameter_to_path() {
        let url = Url::parse("iina://open?url=file%3A%2F%2F%2Ftmp%2Fclip.mov").unwrap();

        assert_eq!(
            launch_request_from_url(&url).map(|request| request.target),
            Some("/tmp/clip.mov".to_string())
        );
    }

    #[test]
    fn ignores_unknown_iina_hosts_and_missing_targets() {
        assert_eq!(
            launch_request_from_url(&Url::parse("iina://preferences").unwrap()),
            None
        );
        assert_eq!(
            launch_request_from_url(&Url::parse("iina://open").unwrap()),
            None
        );
    }

    #[test]
    fn maps_dropped_paths_to_player_targets() {
        let targets = dropped_path_targets(vec![
            PathBuf::from("/tmp/Dragged One.mp4"),
            PathBuf::from("/tmp/Dragged Two.mkv"),
        ]);

        assert_eq!(
            targets,
            vec![
                "/tmp/Dragged One.mp4".to_string(),
                "/tmp/Dragged Two.mkv".to_string()
            ]
        );
    }

    #[test]
    fn recognizes_plugin_packages_case_insensitively() {
        assert!(is_plugin_package_path(std::path::Path::new(
            "Fixture.iinaplgz"
        )));
        assert!(is_plugin_package_path(std::path::Path::new(
            "Fixture.IINAPLGZ"
        )));
        assert!(!is_plugin_package_path(std::path::Path::new("Fixture.mp4")));
    }

    #[test]
    fn repeated_app_delegate_file_callbacks_coalesce_for_two_hundred_milliseconds() {
        let first = Url::from_file_path("/tmp/first.mp4").unwrap();
        let second = Url::from_file_path("/tmp/second.mp4").unwrap();
        pending_opened_file_urls().lock().unwrap().clear();
        OPENED_FILE_SEQUENCE.store(41, Ordering::Release);
        pending_opened_file_urls()
            .lock()
            .unwrap()
            .extend([first.clone(), second.clone()]);

        assert!(take_debounced_opened_file_urls(40).is_empty());
        assert_eq!(take_debounced_opened_file_urls(41), vec![first, second]);
        assert!(pending_opened_file_urls().lock().unwrap().is_empty());
        assert_eq!(OPEN_FILE_REPEAT_TIME, Duration::from_millis(200));
    }

    #[test]
    fn launch_player_selection_reuses_iina_idle_players() {
        let state = AppState::default();

        assert_eq!(
            select_launch_player_label(&state, "main", true).unwrap(),
            "main"
        );
        state.player.lock().unwrap().current_url = Some("/tmp/playing.mp4".to_string());

        let new_label = select_launch_player_label(&state, "main", true).unwrap();
        assert_eq!(new_label, "player-0");
        assert_eq!(
            select_launch_player_label(&state, "main", true).unwrap(),
            "player-0"
        );
        assert_eq!(
            select_launch_player_label(&state, "player-0", false).unwrap(),
            "player-0"
        );
    }

    #[test]
    fn launch_presentation_uses_reference_precedence() {
        let mut request = LaunchMediaRequest::simple("/tmp/movie.mp4".to_string());
        assert_eq!(
            launch_presentation(&[request.clone()]),
            LaunchPresentation::None
        );

        request.pip = true;
        assert_eq!(
            launch_presentation(&[request.clone()]),
            LaunchPresentation::PictureInPicture
        );
        request.music_mode = true;
        assert_eq!(
            launch_presentation(&[request.clone()]),
            LaunchPresentation::MusicMode
        );
        request.full_screen = true;
        assert_eq!(
            launch_presentation(&[request]),
            LaunchPresentation::Fullscreen
        );
    }

    #[test]
    fn launch_mpv_options_are_applied_to_the_selected_session_in_order() {
        let state = AppState::default();
        let snapshot = apply_launch_mpv_options(
            &state,
            "main",
            vec![
                ("fullscreen".to_string(), "yes".to_string()),
                ("volume".to_string(), "20".to_string()),
            ],
        )
        .unwrap();

        assert_eq!(
            snapshot.mpv_operation_log,
            vec![
                MpvClientOperation::SetProperty {
                    name: "fullscreen".to_string(),
                    format: MpvFormat::String,
                    value: "yes".to_string(),
                },
                MpvClientOperation::SetProperty {
                    name: "volume".to_string(),
                    format: MpvFormat::String,
                    value: "20".to_string(),
                },
            ]
        );
    }

    #[test]
    fn command_line_shuffle_is_one_shot_after_loadfile_and_all_appends() {
        let state = AppState::default();
        apply_command_line_mpv_options(
            &state,
            "main",
            vec![
                ("shuffle".to_string(), "yes".to_string()),
                ("volume".to_string(), "20".to_string()),
            ],
        )
        .unwrap();
        state.player.lock().unwrap().open_media_batch(
            vec!["/tmp/first.mp4".to_string(), "/tmp/second.mp4".to_string()],
            Err("media probe is outside this ordering test".to_string()),
        );

        let player = state.player.lock().unwrap();
        assert_eq!(
            player.mpv_operation_log,
            vec![
                MpvClientOperation::SetProperty {
                    name: "volume".to_string(),
                    format: MpvFormat::String,
                    value: "20".to_string(),
                },
                crate::mpv::mpv_command("loadfile", ["/tmp/first.mp4", "replace"]),
                crate::mpv::mpv_command("loadfile", ["/tmp/second.mp4", "append"]),
                crate::mpv::mpv_command("playlist-shuffle", std::iter::empty::<&str>()),
                crate::mpv::mpv_command("playlist-play-index", ["0"]),
            ]
        );

        drop(player);
        state.player.lock().unwrap().open_media_batch(
            vec!["/tmp/later.mp4".to_string()],
            Err("media probe is outside this ordering test".to_string()),
        );
        assert_eq!(
            state
                .player
                .lock()
                .unwrap()
                .mpv_operation_log
                .iter()
                .filter(|operation| {
                    matches!(operation, MpvClientOperation::Command { command, .. } if command == "playlist-shuffle")
                })
                .count(),
            1
        );
    }

    #[test]
    fn command_line_options_do_not_start_hidden_window_playback_runtime() {
        let state = AppState::default();
        apply_command_line_mpv_options(
            &state,
            "main",
            vec![("volume".to_string(), "20".to_string())],
        )
        .unwrap();

        assert_eq!(state.player.lock().unwrap().mpv_operation_log.len(), 1);
        let status = state.mpv_executor_status().unwrap();
        assert_eq!(status.accepted_operation_count, 0);
        assert_eq!(status.executed_operation_count, 0);
        assert_eq!(status.pending_operation_count, 0);
    }

    #[test]
    fn parses_reference_main_executable_command_line_contract() {
        let request = command_line_request_from_args(vec![
            "--separate-windows".to_string(),
            "--music-mode".to_string(),
            "--mpv-volume=20".to_string(),
            "/tmp/first.mp4".to_string(),
            "https://example.com/second.mkv".to_string(),
        ])
        .unwrap()
        .unwrap();

        assert_eq!(
            request,
            CommandLineLaunchRequest {
                targets: vec![
                    "/tmp/first.mp4".to_string(),
                    "https://example.com/second.mkv".to_string(),
                ],
                mpv_options: vec![("volume".to_string(), "20".to_string())],
                stdin: false,
                separate_windows: true,
                music_mode: true,
                pip: false,
            }
        );
    }

    #[test]
    fn command_line_stdin_takes_precedence_and_rejects_music_mode_with_pip() {
        let stdin = command_line_request_from_args(vec![
            "--stdin".to_string(),
            "/tmp/ignored-by-reference.mp4".to_string(),
        ])
        .unwrap()
        .unwrap();
        assert!(stdin.stdin);
        assert_eq!(stdin.targets, vec!["/tmp/ignored-by-reference.mp4"]);

        assert!(command_line_request_from_args(vec![
            "--stdin".to_string(),
            "--music-mode".to_string(),
            "--pip".to_string(),
        ])
        .is_err());
        assert_eq!(
            command_line_request_from_args(Vec::<String>::new()).unwrap(),
            None
        );
        assert!(is_command_line_media_target("file:/tmp/movie.mp4"));
        assert!(is_command_line_media_target(
            "https://example.com/movie.mp4"
        ));
        assert!(!is_command_line_media_target(
            "/tmp/iima-definitely-missing-media.mp4"
        ));
    }

    #[test]
    fn command_line_parser_ignores_unknown_long_options_and_reference_short_pairs() {
        let request = command_line_request_from_args(vec![
            "--future-iina-option=value".to_string(),
            "https://example.com/first.mp4".to_string(),
            "-x".to_string(),
            "https://example.com/dropped-short-option-value.mp4".to_string(),
            "https://example.com/second.mp4".to_string(),
        ])
        .unwrap()
        .unwrap();

        assert_eq!(
            request.targets,
            vec![
                "https://example.com/first.mp4",
                "https://example.com/second.mp4",
            ]
        );
        assert!(request.mpv_options.is_empty());
    }

    #[test]
    fn nonseparate_invalid_filenames_keep_an_empty_open_and_armed_options() {
        let invalid = "/tmp/iima-command-line-invalid-media-does-not-exist.mp4";
        let mut request = command_line_request_from_args(vec![
            "--mpv-shuffle=yes".to_string(),
            "--mpv-volume=20".to_string(),
            invalid.to_string(),
        ])
        .unwrap()
        .unwrap();

        assert_eq!(
            command_line_media_groups(&request),
            vec![Vec::<String>::new()]
        );

        let state = AppState::default();
        apply_command_line_mpv_options(&state, "main", request.mpv_options.clone()).unwrap();
        assert_eq!(
            state.player.lock().unwrap().mpv_operation_log,
            vec![MpvClientOperation::SetProperty {
                name: "volume".to_string(),
                format: MpvFormat::String,
                value: "20".to_string(),
            }]
        );

        // openURLs([]) is a no-op in IINA, so its one-shot shuffle hook remains armed for the
        // next actual open on that PlayerCore.
        state.player.lock().unwrap().open_media_batch(
            vec!["/tmp/later-valid.mp4".to_string()],
            Err("media probe is outside this ordering test".to_string()),
        );
        assert_eq!(
            state.player.lock().unwrap().mpv_operation_log,
            vec![
                MpvClientOperation::SetProperty {
                    name: "volume".to_string(),
                    format: MpvFormat::String,
                    value: "20".to_string(),
                },
                crate::mpv::mpv_command("loadfile", ["/tmp/later-valid.mp4", "replace"]),
                crate::mpv::mpv_command("playlist-shuffle", std::iter::empty::<&str>()),
                crate::mpv::mpv_command("playlist-play-index", ["0"]),
            ]
        );

        request.separate_windows = true;
        assert!(command_line_media_groups(&request).is_empty());
    }

    #[test]
    fn command_line_mpv_options_can_precede_the_stdin_load_operation() {
        let state = AppState::default();
        apply_command_line_mpv_options(
            &state,
            "main",
            vec![("demuxer-lavf-format".to_string(), "mpegts".to_string())],
        )
        .unwrap();
        state.player.lock().unwrap().open_media_batch_with_pause(
            vec!["-".to_string()],
            Err("stdin is intentionally not probed".to_string()),
            false,
        );

        let player = state.player.lock().unwrap();
        assert!(matches!(
            player.mpv_operation_log.first(),
            Some(MpvClientOperation::SetProperty { name, value, .. })
                if name == "demuxer-lavf-format" && value == "mpegts"
        ));
        assert!(matches!(
            player.mpv_operation_log.get(1),
            Some(MpvClientOperation::Command { command, args })
                if command == "loadfile" && args.first().is_some_and(|target| target == "-")
        ));
    }
}
