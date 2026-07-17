//! Native macOS Touch Bar support for player and Mini Player windows.
//!
//! IINA gives every `PlayerCore` one Touch Bar contract, while AppKit can ask either the main
//! player window or its Mini Player to present it.  The bridge therefore stores no global
//! "active player" shortcut: every callback carries the immutable session label installed on the
//! owning `NSWindow` and resolves that exact session before mutating playback.

use crate::key_bindings::active_key_bindings_from_preference;
use crate::media::{MediaThumbnail, ThumbnailProgress, ThumbnailSet};
use crate::player::{PlayerCommand, PlayerState, RelativeSeekOption};
use crate::preferences::preference_file_path;
use crate::state::AppState;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager, Runtime, WebviewWindow};

const ACTION_TOGGLE_PAUSE: i32 = 1;
const ACTION_VOLUME_DELTA: i32 = 2;
const ACTION_ARROW: i32 = 3;
const ACTION_SEEK_RELATIVE: i32 = 4;
const ACTION_PLAYLIST_NAVIGATE: i32 = 5;
const ACTION_TOGGLE_PIP: i32 = 6;
const ACTION_EXIT_FULLSCREEN: i32 = 7;
const ACTION_SEEK_PERCENT: i32 = 8;
const ACTION_SLIDER_BEGIN: i32 = 9;
const ACTION_SLIDER_END: i32 = 10;
const ACTION_TOGGLE_REMAINING: i32 = 11;

const IINA_SPEED_VALUES: [f64; 11] = [
    0.03125, 0.0625, 0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0,
];
const NORMAL_SPEED_INDEX: usize = 5;
const MAX_SESSION_LABEL_BYTES: usize = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TouchBarAction {
    TogglePause,
    VolumeDelta,
    Arrow,
    SeekRelative,
    PlaylistNavigate,
    TogglePip,
    ExitFullscreen,
    SeekPercent,
    SliderBegin,
    SliderEnd,
    ToggleRemaining,
}

impl TryFrom<i32> for TouchBarAction {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            ACTION_TOGGLE_PAUSE => Ok(Self::TogglePause),
            ACTION_VOLUME_DELTA => Ok(Self::VolumeDelta),
            ACTION_ARROW => Ok(Self::Arrow),
            ACTION_SEEK_RELATIVE => Ok(Self::SeekRelative),
            ACTION_PLAYLIST_NAVIGATE => Ok(Self::PlaylistNavigate),
            ACTION_TOGGLE_PIP => Ok(Self::TogglePip),
            ACTION_EXIT_FULLSCREEN => Ok(Self::ExitFullscreen),
            ACTION_SEEK_PERCENT => Ok(Self::SeekPercent),
            ACTION_SLIDER_BEGIN => Ok(Self::SliderBegin),
            ACTION_SLIDER_END => Ok(Self::SliderEnd),
            ACTION_TOGGLE_REMAINING => Ok(Self::ToggleRemaining),
            _ => Err(format!("unsupported Touch Bar action: {value}")),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TouchBarLabels {
    play_pause: String,
    seek: String,
    volume_up: String,
    volume_down: String,
    rewind: String,
    fast_forward: String,
    time: String,
    remaining: String,
    ahead15: String,
    ahead30: String,
    back15: String,
    back30: String,
    next: String,
    previous: String,
    toggle_pip: String,
}

impl TouchBarLabels {
    fn localized() -> Self {
        let localizable =
            |key, source| crate::localization::menu_title_key("Localizable", key, source);
        Self {
            play_pause: localizable("touchbar.play_pause", "Play / Pause"),
            seek: localizable("touchbar.seek", "Seek"),
            volume_up: localizable("touchbar.increase_volume", "Volume +"),
            volume_down: localizable("touchbar.decrease_volume", "Volume -"),
            rewind: localizable("touchbar.rewind", "Rewind"),
            fast_forward: localizable("touchbar.fast_forward", "Fast Forward"),
            time: localizable("touchbar.time", "Time Position"),
            remaining: localizable(
                "touchbar.remainingTimeOrTotalDuration",
                "Show Remaining Time or Total Duration",
            ),
            ahead15: localizable("touchbar.ahead_15", "15sec Ahead"),
            ahead30: localizable("touchbar.ahead_30", "30sec Ahead"),
            back15: localizable("touchbar.back_15", "15sec Back"),
            back30: localizable("touchbar.back_30", "30sec Back"),
            next: localizable("touchbar.next_video", "Next Video"),
            previous: localizable("touchbar.prev_video", "Previous Video"),
            toggle_pip: localizable("touchbar.toggle_pip", "Toggle Picture-in-Picture"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ArrowPlan {
    Speed { speed: f64, resume: bool },
    Playlist { next: bool },
    Seek { seconds: f64 },
}

fn nearest_speed_index(speed: f64) -> usize {
    let speed = if speed.is_finite() && speed > 0.0 {
        speed
    } else {
        1.0
    };
    IINA_SPEED_VALUES
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            (speed / **left)
                .ln()
                .abs()
                .total_cmp(&(speed / **right).ln().abs())
        })
        .map(|(index, _)| index)
        .unwrap_or(NORMAL_SPEED_INDEX)
}

fn arrow_plan(player: &PlayerState, preference: i64, direction: f64) -> ArrowPlan {
    let next = direction >= 0.0;
    match preference {
        1 => ArrowPlan::Playlist { next },
        2 => ArrowPlan::Seek {
            seconds: if next { 10.0 } else { -10.0 },
        },
        _ => {
            let mut index = nearest_speed_index(player.speed);
            if next {
                if index < NORMAL_SPEED_INDEX {
                    index = NORMAL_SPEED_INDEX;
                }
                index = (index + 1).min(IINA_SPEED_VALUES.len() - 1);
            } else {
                if index > NORMAL_SPEED_INDEX {
                    index = NORMAL_SPEED_INDEX;
                }
                index = index.saturating_sub(1);
            }
            ArrowPlan::Speed {
                speed: IINA_SPEED_VALUES[index],
                resume: player.paused,
            }
        }
    }
}

fn exact_percent_seek_seconds(duration: f64, percentage: f64) -> Option<f64> {
    if !duration.is_finite() || duration <= 0.0 || !percentage.is_finite() {
        return None;
    }
    Some(duration * percentage.clamp(0.0, 100.0) / 100.0)
}

fn escape_binding_exits_fullscreen(modeled: Option<&serde_json::Value>) -> bool {
    active_key_bindings_from_preference(modeled)
        .into_iter()
        .find(|binding| binding.normalized_mpv_key == "ESC")
        .is_some_and(|binding| {
            !binding.is_iina_command
                && binding
                    .action
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    == ["set", "fullscreen", "no"]
        })
}

fn valid_session_label(label: &str) -> bool {
    if label.is_empty() || label.len() > MAX_SESSION_LABEL_BYTES || !label.is_ascii() {
        return false;
    }
    label == "main"
        || label.strip_prefix("player-").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn slider_touch_state() -> &'static Mutex<HashMap<String, bool>> {
    static STATE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

pub fn initialize(app: &AppHandle, state: &AppState) -> Result<(), String> {
    if APP_HANDLE.get().is_none() {
        APP_HANDLE
            .set(app.clone())
            .map_err(|_| "Touch Bar app handle was initialized concurrently".to_string())?;
    }
    platform::set_automatic_customization_enabled(false)?;
    if let Some(window) = app.get_webview_window("main") {
        install_window(&window, state, "main")?;
    }
    Ok(())
}

pub fn install_window<R: Runtime>(
    window: &WebviewWindow<R>,
    state: &AppState,
    session_label: &str,
) -> Result<(), String> {
    if !valid_session_label(session_label) {
        return Err(format!("invalid Touch Bar session label: {session_label}"));
    }
    #[cfg(target_os = "macos")]
    platform::install(
        window.ns_window().map_err(|error| error.to_string())?,
        session_label,
        &TouchBarLabels::localized(),
    )?;
    #[cfg(not(target_os = "macos"))]
    platform::install(
        std::ptr::null_mut(),
        session_label,
        &TouchBarLabels::localized(),
    )?;

    let session = state.player_session_for_window(session_label)?;
    let snapshot = session
        .player()
        .lock()
        .map(|player| player.clone())
        .map_err(|error| error.to_string())?;
    sync_session(window.app_handle(), state, session_label, &snapshot)
}

pub fn sync_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    session_label: &str,
    snapshot: &PlayerState,
) -> Result<(), String> {
    if !valid_session_label(session_label) {
        return Err(format!("invalid Touch Bar session label: {session_label}"));
    }
    let (precision, show_remaining, escape_exits_fullscreen) = state
        .preferences
        .lock()
        .map(|preferences| {
            (
                preferences
                    .values
                    .get("timeDisplayPrecision")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0)
                    .clamp(0, 3) as i32,
                preferences
                    .values
                    .get("touchbarShowRemainingTime")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                escape_binding_exits_fullscreen(preferences.values.get("modeledKeyBindings")),
            )
        })
        .map_err(|error| error.to_string())?;
    let fullscreen = app
        .get_webview_window(session_label)
        .and_then(|window| crate::commands::player_window_is_fullscreen(&window).ok())
        .unwrap_or(false);
    platform::update(
        session_label,
        snapshot,
        precision,
        show_remaining,
        fullscreen && escape_exits_fullscreen,
    )
}

pub fn sync_all<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> Result<(), String> {
    let labels = std::iter::once("main".to_string())
        .chain(state.player_session_labels()?)
        .collect::<Vec<_>>();
    for label in labels {
        let session = state.player_session_for_window(&label)?;
        let snapshot = session
            .player()
            .lock()
            .map(|player| player.clone())
            .map_err(|error| error.to_string())?;
        sync_session(app, state, &label, &snapshot)?;
    }
    Ok(())
}

pub fn update_thumbnail_progress(session_label: &str, progress: &ThumbnailProgress) {
    if !valid_session_label(session_label) {
        return;
    }
    platform::update_thumbnails(
        session_label,
        &progress.source_path,
        &progress.thumbnails,
        progress.progress,
        progress.complete,
    );
}

pub fn update_thumbnail_set(session_label: &str, set: &ThumbnailSet) {
    if !valid_session_label(session_label) {
        return;
    }
    platform::update_thumbnails(
        session_label,
        &set.source_path,
        &set.thumbnails,
        set.progress,
        true,
    );
}

pub fn clear_thumbnails(session_label: &str) {
    if valid_session_label(session_label) {
        platform::update_thumbnails(session_label, "", &[], 0.0, true);
    }
}

pub fn remove_session(session_label: &str) {
    if valid_session_label(session_label) {
        platform::remove_session(session_label);
        if let Ok(mut touches) = slider_touch_state().lock() {
            touches.remove(session_label);
        }
    }
}

pub fn shutdown() {
    platform::remove_all();
    if let Ok(mut touches) = slider_touch_state().lock() {
        touches.clear();
    }
}

unsafe extern "C" fn handle_action(
    session_label: *const std::ffi::c_char,
    raw_action: i32,
    value: f64,
    _context: *mut std::ffi::c_void,
) -> i32 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if session_label.is_null() {
            return Err("Touch Bar session label is missing".to_string());
        }
        let label = unsafe { std::ffi::CStr::from_ptr(session_label) }
            .to_str()
            .map_err(|_| "Touch Bar session label is not UTF-8".to_string())?;
        if !valid_session_label(label) {
            return Err("Touch Bar session label is invalid".to_string());
        }
        let action = TouchBarAction::try_from(raw_action)?;
        let app = APP_HANDLE
            .get()
            .ok_or_else(|| "Touch Bar app handle is not initialized".to_string())?;
        execute_action(app, label, action, value)
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            eprintln!("iima: Touch Bar action failed: {error}");
            -1
        }
        Err(_) => {
            eprintln!("iima: Touch Bar action panicked");
            -1
        }
    }
}

fn execute_action(
    app: &AppHandle,
    session_label: &str,
    action: TouchBarAction,
    value: f64,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    if action == TouchBarAction::ToggleRemaining {
        let show_remaining = value >= 0.5;
        let preferences = {
            let mut preferences = state
                .preferences
                .lock()
                .map_err(|error| error.to_string())?;
            preferences.values.insert(
                "touchbarShowRemainingTime".to_string(),
                serde_json::Value::Bool(show_remaining),
            );
            preferences.clone()
        };
        let path = preference_file_path(
            app.path()
                .app_config_dir()
                .map_err(|error| error.to_string())?,
        );
        preferences.save_to_file(&path)?;
        return sync_all(app, state.inner());
    }
    if action == TouchBarAction::ExitFullscreen {
        let window = app
            .get_webview_window(session_label)
            .ok_or_else(|| format!("player window is unavailable for {session_label}"))?;
        crate::commands::set_player_window_fullscreen(app, state.inner(), &window, false)?;
        let session = state.player_session_for_window(session_label)?;
        let snapshot = session
            .player()
            .lock()
            .map(|player| player.clone())
            .map_err(|error| error.to_string())?;
        sync_session(app, state.inner(), session_label, &snapshot)?;
        return Ok(());
    }
    if action == TouchBarAction::TogglePip {
        let snapshot = crate::commands::toggle_picture_in_picture_for_session(
            app,
            state.inner(),
            session_label,
        )?;
        crate::commands::emit_player_state_for_session(app, session_label, &snapshot);
        return Ok(());
    }

    let session = state.player_session_for_window(session_label)?;
    let mut changed = false;
    {
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        if player.current_url.is_none() {
            return Err(format!("Touch Bar session {session_label} has no media"));
        }
        match action {
            TouchBarAction::TogglePause => {
                player.apply(PlayerCommand::TogglePause);
                changed = true;
            }
            TouchBarAction::VolumeDelta if value.is_finite() => {
                let volume = player.volume + value;
                player.apply(PlayerCommand::SetVolume { volume });
                changed = true;
            }
            TouchBarAction::Arrow => {
                let preference = state
                    .preferences
                    .lock()
                    .map(|preferences| {
                        preferences
                            .values
                            .get("arrowBtnAction")
                            .and_then(serde_json::Value::as_i64)
                            .unwrap_or(0)
                    })
                    .map_err(|error| error.to_string())?;
                match arrow_plan(&player, preference, value) {
                    ArrowPlan::Speed { speed, resume } => {
                        if resume {
                            player.apply(PlayerCommand::Resume);
                        }
                        player.apply(PlayerCommand::SetSpeed { speed });
                    }
                    ArrowPlan::Playlist { next } => player.apply(if next {
                        PlayerCommand::PlaylistNext
                    } else {
                        PlayerCommand::PlaylistPrev
                    }),
                    ArrowPlan::Seek { seconds } => player.apply(PlayerCommand::SeekRelative {
                        seconds,
                        option: RelativeSeekOption::Relative,
                    }),
                }
                changed = true;
            }
            TouchBarAction::SeekRelative if value.is_finite() => {
                player.apply(PlayerCommand::SeekRelative {
                    seconds: value,
                    option: RelativeSeekOption::Relative,
                });
                changed = true;
            }
            TouchBarAction::PlaylistNavigate => {
                player.apply(if value >= 0.0 {
                    PlayerCommand::PlaylistNext
                } else {
                    PlayerCommand::PlaylistPrev
                });
                changed = true;
            }
            TouchBarAction::SeekPercent if value.is_finite() => {
                if let Some(seconds) = exact_percent_seek_seconds(player.duration_seconds, value) {
                    player.apply(PlayerCommand::Seek { seconds });
                    changed = true;
                }
            }
            TouchBarAction::SliderBegin => {
                let was_playing = !player.paused;
                slider_touch_state()
                    .lock()
                    .map_err(|error| error.to_string())?
                    .insert(session_label.to_string(), was_playing);
                if was_playing {
                    player.apply(PlayerCommand::Pause);
                    changed = true;
                }
            }
            TouchBarAction::SliderEnd => {
                let was_playing = slider_touch_state()
                    .lock()
                    .map_err(|error| error.to_string())?
                    .remove(session_label)
                    .unwrap_or(false);
                if was_playing {
                    player.apply(PlayerCommand::Resume);
                    changed = true;
                }
            }
            TouchBarAction::VolumeDelta
            | TouchBarAction::SeekRelative
            | TouchBarAction::SeekPercent
            | TouchBarAction::TogglePip
            | TouchBarAction::ExitFullscreen
            | TouchBarAction::ToggleRemaining => {}
        }
    }
    if changed {
        session.sync_mpv_executor_from_player()?;
    }
    let snapshot = session
        .player()
        .lock()
        .map(|player| player.clone())
        .map_err(|error| error.to_string())?;
    crate::commands::emit_player_state_for_session(app, session_label, &snapshot);
    Ok(())
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{handle_action, MediaThumbnail, PlayerState, TouchBarLabels};
    use std::ffi::{c_char, c_int, c_void, CString};

    unsafe extern "C" {
        fn iima_touch_bar_set_automatic_customization_enabled(enabled: c_int) -> c_int;
        fn iima_touch_bar_install(
            ns_window: *mut c_void,
            session_label: *const c_char,
            labels_json: *const c_char,
            callback: Option<unsafe extern "C" fn(*const c_char, c_int, f64, *mut c_void) -> c_int>,
            context: *mut c_void,
        ) -> c_int;
        fn iima_touch_bar_update(
            session_label: *const c_char,
            has_media: c_int,
            paused: c_int,
            position: f64,
            duration: f64,
            volume: f64,
            precision: c_int,
            show_remaining: c_int,
            current_url: *const c_char,
            fullscreen_escape: c_int,
        );
        fn iima_touch_bar_update_thumbnails_json(
            session_label: *const c_char,
            source: *const c_char,
            thumbnails_json: *const c_char,
            progress: f64,
            replace: c_int,
        );
        fn iima_touch_bar_remove_session(session_label: *const c_char);
        fn iima_touch_bar_remove_all();
    }

    pub fn set_automatic_customization_enabled(enabled: bool) -> Result<(), String> {
        let status =
            unsafe { iima_touch_bar_set_automatic_customization_enabled(i32::from(enabled)) };
        if status < 0 {
            Err("unable to configure automatic Touch Bar customization".to_string())
        } else {
            Ok(())
        }
    }

    pub fn install(
        ns_window: *mut c_void,
        session_label: &str,
        labels: &TouchBarLabels,
    ) -> Result<(), String> {
        let session_label = CString::new(session_label)
            .map_err(|_| "Touch Bar session label contains NUL".to_string())?;
        let labels = serde_json::to_string(labels)
            .map_err(|error| format!("unable to encode Touch Bar labels: {error}"))?;
        let labels =
            CString::new(labels).map_err(|_| "Touch Bar labels contain NUL".to_string())?;
        let status = unsafe {
            iima_touch_bar_install(
                ns_window,
                session_label.as_ptr(),
                labels.as_ptr(),
                Some(handle_action),
                std::ptr::null_mut(),
            )
        };
        if status < 0 {
            Err("unable to install native Touch Bar".to_string())
        } else {
            Ok(())
        }
    }

    pub fn update(
        session_label: &str,
        snapshot: &PlayerState,
        precision: i32,
        show_remaining: bool,
        fullscreen_escape: bool,
    ) -> Result<(), String> {
        let session_label = CString::new(session_label)
            .map_err(|_| "Touch Bar session label contains NUL".to_string())?;
        let current_url = CString::new(snapshot.current_url.as_deref().unwrap_or_default())
            .map_err(|_| "Touch Bar media URL contains NUL".to_string())?;
        unsafe {
            iima_touch_bar_update(
                session_label.as_ptr(),
                i32::from(snapshot.current_url.is_some()),
                i32::from(snapshot.paused),
                snapshot.position_seconds,
                snapshot.duration_seconds,
                snapshot.volume,
                precision,
                i32::from(show_remaining),
                current_url.as_ptr(),
                i32::from(fullscreen_escape),
            )
        };
        Ok(())
    }

    pub fn update_thumbnails(
        session_label: &str,
        source: &str,
        thumbnails: &[MediaThumbnail],
        progress: f64,
        replace: bool,
    ) {
        let Ok(session_label) = CString::new(session_label) else {
            return;
        };
        let Ok(source) = CString::new(source) else {
            return;
        };
        let Ok(json) = serde_json::to_string(thumbnails) else {
            return;
        };
        let Ok(json) = CString::new(json) else {
            return;
        };
        unsafe {
            iima_touch_bar_update_thumbnails_json(
                session_label.as_ptr(),
                source.as_ptr(),
                json.as_ptr(),
                progress,
                i32::from(replace),
            )
        };
    }

    pub fn remove_session(session_label: &str) {
        if let Ok(session_label) = CString::new(session_label) {
            unsafe { iima_touch_bar_remove_session(session_label.as_ptr()) };
        }
    }

    pub fn remove_all() {
        unsafe { iima_touch_bar_remove_all() };
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::{MediaThumbnail, PlayerState, TouchBarLabels};
    use std::ffi::c_void;

    pub fn set_automatic_customization_enabled(_enabled: bool) -> Result<(), String> {
        Ok(())
    }
    pub fn install(
        _ns_window: *mut c_void,
        _session_label: &str,
        _labels: &TouchBarLabels,
    ) -> Result<(), String> {
        Ok(())
    }
    pub fn update(
        _session_label: &str,
        _snapshot: &PlayerState,
        _precision: i32,
        _show_remaining: bool,
        _fullscreen_escape: bool,
    ) -> Result<(), String> {
        Ok(())
    }
    pub fn update_thumbnails(
        _session_label: &str,
        _source: &str,
        _thumbnails: &[MediaThumbnail],
        _progress: f64,
        _replace: bool,
    ) {
    }
    pub fn remove_session(_session_label: &str) {}
    pub fn remove_all() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_the_reference_speed_ladder_and_arrow_semantics() {
        let mut player = PlayerState::default();
        player.speed = 1.0;
        assert_eq!(
            arrow_plan(&player, 0, -1.0),
            ArrowPlan::Speed {
                speed: 0.5,
                resume: true,
            }
        );
        player.paused = false;
        player.speed = 8.0;
        assert_eq!(
            arrow_plan(&player, 0, -1.0),
            ArrowPlan::Speed {
                speed: 0.5,
                resume: false,
            }
        );
        assert_eq!(
            arrow_plan(&player, 1, 1.0),
            ArrowPlan::Playlist { next: true }
        );
        assert_eq!(
            arrow_plan(&player, 2, -1.0),
            ArrowPlan::Seek { seconds: -10.0 }
        );
        assert_eq!(IINA_SPEED_VALUES[NORMAL_SPEED_INDEX], 1.0);
    }

    #[test]
    fn escape_replacement_requires_the_exact_active_mpv_binding() {
        assert!(escape_binding_exits_fullscreen(None));
        assert!(escape_binding_exits_fullscreen(Some(&serde_json::json!([
            {
                "rawKey": "ESC",
                "rawAction": "set fullscreen no",
                "isIINACommand": false
            }
        ]))));
        assert!(!escape_binding_exits_fullscreen(Some(&serde_json::json!([
            {
                "rawKey": "ESC",
                "rawAction": "cycle fullscreen",
                "isIINACommand": false
            }
        ]))));
    }

    #[test]
    fn slider_uses_clamped_exact_percentage_while_buttons_remain_relative() {
        assert_eq!(exact_percent_seek_seconds(200.0, 25.0), Some(50.0));
        assert_eq!(exact_percent_seek_seconds(200.0, -5.0), Some(0.0));
        assert_eq!(exact_percent_seek_seconds(200.0, 150.0), Some(200.0));
        assert_eq!(exact_percent_seek_seconds(0.0, 50.0), None);
        assert_eq!(
            arrow_plan(&PlayerState::default(), 2, 1.0),
            ArrowPlan::Seek { seconds: 10.0 }
        );
    }

    #[test]
    fn native_contract_contains_reference_customization_and_thumbnail_geometry() {
        let native = include_str!("native_touch_bar.m");
        let menu = include_str!("menu.rs");
        for contract in [
            "defaultItemIdentifiers",
            "customizationAllowedItemIdentifiers",
            "NSTouchBarItemIdentifierFixedSpaceLarge",
            "escapeKeyReplacementItemIdentifier",
            "CGFloat imageWidth = 60.0",
            "x += 3.0",
            "Show Remaining Time or Total Duration",
            "toggleTouchBarCustomizationPalette:",
        ] {
            assert!(
                native.contains(contract) || menu.contains(contract),
                "missing native Touch Bar contract: {contract}"
            );
        }
        assert!(menu.contains("iina.custom-touch-bar"));
        assert!(native.contains("automaticCustomizeTouchBarMenuItemEnabled = enabled != 0"));
    }

    #[test]
    fn callback_session_labels_are_strictly_bounded_to_player_owners() {
        assert!(valid_session_label("main"));
        assert!(valid_session_label("player-42"));
        assert!(!valid_session_label("mini-player"));
        assert!(!valid_session_label("preferences"));
        assert!(!valid_session_label("player-other"));
        assert!(!valid_session_label(&format!("player-{}", "1".repeat(100))));
    }

    #[test]
    fn touch_bar_labels_use_the_complete_reference_localization_surface() {
        let labels = serde_json::to_value(TouchBarLabels::localized()).unwrap();
        for key in [
            "playPause",
            "seek",
            "volumeUp",
            "volumeDown",
            "rewind",
            "fastForward",
            "time",
            "remaining",
            "ahead15",
            "ahead30",
            "back15",
            "back30",
            "next",
            "previous",
            "togglePip",
        ] {
            assert!(labels
                .get(key)
                .and_then(serde_json::Value::as_str)
                .is_some());
        }
    }
}
