use crate::catalog::{catalog, ReplicationCatalog};
use crate::key_bindings::{KeyBindingProfile, KeyBindingProfileDocument, KeyBindingRepository};
use crate::localization;
use crate::media::{
    clear_thumbnail_cache as clear_thumbnail_cache_directory,
    configured_screenshot_directory_for_options, finalize_mpv_screenshot,
    generate_cached_thumbnails, media_runtime, probe_media, screenshot_cache_directory,
    thumbnail_cache_size, MediaProbe, MediaRuntime, ScreenshotFormat, ScreenshotOptions,
    ScreenshotResult, ThumbnailProgress, ThumbnailSet,
};
use crate::menu;
use crate::mpv::{
    iina_mpv_playback_session_plan, iina_observed_properties, libmpv_runtime_status,
    smoke_libmpv_client_session, LibmpvClientSmokeReport, LibmpvRuntimeStatus, MpvExecutorStatus,
    MpvObservedProperty, MpvPlaybackSessionPlan, MpvPluginGetKind, MpvPluginValue,
};
use crate::native_default_app;
use crate::native_file::{self, FileRemovalMode};
use crate::native_font_picker;
use crate::native_keychain::{self, HttpAuthCredentials};
use crate::native_open_panel;
use crate::native_pasteboard::{self, PlaylistPasteboardKind};
use crate::native_prompt;
use crate::native_recent_documents;
use crate::native_text_encoding;
use crate::native_updater::{self, UpdaterStatus};
use crate::native_video::{self, NativeVideoRendererStatus};
use crate::native_window_behavior;
use crate::online_subtitles::{self, OnlineSubtitleSearchResult};
#[cfg(not(target_os = "macos"))]
use crate::player::RecentDocument;
use crate::player::{
    AutomaticMusicModeTransition, ExternalTrackKind, FilterKind, PlayerCommand, PlayerMode,
    PlayerState,
};
use crate::playlist_actions::{self, IndexedPlaylistPath, PlaylistAutoAddPlan, PlaylistTargets};
use crate::playlist_cache::schedule_playlist_cache;
use crate::plugin_global;
use crate::plugin_websocket;
use crate::plugins::{
    self, PluginGithubUpdate, PluginInstallNotification, PluginInstallResult, PluginMenuDefinition,
    PluginMenuItemDefinition, PluginPageContents, PluginRecord, PluginRuntimeSpec,
};
use crate::preference_effects;
use crate::preferences::{preference_file_path, PreferenceChange, PreferenceStore};
use crate::state::{player_session_label_for_window, AppState, PlayerSessionRef};
use crate::subtitle_autoload;
use crate::window_lifecycle::{
    PipToggleDirective, PipWindowBehavior, PipWindowTransition, PlaybackDirective,
    WindowResizeDirective,
};
use crate::window_size::PlaybackWindowResizeAction;
use crate::{app_logging, auxiliary_windows};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, Runtime, Url, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_plugin_dialog::DialogExt;

pub(crate) const MINI_PLAYER_LABEL: &str = "mini-player";
const PLAYER_WINDOW_URL_PREFIX: &str = "index.html?player-session=";
const PLAYER_STATE_EVENT: &str = "iima-player-state";
const PLAYER_WINDOW_STATUS_EVENT: &str = "iima-player-window-status";
const PLUGIN_MPV_EVENT_BATCH_EVENT: &str = "iima-plugin-mpv-events";
const PLUGIN_HOST_EVENT: &str = "iima-plugin-host-event";
const THUMBNAIL_PROGRESS_EVENT: &str = "iima-thumbnail-progress";
const MINI_PLAYER_CONTROL_HEIGHT: f64 = 72.0;
const MINI_PLAYER_PLAYLIST_HEIGHT: f64 = 300.0;
const MINI_PLAYER_INITIAL_WIDTH: f64 = 300.0;
pub(crate) const PREFERENCE_CHANGED_EVENT: &str = "iima-preference-changed";
static PLAYLIST_TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static PLUGIN_FILE_TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static PLUGIN_FILE_HANDLE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static PREFERENCE_CHANGE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const PLUGIN_FILE_HANDLE_MAX_IO_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PluginFileHandleMode {
    Read,
    Write,
}

struct PluginOpenFileHandle {
    identifier: String,
    window_label: String,
    mode: PluginFileHandleMode,
    file: fs::File,
}

fn plugin_file_handles() -> &'static Mutex<HashMap<String, PluginOpenFileHandle>> {
    static HANDLES: OnceLock<Mutex<HashMap<String, PluginOpenFileHandle>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn cleanup_plugin_file_handles_for_window(window_label: &str) {
    if let Ok(mut handles) = plugin_file_handles().lock() {
        handles.retain(|_, handle| handle.window_label != window_label);
    }
}

pub(crate) fn cleanup_plugin_file_handles_for_identifier(identifier: &str) {
    if let Ok(mut handles) = plugin_file_handles().lock() {
        handles.retain(|_, handle| handle.identifier != identifier);
    }
}

pub(crate) fn cleanup_plugin_file_handle_tokens(tokens: &[String]) {
    if tokens.is_empty() {
        return;
    }
    if let Ok(mut handles) = plugin_file_handles().lock() {
        handles.retain(|token, _| !tokens.iter().any(|expired| expired == token));
    }
}

pub(crate) fn cleanup_all_plugin_file_handles() {
    if let Ok(mut handles) = plugin_file_handles().lock() {
        handles.clear();
    }
}

pub(crate) fn pause_when_open_preference(state: &AppState) -> Result<bool, String> {
    state
        .preferences
        .lock()
        .map(|preferences| bool_preference(&preferences.values, "pauseWhenOpen", false))
        .map_err(|error| error.to_string())
}

fn fullscreen_when_open_preference(state: &AppState) -> Result<bool, String> {
    state
        .preferences
        .lock()
        .map(|preferences| bool_preference(&preferences.values, "fullScreenWhenOpen", false))
        .map_err(|error| error.to_string())
}

fn plan_open_media_paths(state: &AppState, paths: Vec<String>) -> PlaylistAutoAddPlan {
    playlist_actions::plan_playlist_auto_add(&paths, state.playlist_auto_add_at_startup())
}

fn plan_auto_loaded_subtitles(state: &AppState, paths: &[String]) -> Result<Vec<String>, String> {
    let preferences = state
        .preferences
        .lock()
        .map(|preferences| preferences.clone())
        .map_err(|error| error.to_string())?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    Ok(subtitle_autoload::plan(
        paths,
        &preferences,
        state.playlist_auto_add_at_startup(),
        home.as_deref(),
    ))
}

fn open_media_batch_with_plan(
    player: &mut PlayerState,
    plan: PlaylistAutoAddPlan,
    auto_loaded_subtitles: Vec<String>,
    probe: Result<MediaProbe, String>,
    pause_when_open: bool,
) {
    let PlaylistAutoAddPlan {
        paths,
        preceding_sibling_indexes,
    } = plan;
    player.open_media_batch_with_pause(paths, probe, pause_when_open);
    if !preceding_sibling_indexes.is_empty() {
        player.apply(PlayerCommand::MovePlaylistItems {
            indexes: preceding_sibling_indexes,
            destination: 0,
        });
    }
    for path in auto_loaded_subtitles {
        player.apply(PlayerCommand::LoadExternalTrack {
            kind: ExternalTrackKind::Subtitles,
            path,
        });
    }
}

#[cfg(target_os = "macos")]
fn native_video_surface_settings(
    state: &AppState,
) -> Result<native_video::NativeVideoSurfaceSettings, String> {
    state
        .preferences
        .lock()
        .map(|preferences| native_video::surface_settings_from_preferences(&preferences))
        .map_err(|error| error.to_string())
}

fn action_after_launch_preference(state: &AppState) -> Result<i64, String> {
    state
        .preferences
        .lock()
        .map(|preferences| {
            preferences
                .values
                .get("actionAfterLaunch")
                .and_then(Value::as_i64)
                .filter(|value| (0..=2).contains(value))
                .unwrap_or(0)
        })
        .map_err(|error| error.to_string())
}

fn should_open_in_new_player(
    always_open_in_new_window: bool,
    is_alternative_action: bool,
    has_playing_media: bool,
) -> bool {
    has_playing_media && (always_open_in_new_window != is_alternative_action)
}

pub(crate) fn should_open_in_new_player_for_menu_action(
    state: &AppState,
    is_alternative_action: bool,
) -> Result<bool, String> {
    let always_open_in_new_window = state
        .preferences
        .lock()
        .map(|preferences| bool_preference(&preferences.values, "alwaysOpenInNewWindow", true))
        .map_err(|error| error.to_string())?;
    Ok(should_open_in_new_player(
        always_open_in_new_window,
        is_alternative_action,
        state.has_playing_media()?,
    ))
}

fn open_url_submission_route(
    state: &AppState,
    is_alternative_action: bool,
    enqueue: bool,
) -> Result<(String, bool), String> {
    // OpenURLWindowController asks PlayerCore for the active player only when Open is pressed.
    // Do the same here instead of retaining the player that happened to own the window earlier.
    let active_session_label = state.last_active_player_session_label()?;
    if enqueue {
        return Ok((active_session_label, false));
    }
    let active_has_media = state
        .player_session_for_window(&active_session_label)?
        .player()
        .lock()
        .map(|player| player.current_url.is_some())
        .map_err(|error| error.to_string())?;
    let always_open_in_new_window = state
        .preferences
        .lock()
        .map(|preferences| bool_preference(&preferences.values, "alwaysOpenInNewWindow", true))
        .map_err(|error| error.to_string())?;
    Ok((
        active_session_label,
        should_open_in_new_player(
            always_open_in_new_window,
            is_alternative_action,
            active_has_media,
        ),
    ))
}

fn service_open_url_route(
    state: &AppState,
    active_player_session_label: Option<&str>,
    url: String,
) -> Result<Option<(String, String)>, String> {
    // Foundation rejects an empty URL string. Keep this second guard at the command boundary so a
    // future Services caller cannot accidentally turn a rejected pasteboard value into loadfile "".
    if url.is_empty() {
        return Ok(None);
    }
    // AppDelegate.droppedText uses PlayerCore.active directly rather than activeOrNew/lastActive.
    // The native-main-window resolver supplies a player only when NSApp.mainWindow represents an
    // actual PlayerWindowController; Initial/utility/no-main states fall back to PlayerCore.first.
    let target = active_player_session_label.unwrap_or("main");
    Ok(Some((
        state.player_session_for_window(target)?.label().to_string(),
        url,
    )))
}

fn service_player_controller_session_label(
    window_label: &str,
    snapshot: &PlayerState,
    plugin_managed: bool,
) -> Option<String> {
    let is_player_window = window_label == "main"
        || window_label.starts_with("player-")
        || window_label.starts_with("mini-player");
    if !is_player_window {
        return None;
    }
    // Mini Player and JavascriptAPI-created windows always have a PlayerWindowController. An
    // ordinary retained Tauri window emulates InitialWindowController while idle and switches to
    // MainWindowController only while its owning session has a media presentation.
    let is_player_controller = window_label.starts_with("mini-player")
        || plugin_managed
        || player_state_uses_media_window(snapshot);
    is_player_controller.then(|| player_session_label_for_window(window_label).to_string())
}

#[cfg(target_os = "macos")]
fn service_active_player_session_label<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    native_main_window: *mut std::ffi::c_void,
) -> Result<Option<String>, String> {
    if native_main_window.is_null() {
        return Ok(None);
    }
    for (label, window) in app.webview_windows() {
        if label != "main" && !label.starts_with("player-") && !label.starts_with("mini-player") {
            continue;
        }
        let Ok(native_window) = window.ns_window() else {
            continue;
        };
        if native_window != native_main_window {
            continue;
        }

        let session_label = player_session_label_for_window(&label).to_string();
        let Ok(session) = state.player_session_for_window(&session_label) else {
            // A player-ish WebView without an owning PlayerCore is not a PlayerWindowController;
            // match PlayerCore.active by falling back to PlayerCore.first.
            return Ok(None);
        };
        let snapshot = session
            .player()
            .lock()
            .map(|player| player.clone())
            .map_err(|error| error.to_string())?;
        return Ok(service_player_controller_session_label(
            &label,
            &snapshot,
            is_plugin_managed_player_window(&window),
        ));
    }
    Ok(None)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenUrlSubmissionResult {
    target_session_label: String,
    opened_window_label: Option<String>,
    player: Option<PlayerState>,
    enqueued: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreferenceChangedEvent {
    revision: u64,
    key: String,
    value: Value,
    preferences: PreferenceStore,
    origin_label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceSnapshot {
    revision: u64,
    preferences: PreferenceStore,
}

#[tauri::command]
pub async fn complete_initial_launch(
    state: tauri::State<'_, AppState>,
    window: WebviewWindow,
) -> Result<String, String> {
    if window.label() != "main" {
        return Ok("ignored".to_string());
    }
    tauri::async_runtime::spawn_blocking(|| std::thread::sleep(Duration::from_millis(100)))
        .await
        .map_err(|error| error.to_string())?;
    if !state.claim_initial_launch_action() {
        return Ok("suppressed".to_string());
    }
    match action_after_launch_preference(state.inner())? {
        1 => Ok("open-panel".to_string()),
        2 => Ok("none".to_string()),
        _ => {
            show_initial_player_window(window.app_handle(), state.inner(), "main")?;
            Ok("welcome-window".to_string())
        }
    }
}

pub(crate) fn handle_application_reopen<R: Runtime>(
    app: &AppHandle<R>,
    has_visible_windows: bool,
) -> Result<(), String> {
    if has_visible_windows {
        return Ok(());
    }
    match action_after_launch_preference(app.state::<AppState>().inner())? {
        1 => app
            .emit_to(
                "main",
                "iima-menu-request",
                serde_json::json!({ "action": "open" }),
            )
            .map_err(|error| error.to_string()),
        2 => Ok(()),
        _ => show_initial_player_window(app, app.state::<AppState>().inner(), "main").map(|_| ()),
    }
}

pub(crate) fn mini_player_label_for_session(session_label: &str) -> String {
    if session_label == "main" {
        MINI_PLAYER_LABEL.to_string()
    } else {
        format!("mini-player-{session_label}")
    }
}

pub(crate) fn mini_player_session_label(label: &str) -> Option<&str> {
    (label == MINI_PLAYER_LABEL || label.starts_with("mini-player-"))
        .then(|| player_session_label_for_window(label))
}

#[tauri::command]
pub fn get_replication_catalog() -> ReplicationCatalog {
    catalog()
}

#[tauri::command]
pub fn get_media_runtime() -> MediaRuntime {
    media_runtime()
}

#[tauri::command]
pub fn get_mpv_observer_contract() -> Vec<MpvObservedProperty> {
    iina_observed_properties()
}

#[tauri::command]
pub fn get_mpv_playback_session_plan() -> MpvPlaybackSessionPlan {
    iina_mpv_playback_session_plan()
}

#[tauri::command]
pub fn get_libmpv_runtime_status() -> LibmpvRuntimeStatus {
    libmpv_runtime_status()
}

#[tauri::command]
pub fn smoke_libmpv_client() -> LibmpvClientSmokeReport {
    smoke_libmpv_client_session()
}

#[tauri::command]
pub fn get_mpv_executor_status(
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<MpvExecutorStatus, String> {
    state
        .inner()
        .player_session_for_window(window.label())?
        .mpv_executor_status()
}

#[tauri::command]
pub fn get_native_video_renderer_status(
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<NativeVideoRendererStatus, String> {
    let session = state.inner().player_session_for_window(window.label())?;
    Ok(native_video::status(session.label()))
}

#[tauri::command]
pub fn sync_mpv_executor(
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<MpvExecutorStatus, String> {
    state
        .inner()
        .player_session_for_window(window.label())?
        .sync_mpv_executor_from_player()
}

#[tauri::command]
pub fn probe_media_file(path: String) -> Result<MediaProbe, String> {
    probe_media(&path)
}

#[tauri::command]
pub async fn generate_media_thumbnails(
    app: AppHandle,
    window: WebviewWindow,
    path: String,
    width: Option<u32>,
    count: Option<usize>,
) -> Result<ThumbnailSet, String> {
    let session_label = player_session_label_for_window(window.label()).to_string();
    let (enabled, enable_remote_volumes, preference_width, max_cache_size_bytes) = {
        let state = app.state::<AppState>();
        state
            .preferences
            .lock()
            .map(|preferences| {
                let max_cache_mebibytes =
                    integer_preference(&preferences.values, "maxThumbnailPreviewCacheSize")
                        .unwrap_or(500)
                        .max(0) as u64;
                (
                    bool_preference(&preferences.values, "enableThumbnailPreview", true),
                    bool_preference(&preferences.values, "enableThumbnailForRemoteFiles", false),
                    integer_preference(&preferences.values, "thumbnailWidth")
                        .unwrap_or(240)
                        .clamp(64, 720) as u32,
                    max_cache_mebibytes.saturating_mul(1024 * 1024),
                )
            })
            .map_err(|error| error.to_string())?
    };
    let width = width.or(Some(preference_width));
    if !enabled {
        app.state::<AppState>()
            .cancel_thumbnail_generation(&session_label)?;
        crate::native_touch_bar::clear_thumbnails(&session_label);
        return Ok(ThumbnailSet {
            source_path: path,
            width: width.unwrap_or(240),
            requested_count: count.unwrap_or(100),
            thumbnails: Vec::new(),
            progress: 0.0,
            ready: false,
            cache_hit: false,
            cancelled: true,
        });
    }
    if !enable_remote_volumes && !native_video::path_is_on_local_volume(&path) {
        app.state::<AppState>()
            .cancel_thumbnail_generation(&session_label)?;
        crate::native_touch_bar::clear_thumbnails(&session_label);
        return Ok(ThumbnailSet {
            source_path: path,
            width: width.unwrap_or(240),
            requested_count: count.unwrap_or(100),
            thumbnails: Vec::new(),
            progress: 0.0,
            ready: false,
            cache_hit: false,
            cancelled: true,
        });
    }

    let generation_id = app
        .state::<AppState>()
        .begin_thumbnail_generation(&session_label)?;
    let cache_directory = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?
        .join("thumb_cache");
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = worker_app.state::<AppState>();
        let result = generate_cached_thumbnails(
            &path,
            width,
            count,
            &cache_directory,
            max_cache_size_bytes,
            |progress| {
                if state
                    .thumbnail_generation_is_current(&session_label, generation_id)
                    .unwrap_or(false)
                {
                    crate::native_touch_bar::update_thumbnail_progress(&session_label, &progress);
                    let _ = window.emit(
                        THUMBNAIL_PROGRESS_EVENT,
                        ThumbnailProgressEvent {
                            session_label: session_label.clone(),
                            generation_id,
                            progress,
                        },
                    );
                }
            },
            || {
                !state
                    .thumbnail_generation_is_current(&session_label, generation_id)
                    .unwrap_or(false)
            },
        );
        if state
            .thumbnail_generation_is_current(&session_label, generation_id)
            .unwrap_or(false)
        {
            match &result {
                Ok(set) if !set.cancelled => {
                    crate::native_touch_bar::update_thumbnail_set(&session_label, set)
                }
                Ok(_) | Err(_) => crate::native_touch_bar::clear_thumbnails(&session_label),
            }
        }
        result
    })
    .await
    .map_err(|error| format!("Thumbnail worker failed: {error}"))?
}

#[tauri::command]
pub fn cancel_media_thumbnails(
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<(), String> {
    let session_label = player_session_label_for_window(window.label());
    state.cancel_thumbnail_generation(session_label)?;
    crate::native_touch_bar::clear_thumbnails(session_label);
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct ThumbnailCacheStats {
    size_bytes: u64,
}

#[tauri::command]
pub fn get_thumbnail_cache_stats(app: AppHandle) -> Result<ThumbnailCacheStats, String> {
    let cache_directory = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?
        .join("thumb_cache");
    Ok(ThumbnailCacheStats {
        size_bytes: thumbnail_cache_size(&cache_directory),
    })
}

#[tauri::command]
pub fn clear_thumbnail_cache(
    app: AppHandle,
    state: tauri::State<AppState>,
) -> Result<ThumbnailCacheStats, String> {
    state.cancel_all_thumbnail_generations()?;
    let cache_directory = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?
        .join("thumb_cache");
    clear_thumbnail_cache_directory(&cache_directory)?;
    for label in std::iter::once("main".to_string()).chain(state.player_session_labels()?) {
        crate::native_touch_bar::clear_thumbnails(&label);
    }
    Ok(ThumbnailCacheStats { size_bytes: 0 })
}

#[derive(Debug, Clone, Serialize)]
struct ThumbnailProgressEvent {
    session_label: String,
    generation_id: u64,
    progress: ThumbnailProgress,
}

#[tauri::command]
pub fn capture_current_screenshot(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<ScreenshotResult, String> {
    let session = state
        .inner()
        .player_session_for_shortcut_window(window.label())?;
    session.sync_mpv_executor_from_player()?;
    let (path, time_seconds, has_video) = {
        let player = session.player().lock().map_err(|error| error.to_string())?;
        let path = player
            .current_url
            .clone()
            .ok_or_else(|| "No media is open".to_string())?;
        (path, player.position_seconds, player.mpv_properties.vid > 0)
    };
    if !has_video {
        return Err("Cannot capture screenshot: no video track".to_string());
    }
    let options = {
        let preferences = state
            .preferences
            .lock()
            .map_err(|error| error.to_string())?;
        screenshot_options_from_preferences(&preferences.values)
    };
    if !options.save_to_file && !options.copy_to_clipboard {
        return Err("Screenshot output is disabled".to_string());
    }
    let output_directory = if options.save_to_file {
        configured_screenshot_directory_for_options(&options)
    } else {
        screenshot_cache_directory(
            &app.path()
                .app_cache_dir()
                .map_err(|error| error.to_string())?,
        )
    };
    fs::create_dir_all(&output_directory)
        .map_err(|error| format!("Failed to create screenshot directory: {error}"))?;

    session
        .mpv_executor()
        .lock()
        .map_err(|error| error.to_string())?
        .capture_screenshot(
            &output_directory,
            options.format.extension(),
            &options.template,
            options.include_subtitles,
            Duration::from_secs(5),
        )?;
    session.mpv_executor_status()?;

    let result = finalize_mpv_screenshot(&path, time_seconds, &output_directory, &options)?;
    if Path::new(&result.path).is_file() {
        app.asset_protocol_scope()
            .allow_file(&result.path)
            .map_err(|error| format!("Failed to expose screenshot preview: {error}"))?;
    }
    Ok(result)
}

#[tauri::command]
pub fn reveal_screenshot_folder(state: tauri::State<AppState>) -> Result<String, String> {
    let options = {
        let preferences = state
            .preferences
            .lock()
            .map_err(|error| error.to_string())?;
        screenshot_options_from_preferences(&preferences.values)
    };
    let directory = configured_screenshot_directory_for_options(&options);
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Failed to create screenshot directory: {error}"))?;

    Command::new("open")
        .arg(&directory)
        .spawn()
        .map_err(|error| format!("Failed to open screenshot directory: {error}"))?;

    Ok(directory.display().to_string())
}

#[tauri::command]
pub fn reveal_screenshot_file(
    app: AppHandle,
    state: tauri::State<AppState>,
    path: String,
) -> Result<(), String> {
    let path = existing_screenshot_file_path(&app, state, &path)?;
    Command::new("open")
        .args(["-R"])
        .arg(&path)
        .spawn()
        .map_err(|error| format!("Failed to reveal screenshot: {error}"))?;
    Ok(())
}

#[tauri::command]
pub fn open_screenshot_file(
    app: AppHandle,
    state: tauri::State<AppState>,
    path: String,
) -> Result<(), String> {
    let path = existing_screenshot_file_path(&app, state, &path)?;
    Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|error| format!("Failed to open screenshot: {error}"))?;
    Ok(())
}

#[tauri::command]
pub fn delete_screenshot_file(
    app: AppHandle,
    state: tauri::State<AppState>,
    path: String,
) -> Result<(), String> {
    let path = existing_screenshot_file_path(&app, state, &path)?;
    fs::remove_file(&path).map_err(|error| format!("Failed to delete screenshot: {error}"))
}

fn existing_screenshot_file_path(
    app: &AppHandle,
    state: tauri::State<AppState>,
    path: &str,
) -> Result<std::path::PathBuf, String> {
    let path = std::path::PathBuf::from(path);
    if !path.is_file() {
        return Err(format!(
            "Screenshot file does not exist: {}",
            path.display()
        ));
    }
    let options = {
        let preferences = state
            .preferences
            .lock()
            .map_err(|error| error.to_string())?;
        screenshot_options_from_preferences(&preferences.values)
    };
    let file = path
        .canonicalize()
        .map_err(|error| format!("Failed to resolve screenshot path: {error}"))?;
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?;
    let allowed_directories = [
        configured_screenshot_directory_for_options(&options),
        screenshot_cache_directory(&cache_root),
    ];
    for directory in allowed_directories {
        if directory
            .canonicalize()
            .is_ok_and(|directory| file.starts_with(directory))
        {
            return Ok(file);
        }
    }
    Err(format!(
        "Screenshot file is outside the screenshot directories: {}",
        file.display()
    ))
}

fn screenshot_options_from_preferences(
    values: &std::collections::BTreeMap<String, Value>,
) -> ScreenshotOptions {
    ScreenshotOptions {
        save_to_file: bool_preference(values, "screenshotSaveToFile", true),
        copy_to_clipboard: bool_preference(values, "screenshotCopyToClipboard", false),
        include_subtitles: bool_preference(values, "screenShotIncludeSubtitle", true),
        directory: string_preference(values, "screenShotFolder"),
        format: integer_preference(values, "screenShotFormat")
            .and_then(ScreenshotFormat::from_i64)
            .unwrap_or(ScreenshotFormat::Png),
        template: string_preference(values, "screenShotTemplate").unwrap_or_else(|| "%F-%n".into()),
        show_preview: bool_preference(values, "screenshotShowPreview", true),
    }
}

fn bool_preference(
    values: &std::collections::BTreeMap<String, Value>,
    key: &str,
    default_value: bool,
) -> bool {
    values
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(default_value)
}

fn integer_preference(
    values: &std::collections::BTreeMap<String, Value>,
    key: &str,
) -> Option<i64> {
    values.get(key).and_then(Value::as_i64)
}

fn string_preference(
    values: &std::collections::BTreeMap<String, Value>,
    key: &str,
) -> Option<String> {
    values
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerWindowStatus {
    pub fullscreen: bool,
    pub always_on_top: bool,
    pub battery_capacity: Option<u8>,
    pub battery_charging: Option<bool>,
}

fn player_window_status<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Result<PlayerWindowStatus, String> {
    let battery = native_window_behavior::battery_status();
    Ok(PlayerWindowStatus {
        fullscreen: player_window_is_fullscreen(window)?,
        always_on_top: window
            .is_always_on_top()
            .map_err(|error| error.to_string())?,
        battery_capacity: battery.map(|status| status.capacity),
        battery_charging: battery.map(|status| status.charging),
    })
}

fn emit_player_window_status<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    if let Ok(status) = player_window_status(window) {
        let _ = app.emit_to(window.label(), PLAYER_WINDOW_STATUS_EVENT, status);
    }
}

#[tauri::command]
pub fn get_player_window_status(window: WebviewWindow) -> Result<PlayerWindowStatus, String> {
    if !is_player_window_label(window.label()) {
        return Err("Window status is available only in a player window".to_string());
    }
    player_window_status(&window)
}

#[tauri::command]
pub fn get_player_snapshot(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<PlayerState, String> {
    let session_label = state
        .inner()
        .shortcut_player_session_label(window.label())?;
    let session = state.inner().player_session_for_window(&session_label)?;
    let snapshot = player_snapshot_for_session(&session)?;
    let snapshot = automatically_sync_music_mode(&app, state.inner(), &session_label, snapshot)?;
    sync_player_window_surface(&app, state.inner(), &session_label, &snapshot)?;
    sync_player_window_video_size(&app, state.inner(), &session_label, &snapshot)?;
    sync_player_window_always_on_top(&app, state.inner(), &session_label, &snapshot)?;
    Ok(snapshot)
}

#[tauri::command]
pub fn refresh_player_menu(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    if window.is_focused().unwrap_or(false) {
        menu::refresh_iina_menu(&app)?;
    }
    Ok(())
}

fn player_snapshot_for_session(session: &PlayerSessionRef<'_>) -> Result<PlayerState, String> {
    session.mpv_executor_status()?;
    let hdr_status = native_video::hdr_status(session.label());
    let pip_active = native_video::pip_is_active_for_session(session.label());
    let mut snapshot = session
        .player()
        .lock()
        .map(|mut player| {
            player.set_hdr_status(hdr_status.available, hdr_status.enabled);
            player.set_pip_active(pip_active);
            player.clone()
        })
        .map_err(|error| error.to_string())?;

    let cache = session.playlist_cache();
    if let Some(path) = snapshot.current_url.as_deref() {
        cache
            .lock()
            .map_err(|error| error.to_string())?
            .record_runtime(
                path,
                snapshot.duration_seconds,
                snapshot.position_seconds,
                &snapshot.music_title,
                &snapshot.music_album,
                &snapshot.music_artist,
            );
    }
    let playlist_paths = snapshot
        .playlist
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    if session.prefetch_playlist_video_duration()? {
        let _ = schedule_playlist_cache(
            cache.clone(),
            playlist_paths.clone(),
            session.playlist_watch_later_directory()?,
        );
    }
    snapshot.playlist_cache = cache
        .lock()
        .map_err(|error| error.to_string())?
        .snapshot(playlist_paths.iter().map(String::as_str));
    Ok(snapshot)
}

fn reset_hdr_for_player_session(
    state: &AppState,
    session: &PlayerSessionRef<'_>,
) -> Result<(), String> {
    let preferences = state
        .preferences
        .lock()
        .map(|preferences| preferences.clone())
        .map_err(|error| error.to_string())?;
    let enabled = native_video::color_settings_from_preferences(&preferences).enable_hdr_support;
    session
        .player()
        .lock()
        .map_err(|error| error.to_string())?
        .apply(PlayerCommand::SetHdrEnabled { enabled });
    native_video::set_hdr_enabled(enabled, session.label());
    Ok(())
}

fn apply_open_window_preferences<R: Runtime>(
    window: &WebviewWindow<R>,
    state: &AppState,
    snapshot: &PlayerState,
    opened_manually: bool,
) -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, state, snapshot, opened_manually);
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        let (use_physical_resolution, resize_timing, resize_option, configured_geometry) = state
            .preferences
            .lock()
            .map(|preferences| {
                (
                    bool_preference(&preferences.values, "usePhysicalResolution", true),
                    integer_preference(&preferences.values, "resizeWindowTiming").unwrap_or(1),
                    integer_preference(&preferences.values, "resizeWindowOption").unwrap_or(2),
                    preferences
                        .values
                        .get("initialWindowSizePosition")
                        .and_then(Value::as_str)
                        .filter(|geometry| !geometry.is_empty())
                        .map(ToString::to_string),
                )
            })
            .map_err(|error| error.to_string())?;
        let directive = if opened_manually {
            WindowResizeDirective::ManuallyOpenedFile
        } else {
            WindowResizeDirective::AutomaticallyStartedFile
        };
        let action = playback_window_resize_action(directive, resize_timing, resize_option);
        let video_size = snapshot.video_size_for_display();
        let (file_generation, video_reconfiguration_generation, _, _) =
            snapshot.window_resize_observation();
        let (geometry, video_resize_claimed) = {
            let mut lifecycle_by_window = state
                .player_window_lifecycle
                .lock()
                .map_err(|error| error.to_string())?;
            let lifecycle = lifecycle_by_window
                .entry(window.label().to_string())
                .or_default();
            let geometry = if configured_geometry.is_some() && lifecycle.claim_initial_geometry() {
                configured_geometry
            } else {
                None
            };
            let video_resize_claimed = video_size.is_some_and(|video_size| {
                lifecycle
                    .claim_pending_window_resize(
                        file_generation,
                        video_reconfiguration_generation,
                        opened_manually,
                        video_size,
                    )
                    .is_some()
            });
            (geometry, video_resize_claimed)
        };
        if video_size.is_some() && !video_resize_claimed && geometry.is_none() {
            return Ok(());
        }
        match action {
            PlaybackWindowResizeAction::Preference(_) => {
                if video_size.is_some() || geometry.is_some() {
                    crate::window_size::resize_player_window_for_open(
                        window,
                        video_size,
                        use_physical_resolution,
                        resize_option,
                        geometry.as_deref(),
                    )?;
                }
            }
            PlaybackWindowResizeAction::PreserveWidth
            | PlaybackWindowResizeAction::VideoReconfigured => {
                if let Some(video_size) = video_size {
                    crate::window_size::resize_player_window_for_playback(
                        window,
                        video_size,
                        use_physical_resolution,
                        action,
                    )?;
                }
            }
        }
        Ok(())
    }
}

fn playback_window_resize_action(
    directive: WindowResizeDirective,
    resize_timing: i64,
    resize_option: i64,
) -> PlaybackWindowResizeAction {
    if directive == WindowResizeDirective::VideoReconfigured {
        return PlaybackWindowResizeAction::VideoReconfigured;
    }
    let resize_file = resize_timing == 0
        || (resize_timing == 1 && directive == WindowResizeDirective::ManuallyOpenedFile);
    if resize_file {
        PlaybackWindowResizeAction::Preference(resize_option)
    } else {
        PlaybackWindowResizeAction::PreserveWidth
    }
}

fn sync_player_window_video_size<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    session_label: &str,
    snapshot: &PlayerState,
) -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, state, session_label, snapshot);
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        if snapshot.file_loading {
            return Ok(());
        }
        let Some(window) = app.get_webview_window(session_label) else {
            return Ok(());
        };
        let Some(video_size) = snapshot.video_size_for_display() else {
            return Ok(());
        };
        let (file_generation, video_reconfiguration_generation, opened_manually, geometry_ready) =
            snapshot.window_resize_observation();
        if !geometry_ready {
            return Ok(());
        }
        let (use_physical_resolution, resize_timing, resize_option, configured_geometry) = state
            .preferences
            .lock()
            .map(|preferences| {
                (
                    bool_preference(&preferences.values, "usePhysicalResolution", true),
                    integer_preference(&preferences.values, "resizeWindowTiming").unwrap_or(1),
                    integer_preference(&preferences.values, "resizeWindowOption").unwrap_or(2),
                    preferences
                        .values
                        .get("initialWindowSizePosition")
                        .and_then(Value::as_str)
                        .filter(|geometry| !geometry.is_empty())
                        .map(ToString::to_string),
                )
            })
            .map_err(|error| error.to_string())?;
        let (directive, geometry) = {
            let mut lifecycle_by_window = state
                .player_window_lifecycle
                .lock()
                .map_err(|error| error.to_string())?;
            let lifecycle = lifecycle_by_window
                .entry(session_label.to_string())
                .or_default();
            let Some(directive) = lifecycle.claim_pending_window_resize(
                file_generation,
                video_reconfiguration_generation,
                opened_manually,
                video_size,
            ) else {
                return Ok(());
            };
            let geometry = if directive != WindowResizeDirective::VideoReconfigured
                && configured_geometry.is_some()
                && lifecycle.claim_initial_geometry()
            {
                configured_geometry
            } else {
                None
            };
            (directive, geometry)
        };
        let action = playback_window_resize_action(directive, resize_timing, resize_option);
        if matches!(action, PlaybackWindowResizeAction::Preference(_)) {
            crate::window_size::resize_player_window_for_open(
                &window,
                Some(video_size),
                use_physical_resolution,
                resize_option,
                geometry.as_deref(),
            )?;
        } else {
            crate::window_size::resize_player_window_for_playback(
                &window,
                video_size,
                use_physical_resolution,
                action,
            )?;
        }
        emit_plugin_window_rect_event(app, &window, "iina.window-size-adjusted");
        Ok(())
    }
}

pub(crate) fn sync_all_player_window_video_sizes<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut session_labels = vec!["main".to_string()];
    session_labels.extend(state.player_session_labels()?);
    let mut first_error = None;
    for session_label in session_labels {
        if app.get_webview_window(&session_label).is_none() {
            continue;
        }
        let result = (|| {
            let session = state.player_session_for_window(&session_label)?;
            let snapshot = session
                .player()
                .lock()
                .map(|player| player.clone())
                .map_err(|error| error.to_string())?;
            sync_player_window_surface(app, state.inner(), &session_label, &snapshot)?;
            sync_player_window_video_size(app, state.inner(), &session_label, &snapshot)
        })();
        if let Err(error) = result {
            first_error.get_or_insert_with(|| format!("{session_label}: {error}"));
        }
    }
    first_error.map_or(Ok(()), Err)
}

pub(crate) fn emit_all_player_mpv_event_batches<R: Runtime>(
    app: &AppHandle<R>,
    emitted_cursors: &mut HashMap<String, u64>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut session_labels = vec!["main".to_string()];
    session_labels.extend(state.player_session_labels()?);
    emitted_cursors.retain(|label, _| session_labels.contains(label));

    let mut first_error = None;
    for session_label in session_labels {
        if app.get_webview_window(&session_label).is_none() {
            continue;
        }
        let cursor = emitted_cursors.get(&session_label).copied().unwrap_or(0);
        let batch = state
            .player_session_for_window(&session_label)?
            .player()
            .lock()
            .map(|player| player.plugin_mpv_events_after(cursor))
            .map_err(|error| error.to_string())?;
        if batch.events.is_empty() {
            continue;
        }
        match app.emit_to(&session_label, PLUGIN_MPV_EVENT_BATCH_EVENT, &batch) {
            Ok(()) => {
                emitted_cursors.insert(session_label, batch.cursor);
            }
            Err(error) => {
                first_error.get_or_insert_with(|| error.to_string());
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

pub(crate) fn emit_plugin_host_event<R: Runtime>(
    app: &AppHandle<R>,
    window_label: &str,
    name: &str,
    args: Value,
) {
    let _ = app.emit_to(
        window_label,
        PLUGIN_HOST_EVENT,
        serde_json::json!({ "name": name, "args": args }),
    );
}

pub(crate) fn emit_plugin_window_rect_event<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
    name: &str,
) {
    let args = crate::window_size::current_player_window_frame(window)
        .map(|frame| serde_json::json!([frame]))
        .unwrap_or_else(|_| serde_json::json!([]));
    emit_plugin_host_event(app, window.label(), name, args);
}

pub(crate) fn open_new_player_window<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    paths: Vec<String>,
    mpv_options: Vec<(String, String)>,
) -> Result<(String, PlayerState), String> {
    let label = if let Some(label) = reusable_idle_player_session_label(app, state)? {
        if app.get_webview_window(&label).is_some() {
            let snapshot = open_media_paths_in_window(app, state, &label, paths, mpv_options)?;
            return Ok((label, snapshot));
        }
        label
    } else {
        state.create_player_session()?.0
    };
    let url = format!("{PLAYER_WINDOW_URL_PREFIX}{label}");
    open_player_window_for_session(app, state, label, url, true, paths, mpv_options)
}

fn player_window_url_is_plugin_managed(url: &Url) -> bool {
    url.query_pairs()
        .any(|(key, value)| key == "plugin-managed" && !value.is_empty())
}

fn player_window_url_disables_ui(url: &Url) -> bool {
    url.query_pairs().any(|(key, value)| {
        key == "plugin-disable-ui" && (value == "1" || value.eq_ignore_ascii_case("true"))
    })
}

pub(crate) fn is_plugin_managed_player_window<R: Runtime>(window: &WebviewWindow<R>) -> bool {
    window
        .url()
        .is_ok_and(|url| player_window_url_is_plugin_managed(&url))
}

fn is_plugin_disable_ui_player_window<R: Runtime>(window: &WebviewWindow<R>) -> bool {
    window
        .url()
        .is_ok_and(|url| player_window_url_disables_ui(&url))
}

fn select_reusable_idle_player_label(
    candidates: impl IntoIterator<Item = (String, bool, bool)>,
) -> Option<String> {
    candidates
        .into_iter()
        .find_map(|(label, idle, managed_by_plugin)| (idle && !managed_by_plugin).then_some(label))
}

fn player_session_creation_index(label: &str) -> Option<u64> {
    label.strip_prefix("player-")?.parse().ok()
}

pub(crate) fn reusable_idle_player_session_label<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
) -> Result<Option<String>, String> {
    let mut labels = vec!["main".to_string()];
    let mut secondary_labels = state.player_session_labels()?;
    secondary_labels.sort_by_key(|label| player_session_creation_index(label).unwrap_or(u64::MAX));
    labels.extend(secondary_labels);
    let mut candidates = Vec::with_capacity(labels.len());
    for label in labels {
        let managed_by_plugin = app
            .get_webview_window(&label)
            .as_ref()
            .is_some_and(is_plugin_managed_player_window);
        let session = state.player_session_for_window(&label)?;
        let idle = session
            .player()
            .lock()
            .map(|player| player.current_url.is_none() && !player.file_loading)
            .map_err(|error| error.to_string())?;
        candidates.push((label, idle, managed_by_plugin));
    }
    // JavascriptAPIGlobal creates managed PlayerCore instances outside
    // PlayerCore.playerCores, so File > New Window must never borrow one.
    Ok(select_reusable_idle_player_label(candidates))
}

fn allocate_new_empty_player_session_label<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
) -> Result<String, String> {
    // PlayerCore.newPlayerCore is explicitly findIdlePlayerCore() ?? createPlayerCore().
    // Cmd+N therefore reuses the first non-loading idle IINA-owned core.
    reusable_idle_player_session_label(app, state)?
        .map(Ok)
        .unwrap_or_else(|| state.create_player_session().map(|(label, _)| label))
}

pub(crate) fn open_new_empty_player_window<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
) -> Result<(String, PlayerState), String> {
    let label = allocate_new_empty_player_session_label(app, state)?;
    if app.get_webview_window(&label).is_some() {
        let snapshot = show_initial_player_window(app, state, &label)?;
        return Ok((label, snapshot));
    }
    let url = format!("{PLAYER_WINDOW_URL_PREFIX}{label}");
    open_player_window_for_session(app, state, label, url, true, Vec::new(), Vec::new())
}

pub(crate) fn open_plugin_managed_player_window<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    label: String,
    identifier: &str,
    instance_id: u64,
    user_label: Option<&str>,
    enable_plugins: bool,
    disable_ui: bool,
    disable_window_animation: bool,
    paths: Vec<String>,
) -> Result<(String, PlayerState), String> {
    let user_label_query = user_label
        .map(percent_encode_query_component)
        .map(|label| format!("&plugin-user-label={label}"))
        .unwrap_or_default();
    let url = format!(
        "{PLAYER_WINDOW_URL_PREFIX}{label}&plugin-managed={identifier}&plugin-instance={instance_id}{user_label_query}&plugin-enable-all={}&plugin-disable-ui={}&plugin-disable-window-animation={}",
        u8::from(enable_plugins),
        u8::from(disable_ui),
        u8::from(disable_window_animation),
    );
    open_player_window_for_session(app, state, label, url, !disable_ui, paths, Vec::new())
}

fn percent_encode_query_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(*byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn open_player_window_for_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    label: String,
    url: String,
    decorations: bool,
    paths: Vec<String>,
    mpv_options: Vec<(String, String)>,
) -> Result<(String, PlayerState), String> {
    let auto_loaded_subtitles = plan_auto_loaded_subtitles(state, &paths)?;
    let plan = plan_open_media_paths(state, paths);
    let plugin_managed = url.contains("plugin-managed=");
    let has_media = !plan.paths.is_empty();
    let presentation_mode = if !plugin_managed && !has_media {
        "initial"
    } else {
        "player"
    };
    let presentation = window_presentation(presentation_mode)?;
    let show_after_setup = !plugin_managed || has_media;
    let builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(url.into()))
        .title("IINA")
        .inner_size(presentation.width, presentation.height)
        .min_inner_size(presentation.min_width, presentation.min_height)
        .resizable(presentation.resizable)
        .visible(false)
        .transparent(true)
        .decorations(decorations);
    #[cfg(target_os = "macos")]
    let mut builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    #[cfg(not(target_os = "macos"))]
    let mut builder = builder;
    if !presentation.resizable {
        builder = builder.max_inner_size(presentation.width, presentation.height);
    }
    let window = match builder.build() {
        Ok(window) => window,
        Err(error) => {
            let _ = state.remove_player_session(&label);
            return Err(error.to_string());
        }
    };

    let result = (|| -> Result<PlayerState, String> {
        #[cfg(target_os = "macos")]
        let (surface_settings, color_settings) = {
            let preferences = state
                .preferences
                .lock()
                .map(|preferences| preferences.clone())
                .map_err(|error| error.to_string())?;
            (
                native_video::surface_settings_from_preferences(&preferences),
                native_video::color_settings_from_preferences(&preferences),
            )
        };
        #[cfg(target_os = "macos")]
        {
            // A freshly built hidden WebView is normally already attached to its NSWindow, but
            // AppKit is allowed to finish that relationship only when the window is presented.
            // Defer exactly that retryable state; invalid hosts and view creation failures remain
            // fatal, and the required post-show install below is never softened.
            let _installed_before_show = native_video::install_if_ready(
                window.ns_view().map_err(|error| error.to_string())?,
                &label,
                &surface_settings,
            )?;
            native_video::configure_color(&color_settings, &label);
            configure_player_window_behavior(state, &window)?;
            native_window_behavior::install_player_input_monitor(
                app,
                window.ns_window().map_err(|error| error.to_string())?,
                &label,
            )?;
        }

        let session = state.player_session_for_window(&label)?;
        if let Some(first_path) = plan.paths.first() {
            let probe = probe_media(first_path);
            let pause_when_open = pause_when_open_preference(state)?;
            {
                let mut player = session.player().lock().map_err(|error| error.to_string())?;
                for (property, value) in mpv_options {
                    player.apply(PlayerCommand::PluginMpvSet { property, value });
                }
                open_media_batch_with_plan(
                    &mut player,
                    plan,
                    auto_loaded_subtitles,
                    probe,
                    pause_when_open,
                );
            }
            reset_hdr_for_player_session(state, &session)?;
        }
        let snapshot = player_snapshot_for_session(&session)?;
        if presentation_mode == "initial" {
            prepare_initial_player_window(state, &label)?;
        } else {
            observe_player_window_surface(state, &label, &snapshot)?;
        }
        apply_window_presentation_mode(&window, presentation_mode)?;
        sync_player_window_surface(app, state, &label, &snapshot)?;
        if has_media {
            apply_open_window_preferences(&window, state, &snapshot, true)?;
        }
        let _ = window.emit("iima-player-state", &snapshot);
        if show_after_setup {
            window.unminimize().map_err(|error| error.to_string())?;
            window.show().map_err(|error| error.to_string())?;
            #[cfg(target_os = "macos")]
            {
                // Reinstalling is idempotent and rebinds the native child window after its Tauri
                // parent is visible. The executor is deliberately still untouched here.
                native_video::install(
                    window.ns_view().map_err(|error| error.to_string())?,
                    &label,
                    &surface_settings,
                )?;
                // The pre-show attempt may have been deferred before a native view existed, in
                // which case its color configuration was intentionally a no-op.
                native_video::configure_color(&color_settings, &label);
            }
            window.set_focus().map_err(|error| error.to_string())?;
        }
        if has_media {
            // Match IINA's MainWindowController ordering: make the window/video host visible,
            // attach the render context, and only then consume loadfile and its ordered appends.
            session.sync_mpv_executor_from_player()?;
            record_recent_media_open(app, state, &session)?;
        }
        if has_media && fullscreen_when_open_preference(state)? {
            set_player_window_fullscreen(app, state, &window, true)?;
        }
        Ok(snapshot)
    })();
    let snapshot = match result {
        Ok(snapshot) => snapshot,
        Err(error) => {
            native_video::remove_session(&label);
            // Bypass the normal retained-player CloseRequested path: this half-built window has
            // no valid PlayerCore surface to retain or reuse.
            let _ = window.destroy();
            let _ = state.remove_player_session(&label);
            return Err(error);
        }
    };
    Ok((label, snapshot))
}

#[tauri::command]
pub fn open_media_in_new_window(
    app: AppHandle,
    state: tauri::State<AppState>,
    path: String,
) -> Result<String, String> {
    open_new_player_window(&app, state.inner(), vec![path], Vec::new()).map(|(label, _)| label)
}

#[tauri::command]
pub fn get_preferences(state: tauri::State<AppState>) -> Result<PreferenceStore, String> {
    current_preferences_with_revision(state.inner()).map(|snapshot| snapshot.preferences)
}

#[tauri::command]
pub fn get_preference_snapshot(
    state: tauri::State<AppState>,
) -> Result<PreferenceSnapshot, String> {
    current_preferences_with_revision(state.inner())
}

fn current_preferences_with_revision(state: &AppState) -> Result<PreferenceSnapshot, String> {
    let (mut preferences, revision) = {
        let preferences = state
            .preferences
            .lock()
            .map_err(|error| error.to_string())?;
        (
            preferences.clone(),
            PREFERENCE_CHANGE_SEQUENCE
                .load(Ordering::Acquire)
                .saturating_sub(1),
        )
    };
    if let Ok(status) = native_updater::status() {
        if status.available {
            preferences.values.insert(
                "updaterAutomaticallyChecks".to_string(),
                serde_json::json!(status.automatically_checks_for_updates),
            );
            preferences.values.insert(
                "updaterCheckInterval".to_string(),
                serde_json::json!(status.update_check_interval),
            );
        }
    }
    Ok(PreferenceSnapshot {
        revision,
        preferences,
    })
}

#[tauri::command]
pub fn get_updater_status() -> Result<UpdaterStatus, String> {
    native_updater::status()
}

#[tauri::command]
pub fn check_for_updates() -> Result<UpdaterStatus, String> {
    native_updater::check_for_updates()?;
    native_updater::status()
}

#[tauri::command]
pub async fn read_http_auth_credentials(
    url: String,
) -> Result<Option<HttpAuthCredentials>, String> {
    let (server, port) = http_auth_key_from_url(&url)?;
    tauri::async_runtime::spawn_blocking(move || native_keychain::read(&server, port))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn write_http_auth_credentials(
    url: String,
    username: String,
    password: String,
) -> Result<(), String> {
    if username.is_empty() {
        return Err("HTTP authentication username cannot be empty".to_string());
    }
    let (server, port) = http_auth_key_from_url(&url)?;
    tauri::async_runtime::spawn_blocking(move || {
        native_keychain::write(&server, port, &username, &password)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn login_opensubtitles_account(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    username: String,
    password: String,
) -> Result<PreferenceStore, String> {
    let username = username.trim().to_string();
    if username.is_empty() {
        return Err("OPEN_SUBTITLES_LOGIN:Username cannot be empty".to_string());
    }
    let login_username = username.clone();
    let login_password = password.clone();
    let rate_limiter = state.opensubtitles_rate_limiter.clone();
    let session = tauri::async_runtime::spawn_blocking(move || {
        online_subtitles::login_opensubtitles(
            &login_username,
            &login_password,
            rate_limiter.as_ref(),
        )
    })
    .await
    .map_err(|error| format!("OPEN_SUBTITLES_LOGIN:{error}"))?
    .map_err(|error| format!("OPEN_SUBTITLES_LOGIN:{error}"))?;
    *state
        .opensubtitles_session
        .lock()
        .map_err(|error| error.to_string())? = Some(session);

    let keychain_username = username.clone();
    tauri::async_runtime::spawn_blocking(move || {
        native_keychain::write_opensubtitles_password(&keychain_username, &password)
    })
    .await
    .map_err(|error| format!("OPEN_SUBTITLES_KEYCHAIN:{error}"))?
    .map_err(|error| format!("OPEN_SUBTITLES_KEYCHAIN:{error}"))?;
    persist_open_sub_username(&app, state.inner(), username)
}

#[tauri::command]
pub fn logout_opensubtitles_account(
    app: AppHandle,
    state: tauri::State<AppState>,
) -> Result<PreferenceStore, String> {
    persist_open_sub_username(&app, state.inner(), String::new())
}

fn persist_open_sub_username(
    app: &AppHandle,
    state: &AppState,
    username: String,
) -> Result<PreferenceStore, String> {
    let preferences = {
        let mut preferences = state
            .preferences
            .lock()
            .map_err(|error| error.to_string())?;
        preferences.set(PreferenceChange {
            key: "openSubUsername".to_string(),
            value: Value::String(username),
        });
        preferences.clone()
    };
    let path = preference_file_path(
        app.path()
            .app_config_dir()
            .map_err(|error| error.to_string())?,
    );
    preferences.save_to_file(&path)?;
    Ok(preferences)
}

fn http_auth_key_from_url(raw: &str) -> Result<(String, Option<u16>), String> {
    let url = Url::parse(raw).map_err(|_| "HTTP authentication URL is invalid".to_string())?;
    let server = url
        .host_str()
        .filter(|host| !host.is_empty())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "HTTP authentication URL has no host".to_string())?;
    Ok((server, url.port()))
}

#[tauri::command]
pub fn set_preference(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    change: PreferenceChange,
) -> Result<PreferenceStore, String> {
    validate_updater_preference(&change)?;
    preference_effects::validate_change(&change)?;
    let updater_preference = (change.key.clone(), change.value.clone());
    let preference_event_key = change.key.clone();
    let preference_event_value = change.value.clone();
    let preference_effect_key = change.key.clone();
    let preference_effect_class = preference_effects::effect_class(&preference_effect_key);
    let disable_plugin_system =
        change.key == "iinaEnablePluginSystem" && change.value.as_bool() == Some(false);
    let clear_recent_documents =
        change.key == "recordRecentFiles" && change.value.as_bool() == Some(false);
    let refresh_mpv_startup = matches!(
        change.key.as_str(),
        "resumeLastPosition" | "currentInputConfigName"
    ) || preference_effect_class.has_startup_effect();
    let refresh_native_menu = change.key == "alwaysOpenInNewWindow"
        || change.key == "enableCmdN"
        || change.key == "recordRecentFiles"
        || matches!(
            change.key.as_str(),
            "savedVideoFilters"
                | "savedAudioFilters"
                | "modeledKeyBindings"
                | "currentInputConfigName"
        );
    let (preferences, preference_event_revision) = {
        let mut preferences = state
            .preferences
            .lock()
            .map_err(|error| error.to_string())?;
        preferences.set(change);
        if clear_recent_documents {
            preferences
                .values
                .insert("recentDocuments".to_string(), serde_json::json!([]));
        }
        (
            preferences.clone(),
            PREFERENCE_CHANGE_SEQUENCE.fetch_add(1, Ordering::AcqRel),
        )
    };
    if clear_recent_documents {
        #[cfg(target_os = "macos")]
        native_recent_documents::clear()?;
        state.inner().clear_recent_documents()?;
    }
    if preference_effect_class.has_native_color_effect() {
        let color_settings = native_video::color_settings_from_preferences(&preferences);
        let session_labels = std::iter::once("main".to_string())
            .chain(state.player_session_labels()?)
            .collect::<Vec<_>>();
        for label in session_labels {
            let session = state.player_session_for_window(&label)?;
            let hdr_enabled = session
                .player()
                .lock()
                .map(|player| player.quick_settings.hdr_enabled)
                .map_err(|error| error.to_string())?;
            native_video::configure_color(&color_settings, session.label());
            native_video::set_hdr_enabled(hdr_enabled, session.label());
        }
    }
    if preference_effect_class.has_application_logging_effect() {
        prepare_advanced_logging_directory(&app, &preferences)?;
    }
    let path = preference_file_path(
        app.path()
            .app_config_dir()
            .map_err(|error| error.to_string())?,
    );
    preferences.save_to_file(&path)?;
    apply_updater_preference(&updater_preference.0, &updater_preference.1)?;
    if refresh_mpv_startup {
        state.inner().refresh_mpv_startup_configuration()?;
    }
    state
        .inner()
        .apply_live_preference_effects(&preference_effect_key, &preferences)?;
    refresh_native_general_preference(&app, state.inner(), &preference_effect_key)?;
    if refresh_native_menu {
        menu::refresh_iina_menu(&app)?;
    }
    if disable_plugin_system {
        plugin_global::stop_all(&app);
        plugin_websocket::stop_all(&app);
    }
    if let Err(error) = crate::native_system_media::sync(&app, state.inner()) {
        eprintln!("iima: unable to apply system media preference change: {error}");
    }
    if let Err(error) = crate::native_touch_bar::sync_all(&app, state.inner()) {
        eprintln!("iima: unable to apply Touch Bar preference change: {error}");
    }
    let event = PreferenceChangedEvent {
        revision: preference_event_revision,
        key: preference_event_key,
        value: preference_event_value,
        preferences: preferences.clone(),
        origin_label: window.label().to_string(),
    };
    if let Err(error) = app.emit(PREFERENCE_CHANGED_EVENT, event) {
        eprintln!("iima: unable to broadcast preference change: {error}");
    }
    Ok(preferences)
}

fn validate_updater_preference(change: &PreferenceChange) -> Result<(), String> {
    match change.key.as_str() {
        "receiveBetaUpdate" | "updaterAutomaticallyChecks" => change
            .value
            .as_bool()
            .map(|_| ())
            .ok_or_else(|| format!("{} must be a boolean", change.key)),
        "updaterCheckInterval" => change
            .value
            .as_f64()
            .ok_or_else(|| "updaterCheckInterval must be numeric".to_string())
            .and_then(native_updater::validated_update_interval)
            .map(|_| ()),
        _ => Ok(()),
    }
}

fn apply_updater_preference(key: &str, value: &Value) -> Result<(), String> {
    match key {
        "receiveBetaUpdate" => {
            native_updater::set_receive_beta(value.as_bool().unwrap_or(false))?;
            Ok(())
        }
        "updaterAutomaticallyChecks" => {
            if native_updater::status()?.available {
                native_updater::set_automatic_checks(value.as_bool().unwrap_or(false))?;
            }
            Ok(())
        }
        "updaterCheckInterval" => {
            let interval =
                native_updater::validated_update_interval(value.as_f64().unwrap_or(86400.0))?;
            if native_updater::status()?.available {
                native_updater::set_check_interval(interval)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(crate) fn prepare_advanced_logging_directory<R: Runtime>(
    app: &AppHandle<R>,
    preferences: &PreferenceStore,
) -> Result<(), String> {
    let home = app
        .path()
        .home_dir()
        .map_err(|error| format!("Unable to resolve the log directory: {error}"))?;
    let enabled = bool_preference(&preferences.values, "enableAdvancedSettings", false)
        && bool_preference(&preferences.values, "enableLogging", false);
    let preferred_level = preferences
        .values
        .get("logLevel")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    app_logging::initialize(&home, enabled, preferred_level)?;
    app_logging::log("iina", 1, "IINA 1.3.5 Tauri runtime initialized");
    Ok(())
}

#[tauri::command]
pub fn choose_advanced_config_directory(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let selected = app
        .dialog()
        .file()
        .set_title("Choose config directory")
        .set_can_create_directories(false)
        .blocking_pick_folder();
    selected
        .map(|path| {
            path.into_path()
                .map(|path| path.display().to_string())
                .map_err(|error| error.to_string())
        })
        .transpose()
}

#[tauri::command]
pub fn open_log_directory(app: AppHandle) -> Result<String, String> {
    auxiliary_windows::open_log_directory(&app)
}

#[tauri::command]
pub fn show_log_viewer(app: AppHandle) -> Result<String, String> {
    auxiliary_windows::show_log_viewer_window(&app)
}

#[tauri::command]
pub fn open_advanced_help() -> Result<String, String> {
    Command::new("/usr/bin/open")
        .arg(preference_effects::ADVANCED_HELP_URL)
        .spawn()
        .map_err(|error| format!("Unable to open Advanced preferences help: {error}"))?;
    Ok(preference_effects::ADVANCED_HELP_URL.to_string())
}

#[tauri::command]
pub fn choose_screenshot_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let selected = app
        .dialog()
        .file()
        .set_title("Choose screenshot save path")
        .set_can_create_directories(true)
        .blocking_pick_folder();
    selected
        .map(|path| {
            path.into_path()
                .map(|path| path.display().to_string())
                .map_err(|error| error.to_string())
        })
        .transpose()
}

#[tauri::command]
pub fn export_key_bindings_config(
    app: tauri::AppHandle,
    filename: String,
    contents: String,
) -> Result<Option<String>, String> {
    let selected = app
        .dialog()
        .file()
        .set_title("Export Key Bindings")
        .set_file_name(sanitized_config_filename(&filename))
        .add_filter("Input Config", &["conf"])
        .blocking_save_file();

    selected
        .map(|path| {
            path.into_path()
                .map_err(|error| error.to_string())
                .and_then(|path| {
                    fs::write(&path, contents)
                        .map_err(|error| format!("Failed to export key bindings: {error}"))?;
                    Ok(path.display().to_string())
                })
        })
        .transpose()
}

fn key_binding_repository(app: &AppHandle) -> Result<KeyBindingRepository, String> {
    app.path()
        .app_config_dir()
        .map(KeyBindingRepository::new)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_key_binding_profiles(app: AppHandle) -> Result<Vec<KeyBindingProfile>, String> {
    key_binding_repository(&app)?
        .list_profiles()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn read_key_binding_profile(
    app: AppHandle,
    name: String,
) -> Result<KeyBindingProfileDocument, String> {
    key_binding_repository(&app)?
        .read_profile(&name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_key_binding_profile(
    app: AppHandle,
    name: String,
) -> Result<KeyBindingProfileDocument, String> {
    key_binding_repository(&app)?
        .create_empty_profile(&name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn duplicate_key_binding_profile(
    app: AppHandle,
    source_name: String,
    new_name: String,
) -> Result<KeyBindingProfileDocument, String> {
    key_binding_repository(&app)?
        .duplicate_profile(&source_name, &new_name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn import_key_binding_profile(
    app: AppHandle,
    source_path: String,
    name: Option<String>,
) -> Result<KeyBindingProfileDocument, String> {
    key_binding_repository(&app)?
        .import_profile(source_path, name.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_key_binding_profile(
    app: AppHandle,
    name: String,
    contents: String,
) -> Result<KeyBindingProfileDocument, String> {
    key_binding_repository(&app)?
        .save_profile(&name, &contents)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_key_binding_profile(app: AppHandle, name: String) -> Result<(), String> {
    key_binding_repository(&app)?
        .delete_profile(&name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_key_binding_profile_path(app: AppHandle, name: String) -> Result<String, String> {
    key_binding_repository(&app)?
        .reveal_path(&name)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn reveal_key_binding_profile(app: AppHandle, name: String) -> Result<String, String> {
    let path = key_binding_repository(&app)?
        .reveal_path(&name)
        .map_err(|error| error.to_string())?;
    Command::new("/usr/bin/open")
        .arg("-R")
        .arg(&path)
        .spawn()
        .map_err(|error| format!("Failed to reveal key binding profile: {error}"))?;
    Ok(path.to_string_lossy().into_owned())
}

fn sanitized_config_filename(filename: &str) -> String {
    let mut sanitized = filename
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        sanitized = "input.conf".into();
    }
    if !sanitized.ends_with(".conf") {
        sanitized.push_str(".conf");
    }
    sanitized
}

#[tauri::command]
pub fn open_media(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    path: String,
) -> Result<PlayerState, String> {
    let target = state
        .inner()
        .shortcut_player_session_label(window.label())?;
    open_media_paths_in_window(&app, state.inner(), &target, vec![path], Vec::new())
}

#[tauri::command]
pub fn submit_open_url(
    app: AppHandle,
    state: tauri::State<AppState>,
    url: String,
    is_alternative_action: bool,
    enqueue: bool,
) -> Result<OpenUrlSubmissionResult, String> {
    if url.trim().is_empty() {
        return Err("Open URL submission is empty".to_string());
    }
    let (target_session_label, open_in_new_window) =
        open_url_submission_route(state.inner(), is_alternative_action, enqueue)?;
    if enqueue {
        let player =
            enqueue_media_paths_in_session(&app, state.inner(), &target_session_label, vec![url])?;
        return Ok(OpenUrlSubmissionResult {
            target_session_label,
            opened_window_label: None,
            player: Some(player),
            enqueued: true,
        });
    }
    if open_in_new_window {
        let (opened_window_label, _) =
            open_new_player_window(&app, state.inner(), vec![url], Vec::new())?;
        return Ok(OpenUrlSubmissionResult {
            target_session_label,
            opened_window_label: Some(opened_window_label),
            player: None,
            enqueued: false,
        });
    }
    let player = open_media_paths_in_window(
        &app,
        state.inner(),
        &target_session_label,
        vec![url],
        Vec::new(),
    )?;
    Ok(OpenUrlSubmissionResult {
        target_session_label,
        opened_window_label: None,
        player: Some(player),
        enqueued: false,
    })
}

pub(crate) fn open_service_url_in_active_player<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    native_main_window: *mut std::ffi::c_void,
    url: String,
) -> Result<Option<PlayerState>, String> {
    #[cfg(target_os = "macos")]
    let active_player_session_label =
        service_active_player_session_label(app, state, native_main_window)?;
    #[cfg(not(target_os = "macos"))]
    let active_player_session_label: Option<String> = {
        let _ = native_main_window;
        None
    };
    let Some((target_session_label, url)) =
        service_open_url_route(state, active_player_session_label.as_deref(), url)?
    else {
        return Ok(None);
    };
    open_media_paths_in_window(app, state, &target_session_label, vec![url], Vec::new()).map(Some)
}

#[tauri::command]
pub async fn open_media_dialog(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    window: WebviewWindow,
) -> Result<Option<PlayerState>, String> {
    let title = localization::menu_title("Choose Media Files");
    let selected =
        tauri::async_runtime::spawn_blocking(move || native_open_panel::choose_media_paths(&title))
            .await
            .map_err(|error| error.to_string())??;
    let Some(paths) = selected else {
        return Ok(None);
    };
    record_recent_open_panel_selection(&app, state.inner(), &paths)?;

    if should_open_in_new_player_for_menu_action(state.inner(), false)? {
        open_new_player_window(&app, state.inner(), paths, Vec::new())?;
        Ok(None)
    } else {
        open_media_paths_in_window(&app, state.inner(), window.label(), paths, Vec::new()).map(Some)
    }
}

#[tauri::command]
pub async fn open_media_dialog_new_window(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    window: WebviewWindow,
) -> Result<Option<String>, String> {
    let title = localization::menu_title("Choose Media Files");
    let selected =
        tauri::async_runtime::spawn_blocking(move || native_open_panel::choose_media_paths(&title))
            .await
            .map_err(|error| error.to_string())??;
    let Some(paths) = selected else {
        return Ok(None);
    };
    record_recent_open_panel_selection(&app, state.inner(), &paths)?;
    if should_open_in_new_player_for_menu_action(state.inner(), true)? {
        open_new_player_window(&app, state.inner(), paths, Vec::new()).map(|(label, _)| Some(label))
    } else {
        let label = player_session_label_for_window(window.label()).to_string();
        open_media_paths_in_window(&app, state.inner(), window.label(), paths, Vec::new())?;
        Ok(Some(label))
    }
}

#[tauri::command]
pub async fn enqueue_media_dialog(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    window: WebviewWindow,
) -> Result<Option<PlayerState>, String> {
    let title = localization::menu_title("Add to playlist");
    let selected =
        tauri::async_runtime::spawn_blocking(move || native_open_panel::choose_media_paths(&title))
            .await
            .map_err(|error| error.to_string())??;
    let Some(paths) = selected else {
        return Ok(None);
    };
    insert_playlist_paths_in_window(&app, state.inner(), window.label(), paths, usize::MAX)
        .map(Some)
}

#[tauri::command]
pub async fn load_external_track_dialog(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    window: WebviewWindow,
    kind: ExternalTrackKind,
) -> Result<Option<PlayerState>, String> {
    const AUDIO_EXTENSIONS: &[&str] = &[
        "mp3", "aac", "mka", "dts", "flac", "ogg", "oga", "mogg", "m4a", "ac3", "opus", "wav",
        "wv", "aiff", "aif", "ape", "tta", "tak",
    ];
    const SUBTITLE_EXTENSIONS: &[&str] = &[
        "utf", "utf8", "utf-8", "idx", "sub", "srt", "smi", "rt", "ssa", "aqt", "jss", "js", "ass",
        "mks", "vtt", "sup", "scc",
    ];
    const VIDEO_EXTENSIONS: &[&str] = &[
        "mkv", "mp4", "m4v", "avi", "mov", "webm", "flv", "wmv", "mpeg", "mpg", "ts", "m2ts",
    ];

    let selected = match kind {
        ExternalTrackKind::Video => app
            .dialog()
            .file()
            .set_title("Load external video file")
            .set_can_create_directories(false)
            .add_filter("Video", VIDEO_EXTENSIONS)
            .blocking_pick_file(),
        ExternalTrackKind::Audio => app
            .dialog()
            .file()
            .set_title("Load external audio file")
            .set_can_create_directories(false)
            .add_filter("Audio", AUDIO_EXTENSIONS)
            .blocking_pick_file(),
        ExternalTrackKind::Subtitles => app
            .dialog()
            .file()
            .set_title("Load external subtitle file")
            .set_can_create_directories(false)
            .add_filter("Subtitles", SUBTITLE_EXTENSIONS)
            .blocking_pick_file(),
    };
    let Some(path) = selected else {
        return Ok(None);
    };
    let path = path
        .into_path()
        .map(|path| path.display().to_string())
        .map_err(|error| error.to_string())?;

    player_command(
        state,
        window,
        PlayerCommand::LoadExternalTrack { kind, path },
    )
    .map(Some)
}

#[tauri::command]
pub fn choose_subtitle_font_dialog(
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<Option<PlayerState>, String> {
    let Some(font) = native_font_picker::choose_font()? else {
        return Ok(None);
    };
    player_command(state, window, PlayerCommand::SetSubtitleFont { font }).map(Some)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OnlineSubtitleDownloadResult {
    pub player: PlayerState,
    pub downloaded_paths: Vec<String>,
}

#[tauri::command]
pub fn search_online_subtitles(
    state: tauri::State<AppState>,
    window: WebviewWindow,
    provider_id: Option<String>,
) -> Result<OnlineSubtitleSearchResult, String> {
    let session = state.inner().player_session_for_window(window.label())?;
    let (current_url, media_title) = session
        .player()
        .lock()
        .map_err(|error| error.to_string())
        .and_then(|player| {
            player
                .current_url
                .clone()
                .map(|url| (url, player.media_title.clone()))
                .ok_or_else(|| "No media is open".to_string())
        })?;
    let preferences = state
        .preferences
        .lock()
        .map(|preferences| preferences.clone())
        .map_err(|error| error.to_string())?;
    let preferences =
        online_subtitle_preferences_for_request(&preferences, provider_id.as_deref())?;
    let opensubtitles_session = if online_subtitles::uses_opensubtitles_provider(&preferences) {
        online_subtitles::opensubtitles_session_for_preferences(
            &preferences,
            &state.opensubtitles_session,
            state.opensubtitles_rate_limiter.as_ref(),
        )?
    } else {
        None
    };
    let mut store = session
        .online_subtitles()
        .lock()
        .map_err(|error| error.to_string())?;
    let result = online_subtitles::search_with_opensubtitles_session(
        &current_url,
        &media_title,
        &preferences,
        &mut store,
        opensubtitles_session.as_ref(),
        state.opensubtitles_rate_limiter.as_ref(),
    );
    abandon_rejected_opensubtitles_session(state.inner(), &result)?;
    result
}

fn online_subtitle_preferences_for_request(
    preferences: &PreferenceStore,
    provider_id: Option<&str>,
) -> Result<PreferenceStore, String> {
    let Some(provider_id) = provider_id else {
        return Ok(preferences.clone());
    };
    if !matches!(provider_id, ":opensubtitles" | ":assrt" | ":shooter") {
        return Err(format!(
            "Online subtitle provider override {provider_id} is not a built-in provider"
        ));
    }
    let mut request_preferences = preferences.clone();
    request_preferences.values.insert(
        "onlineSubProvider".to_string(),
        Value::String(provider_id.to_string()),
    );
    Ok(request_preferences)
}

#[tauri::command]
pub fn download_online_subtitles(
    state: tauri::State<AppState>,
    window: WebviewWindow,
    candidates: Vec<String>,
) -> Result<OnlineSubtitleDownloadResult, String> {
    let session = state.inner().player_session_for_window(window.label())?;
    let selected = session
        .online_subtitles()
        .lock()
        .map_err(|error| error.to_string())?
        .selected(&candidates)?;
    let result = online_subtitles::download(&selected, state.opensubtitles_rate_limiter.as_ref());
    abandon_rejected_opensubtitles_session(state.inner(), &result)?;
    let downloaded_paths = result?;
    attach_downloaded_subtitles(&session, downloaded_paths)
}

fn abandon_rejected_opensubtitles_session<T>(
    state: &AppState,
    result: &Result<T, String>,
) -> Result<(), String> {
    if result
        .as_ref()
        .is_err_and(|error| online_subtitles::is_opensubtitles_invalid_token_error(error))
    {
        *state
            .opensubtitles_session
            .lock()
            .map_err(|error| error.to_string())? = None;
    }
    Ok(())
}

#[tauri::command]
pub fn download_plugin_subtitles(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    urls: Vec<String>,
) -> Result<OnlineSubtitleDownloadResult, String> {
    ensure_plugin_runtime_is_enabled(&app, state.inner(), &identifier)?;
    plugins::validate_plugin_network_urls(&app, &identifier, &urls)?;
    let downloaded_paths = online_subtitles::download_plugin_urls(&identifier, &urls)?;
    let session = state.inner().player_session_for_window(window.label())?;
    attach_downloaded_subtitles(&session, downloaded_paths)
}

fn attach_downloaded_subtitles(
    session: &PlayerSessionRef<'_>,
    downloaded_paths: Vec<String>,
) -> Result<OnlineSubtitleDownloadResult, String> {
    if downloaded_paths.is_empty() {
        return Err("The subtitle provider did not return downloadable files".to_string());
    }
    session
        .online_subtitles()
        .lock()
        .map_err(|error| error.to_string())?
        .record_downloads(&downloaded_paths);
    {
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        for path in &downloaded_paths {
            player.apply(PlayerCommand::LoadExternalTrack {
                kind: ExternalTrackKind::Subtitles,
                path: path.clone(),
            });
        }
        player.send_osd(format!(
            "Downloaded {} subtitle file{}",
            downloaded_paths.len(),
            if downloaded_paths.len() == 1 { "" } else { "s" }
        ));
    }
    session.sync_mpv_executor_from_player()?;
    Ok(OnlineSubtitleDownloadResult {
        player: player_snapshot_for_session(session)?,
        downloaded_paths,
    })
}

#[tauri::command]
pub fn save_downloaded_subtitle_dialog(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<Option<String>, String> {
    let session = state.inner().player_session_for_window(window.label())?;
    let selected_track_path = session
        .player()
        .lock()
        .map_err(|error| error.to_string())?
        .tracks
        .subtitles
        .iter()
        .find(|track| track.selected)
        .and_then(|track| track.metadata.external_filename.clone());
    let source = selected_track_path
        .filter(|path| online_subtitles::is_downloaded_subtitle_path(std::path::Path::new(path)))
        .or_else(|| {
            session
                .online_subtitles()
                .lock()
                .ok()
                .and_then(|store| store.latest_download())
        })
        .ok_or_else(|| "No downloaded subtitle is selected".to_string())?;
    let source_path = std::path::PathBuf::from(&source);
    let file_name = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("subtitle.srt");
    let destination = app
        .dialog()
        .file()
        .set_title(localization::menu_title("Save Downloaded Subtitle"))
        .set_file_name(file_name)
        .blocking_save_file();
    let Some(destination) = destination else {
        return Ok(None);
    };
    let destination = destination.into_path().map_err(|error| error.to_string())?;
    fs::copy(&source_path, &destination)
        .map_err(|error| format!("Unable to save downloaded subtitle: {error}"))?;
    Ok(Some(destination.display().to_string()))
}

#[tauri::command]
pub fn get_plugins(app: AppHandle) -> Result<Vec<PluginRecord>, String> {
    plugins::list(&app)
}

#[tauri::command]
pub fn get_plugin_runtime_specs(
    app: AppHandle,
    state: tauri::State<AppState>,
) -> Result<Vec<PluginRuntimeSpec>, String> {
    let plugin_system_enabled = state
        .preferences
        .lock()
        .map_err(|error| error.to_string())?
        .values
        .get("iinaEnablePluginSystem")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if !plugin_system_enabled {
        return Ok(Vec::new());
    }
    plugins::runtime_specs(&app)
}

#[tauri::command]
pub fn get_plugin_page_contents(
    app: AppHandle,
    identifier: String,
) -> Result<PluginPageContents, String> {
    plugins::page_contents(&app, &identifier)
}

#[tauri::command]
pub fn install_plugin_dialog(app: AppHandle) -> Result<Option<PluginInstallResult>, String> {
    plugins::install_from_dialog(&app)
}

#[tauri::command]
pub fn install_plugin_from_github(
    app: AppHandle,
    source: String,
) -> Result<PluginInstallResult, String> {
    plugins::install_from_github(&app, &source)
}

#[tauri::command]
pub fn confirm_plugin_reinstall(app: AppHandle, token: String) -> Result<PluginRecord, String> {
    plugins::confirm_reinstall(&app, &token)
}

#[tauri::command]
pub fn cancel_plugin_reinstall(app: AppHandle, token: String) -> Result<bool, String> {
    plugins::cancel_reinstall(&app, &token)
}

#[tauri::command]
pub fn confirm_plugin_permissions(
    app: AppHandle,
    state: tauri::State<AppState>,
    token: String,
) -> Result<PluginInstallResult, String> {
    let result = plugins::confirm_permissions(&app, &token)?;
    if let PluginInstallResult::Installed { record } = &result {
        state
            .plugin_menus
            .lock()
            .map_err(|error| error.to_string())?
            .retain(|menu| menu.identifier != record.identifier);
        menu::refresh_iina_menu(&app)?;
    }
    Ok(result)
}

#[tauri::command]
pub fn cancel_plugin_permissions(app: AppHandle, token: String) -> Result<bool, String> {
    plugins::cancel_permissions(&app, &token)
}

#[tauri::command]
pub fn claim_pending_plugin_install(
    window: WebviewWindow,
) -> Result<Option<PluginInstallNotification>, String> {
    if window.label() != crate::auxiliary_player_windows::PREFERENCES_WINDOW_LABEL {
        return Err("Plugin install notifications belong to the Preferences window".to_string());
    }
    plugins::claim_install_notification()
}

#[tauri::command]
pub fn has_pending_plugin_installs(window: WebviewWindow) -> Result<bool, String> {
    if window.label() != "main" {
        return Err("Only the main window can inspect pending plugin notifications".to_string());
    }
    plugins::has_pending_install_notification()
}

#[tauri::command]
pub fn check_plugin_github_update(
    app: AppHandle,
    identifier: String,
) -> Result<Option<PluginGithubUpdate>, String> {
    plugins::check_for_github_update(&app, &identifier)
}

#[tauri::command]
pub fn update_plugin_from_github(
    app: AppHandle,
    state: tauri::State<AppState>,
    identifier: String,
) -> Result<PluginInstallResult, String> {
    let result = plugins::update_from_github(&app, &identifier)?;
    if matches!(result, PluginInstallResult::Installed { .. }) {
        state
            .plugin_menus
            .lock()
            .map_err(|error| error.to_string())?
            .retain(|menu| menu.identifier != identifier);
        menu::refresh_iina_menu(&app)?;
    }
    Ok(result)
}

#[tauri::command]
pub fn set_plugin_enabled(
    app: AppHandle,
    state: tauri::State<AppState>,
    identifier: String,
    enabled: bool,
) -> Result<Vec<PluginRecord>, String> {
    let records = plugins::set_enabled(&app, &identifier, enabled)?;
    if !enabled {
        crate::plugin_sync::cleanup_identifier(&identifier);
        plugin_global::stop_identifier(&app, &identifier);
        crate::plugin_mpv_hooks::stop_identifier(state.inner(), &identifier);
        plugin_websocket::stop_identifier(&app, &identifier);
        state
            .plugin_menus
            .lock()
            .map_err(|error| error.to_string())?
            .retain(|menu| menu.identifier != identifier);
        menu::refresh_iina_menu(&app)?;
    }
    Ok(records)
}

#[tauri::command]
pub fn reorder_plugin(
    app: AppHandle,
    identifier: String,
    destination_index: usize,
) -> Result<Vec<PluginRecord>, String> {
    plugins::reorder(&app, &identifier, destination_index)
}

#[tauri::command]
pub fn reveal_plugin_in_finder(app: AppHandle, identifier: String) -> Result<(), String> {
    let root = plugins::installed_root(&app, &identifier)?;
    native_file::reveal(&[root])
}

#[tauri::command]
pub fn remove_plugin(
    app: AppHandle,
    state: tauri::State<AppState>,
    identifier: String,
) -> Result<Vec<PluginRecord>, String> {
    let records = plugins::remove(&app, &identifier)?;
    crate::plugin_sync::cleanup_identifier(&identifier);
    plugin_global::stop_identifier(&app, &identifier);
    crate::plugin_mpv_hooks::stop_identifier(state.inner(), &identifier);
    plugin_websocket::stop_identifier(&app, &identifier);
    state
        .plugin_menus
        .lock()
        .map_err(|error| error.to_string())?
        .retain(|menu| menu.identifier != identifier);
    menu::refresh_iina_menu(&app)?;
    Ok(records)
}

#[tauri::command]
pub fn set_plugin_menu_items(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    role: String,
    items: Vec<PluginMenuItemDefinition>,
) -> Result<(), String> {
    if !matches!(role.as_str(), "entry" | "global") {
        return Err("Plugin menu role must be entry or global".to_string());
    }
    if window.label() != "main" && !window.label().starts_with("player-") {
        return Err("Plugin menus belong to a player runtime".to_string());
    }
    if !plugins::plugin_is_enabled(&app, &identifier)? {
        return Err("Plugin is not enabled".to_string());
    }
    plugins::validate_menu_items(&items)?;
    let (order_index, spec) = plugins::runtime_specs(&app)?
        .into_iter()
        .enumerate()
        .find(|(_, spec)| spec.identifier == identifier)
        .ok_or_else(|| "Plugin runtime is unavailable".to_string())?;
    if role == "global" && (window.label() != "main" || spec.global_entry.is_none()) {
        return Err("Global plugin menus belong to the primary global runtime".to_string());
    }
    let owner_label = if role == "global" {
        "main".to_string()
    } else {
        state
            .player_session_for_window(window.label())?
            .label()
            .to_string()
    };
    let mut plugin_menus = state
        .plugin_menus
        .lock()
        .map_err(|error| error.to_string())?;
    plugin_menus.retain(|menu| {
        menu.identifier != identifier || menu.owner_label != owner_label || menu.role != role
    });
    plugin_menus.push(PluginMenuDefinition {
        order_index,
        owner_label,
        role,
        identifier,
        name: spec.name,
        has_global_instance: spec.global_entry.is_some(),
        items,
    });
    drop(plugin_menus);
    menu::refresh_iina_menu(&app)
}

#[tauri::command]
pub fn plugin_mpv_command(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    command: String,
    args: Vec<String>,
) -> Result<PlayerState, String> {
    ensure_plugin_runtime_is_enabled(&app, state.inner(), &identifier)?;
    let session = state.inner().player_session_for_window(window.label())?;
    {
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        player.apply(PlayerCommand::PluginMpvCommand { command, args });
    }
    session.sync_mpv_executor_from_player()?;
    player_snapshot_for_session(&session)
}

#[tauri::command]
pub fn plugin_mpv_set(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    property: String,
    value: String,
) -> Result<PlayerState, String> {
    ensure_plugin_runtime_is_enabled(&app, state.inner(), &identifier)?;
    let session = state.inner().player_session_for_window(window.label())?;
    {
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        player.apply(PlayerCommand::PluginMpvSet { property, value });
    }
    session.sync_mpv_executor_from_player()?;
    player_snapshot_for_session(&session)
}

pub(crate) fn plugin_mpv_get_sync(
    app: &AppHandle,
    state: &AppState,
    window: &WebviewWindow,
    identifier: &str,
    property: &str,
    kind: MpvPluginGetKind,
) -> Result<MpvPluginValue, String> {
    ensure_plugin_runtime_is_enabled(app, state, identifier)?;
    let session = state.player_session_for_window(window.label())?;
    let value = session
        .mpv_executor()
        .lock()
        .map_err(|error| error.to_string())?
        .plugin_property(property, kind)?;
    Ok(value)
}

pub(crate) fn plugin_mpv_set_sync(
    app: &AppHandle,
    state: &AppState,
    window: &WebviewWindow,
    identifier: &str,
    property: String,
    value: MpvPluginValue,
) -> Result<(), String> {
    ensure_plugin_runtime_is_enabled(app, state, identifier)?;
    let session = state.player_session_for_window(window.label())?;
    {
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        player.apply(PlayerCommand::PluginMpvSetNative { property, value });
    }
    session.sync_mpv_executor_from_player()?;
    Ok(())
}

pub(crate) fn plugin_mpv_command_sync(
    app: &AppHandle,
    state: &AppState,
    window: &WebviewWindow,
    identifier: &str,
    command: String,
    args: Vec<String>,
) -> Result<(), String> {
    ensure_plugin_runtime_is_enabled(app, state, identifier)?;
    let session = state.player_session_for_window(window.label())?;
    {
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        player.apply(PlayerCommand::PluginMpvCommand { command, args });
    }
    session.sync_mpv_executor_from_player()?;
    Ok(())
}

#[tauri::command]
pub fn plugin_mpv_observe_property(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    property: String,
) -> Result<bool, String> {
    ensure_plugin_runtime_is_enabled(&app, state.inner(), &identifier)?;
    let session = state.inner().player_session_for_window(window.label())?;
    let result = session
        .mpv_executor()
        .lock()
        .map_err(|error| error.to_string())?
        .observe_plugin_property(&property);
    result
}

fn ensure_plugin_runtime_is_enabled(
    app: &AppHandle,
    state: &AppState,
    identifier: &str,
) -> Result<(), String> {
    let plugin_system_enabled = state
        .preferences
        .lock()
        .map_err(|error| error.to_string())?
        .values
        .get("iinaEnablePluginSystem")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if !plugin_system_enabled || !plugins::plugin_is_enabled(app, identifier)? {
        return Err("Plugin is not enabled".to_string());
    }
    Ok(())
}

fn plugin_keychain_service(identifier: &str, service: &str) -> Result<String, String> {
    if service.is_empty() {
        return Err("Plugin Keychain service cannot be empty".to_string());
    }
    if service.len() > 256 || identifier.len() > 256 || service.contains('\0') {
        return Err("Plugin Keychain service is invalid".to_string());
    }
    Ok(format!("{identifier} - {service}"))
}

#[tauri::command]
pub async fn plugin_keychain_read(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    identifier: String,
    service: String,
    name: String,
) -> Result<Option<String>, String> {
    ensure_plugin_runtime_is_enabled(&app, state.inner(), &identifier)?;
    if name.len() > 1024 || name.contains('\0') {
        return Err("Plugin Keychain account is invalid".to_string());
    }
    let service = plugin_keychain_service(&identifier, &service)?;
    tauri::async_runtime::spawn_blocking(move || {
        native_keychain::read_generic_password(&service, &name)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn plugin_keychain_write(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    identifier: String,
    service: String,
    name: String,
    password: String,
) -> Result<(), String> {
    ensure_plugin_runtime_is_enabled(&app, state.inner(), &identifier)?;
    if name.len() > 1024
        || name.contains('\0')
        || password.len() > 64 * 1024
        || password.contains('\0')
    {
        return Err("Plugin Keychain account or password is invalid".to_string());
    }
    let service = plugin_keychain_service(&identifier, &service)?;
    tauri::async_runtime::spawn_blocking(move || {
        native_keychain::write_generic_password(&service, &name, &password)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginHttpResponse {
    pub status_code: Option<u16>,
    pub reason: String,
    pub data: Option<Value>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginHttpDownloadResult {
    pub destination: Option<String>,
    pub response: PluginHttpResponse,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginFileEntry {
    pub filename: String,
    pub path: String,
    pub is_dir: bool,
}

#[tauri::command]
pub fn plugin_http_request(
    app: AppHandle,
    state: tauri::State<AppState>,
    identifier: String,
    method: String,
    url: String,
    options: Option<Value>,
    permission_required: Option<bool>,
) -> Result<PluginHttpResponse, String> {
    ensure_plugin_runtime_is_enabled(&app, state.inner(), &identifier)?;
    if permission_required.unwrap_or(true) {
        plugins::validate_plugin_network_urls(&app, &identifier, std::slice::from_ref(&url))?;
    } else {
        plugins::validate_plugin_network_urls_without_permission(
            &app,
            &identifier,
            std::slice::from_ref(&url),
        )?;
    }
    let method = method.trim().to_ascii_uppercase();
    if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
        return Err("Plugin HTTP method is not supported".to_string());
    }
    let options = options.unwrap_or(Value::Null);
    let headers = plugin_http_string_map(&options, "headers")?;
    let params = plugin_http_string_map(&options, "params")?;
    let data = options.get("data").filter(|value| !value.is_null());
    let mut command = Command::new("/usr/bin/curl");
    command.args([
        "--silent",
        "--show-error",
        "--location",
        "--max-time",
        "30",
        "--request",
        method.as_str(),
        "--write-out",
        "\n%{http_code}",
    ]);
    if method == "GET" {
        command.arg("--get");
    }
    for (name, value) in headers {
        if name.trim().is_empty()
            || name.contains(['\r', '\n'])
            || value.contains(['\r', '\n'])
            || name.len() + value.len() > 4096
        {
            return Err("Plugin HTTP header is invalid".to_string());
        }
        command.arg("--header").arg(format!("{name}: {value}"));
    }
    for (name, value) in params {
        command
            .arg("--data-urlencode")
            .arg(format!("{name}={value}"));
    }
    if let Some(data) = data {
        let body = match data {
            Value::String(value) => value.clone(),
            value => serde_json::to_string(value)
                .map_err(|error| format!("Plugin HTTP data cannot be serialized: {error}"))?,
        };
        if body.len() > 1024 * 1024 {
            return Err("Plugin HTTP request body exceeds 1 MiB".to_string());
        }
        command.arg("--data").arg(body);
    }
    command.arg(&url);
    let output = command
        .output()
        .map_err(|error| format!("Unable to start curl for plugin: {error}"))?;
    let (body, status_code) = split_plugin_http_write_out(&output.stdout);
    if body.len() > 2 * 1024 * 1024 {
        return Err("Plugin HTTP response exceeds 2 MiB".to_string());
    }
    Ok(plugin_http_response(
        &body,
        status_code,
        String::from_utf8_lossy(&output.stderr).trim(),
    ))
}

#[tauri::command]
pub fn plugin_http_download(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    url: String,
    destination: String,
    options: Option<Value>,
) -> Result<PluginHttpDownloadResult, String> {
    ensure_plugin_runtime_is_enabled(&app, state.inner(), &identifier)?;
    let session = state.inner().player_session_for_window(window.label())?;
    plugins::validate_plugin_network_urls(&app, &identifier, std::slice::from_ref(&url))?;
    let options = options.unwrap_or(Value::Null);
    let method = plugin_http_method(&options)?;
    let headers = plugin_http_string_map(&options, "headers")?;
    let params = plugin_http_string_map(&options, "params")?;
    let body = plugin_http_body(&options)?;
    let current_media = session
        .player()
        .lock()
        .map_err(|error| error.to_string())?
        .current_url
        .clone();
    let destination = plugins::resolve_plugin_file_path(
        &app,
        &identifier,
        &destination,
        current_media.as_deref(),
    )?
    .path;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| "Plugin download destination has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Unable to create plugin download directory: {error}"))?;
    let temporary = plugin_download_temporary_path(parent, &destination)?;

    let mut command = Command::new("/usr/bin/curl");
    command.args([
        "--silent",
        "--show-error",
        "--location",
        "--max-time",
        "60",
        "--request",
        method.as_str(),
        "--write-out",
        "%{http_code}",
        "--output",
    ]);
    command.arg(&temporary);
    if method == "GET" {
        command.arg("--get");
    }
    for (name, value) in headers {
        validate_plugin_http_header(&name, &value)?;
        command.arg("--header").arg(format!("{name}: {value}"));
    }
    for (name, value) in params {
        command
            .arg("--data-urlencode")
            .arg(format!("{name}={value}"));
    }
    if let Some(body) = body {
        command.arg("--data").arg(body);
    }
    command.arg(&url);
    let output = command
        .output()
        .map_err(|error| format!("Unable to start curl for plugin: {error}"))?;
    let status_code = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|status| *status != 0);
    let ok = plugin_http_response_is_ok(status_code);
    let body = if ok {
        Vec::new()
    } else {
        fs::metadata(&temporary)
            .ok()
            .filter(|metadata| metadata.len() <= 2 * 1024 * 1024)
            .and_then(|_| fs::read(&temporary).ok())
            .unwrap_or_default()
    };
    let response = plugin_http_response(
        &body,
        status_code,
        String::from_utf8_lossy(&output.stderr).trim(),
    );
    if !ok {
        let _ = fs::remove_file(&temporary);
        return Ok(PluginHttpDownloadResult {
            destination: None,
            response,
        });
    }
    fs::rename(&temporary, &destination)
        .map_err(|error| format!("Unable to save plugin download: {error}"))?;
    Ok(PluginHttpDownloadResult {
        destination: Some(destination.display().to_string()),
        response,
    })
}

fn split_plugin_http_write_out(output: &[u8]) -> (Vec<u8>, Option<u16>) {
    let Some(separator) = output.iter().rposition(|byte| *byte == b'\n') else {
        return (output.to_vec(), None);
    };
    let parsed_status = std::str::from_utf8(&output[separator + 1..])
        .ok()
        .and_then(|value| value.trim().parse::<u16>().ok());
    let Some(parsed_status) = parsed_status else {
        return (output.to_vec(), None);
    };
    (
        output[..separator].to_vec(),
        (parsed_status != 0).then_some(parsed_status),
    )
}

fn plugin_http_response(
    body: &[u8],
    status_code: Option<u16>,
    transport_reason: &str,
) -> PluginHttpResponse {
    let text = if body.is_empty() && status_code.is_none() {
        None
    } else {
        String::from_utf8(body.to_vec()).ok()
    };
    PluginHttpResponse {
        status_code,
        reason: plugin_http_reason(status_code, transport_reason),
        data: serde_json::from_slice(body)
            .ok()
            .filter(|value: &Value| value.is_array() || value.is_object()),
        text,
    }
}

fn plugin_http_response_is_ok(status_code: Option<u16>) -> bool {
    status_code.is_some_and(|status| !(400..600).contains(&status))
}

fn plugin_http_reason(status_code: Option<u16>, transport_reason: &str) -> String {
    let description = status_code.and_then(|status| match status {
        100 => Some("continue"),
        101 => Some("switching protocols"),
        102 => Some("processing"),
        103 => Some("checkpoint"),
        122 => Some("uri too long"),
        200 => Some("ok"),
        201 => Some("created"),
        202 => Some("accepted"),
        203 => Some("non authoritative info"),
        204 => Some("no content"),
        205 => Some("reset content"),
        206 => Some("partial content"),
        207 => Some("multi status"),
        208 => Some("already reported"),
        226 => Some("im used"),
        300 => Some("multiple choices"),
        301 => Some("moved permanently"),
        302 => Some("found"),
        303 => Some("see other"),
        304 => Some("not modified"),
        305 => Some("use proxy"),
        306 => Some("switch proxy"),
        307 => Some("temporary redirect"),
        308 => Some("permanent redirect"),
        400 => Some("bad request"),
        401 => Some("unauthorized"),
        402 => Some("payment required"),
        403 => Some("forbidden"),
        404 => Some("not found"),
        405 => Some("method not allowed"),
        406 => Some("not acceptable"),
        407 => Some("proxy authentication required"),
        408 => Some("request timeout"),
        409 => Some("conflict"),
        410 => Some("gone"),
        411 => Some("length required"),
        412 => Some("precondition failed"),
        413 => Some("request entity too large"),
        414 => Some("request uri too large"),
        415 => Some("unsupported media type"),
        416 => Some("requested range not satisfiable"),
        417 => Some("expectation failed"),
        418 => Some("im a teapot"),
        422 => Some("unprocessable entity"),
        423 => Some("locked"),
        424 => Some("failed dependency"),
        425 => Some("unordered collection"),
        426 => Some("upgrade required"),
        428 => Some("precondition required"),
        429 => Some("too many requests"),
        431 => Some("header fields too large"),
        444 => Some("no response"),
        449 => Some("retry with"),
        450 => Some("blocked by windows parental controls"),
        451 => Some("unavailable for legal reasons"),
        499 => Some("client closed request"),
        500 => Some("internal server error"),
        501 => Some("not implemented"),
        502 => Some("bad gateway"),
        503 => Some("service unavailable"),
        504 => Some("gateway timeout"),
        505 => Some("http version not supported"),
        506 => Some("variant also negotiates"),
        507 => Some("insufficient storage"),
        509 => Some("bandwidth limit exceeded"),
        510 => Some("not extended"),
        _ => None,
    });
    description
        .map(str::to_string)
        .or_else(|| {
            let reason = transport_reason.trim();
            (!reason.is_empty()).then(|| reason.to_string())
        })
        .unwrap_or_else(|| "Unknown".to_string())
}

fn plugin_download_temporary_path(
    parent: &Path,
    destination: &Path,
) -> Result<std::path::PathBuf, String> {
    let file_name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Plugin download destination must name a file".to_string())?;
    Ok(parent.join(format!(".{file_name}.iima-download-{}", std::process::id())))
}

fn plugin_http_method(options: &Value) -> Result<String, String> {
    let method = options
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_string();
    if matches!(
        method.as_str(),
        "DELETE" | "GET" | "HEAD" | "OPTIONS" | "PATCH" | "POST" | "PUT"
    ) {
        Ok(method)
    } else {
        Err("method is invalid.".to_string())
    }
}

fn plugin_http_body(options: &Value) -> Result<Option<String>, String> {
    let Some(data) = options.get("data").filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let body = match data {
        Value::String(value) => value.clone(),
        value => serde_json::to_string(value)
            .map_err(|error| format!("Plugin HTTP data cannot be serialized: {error}"))?,
    };
    if body.len() > 1024 * 1024 {
        return Err("Plugin HTTP request body exceeds 1 MiB".to_string());
    }
    Ok(Some(body))
}

fn validate_plugin_http_header(name: &str, value: &str) -> Result<(), String> {
    if name.trim().is_empty()
        || name.contains(['\r', '\n'])
        || value.contains(['\r', '\n'])
        || name.len() + value.len() > 4096
    {
        Err("Plugin HTTP header is invalid".to_string())
    } else {
        Ok(())
    }
}

fn plugin_http_string_map(options: &Value, key: &str) -> Result<Vec<(String, String)>, String> {
    let Some(values) = options.get(key) else {
        return Ok(Vec::new());
    };
    let object = values
        .as_object()
        .ok_or_else(|| format!("Plugin HTTP {key} must be an object"))?;
    if object.len() > 50 {
        return Err(format!("Plugin HTTP {key} contains too many entries"));
    }
    object
        .iter()
        .map(|(name, value)| {
            value
                .as_str()
                .map(|value| (name.clone(), value.to_string()))
                .ok_or_else(|| format!("Plugin HTTP {key} values must be strings"))
        })
        .collect()
}

pub(crate) fn plugin_file_path_for_command(
    app: &AppHandle,
    state: &AppState,
    session: &PlayerSessionRef<'_>,
    identifier: &str,
    path: &str,
) -> Result<plugins::PluginFilePath, String> {
    ensure_plugin_runtime_is_enabled(app, state, identifier)?;
    let player = session.player().lock().map_err(|error| error.to_string())?;
    if let Some(path) = plugin_track_file_path(&player, path)? {
        return Ok(plugins::PluginFilePath {
            path,
            is_private: false,
        });
    }
    let current_media = player.current_url.clone();
    plugins::resolve_plugin_file_path(app, identifier, path, current_media.as_deref())
}

pub(crate) fn plugin_core_resolve_open_path(
    app: &AppHandle,
    state: &AppState,
    window: &WebviewWindow,
    identifier: &str,
    raw_path: &str,
) -> Result<String, String> {
    ensure_plugin_runtime_is_enabled(app, state, identifier)?;
    let session = state.player_session_for_window(window.label())?;
    if raw_path.starts_with("@tmp/")
        || raw_path.starts_with("@data/")
        || raw_path.starts_with("@video/")
        || raw_path.starts_with("@audio/")
        || raw_path.starts_with("@sub")
    {
        return plugin_file_path_for_command(app, state, &session, identifier, raw_path)
            .map(|path| path.path.display().to_string());
    }

    plugins::require_plugin_permission(app, identifier, "file-system")?;
    if Url::parse(raw_path).is_ok() {
        return Ok(raw_path.to_string());
    }
    if Path::new(raw_path).is_absolute()
        || raw_path.starts_with("@current/")
        || raw_path.starts_with("~/")
    {
        return plugin_file_path_for_command(app, state, &session, identifier, raw_path)
            .map(|path| path.path.display().to_string());
    }
    // `parsePath(forceLocalPath: false)` deliberately preserves relative strings for mpv.
    Ok(raw_path.to_string())
}

fn plugin_track_file_path(player: &PlayerState, raw_path: &str) -> Result<Option<PathBuf>, String> {
    let tracks = if raw_path.starts_with("@video/") {
        &player.tracks.video
    } else if raw_path.starts_with("@audio/") {
        &player.tracks.audio
    } else if raw_path.starts_with("@sub") {
        &player.tracks.subtitles
    } else {
        return Ok(None);
    };
    let components = raw_path
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    let id = (components.len() == 2)
        .then(|| components[1])
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| format!("The path {raw_path} is invalid"))?;
    let path = tracks
        .iter()
        .find(|track| track.id == id)
        .and_then(|track| track.metadata.external_filename.as_deref())
        .ok_or_else(|| {
            format!(
                "Cannot find the file path of track {raw_path}. Perhaps it's an internal stream?"
            )
        })?;
    Ok(Some(PathBuf::from(path)))
}

#[tauri::command]
pub fn plugin_file_exists(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    path: String,
) -> Result<bool, String> {
    let session = state.inner().player_session_for_window(window.label())?;
    let file = plugin_file_path_for_command(&app, state.inner(), &session, &identifier, &path)?;
    Ok(file.path.exists())
}

#[tauri::command]
pub fn plugin_file_list(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    path: String,
    include_sub_dir: Option<bool>,
) -> Result<Vec<PluginFileEntry>, String> {
    let session = state.inner().player_session_for_window(window.label())?;
    let file = plugin_file_path_for_command(&app, state.inner(), &session, &identifier, &path)?;
    if !file.path.is_dir() {
        return Err("Plugin file path is not a directory".to_string());
    }
    let mut entries = Vec::new();
    collect_plugin_file_entries(
        &file.path,
        &file.path,
        include_sub_dir.unwrap_or(false),
        &mut entries,
    )?;
    Ok(entries)
}

fn collect_plugin_file_entries(
    root: &Path,
    directory: &Path,
    _include_sub_dir: bool,
    entries: &mut Vec<PluginFileEntry>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        if entries.len() >= 1000 {
            return Err("Plugin file listing exceeds 1000 entries".to_string());
        }
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        // URL.isExistingDirectory in IINA follows a directory symlink but
        // reports false for a dangling link. Path::is_dir has those semantics.
        let is_dir = path.is_dir();
        let relative = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        entries.push(PluginFileEntry {
            filename: entry.file_name().to_string_lossy().into_owned(),
            path: format!("/{relative}"),
            is_dir,
        });
    }
    Ok(())
}

#[tauri::command]
pub fn plugin_file_read(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    path: String,
    encoding: Option<String>,
) -> Result<String, String> {
    let session = state.inner().player_session_for_window(window.label())?;
    let file = plugin_file_path_for_command(&app, state.inner(), &session, &identifier, &path)?;
    let metadata =
        fs::metadata(&file.path).map_err(|error| format!("Cannot read file: {error}"))?;
    if metadata.len() > 8 * 1024 * 1024 {
        return Err("Plugin file read exceeds the 8 MiB limit".to_string());
    }
    let bytes = fs::read(&file.path).map_err(|error| format!("Cannot read file: {error}"))?;
    let decoded = native_text_encoding::decode(&bytes, encoding.as_deref().unwrap_or("utf8"))?;
    if decoded.len() > 8 * 1024 * 1024 {
        return Err("Plugin file read exceeds the 8 MiB decoded-text limit".to_string());
    }
    Ok(decoded)
}

#[tauri::command]
pub fn plugin_file_write(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    path: String,
    content: String,
) -> Result<(), String> {
    if content.len() > 8 * 1024 * 1024 {
        return Err("Plugin file write exceeds the 8 MiB limit".to_string());
    }
    let session = state.inner().player_session_for_window(window.label())?;
    let file = plugin_file_path_for_command(&app, state.inner(), &session, &identifier, &path)?;
    if !file.is_private && file.path.exists() {
        return Err("Cannot overwrite an existing external file; use @tmp or @data".to_string());
    }
    write_plugin_text_atomically(&file.path, content.as_bytes())
}

fn write_plugin_text_atomically(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let sequence = PLUGIN_FILE_TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut temporary_file = None;
    for attempt in 0..100_u64 {
        let candidate = parent.join(format!(
            ".iima-plugin-write-{}-{sequence}-{attempt}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&candidate)
        {
            Ok(file) => {
                temporary_file = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Cannot create temporary file: {error}")),
        }
    }
    let Some((temporary_path, mut file)) = temporary_file else {
        return Err("Cannot allocate a temporary plugin file".to_string());
    };
    if let Err(error) = file.write_all(contents).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("Cannot write file: {error}"));
    }
    drop(file);
    if let Err(error) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("Cannot write file atomically: {error}"));
    }
    Ok(())
}

#[tauri::command]
pub fn plugin_file_delete(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    path: String,
) -> Result<(), String> {
    let session = state.inner().player_session_for_window(window.label())?;
    let file = plugin_file_path_for_command(&app, state.inner(), &session, &identifier, &path)?;
    if !file.is_private {
        return Err("Plugin file.delete only permits @tmp and @data paths".to_string());
    }
    if file.path.is_dir() {
        fs::remove_dir_all(&file.path).map_err(|error| format!("Cannot delete directory: {error}"))
    } else {
        fs::remove_file(&file.path).map_err(|error| format!("Cannot delete file: {error}"))
    }
}

#[tauri::command]
pub fn plugin_file_trash(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    path: String,
) -> Result<(), String> {
    let session = state.inner().player_session_for_window(window.label())?;
    let file = plugin_file_path_for_command(&app, state.inner(), &session, &identifier, &path)?;
    native_file::remove(&file.path, FileRemovalMode::Trash)
        .map_err(|error| format!("Cannot trash file: {error}"))
}

#[tauri::command]
pub fn plugin_file_move(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    source: String,
    destination: String,
) -> Result<(), String> {
    let session = state.inner().player_session_for_window(window.label())?;
    let source = plugin_file_path_for_command(&app, state.inner(), &session, &identifier, &source)?;
    let destination =
        plugin_file_path_for_command(&app, state.inner(), &session, &identifier, &destination)?;
    if destination.path.exists() {
        return Err("Plugin file move destination already exists".to_string());
    }
    if destination.is_private {
        let parent = destination
            .path
            .parent()
            .ok_or_else(|| "Plugin file path has no parent directory".to_string())?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("Cannot create plugin file directory: {error}"))?;
    }
    fs::rename(source.path, destination.path).map_err(|error| format!("Cannot move file: {error}"))
}

#[tauri::command]
pub fn plugin_file_handle_open(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    path: String,
    mode: String,
) -> Result<String, String> {
    let session = state.inner().player_session_for_window(window.label())?;
    let path = plugin_file_path_for_command(&app, state.inner(), &session, &identifier, &path)?;
    let (mode, file) = match mode.as_str() {
        "read" => (
            PluginFileHandleMode::Read,
            fs::File::open(&path.path)
                .map_err(|error| format!("Cannot create file handle: {error}"))?,
        ),
        "write" => (
            PluginFileHandleMode::Write,
            OpenOptions::new()
                .write(true)
                .open(&path.path)
                .map_err(|error| format!("Cannot create file handle: {error}"))?,
        ),
        _ => return Err("file.handle: mode should be \"read\" or \"write\"".to_string()),
    };
    let mut handles = plugin_file_handles()
        .lock()
        .map_err(|error| error.to_string())?;
    if handles.len() >= 64 {
        return Err("Too many plugin file handles are open".to_string());
    }
    let sequence = PLUGIN_FILE_HANDLE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let token = format!("plugin-file-handle-{}-{sequence}", std::process::id());
    handles.insert(
        token.clone(),
        PluginOpenFileHandle {
            identifier,
            window_label: window.label().to_string(),
            mode,
            file,
        },
    );
    Ok(token)
}

fn with_plugin_file_handle<T>(
    identifier: &str,
    window_label: &str,
    token: &str,
    operation: impl FnOnce(&mut PluginOpenFileHandle) -> Result<T, String>,
) -> Result<T, String> {
    let mut handles = plugin_file_handles()
        .lock()
        .map_err(|error| error.to_string())?;
    let handle = handles
        .get_mut(token)
        .ok_or_else(|| "Plugin file handle is closed or invalid".to_string())?;
    if handle.identifier != identifier || handle.window_label != window_label {
        return Err("Plugin file handle belongs to another plugin instance".to_string());
    }
    operation(handle)
}

#[tauri::command]
pub fn plugin_file_handle_offset(
    window: WebviewWindow,
    identifier: String,
    token: String,
) -> Result<u64, String> {
    with_plugin_file_handle(&identifier, window.label(), &token, |handle| {
        handle
            .file
            .stream_position()
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn plugin_file_handle_seek(
    window: WebviewWindow,
    identifier: String,
    token: String,
    offset: u64,
) -> Result<(), String> {
    with_plugin_file_handle(&identifier, window.label(), &token, |handle| {
        handle
            .file
            .seek(SeekFrom::Start(offset))
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn plugin_file_handle_seek_to_end(
    window: WebviewWindow,
    identifier: String,
    token: String,
) -> Result<(), String> {
    with_plugin_file_handle(&identifier, window.label(), &token, |handle| {
        handle
            .file
            .seek(SeekFrom::End(0))
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
}

fn read_plugin_file_handle(
    handle: &mut PluginOpenFileHandle,
    length: usize,
) -> Result<Vec<u8>, String> {
    if handle.mode != PluginFileHandleMode::Read {
        return Err("Plugin file handle is not open for reading".to_string());
    }
    if length > PLUGIN_FILE_HANDLE_MAX_IO_BYTES {
        return Err("Plugin file handle read exceeds the 8 MiB limit".to_string());
    }
    let mut data = Vec::with_capacity(length.min(64 * 1024));
    Read::by_ref(&mut handle.file)
        .take(length as u64)
        .read_to_end(&mut data)
        .map_err(|error| error.to_string())?;
    Ok(data)
}

fn read_plugin_file_handle_to_end(
    handle: &mut PluginOpenFileHandle,
    maximum_length: usize,
) -> Result<Vec<u8>, String> {
    if handle.mode != PluginFileHandleMode::Read {
        return Err("Plugin file handle is not open for reading".to_string());
    }
    let mut data = Vec::with_capacity(maximum_length.min(64 * 1024));
    Read::by_ref(&mut handle.file)
        .take(maximum_length.saturating_add(1) as u64)
        .read_to_end(&mut data)
        .map_err(|error| error.to_string())?;
    if data.len() > maximum_length {
        return Err("Plugin file handle read exceeds the 8 MiB limit".to_string());
    }
    Ok(data)
}

#[tauri::command]
pub fn plugin_file_handle_read(
    window: WebviewWindow,
    identifier: String,
    token: String,
    length: usize,
) -> Result<Vec<u8>, String> {
    with_plugin_file_handle(&identifier, window.label(), &token, |handle| {
        read_plugin_file_handle(handle, length)
    })
}

#[tauri::command]
pub fn plugin_file_handle_read_to_end(
    window: WebviewWindow,
    identifier: String,
    token: String,
) -> Result<Vec<u8>, String> {
    with_plugin_file_handle(&identifier, window.label(), &token, |handle| {
        read_plugin_file_handle_to_end(handle, PLUGIN_FILE_HANDLE_MAX_IO_BYTES)
    })
}

fn plugin_file_handle_bytes(data: Value) -> Result<Vec<u8>, String> {
    let bytes = match data {
        Value::String(value) => value.into_bytes(),
        Value::Array(values) => values
            .into_iter()
            .map(|value| {
                value
                    .as_u64()
                    .filter(|byte| *byte <= u8::MAX as u64)
                    .map(|byte| byte as u8)
                    .ok_or_else(|| "Plugin file handle data must contain bytes".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err("Plugin file handle data must be a string or byte array".to_string()),
    };
    if bytes.len() > PLUGIN_FILE_HANDLE_MAX_IO_BYTES {
        return Err("Plugin file handle write exceeds the 8 MiB limit".to_string());
    }
    Ok(bytes)
}

#[tauri::command]
pub fn plugin_file_handle_write(
    window: WebviewWindow,
    identifier: String,
    token: String,
    data: Value,
) -> Result<(), String> {
    let bytes = plugin_file_handle_bytes(data)?;
    with_plugin_file_handle(&identifier, window.label(), &token, |handle| {
        if handle.mode != PluginFileHandleMode::Write {
            return Err("Plugin file handle is not open for writing".to_string());
        }
        handle
            .file
            .write_all(&bytes)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn plugin_file_handle_close(
    window: WebviewWindow,
    identifier: String,
    token: String,
) -> Result<(), String> {
    let mut handles = plugin_file_handles()
        .lock()
        .map_err(|error| error.to_string())?;
    let handle = handles
        .get(&token)
        .ok_or_else(|| "Plugin file handle is closed or invalid".to_string())?;
    if handle.identifier != identifier || handle.window_label != window.label() {
        return Err("Plugin file handle belongs to another plugin instance".to_string());
    }
    handles.remove(&token);
    Ok(())
}

#[tauri::command]
pub fn plugin_file_show_in_finder(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    identifier: String,
    path: String,
) -> Result<(), String> {
    let session = state.inner().player_session_for_window(window.label())?;
    let file = plugin_file_path_for_command(&app, state.inner(), &session, &identifier, &path)?;
    Command::new("open")
        .arg("-R")
        .arg(file.path)
        .spawn()
        .map_err(|error| format!("Cannot reveal plugin file: {error}"))?;
    Ok(())
}

#[tauri::command]
pub fn open_media_paths(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    paths: Vec<String>,
) -> Result<PlayerState, String> {
    open_media_paths_in_window(&app, state.inner(), window.label(), paths, Vec::new())
}

#[tauri::command]
pub fn open_dropped_media_paths(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    paths: Vec<String>,
) -> Result<PlayerState, String> {
    open_dropped_media_paths_in_window(&app, state.inner(), window.label(), paths)
}

pub(crate) fn open_dropped_media_paths_in_window<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    window_label: &str,
    paths: Vec<String>,
) -> Result<PlayerState, String> {
    let plan = match playlist_actions::plan_dropped_media(&paths) {
        playlist_actions::DroppedMediaPlan::Open(paths) => {
            return open_media_paths_in_window(app, state, window_label, paths, Vec::new());
        }
        plan => plan,
    };

    let session = state.player_session_for_window(window_label)?;
    {
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        match plan {
            playlist_actions::DroppedMediaPlan::Lut3d(path) => {
                player.apply(PlayerCommand::AddFilter {
                    kind: FilterKind::Video,
                    filter: format!("@iina_quickl3d:lavfi=[lut3d=file={path}:interp=nearest]"),
                });
            }
            playlist_actions::DroppedMediaPlan::Subtitles(paths) => {
                for path in paths {
                    player.apply(PlayerCommand::LoadExternalTrack {
                        kind: ExternalTrackKind::Subtitles,
                        path,
                    });
                }
            }
            playlist_actions::DroppedMediaPlan::None => {}
            playlist_actions::DroppedMediaPlan::Open(_) => unreachable!(),
        }
    }
    session.sync_mpv_executor_from_player()?;
    let snapshot = player_snapshot_for_session(&session)?;
    emit_player_state_for_session(app, session.label(), &snapshot);
    Ok(snapshot)
}

pub(crate) fn open_media_paths_in_window<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    window_label: &str,
    paths: Vec<String>,
    mpv_options: Vec<(String, String)>,
) -> Result<PlayerState, String> {
    let session = state.player_session_for_window(window_label)?;
    let auto_loaded_subtitles = plan_auto_loaded_subtitles(state, &paths)?;
    let plan = plan_open_media_paths(state, paths);
    let Some(first_path) = plan.paths.first() else {
        return player_snapshot_for_session(&session);
    };
    let probe = probe_media(first_path);
    let pause_when_open = pause_when_open_preference(state)?;
    {
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        for (property, value) in mpv_options {
            player.apply(PlayerCommand::PluginMpvSet { property, value });
        }
        open_media_batch_with_plan(
            &mut player,
            plan,
            auto_loaded_subtitles,
            probe,
            pause_when_open,
        );
    }
    reset_hdr_for_player_session(state, &session)?;
    let snapshot = player_snapshot_for_session(&session)?;
    observe_player_window_surface(state, session.label(), &snapshot)?;
    if let Some(window) = app.get_webview_window(window_label) {
        apply_window_presentation_mode(&window, "player")?;
        sync_player_window_surface(app, state, session.label(), &snapshot)?;
        apply_open_window_preferences(&window, state, &snapshot, true)?;
        window.unminimize().map_err(|error| error.to_string())?;
        window.show().map_err(|error| error.to_string())?;
        #[cfg(target_os = "macos")]
        {
            let preferences = state
                .preferences
                .lock()
                .map(|preferences| preferences.clone())
                .map_err(|error| error.to_string())?;
            native_video::install(
                window.ns_view().map_err(|error| error.to_string())?,
                session.label(),
                &native_video::surface_settings_from_preferences(&preferences),
            )?;
            native_video::configure_color(
                &native_video::color_settings_from_preferences(&preferences),
                session.label(),
            );
        }
        window.set_focus().map_err(|error| error.to_string())?;
    }
    // IINA makes the player window and video host visible before creating its render context and
    // sending the first loadfile. Keeping the complete operation log queued until this point also
    // lets the executor retry a transient native-surface attachment without losing media commands.
    session.sync_mpv_executor_from_player()?;
    record_recent_media_open(app, state, &session)?;
    if fullscreen_when_open_preference(state)? {
        if let Some(window) = app.get_webview_window(window_label) {
            set_player_window_fullscreen(app, state, &window, true)?;
        }
    }
    emit_player_state_for_session(app, session.label(), &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub fn enqueue_media_paths(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    paths: Vec<String>,
) -> Result<PlayerState, String> {
    let target = state
        .inner()
        .shortcut_player_session_label(window.label())?;
    enqueue_media_paths_in_session(&app, state.inner(), &target, paths)
}

fn enqueue_media_paths_in_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    target: &str,
    paths: Vec<String>,
) -> Result<PlayerState, String> {
    let session = state.player_session_for_window(target)?;
    let Some(first_path) = paths.first().cloned() else {
        return player_snapshot_for_session(&session);
    };
    let should_open = {
        let player = session.player().lock().map_err(|error| error.to_string())?;
        player.playlist.is_empty()
    };
    let pause_when_open = should_open
        .then(|| pause_when_open_preference(state))
        .transpose()?
        .unwrap_or(false);
    let plan = if should_open {
        Some(plan_open_media_paths(state, paths.clone()))
    } else {
        None
    };
    let auto_loaded_subtitles = if should_open {
        plan_auto_loaded_subtitles(state, &paths)?
    } else {
        Vec::new()
    };

    {
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        if should_open {
            open_media_batch_with_plan(
                &mut player,
                plan.expect("an auto-add plan exists when opening an empty playlist"),
                auto_loaded_subtitles,
                probe_media(&first_path),
                pause_when_open,
            );
        } else {
            player.enqueue_media(paths);
        }
    }
    if should_open {
        reset_hdr_for_player_session(state, &session)?;
        record_recent_media_open(app, state, &session)?;
    }
    session.sync_mpv_executor_from_player()?;
    let snapshot = player_snapshot_for_session(&session)?;
    emit_player_state_for_session(app, session.label(), &snapshot);
    if should_open {
        let target_window = app
            .get_webview_window(target)
            .ok_or_else(|| format!("player window is not available for session {target}"))?;
        apply_open_window_preferences(&target_window, state, &snapshot, true)?;
        if fullscreen_when_open_preference(state)? {
            set_player_window_fullscreen(app, state, &target_window, true)?;
        }
    }
    Ok(snapshot)
}

#[tauri::command]
pub fn clear_recent_documents(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<PlayerState, String> {
    clear_recent_documents_and_persist(&app, state.inner())?;
    menu::refresh_iina_menu(&app)?;
    let session_label = state
        .inner()
        .shortcut_player_session_label(window.label())?;
    let session = state.inner().player_session_for_window(&session_label)?;
    player_snapshot_for_session(&session)
}

#[tauri::command]
pub fn clear_saved_playback_progress(app: AppHandle) -> Result<(), String> {
    let watch_later_directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("watch_later");
    recreate_directory(&watch_later_directory)
}

#[tauri::command]
pub fn get_playback_history(
    state: tauri::State<AppState>,
) -> Result<Vec<crate::history::PlaybackHistoryItem>, String> {
    state.inner().playback_history()
}

#[tauri::command]
pub fn show_playback_history(app: AppHandle) -> Result<(), String> {
    show_playback_history_window(&app)
}

pub(crate) fn show_playback_history_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let label = crate::auxiliary_player_windows::PLAYBACK_HISTORY_WINDOW_LABEL;
    if let Some(window) = app.get_webview_window(label) {
        window.unminimize().map_err(|error| error.to_string())?;
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::App("history.html".into()))
        .title(localization::menu_title("Playback History"))
        .inner_size(600.0, 400.0)
        .min_inner_size(400.0, 200.0)
        .resizable(true)
        .decorations(true)
        .center()
        .build()
        .map_err(|error| error.to_string())?;
    crate::auxiliary_player_windows::configure_retained_window(&window, "PlaybackHistoryWindow")
}

#[tauri::command]
pub fn remove_playback_history_entries(
    state: tauri::State<AppState>,
    ids: Vec<String>,
) -> Result<Vec<crate::history::PlaybackHistoryItem>, String> {
    state.inner().remove_playback_history_entries(&ids)
}

#[tauri::command]
pub fn open_playback_history_item(
    app: AppHandle,
    state: tauri::State<AppState>,
    path: String,
    new_window: bool,
) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("Playback history path is empty".to_string());
    }
    let target = menu::active_player_window_label(&app);
    if new_window || app.get_webview_window(&target).is_none() {
        open_new_player_window(&app, state.inner(), vec![path], Vec::new())?;
        return Ok(());
    }

    let session = state.inner().player_session_for_window(&target)?;
    let auto_loaded_subtitles = plan_auto_loaded_subtitles(state.inner(), &[path.clone()])?;
    let plan = plan_open_media_paths(state.inner(), vec![path.clone()]);
    let pause_when_open = pause_when_open_preference(state.inner())?;
    {
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        open_media_batch_with_plan(
            &mut player,
            plan,
            auto_loaded_subtitles,
            probe_media(&path),
            pause_when_open,
        );
    }
    reset_hdr_for_player_session(state.inner(), &session)?;
    session.sync_mpv_executor_from_player()?;
    record_recent_media_open(&app, state.inner(), &session)?;
    let snapshot = player_snapshot_for_session(&session)?;
    emit_player_state_for_session(&app, session.label(), &snapshot);
    if let Some(window) = app.get_webview_window(&target) {
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        apply_open_window_preferences(&window, state.inner(), &snapshot, true)?;
        if fullscreen_when_open_preference(state.inner())? {
            set_player_window_fullscreen(&app, state.inner(), &window, true)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn reveal_playback_history_items(paths: Vec<String>) -> Result<usize, String> {
    let paths = paths
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return Ok(0);
    }
    Command::new("/usr/bin/open")
        .arg("-R")
        .args(&paths)
        .spawn()
        .map_err(|error| format!("Unable to reveal playback history items: {error}"))?;
    Ok(paths.len())
}

#[tauri::command]
pub fn clear_playback_history(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<PlayerState, String> {
    let history_file = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("history.plist");
    remove_file_if_present(&history_file)?;
    state.inner().clear_playback_history()?;
    clear_recent_documents_and_persist(&app, state.inner())?;
    menu::refresh_iina_menu(&app)?;
    let session_label = state
        .inner()
        .shortcut_player_session_label(window.label())?;
    let session = state.inner().player_session_for_window(&session_label)?;
    player_snapshot_for_session(&session)
}

#[tauri::command]
pub fn restore_suppressed_alerts(
    app: AppHandle,
    state: tauri::State<AppState>,
) -> Result<PreferenceStore, String> {
    let preferences = {
        let mut preferences = state
            .preferences
            .lock()
            .map_err(|error| error.to_string())?;
        preferences.values.insert(
            "suppressCannotPreventDisplaySleep".to_string(),
            Value::Bool(false),
        );
        preferences.clone()
    };
    preferences.save_to_file(&preference_file_path(
        app.path()
            .app_config_dir()
            .map_err(|error| error.to_string())?,
    ))?;
    Ok(preferences)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DefaultApplicationResult {
    success_count: i32,
    failed_count: i32,
}

#[tauri::command]
pub fn set_default_application(
    video: bool,
    audio: bool,
    playlist: bool,
) -> Result<DefaultApplicationResult, String> {
    let (success_count, failed_count) =
        native_default_app::set_default_application(video, audio, playlist)?;
    Ok(DefaultApplicationResult {
        success_count,
        failed_count,
    })
}

#[tauri::command]
pub fn open_browser_extension(browser: String) -> Result<(), String> {
    let url = browser_extension_url(&browser)
        .ok_or_else(|| format!("Unsupported browser extension: {browser}"))?;
    Command::new("/usr/bin/open")
        .arg(url)
        .spawn()
        .map_err(|error| format!("Unable to open browser extension page: {error}"))?;
    Ok(())
}

fn browser_extension_url(browser: &str) -> Option<&'static str> {
    match browser {
        "chrome" => Some(
            "https://chrome.google.com/webstore/detail/open-in-iina/pdnojahnhpgmdhjdhgphgdcecehkbhfo",
        ),
        "firefox" => Some("https://addons.mozilla.org/addon/open-in-iina-x"),
        _ => None,
    }
}

fn recreate_directory(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_dir_all(path)
            .map_err(|error| format!("Unable to clear {}: {error}", path.display()))?;
    }
    fs::create_dir_all(path)
        .map_err(|error| format!("Unable to create {}: {error}", path.display()))
}

fn remove_file_if_present(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Unable to delete {}: {error}", path.display())),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecentDocumentSource {
    OpenPanel,
    FileLoaded,
}

fn should_note_recent_document(
    record_recent_files: bool,
    track_all_files: bool,
    source: RecentDocumentSource,
) -> bool {
    record_recent_files && (source == RecentDocumentSource::OpenPanel || track_all_files)
}

fn recent_document_recording_preferences(state: &AppState) -> Result<(bool, bool), String> {
    state
        .preferences
        .lock()
        .map(|preferences| {
            (
                bool_preference(&preferences.values, "recordRecentFiles", true),
                bool_preference(&preferences.values, "trackAllFilesInRecentOpenMenu", true),
            )
        })
        .map_err(|error| error.to_string())
}

fn record_recent_open_panel_selection<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    paths: &[String],
) -> Result<(), String> {
    let candidates = paths
        .iter()
        .filter(|path| should_record_recent_media_path(path))
        .map(|path| (path.clone(), recent_document_title(path)))
        .collect::<Vec<_>>();
    record_recent_document_candidates(app, state, &candidates, RecentDocumentSource::OpenPanel)
}

pub(crate) fn record_recent_media_open<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    session: &PlayerSessionRef<'_>,
) -> Result<(), String> {
    let candidate = session
        .player()
        .lock()
        .map(|player| {
            player
                .current_url
                .clone()
                .filter(|path| should_record_recent_media_path(path))
                .map(|path| (path, player.media_title.clone()))
        })
        .map_err(|error| error.to_string())?;
    record_recent_document_candidates(
        app,
        state,
        &candidate.into_iter().collect::<Vec<_>>(),
        RecentDocumentSource::FileLoaded,
    )
}

fn should_record_recent_media_path(path: &str) -> bool {
    path != "-"
}

fn recent_document_title(path: &str) -> String {
    Url::parse(path)
        .ok()
        .and_then(|url| {
            url.path_segments()
                .and_then(|mut segments| segments.next_back())
                .filter(|title| !title.is_empty())
                .map(str::to_string)
                .or_else(|| url.host_str().map(str::to_string))
        })
        .or_else(|| {
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| path.to_string())
}

fn record_recent_document_candidates<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    candidates: &[(String, String)],
    source: RecentDocumentSource,
) -> Result<(), String> {
    let (record_recent_files, track_all_files) = recent_document_recording_preferences(state)?;
    let should_note = should_note_recent_document(record_recent_files, track_all_files, source);

    #[cfg(target_os = "macos")]
    {
        if should_note {
            for (path, _) in candidates {
                native_recent_documents::note(path)?;
            }
        }
        let native_documents = synchronize_recent_documents_from_native(state)?;
        if should_note {
            persist_native_recent_documents(app, state, &native_documents)?;
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        if should_note {
            for (path, title) in candidates {
                state.record_recent_document(path.clone(), title.clone())?;
            }
            save_modeled_recent_documents(app, state)?;
        } else {
            state.restore_recent_documents(state.recent_documents()?)?;
        }
    }

    let _ = menu::refresh_iina_menu(app);
    Ok(())
}

#[cfg(target_os = "macos")]
fn synchronize_recent_documents_from_native(
    state: &AppState,
) -> Result<Vec<native_recent_documents::NativeRecentDocument>, String> {
    let native_documents = native_recent_documents::snapshot()?;
    state.restore_recent_documents(native_recent_documents::player_documents(&native_documents))?;
    Ok(native_documents)
}

#[cfg(target_os = "macos")]
fn persist_native_recent_documents<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    native_documents: &[native_recent_documents::NativeRecentDocument],
) -> Result<(), String> {
    if !native_recent_documents::is_sonoma_or_newer() {
        return Ok(());
    }
    let preferences = {
        let mut preferences = state
            .preferences
            .lock()
            .map_err(|error| error.to_string())?;
        preferences.values.insert(
            "recentDocuments".to_string(),
            native_recent_documents::persistence_value(native_documents),
        );
        preferences.clone()
    };
    preferences.save_to_file(&preference_file_path(
        app.path()
            .app_config_dir()
            .map_err(|error| error.to_string())?,
    ))
}

#[cfg(not(target_os = "macos"))]
fn save_modeled_recent_documents<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
) -> Result<(), String> {
    let recent_documents: Vec<RecentDocument> = state.recent_documents()?;
    let preferences = {
        let mut preferences = state
            .preferences
            .lock()
            .map_err(|error| error.to_string())?;
        preferences.values.insert(
            "recentDocuments".to_string(),
            serde_json::json!(recent_documents),
        );
        preferences.clone()
    };
    preferences.save_to_file(&preference_file_path(
        app.path()
            .app_config_dir()
            .map_err(|error| error.to_string())?,
    ))
}

fn clear_recent_documents_and_persist<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        native_recent_documents::clear()?;
        let native_documents = synchronize_recent_documents_from_native(state)?;
        persist_native_recent_documents(app, state, &native_documents)
    }
    #[cfg(not(target_os = "macos"))]
    {
        state.clear_recent_documents()?;
        save_modeled_recent_documents(app, state)
    }
}

fn format_video_time_with_precision(seconds: f64, precision: usize) -> String {
    let seconds = if seconds.is_finite() {
        seconds.max(0.0)
    } else {
        0.0
    };
    let hour = (seconds / 3_600.0).floor() as u64;
    let minute = ((seconds % 3_600.0) / 60.0).floor() as u64;
    let second = seconds % 60.0;
    let second_width = precision + 3;
    if hour == 0 {
        format!(
            "{minute:02}:{second:0second_width$.precision$}",
            second_width = second_width,
            precision = precision
        )
    } else {
        format!(
            "{hour}:{minute:02}:{second:0second_width$.precision$}",
            second_width = second_width,
            precision = precision
        )
    }
}

fn parse_video_time(input: &str) -> Option<f64> {
    let components = input
        .split(':')
        .filter(|component| !component.is_empty())
        .rev()
        .collect::<Vec<_>>();
    let hour = components
        .get(2)
        .and_then(|component| component.parse::<i64>().ok());
    let minute = components
        .get(1)
        .and_then(|component| component.parse::<i64>().ok());
    let second = components
        .first()
        .and_then(|component| component.parse::<f64>().ok())
        .filter(|value| value.is_finite());
    if hour.is_none() && minute.is_none() && second.is_none() {
        return None;
    }
    let total = hour.unwrap_or_default() as f64 * 3_600.0
        + minute.unwrap_or_default() as f64 * 60.0
        + second.unwrap_or_default();
    total.is_finite().then_some(total)
}

fn serialize_m3u8_playlist<'a>(filenames: impl IntoIterator<Item = &'a str>) -> String {
    let mut contents = String::new();
    for filename in filenames {
        contents.push_str(filename);
        contents.push('\n');
    }
    contents
}

fn format_localized_string_arguments(template: &str, arguments: &[&str]) -> String {
    arguments
        .iter()
        .fold(template.to_string(), |message, value| {
            message.replacen("%@", value, 1)
        })
}

fn write_atomic_playlist(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let sequence = PLAYLIST_TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut temporary_file = None;
    for attempt in 0..100_u64 {
        let candidate = parent.join(format!(
            ".iima-playlist-{}-{sequence}-{attempt}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&candidate)
        {
            Ok(file) => {
                temporary_file = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Unable to create temporary playlist file: {error}")),
        }
    }
    let Some((temporary_path, mut file)) = temporary_file else {
        return Err("Unable to allocate a temporary playlist file".to_string());
    };
    if let Err(error) = file.write_all(contents).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("Unable to write playlist: {error}"));
    }
    drop(file);
    if let Err(error) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("Unable to save playlist atomically: {error}"));
    }
    Ok(())
}

#[tauri::command]
pub fn jump_to_time_dialog(
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<Option<PlayerState>, String> {
    let initial_value = {
        let session = state.inner().player_session_for_window(window.label())?;
        let player = player_snapshot_for_session(&session)?;
        format_video_time_with_precision(player.position_seconds, 3)
    };
    let selected = native_prompt::prompt_text(
        &localization::menu_title("Jump to"),
        &localization::menu_title("Please enter the time you want to jump to. Example: 20:35"),
        &initial_value,
        &localization::menu_title("OK"),
        &localization::menu_title("Cancel"),
    )?;
    let Some(seconds) = selected.as_deref().and_then(parse_video_time) else {
        return Ok(None);
    };
    player_command(state, window, PlayerCommand::SeekAbsoluteExact { seconds }).map(Some)
}

#[tauri::command]
pub fn save_current_playlist(
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<Option<String>, String> {
    let destination = window
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Save to playlist")
        .set_can_create_directories(true)
        .add_filter("M3U8 Playlist", &["m3u8"])
        .blocking_save_file();
    let Some(destination) = destination else {
        return Ok(None);
    };
    let destination = destination.into_path().map_err(|error| error.to_string())?;
    let contents = {
        let session = state.inner().player_session_for_window(window.label())?;
        let player = player_snapshot_for_session(&session)?;
        serialize_m3u8_playlist(player.playlist.iter().map(|item| item.path.as_str()))
    };
    if let Err(error) = write_atomic_playlist(&destination, contents.as_bytes()) {
        let message = format_localized_string_arguments(
            &localization::menu_title("Error occurred when saving %@: %@"),
            &["subtitle", &error],
        );
        native_prompt::show_error(&localization::menu_title("Error"), &message)?;
        return Ok(None);
    }
    Ok(Some(destination.display().to_string()))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlaylistFileFailure {
    pub index: usize,
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistTrashResult {
    pub player: PlayerState,
    pub trashed_indexes: Vec<usize>,
    pub failures: Vec<PlaylistFileFailure>,
}

fn playlist_targets_for_window(
    state: &AppState,
    window_label: &str,
    indexes: &[usize],
) -> Result<PlaylistTargets, String> {
    let session = state.player_session_for_window(window_label)?;
    let paths = session
        .player()
        .lock()
        .map(|player| {
            player
                .playlist
                .iter()
                .map(|item| item.path.clone())
                .collect::<Vec<_>>()
        })
        .map_err(|error| error.to_string())?;
    Ok(playlist_actions::targets(&paths, indexes))
}

fn apply_playlist_command(
    state: &AppState,
    window_label: &str,
    command: PlayerCommand,
) -> Result<PlayerState, String> {
    let session = state.player_session_for_window(window_label)?;
    {
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        player.apply(command);
    }
    session.sync_mpv_executor_from_player()?;
    player_snapshot_for_session(&session)
}

fn insert_playlist_paths_in_window<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    window_label: &str,
    paths: Vec<String>,
    destination: usize,
) -> Result<PlayerState, String> {
    let paths = playlist_actions::resolve_playable_targets(&paths);
    let session = state.player_session_for_window(window_label)?;
    if paths.is_empty() {
        return player_snapshot_for_session(&session);
    }
    let should_open = session
        .player()
        .lock()
        .map(|player| player.playlist.is_empty())
        .map_err(|error| error.to_string())?;
    if should_open {
        return open_media_paths_in_window(app, state, window_label, paths, Vec::new());
    }
    apply_playlist_command(
        state,
        window_label,
        PlayerCommand::InsertPlaylistItems { paths, destination },
    )
}

fn execute_playlist_trash_plan<T>(
    targets: &[IndexedPlaylistPath],
    mut trash: impl FnMut(&Path) -> Result<(), String>,
    remove_successes: impl FnOnce(Vec<usize>) -> Result<T, String>,
) -> Result<(T, Vec<usize>, Vec<PlaylistFileFailure>), String> {
    let mut successes = Vec::new();
    let mut failures = Vec::new();
    for target in targets {
        match trash(Path::new(&target.path)) {
            Ok(()) => successes.push(target.index),
            Err(error) => failures.push(PlaylistFileFailure {
                index: target.index,
                path: target.path.clone(),
                error,
            }),
        }
    }
    let result = remove_successes(successes.clone())?;
    Ok((result, successes, failures))
}

#[tauri::command]
pub fn playlist_play_next(
    state: tauri::State<AppState>,
    window: WebviewWindow,
    indexes: Vec<usize>,
) -> Result<PlayerState, String> {
    apply_playlist_command(
        state.inner(),
        window.label(),
        PlayerCommand::PlayPlaylistItemsNext { indexes },
    )
}

#[tauri::command]
pub fn playlist_remove_items(
    state: tauri::State<AppState>,
    window: WebviewWindow,
    indexes: Vec<usize>,
) -> Result<PlayerState, String> {
    apply_playlist_command(
        state.inner(),
        window.label(),
        PlayerCommand::RemovePlaylistItems { indexes },
    )
}

#[tauri::command]
pub fn playlist_insert_items(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    paths: Vec<String>,
    destination: usize,
) -> Result<PlayerState, String> {
    insert_playlist_paths_in_window(&app, state.inner(), window.label(), paths, destination)
}

#[tauri::command]
pub fn playlist_copy_items(
    state: tauri::State<AppState>,
    window: WebviewWindow,
    indexes: Vec<usize>,
) -> Result<usize, String> {
    let targets = playlist_targets_for_window(state.inner(), window.label(), &indexes)?;
    let normalized_indexes = targets
        .selected
        .iter()
        .map(|item| item.index)
        .collect::<Vec<_>>();
    let paths = targets
        .selected
        .into_iter()
        .map(|item| item.path)
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return Ok(0);
    }
    native_pasteboard::write(&normalized_indexes, &paths)?;
    Ok(paths.len())
}

#[tauri::command]
pub fn playlist_can_paste_filenames() -> bool {
    native_pasteboard::has_filenames()
}

#[tauri::command]
pub fn playlist_paste_items(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    destination: usize,
) -> Result<Option<PlayerState>, String> {
    let Some(payload) = native_pasteboard::read()? else {
        return Ok(None);
    };
    let paths = match payload.kind {
        PlaylistPasteboardKind::Filenames | PlaylistPasteboardKind::Urls => payload.values,
        PlaylistPasteboardKind::String => payload
            .values
            .into_iter()
            .filter(|value| playlist_actions::is_network_resource(value))
            .collect(),
    };
    if paths.is_empty() {
        return Ok(None);
    }
    insert_playlist_paths_in_window(&app, state.inner(), window.label(), paths, destination)
        .map(Some)
}

#[tauri::command]
pub fn playlist_add_url_dialog(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<Option<PlayerState>, String> {
    let Some(url) = native_prompt::prompt_text(
        &localization::menu_title("Add URL"),
        &localization::menu_title("Please enter the URL:"),
        "",
        &localization::menu_title("OK"),
        &localization::menu_title("Cancel"),
    )?
    else {
        return Ok(None);
    };
    if !playlist_actions::is_network_resource(&url) {
        native_prompt::show_error(
            &localization::menu_title("Error"),
            &localization::menu_title("Wrong URL format."),
        )?;
        return Ok(None);
    }
    let destination = state
        .inner()
        .player_session_for_window(window.label())?
        .player()
        .lock()
        .map(|player| player.playlist.len())
        .map_err(|error| error.to_string())?;
    insert_playlist_paths_in_window(&app, state.inner(), window.label(), vec![url], destination)
        .map(Some)
}

#[tauri::command]
pub fn playlist_open_items_in_new_window(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    indexes: Vec<usize>,
) -> Result<Option<String>, String> {
    let targets = playlist_targets_for_window(state.inner(), window.label(), &indexes)?;
    let paths = targets
        .selected
        .into_iter()
        .map(|item| item.path)
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return Ok(None);
    }
    open_new_player_window(&app, state.inner(), paths, Vec::new()).map(|(label, _)| Some(label))
}

#[tauri::command]
pub fn playlist_trash_items(
    state: tauri::State<AppState>,
    window: WebviewWindow,
    indexes: Vec<usize>,
) -> Result<PlaylistTrashResult, String> {
    let window_label = window.label().to_string();
    let targets = playlist_targets_for_window(state.inner(), &window_label, &indexes)?;
    let (player, trashed_indexes, failures) = execute_playlist_trash_plan(
        &targets.local,
        |path| native_file::remove(path, FileRemovalMode::Trash),
        |successful_indexes| {
            if successful_indexes.is_empty() {
                let session = state.inner().player_session_for_window(&window_label)?;
                player_snapshot_for_session(&session)
            } else {
                apply_playlist_command(
                    state.inner(),
                    &window_label,
                    PlayerCommand::RemovePlaylistItems {
                        indexes: successful_indexes,
                    },
                )
            }
        },
    )?;
    Ok(PlaylistTrashResult {
        player,
        trashed_indexes,
        failures,
    })
}

#[tauri::command]
pub fn playlist_open_network_items(
    state: tauri::State<AppState>,
    window: WebviewWindow,
    indexes: Vec<usize>,
) -> Result<usize, String> {
    let targets = playlist_targets_for_window(state.inner(), window.label(), &indexes)?;
    let urls = targets
        .network
        .into_iter()
        .map(|item| item.path)
        .collect::<Vec<_>>();
    if urls.is_empty() {
        return Ok(0);
    }
    Command::new("/usr/bin/open")
        .args(&urls)
        .spawn()
        .map_err(|error| format!("Unable to open playlist URLs: {error}"))?;
    Ok(urls.len())
}

#[tauri::command]
pub fn playlist_copy_network_urls(
    state: tauri::State<AppState>,
    window: WebviewWindow,
    indexes: Vec<usize>,
) -> Result<String, String> {
    let targets = playlist_targets_for_window(state.inner(), window.label(), &indexes)?;
    let text = targets
        .network
        .into_iter()
        .map(|item| item.path)
        .collect::<Vec<_>>()
        .join("\n");
    native_file::copy_text(&text)?;
    Ok(text)
}

#[tauri::command]
pub fn playlist_reveal_items(
    state: tauri::State<AppState>,
    window: WebviewWindow,
    indexes: Vec<usize>,
) -> Result<usize, String> {
    let targets = playlist_targets_for_window(state.inner(), window.label(), &indexes)?;
    let paths = targets
        .local
        .into_iter()
        .map(|item| PathBuf::from(item.path))
        .collect::<Vec<_>>();
    native_file::reveal(&paths)?;
    Ok(paths.len())
}

#[tauri::command]
pub fn player_command(
    state: tauri::State<AppState>,
    window: WebviewWindow,
    command: PlayerCommand,
) -> Result<PlayerState, String> {
    let app = window.app_handle().clone();
    let target_label = state
        .inner()
        .shortcut_player_session_label(window.label())?;
    if matches!(&command, PlayerCommand::Stop) {
        state
            .inner()
            .save_playback_position_for_window(&target_label)?;
    }
    let session = state.inner().player_session_for_window(&target_label)?;
    let hdr_enabled = match &command {
        PlayerCommand::SetHdrEnabled { enabled } => Some(*enabled),
        _ => None,
    };
    {
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        player.apply(command);
    }
    if let Some(enabled) = hdr_enabled {
        native_video::set_hdr_enabled(enabled, session.label());
    }
    session.sync_mpv_executor_from_player()?;
    let snapshot = player_snapshot_for_session(&session)?;
    sync_player_window_surface(&app, state.inner(), &target_label, &snapshot)?;
    Ok(snapshot)
}

#[tauri::command]
pub fn toggle_music_mode(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<PlayerState, String> {
    let session_label = state
        .inner()
        .shortcut_player_session_label(window.label())?;
    toggle_music_mode_window_for_session(&app, state.inner(), &session_label)
}

#[tauri::command]
pub fn close_mini_player(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<PlayerState, String> {
    let session_label = state
        .inner()
        .shortcut_player_session_label(window.label())?;
    close_mini_player_window_for_session(&app, state.inner(), &session_label)
}

#[tauri::command]
pub fn toggle_picture_in_picture(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<PlayerState, String> {
    let session_label = state
        .inner()
        .shortcut_player_session_label(window.label())?;
    toggle_picture_in_picture_for_session(&app, state.inner(), &session_label)
}

fn apply_pip_window_transition<R: Runtime>(
    window: &WebviewWindow<R>,
    transition: PipWindowTransition,
) -> Result<(), String> {
    if transition.hide {
        window.hide().map_err(|error| error.to_string())?;
    }
    if transition.minimize {
        window.minimize().map_err(|error| error.to_string())?;
    }
    if transition.deminimize {
        window.unminimize().map_err(|error| error.to_string())?;
    }
    if transition.show {
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn toggle_picture_in_picture_for_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    session_label: &str,
) -> Result<PlayerState, String> {
    let session = state.player_session_for_window(session_label)?;
    if native_video::pip_is_active() && !native_video::pip_is_active_for_session(session.label()) {
        return Err("Picture in Picture is already active in another player window".to_string());
    }
    let entering = !native_video::pip_is_active_for_session(session.label());
    let (playing, title, video_size) = session
        .player()
        .lock()
        .map(|player| {
            (
                !player.paused,
                player.media_title.clone(),
                player.video_size_for_display(),
            )
        })
        .map_err(|error| error.to_string())?;
    let window = app.get_webview_window(session.label()).ok_or_else(|| {
        format!(
            "player window is not available for session {}",
            session.label()
        )
    })?;
    let origin_fullscreen = player_window_is_fullscreen(&window)?;
    native_video::register_pip_will_close_emitter(app);
    native_video::toggle_pip(
        playing,
        &title,
        session.label(),
        video_size,
        origin_fullscreen,
    )?;
    let preferences = state
        .preferences
        .lock()
        .map(|preferences| preferences.clone())
        .map_err(|error| error.to_string())?;
    let transition = {
        let mut lifecycle_by_window = state
            .player_window_lifecycle
            .lock()
            .map_err(|error| error.to_string())?;
        let lifecycle = lifecycle_by_window
            .entry(session.label().to_string())
            .or_default();
        if entering {
            let transition_allowed = !player_window_is_fullscreen(&window)?
                && !window.is_minimized().map_err(|error| error.to_string())?;
            lifecycle.begin_picture_in_picture(
                PipWindowBehavior::from(
                    integer_preference(&preferences.values, "windowBehaviorWhenPip").unwrap_or(0),
                ),
                transition_allowed,
            )
        } else {
            lifecycle.finish_picture_in_picture()
        }
    };
    apply_pip_window_transition(&window, transition)?;
    if entering && playing && bool_preference(&preferences.values, "pauseWhenPip", false) {
        session
            .player()
            .lock()
            .map_err(|error| error.to_string())?
            .apply(PlayerCommand::Pause);
        session.sync_mpv_executor_from_player()?;
    }
    player_snapshot_for_session(&session)
}

pub(crate) fn toggle_music_mode_window_for_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    session_label: &str,
) -> Result<PlayerState, String> {
    exit_pip_before_window_transition(app, state, session_label)?;
    let session = state.player_session_for_window(session_label)?;
    let is_in_mini_player = session
        .player()
        .lock()
        .map(|player| matches!(player.mode, PlayerMode::MiniPlayer))
        .map_err(|error| error.to_string())?;
    if is_in_mini_player {
        leave_music_mode_window_for_session(app, state, session_label, false)
    } else {
        enter_music_mode_window_for_session(app, state, session_label, false)
    }
}

fn automatically_sync_music_mode<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    session_label: &str,
    snapshot: PlayerState,
) -> Result<PlayerState, String> {
    let enabled = state
        .preferences
        .lock()
        .map(|preferences| bool_preference(&preferences.values, "autoSwitchToMusicMode", true))
        .map_err(|error| error.to_string())?;
    if !enabled {
        return Ok(snapshot);
    }

    match snapshot.automatic_music_mode_transition() {
        Some(AutomaticMusicModeTransition::Enter) => {
            let is_fullscreen = app
                .get_webview_window(session_label)
                .and_then(|window| window.is_fullscreen().ok())
                .unwrap_or(false);
            if is_fullscreen {
                Ok(snapshot)
            } else {
                enter_music_mode_window_for_session(app, state, session_label, true)
            }
        }
        Some(AutomaticMusicModeTransition::Leave) => {
            leave_music_mode_window_for_session(app, state, session_label, true)
        }
        None => Ok(snapshot),
    }
}

pub(crate) fn close_mini_player_window_for_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    session_label: &str,
) -> Result<PlayerState, String> {
    exit_pip_before_window_transition(app, state, session_label)?;
    let player_window = app
        .get_webview_window(session_label)
        .ok_or_else(|| format!("player window is not available for session {session_label}"))?;
    let mini_label = mini_player_label_for_session(session_label);

    #[cfg(target_os = "macos")]
    native_video::install(
        player_window.ns_view().map_err(|error| error.to_string())?,
        session_label,
        &native_video_surface_settings(state)?,
    )?;

    if let Some(mini_window) = app.get_webview_window(&mini_label) {
        mini_window.hide().map_err(|error| error.to_string())?;
    }
    state.save_playback_position_for_window(session_label)?;
    {
        let session = state.player_session_for_window(session_label)?;
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        player.reset_music_mode_switch_history();
        player.leave_mini_player(true);
        player.apply(PlayerCommand::Stop);
    }
    let session = state.player_session_for_window(session_label)?;
    session.sync_mpv_executor_from_player()?;
    let snapshot = player_snapshot_for_session(&session)?;
    emit_player_state_for_session(app, session_label, &snapshot);
    player_window.show().map_err(|error| error.to_string())?;
    player_window
        .set_focus()
        .map_err(|error| error.to_string())?;
    Ok(snapshot)
}

pub(crate) fn leave_music_mode_window_for_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    session_label: &str,
    automatically: bool,
) -> Result<PlayerState, String> {
    exit_pip_before_window_transition(app, state, session_label)?;
    let player_window = app
        .get_webview_window(session_label)
        .ok_or_else(|| format!("player window is not available for session {session_label}"))?;
    let mini_label = mini_player_label_for_session(session_label);

    #[cfg(target_os = "macos")]
    native_video::install(
        player_window.ns_view().map_err(|error| error.to_string())?,
        session_label,
        &native_video_surface_settings(state)?,
    )?;

    if let Some(mini_window) = app.get_webview_window(&mini_label) {
        mini_window.hide().map_err(|error| error.to_string())?;
    }
    player_window.show().map_err(|error| error.to_string())?;
    player_window
        .set_focus()
        .map_err(|error| error.to_string())?;

    {
        let session = state.player_session_for_window(session_label)?;
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        player.leave_mini_player(automatically);
    }
    let session = state.player_session_for_window(session_label)?;
    session.sync_mpv_executor_from_player()?;
    let snapshot = player_snapshot_for_session(&session)?;
    emit_player_state_for_session(app, session_label, &snapshot);
    Ok(snapshot)
}

fn exit_pip_before_window_transition<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    session_label: &str,
) -> Result<(), String> {
    if !native_video::pip_is_active_for_session(session_label) {
        return Ok(());
    }
    let _ = toggle_picture_in_picture_for_session(app, state, session_label)?;
    if native_video::pip_is_active_for_session(session_label) {
        return Err("Picture in Picture is closing; try Music Mode again momentarily".to_string());
    }
    Ok(())
}

fn enter_music_mode_window_for_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    session_label: &str,
    automatically: bool,
) -> Result<PlayerState, String> {
    let player_window = app
        .get_webview_window(session_label)
        .ok_or_else(|| format!("player window is not available for session {session_label}"))?;
    let mini_label = mini_player_label_for_session(session_label);
    let (show_album_art, show_playlist, always_on_top) = state
        .preferences
        .lock()
        .map(|preferences| {
            (
                bool_preference(&preferences.values, "musicModeShowAlbumArt", true),
                bool_preference(&preferences.values, "musicModeShowPlaylist", false),
                bool_preference(&preferences.values, "alwaysFloatOnTop", false),
            )
        })
        .map_err(|error| error.to_string())?;
    let initial_layout = mini_player_layout(
        MINI_PLAYER_INITIAL_WIDTH,
        show_album_art,
        show_playlist,
        1.0,
    );
    let mini_window = match app.get_webview_window(&mini_label) {
        Some(window) => window,
        None => {
            let builder = WebviewWindowBuilder::new(
                app,
                &mini_label,
                WebviewUrl::App(format!("index.html?mini-player={session_label}").into()),
            )
            .title("IINA")
            .inner_size(initial_layout.width, initial_layout.height)
            .min_inner_size(MINI_PLAYER_INITIAL_WIDTH, MINI_PLAYER_CONTROL_HEIGHT)
            .resizable(true)
            .transparent(true)
            .decorations(true);
            #[cfg(target_os = "macos")]
            let builder = builder
                .title_bar_style(tauri::TitleBarStyle::Overlay)
                .hidden_title(true);
            builder
                .always_on_top(always_on_top)
                .build()
                .map_err(|error| error.to_string())?
        }
    };
    mini_window
        .set_always_on_top(always_on_top)
        .map_err(|error| error.to_string())?;

    #[cfg(target_os = "macos")]
    native_video::configure_mini_player_window(
        mini_window.ns_window().map_err(|error| error.to_string())?,
    )?;

    #[cfg(target_os = "macos")]
    native_window_behavior::install_mini_player_layout_observer(
        app,
        mini_window.ns_window().map_err(|error| error.to_string())?,
        &mini_label,
    )?;

    #[cfg(target_os = "macos")]
    native_video::install(
        mini_window.ns_view().map_err(|error| error.to_string())?,
        session_label,
        &native_video_surface_settings(state)?,
    )?;

    #[cfg(target_os = "macos")]
    native_window_behavior::install_player_input_monitor(
        app,
        mini_window.ns_window().map_err(|error| error.to_string())?,
        &mini_label,
    )?;

    #[cfg(target_os = "macos")]
    crate::native_touch_bar::install_window(&mini_window, state, session_label)?;

    {
        let session = state.player_session_for_window(session_label)?;
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        player.enter_mini_player(automatically);
    }
    let session = state.player_session_for_window(session_label)?;
    session.sync_mpv_executor_from_player()?;
    mini_window.show().map_err(|error| error.to_string())?;
    mini_window.set_focus().map_err(|error| error.to_string())?;
    player_window.hide().map_err(|error| error.to_string())?;

    let snapshot = player_snapshot_for_session(&session)?;
    emit_player_state_for_session(app, session_label, &snapshot);
    Ok(snapshot)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MiniPlayerLayout {
    width: f64,
    height: f64,
    video_height: f64,
    playlist_height: f64,
    playlist_visible: bool,
}

fn mini_player_layout(
    width: f64,
    video_visible: bool,
    playlist_visible: bool,
    video_aspect: f64,
) -> MiniPlayerLayout {
    let width = width.max(MINI_PLAYER_INITIAL_WIDTH);
    let aspect = if video_aspect.is_finite() && video_aspect > 0.0 {
        video_aspect.clamp(0.05, 20.0)
    } else {
        1.0
    };
    let video_height = if video_visible {
        (width / aspect).round()
    } else {
        0.0
    };
    let playlist_height = if playlist_visible {
        MINI_PLAYER_PLAYLIST_HEIGHT
    } else {
        0.0
    };
    MiniPlayerLayout {
        width,
        height: video_height + MINI_PLAYER_CONTROL_HEIGHT + playlist_height,
        video_height,
        playlist_height,
        playlist_visible,
    }
}

#[tauri::command]
pub fn set_mini_player_layout(
    window: WebviewWindow,
    video_visible: bool,
    playlist_visible: bool,
    video_aspect: f64,
) -> Result<MiniPlayerLayout, String> {
    if mini_player_session_label(window.label()).is_none() {
        return Err("Mini Player layout can only be applied to a Mini Player window".to_string());
    }
    window
        .set_min_size(Some(LogicalSize::new(
            MINI_PLAYER_INITIAL_WIDTH,
            MINI_PLAYER_CONTROL_HEIGHT,
        )))
        .map_err(|error| error.to_string())?;

    #[cfg(target_os = "macos")]
    {
        let layout = native_window_behavior::apply_mini_player_layout(
            window.ns_window().map_err(|error| error.to_string())?,
            video_visible,
            playlist_visible,
            video_aspect,
        )?;
        return Ok(MiniPlayerLayout {
            width: layout.width,
            height: layout.height,
            video_height: layout.video_height,
            playlist_height: layout.playlist_height,
            playlist_visible: layout.playlist_visible,
        });
    }

    #[cfg(not(target_os = "macos"))]
    {
        let scale = window.scale_factor().map_err(|error| error.to_string())?;
        let current_size = window.inner_size().map_err(|error| error.to_string())?;
        let current_width = f64::from(current_size.width) / scale;
        let layout =
            mini_player_layout(current_width, video_visible, playlist_visible, video_aspect);
        let current_height = f64::from(current_size.height) / scale;
        if (current_width - layout.width).abs() > 0.5
            || (current_height - layout.height).abs() > 0.5
        {
            window
                .set_size(LogicalSize::new(layout.width, layout.height))
                .map_err(|error| error.to_string())?;
        }
        Ok(layout)
    }
}

fn sync_player_window_always_on_top<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    session_label: &str,
    snapshot: &PlayerState,
) -> Result<(), String> {
    let Some(window) = app.get_webview_window(session_label) else {
        return Ok(());
    };
    let enabled = state
        .preferences
        .lock()
        .map(|preferences| bool_preference(&preferences.values, "alwaysFloatOnTop", false))
        .map_err(|error| error.to_string())?;
    let fullscreen = player_window_is_fullscreen(&window)?;
    let directive = state
        .player_window_lifecycle
        .lock()
        .map_err(|error| error.to_string())?
        .entry(session_label.to_string())
        .or_default()
        .observe_always_float_on_top(
            enabled,
            snapshot.current_url.is_some() && !snapshot.paused,
            fullscreen,
        );
    if let Some(always_on_top) = directive {
        window
            .set_always_on_top(always_on_top)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn emit_player_state_for_session<R: Runtime>(
    app: &AppHandle<R>,
    session_label: &str,
    snapshot: &PlayerState,
) {
    let mut is_last_active_session = false;
    if let Some(state) = app.try_state::<AppState>() {
        let _ = sync_player_window_surface(app, state.inner(), session_label, snapshot);
        let _ = sync_player_window_always_on_top(app, state.inner(), session_label, snapshot);
        let _ = crate::native_system_media::sync(app, state.inner());
        let _ = crate::native_touch_bar::sync_session(app, state.inner(), session_label, snapshot);
        is_last_active_session = state
            .last_active_player_session_label()
            .is_ok_and(|label| label == session_label);
    }
    let _ = app.emit_to(session_label, PLAYER_STATE_EVENT, snapshot);
    let mini_label = mini_player_label_for_session(session_label);
    if app.get_webview_window(&mini_label).is_some() {
        let _ = app.emit_to(&mini_label, PLAYER_STATE_EVENT, snapshot);
    }
    if is_last_active_session {
        for label in [
            crate::auxiliary_player_windows::VIDEO_FILTER_WINDOW_LABEL,
            crate::auxiliary_player_windows::AUDIO_FILTER_WINDOW_LABEL,
        ] {
            if app.get_webview_window(label).is_some() {
                let _ = app.emit_to(label, PLAYER_STATE_EVENT, snapshot);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct WindowPresentation {
    width: f64,
    height: f64,
    min_width: f64,
    min_height: f64,
    resizable: bool,
}

fn window_presentation(mode: &str) -> Result<WindowPresentation, String> {
    match mode {
        "initial" => Ok(WindowPresentation {
            width: 640.0,
            height: 400.0,
            min_width: 640.0,
            min_height: 400.0,
            resizable: false,
        }),
        "player" => Ok(WindowPresentation {
            // MainWindowController.xib starts at 640x400. Once media metadata is available,
            // apply_open_window_preferences owns the actual video-sized frame.
            width: 640.0,
            height: 400.0,
            min_width: 285.0,
            min_height: 120.0,
            resizable: true,
        }),
        value => Err(format!("unsupported window presentation mode: {value}")),
    }
}

#[tauri::command]
pub fn set_window_presentation_mode(window: WebviewWindow, mode: String) -> Result<String, String> {
    apply_window_presentation_mode(&window, &mode)?;
    Ok(mode)
}

fn apply_window_presentation_mode<R: Runtime>(
    window: &WebviewWindow<R>,
    mode: &str,
) -> Result<(), String> {
    let presentation = window_presentation(&mode)?;
    if !is_player_window_label(window.label()) {
        return Ok(());
    }
    let managed_by_plugin = is_plugin_managed_player_window(window);
    let plugin_disables_ui = is_plugin_disable_ui_player_window(window);
    if mode == "initial" && managed_by_plugin {
        return Ok(());
    }

    if mode == "initial" && window.is_fullscreen().map_err(|error| error.to_string())? {
        window
            .set_fullscreen(false)
            .map_err(|error| error.to_string())?;
    }
    window
        .set_max_size(None::<LogicalSize<f64>>)
        .map_err(|error| error.to_string())?;
    window
        .set_min_size(Some(LogicalSize::new(
            presentation.min_width,
            presentation.min_height,
        )))
        .map_err(|error| error.to_string())?;
    if !presentation.resizable {
        window
            .set_size(LogicalSize::new(presentation.width, presentation.height))
            .map_err(|error| error.to_string())?;
        window
            .set_max_size(Some(LogicalSize::new(
                presentation.width,
                presentation.height,
            )))
            .map_err(|error| error.to_string())?;
    }
    window
        .set_resizable(presentation.resizable)
        .map_err(|error| error.to_string())?;
    #[cfg(target_os = "macos")]
    let restored_initial_frame = if plugin_disables_ui {
        // disableUI is represented by a genuinely undecorated plugin-managed window. The native
        // retained-pair bridge must not add AppKit title controls back to that private core.
        false
    } else {
        native_window_behavior::configure_player_presentation(
            window.ns_window().map_err(|error| error.to_string())?,
            mode == "initial",
        )?
    };
    #[cfg(not(target_os = "macos"))]
    let restored_initial_frame = false;
    if mode == "initial" && !restored_initial_frame {
        window.center().map_err(|error| error.to_string())?;
        #[cfg(target_os = "macos")]
        native_video::center_window_after_delay(
            window.ns_window().map_err(|error| error.to_string())?,
            120,
        )?;
        #[cfg(not(target_os = "macos"))]
        {
            let delayed_center_window = window.clone();
            std::thread::Builder::new()
                .name("iina-window-presentation-center".to_string())
                .spawn(move || {
                    std::thread::sleep(Duration::from_millis(120));
                    let _ = delayed_center_window.center();
                })
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn player_state_uses_media_window(snapshot: &PlayerState) -> bool {
    snapshot.current_url.is_some()
        || snapshot.file_loading
        || !matches!(snapshot.mode, PlayerMode::Initial)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PlayerWindowTitlePlan<'a> {
    RepresentedFile(std::borrow::Cow<'a, str>),
    Plain(&'a str),
}

fn represented_player_file_path(current_url: &str) -> Option<std::borrow::Cow<'_, str>> {
    if Path::new(current_url).is_absolute() {
        return Some(std::borrow::Cow::Borrowed(current_url));
    }
    let url = Url::parse(current_url).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    let path = url.to_file_path().ok()?;
    path.is_absolute()
        .then(|| std::borrow::Cow::Owned(path.to_string_lossy().into_owned()))
}

fn player_window_title_plan(snapshot: &PlayerState) -> PlayerWindowTitlePlan<'_> {
    match snapshot.current_url.as_deref() {
        Some(path) => represented_player_file_path(path)
            .map(PlayerWindowTitlePlan::RepresentedFile)
            .unwrap_or(PlayerWindowTitlePlan::Plain(&snapshot.media_title)),
        None => PlayerWindowTitlePlan::Plain("IINA"),
    }
}

fn sync_player_window_title<R: Runtime>(
    window: &WebviewWindow<R>,
    snapshot: &PlayerState,
) -> Result<(), String> {
    if !is_player_window_label(window.label()) || is_plugin_disable_ui_player_window(window) {
        return Ok(());
    }
    let (represented_path, plain_title) = match player_window_title_plan(snapshot) {
        PlayerWindowTitlePlan::RepresentedFile(path) => (Some(path), snapshot.media_title.as_str()),
        PlayerWindowTitlePlan::Plain(title) => (None, title),
    };
    #[cfg(target_os = "macos")]
    native_window_behavior::sync_player_window_title(
        window.ns_window().map_err(|error| error.to_string())?,
        represented_path.as_deref(),
        plain_title,
    )?;
    #[cfg(not(target_os = "macos"))]
    let _ = (represented_path, plain_title);
    Ok(())
}

fn prepare_initial_player_window(state: &AppState, session_label: &str) -> Result<(), String> {
    state
        .player_window_lifecycle
        .lock()
        .map_err(|error| error.to_string())?
        .entry(session_label.to_string())
        .or_default()
        .prepare_initial_window();
    Ok(())
}

fn observe_player_window_surface(
    state: &AppState,
    session_label: &str,
    snapshot: &PlayerState,
) -> Result<bool, String> {
    let should_hide = state
        .player_window_lifecycle
        .lock()
        .map_err(|error| error.to_string())?
        .entry(session_label.to_string())
        .or_default()
        .observe_media_window(player_state_uses_media_window(snapshot));
    Ok(should_hide)
}

fn sync_player_window_surface<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    session_label: &str,
    snapshot: &PlayerState,
) -> Result<(), String> {
    let window = app.get_webview_window(session_label);
    if let Some(window) = window.as_ref() {
        sync_player_window_title(window, snapshot)?;
    }
    if !observe_player_window_surface(state, session_label, snapshot)? {
        return Ok(());
    }
    let Some(window) = window else {
        return Ok(());
    };
    // Hide before replacing the emulated Main surface with Initial; otherwise the user sees a
    // one-frame welcome-window flash at stop/EOF, which the retained AppKit pair never produces.
    window.hide().map_err(|error| error.to_string())?;
    apply_window_presentation_mode(&window, "initial")
}

fn show_initial_player_window<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    session_label: &str,
) -> Result<PlayerState, String> {
    let session = state.player_session_for_window(session_label)?;
    let snapshot = player_snapshot_for_session(&session)?;
    if player_state_uses_media_window(&snapshot) {
        return Err(format!(
            "player session {session_label} is not idle and cannot present its initial window"
        ));
    }
    let window = app
        .get_webview_window(session_label)
        .ok_or_else(|| format!("player window is not available for session {session_label}"))?;
    prepare_initial_player_window(state, session_label)?;
    apply_window_presentation_mode(&window, "initial")?;
    sync_player_window_surface(app, state, session_label, &snapshot)?;
    let _ = window.emit(PLAYER_STATE_EVENT, &snapshot);
    window.unminimize().map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    state.note_player_session_active(session_label)?;
    Ok(snapshot)
}

fn is_player_window_label(label: &str) -> bool {
    label == "main" || label.starts_with("player-")
}

/// Returns the concrete main player window that owns a shortcut invocation.
/// Mini Player input resolves to its own session's main window for window-level actions, while
/// utility windows resolve to the main window of `lastActive`. Player-ish labels remain strict so
/// a stale secondary or Mini Player can never cross-route into another session.
pub(crate) fn shortcut_player_window<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    invoking_window: &WebviewWindow<R>,
) -> Result<WebviewWindow<R>, String> {
    let session_label = state.shortcut_player_session_label(invoking_window.label())?;
    if is_player_window_label(invoking_window.label()) {
        return Ok(invoking_window.clone());
    }
    app.get_webview_window(&session_label)
        .ok_or_else(|| format!("player window is not available for session {session_label}"))
}

fn should_quit_after_last_window_closes(
    preference_enabled: bool,
    closing_window_visible: bool,
    another_window_visible: bool,
) -> bool {
    preference_enabled && closing_window_visible && !another_window_visible
}

pub(crate) fn close_retained_player_window<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    session_label: &str,
) -> Result<PlayerState, String> {
    if native_video::pip_is_active_for_session(session_label) {
        let _ = toggle_picture_in_picture_for_session(app, state, session_label);
    }
    if let Some(mini_window) = app.get_webview_window(&mini_player_label_for_session(session_label))
    {
        let _ = mini_window.hide();
    }
    state.save_playback_position_for_window(session_label)?;
    let session = state.player_session_for_window(session_label)?;
    let should_stop = session
        .player()
        .lock()
        .map(|player| player.current_url.is_some())
        .map_err(|error| error.to_string())?;
    if should_stop {
        session
            .player()
            .lock()
            .map_err(|error| error.to_string())?
            .apply(PlayerCommand::Stop);
        session.sync_mpv_executor_from_player()?;
    }
    let snapshot = player_snapshot_for_session(&session)?;
    if let Some(window) = app.get_webview_window(session_label) {
        sync_player_window_title(&window, &snapshot)?;
    }
    observe_player_window_surface(state, session_label, &snapshot)?;

    if let Some(window) = app.get_webview_window(session_label) {
        if window.is_fullscreen().map_err(|error| error.to_string())? {
            window
                .set_fullscreen(false)
                .map_err(|error| error.to_string())?;
        }
        #[cfg(target_os = "macos")]
        if let Ok(native_window) = window.ns_window() {
            native_window_behavior::prepare_player_window_close(native_window);
            let _ = native_window_behavior::set_blackout(native_window, false);
        }
        let _ = window.set_always_on_top(false);
        window.hide().map_err(|error| error.to_string())?;
    }
    Ok(snapshot)
}

pub(crate) fn should_quit_after_closing_window<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    closing_label: &str,
) -> Result<bool, String> {
    if !is_player_window_label(closing_label) {
        return Ok(false);
    }
    let preference_enabled = state
        .preferences
        .lock()
        .map(|preferences| bool_preference(&preferences.values, "quitWhenNoOpenedWindow", false))
        .map_err(|error| error.to_string())?;
    let closing_window_visible = app
        .get_webview_window(closing_label)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false);
    let another_window_visible = app
        .webview_windows()
        .iter()
        .any(|(label, window)| label != closing_label && window.is_visible().unwrap_or(false));
    Ok(should_quit_after_last_window_closes(
        preference_enabled,
        closing_window_visible,
        another_window_visible,
    ))
}

fn use_legacy_fullscreen_preference(state: &AppState) -> Result<bool, String> {
    state
        .preferences
        .lock()
        .map(|preferences| bool_preference(&preferences.values, "useLegacyFullScreen", false))
        .map_err(|error| error.to_string())
}

fn legacy_fullscreen_animation_preference(state: &AppState) -> Result<bool, String> {
    state
        .preferences
        .lock()
        .map(|preferences| bool_preference(&preferences.values, "legacyFullScreenAnimation", false))
        .map_err(|error| error.to_string())
}

pub(crate) fn player_window_is_fullscreen<R: Runtime>(
    window: &tauri::WebviewWindow<R>,
) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    if native_window_behavior::is_legacy_fullscreen(
        window.ns_window().map_err(|error| error.to_string())?,
    ) {
        return Ok(true);
    }
    window.is_fullscreen().map_err(|error| error.to_string())
}

pub(crate) fn configure_player_window_behavior<R: Runtime>(
    state: &AppState,
    window: &tauri::WebviewWindow<R>,
) -> Result<(), String> {
    if !is_player_window_label(window.label()) {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        let native_window = window.ns_window().map_err(|error| error.to_string())?;
        native_window_behavior::configure_fullscreen_mode(
            native_window,
            use_legacy_fullscreen_preference(state)?,
        )?;
        let theme = state
            .preferences
            .lock()
            .map(|preferences| {
                integer_preference(&preferences.values, "themeMaterial").unwrap_or(0)
            })
            .map_err(|error| error.to_string())?;
        native_window_behavior::set_window_theme(native_window, theme)?;
        crate::native_touch_bar::install_window(window, state, window.label())?;
    }
    Ok(())
}

fn apply_playback_directive(
    player: &mut PlayerState,
    directive: Option<PlaybackDirective>,
    playing: &mut bool,
) -> bool {
    if player.current_url.is_none() {
        return false;
    }
    match directive {
        Some(PlaybackDirective::Pause) if *playing => {
            player.apply(PlayerCommand::Pause);
            *playing = false;
            true
        }
        Some(PlaybackDirective::Resume) if !*playing => {
            player.apply(PlayerCommand::Resume);
            *playing = true;
            true
        }
        _ => false,
    }
}

pub(crate) fn observe_player_window_lifecycle<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    label: &str,
    focused_event: Option<bool>,
) -> Result<(), String> {
    if !is_player_window_label(label) {
        return Ok(());
    }
    let Some(window) = app.get_webview_window(label) else {
        return Ok(());
    };
    let minimized = window.is_minimized().map_err(|error| error.to_string())?;
    let fullscreen = player_window_is_fullscreen(&window)?;
    let focused = match focused_event {
        Some(focused) => focused,
        None => window.is_focused().map_err(|error| error.to_string())?,
    };
    let screen_key = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .map(|monitor| {
            format!(
                "{}:{}:{}:{}:{}:{}",
                monitor.name().map(String::as_str).unwrap_or(""),
                monitor.position().x,
                monitor.position().y,
                monitor.size().width,
                monitor.size().height,
                monitor.scale_factor(),
            )
        });
    let another_player_focused = app.webview_windows().iter().any(|(other_label, other)| {
        other_label != label
            && (is_player_window_label(other_label) || other_label.starts_with("mini-player"))
            && other.is_focused().unwrap_or(false)
    });
    let application_has_key_window = native_window_behavior::application_is_active();
    let should_pause_when_unfocused = !application_has_key_window || another_player_focused;
    let player_window_is_main = focused || (application_has_key_window && !another_player_focused);
    let preferences = state
        .preferences
        .lock()
        .map(|preferences| preferences.clone())
        .map_err(|error| error.to_string())?;
    let session = state.player_session_for_window(label)?;
    let pip_active = native_video::pip_is_active_for_session(label);
    let (changed, pip_toggle, pip_cleanup, minimized_event, screen_changed) = {
        let mut lifecycle_by_window = state
            .player_window_lifecycle
            .lock()
            .map_err(|error| error.to_string())?;
        let lifecycle = lifecycle_by_window.entry(label.to_string()).or_default();
        let minimized_event = (lifecycle.minimized != minimized).then_some(minimized);
        let screen_changed = lifecycle.observe_screen(screen_key);
        let mut player = session.player().lock().map_err(|error| error.to_string())?;
        let mut playing = player.current_url.is_some() && !player.paused;
        let mut changed = false;
        let directive = lifecycle.observe_minimized(
            minimized,
            bool_preference(&preferences.values, "pauseWhenMinimized", false),
            playing,
        );
        changed |= apply_playback_directive(&mut player, directive, &mut playing);
        let directive = lifecycle.observe_fullscreen(
            fullscreen,
            bool_preference(&preferences.values, "playWhenEnteringFullScreen", false),
            bool_preference(&preferences.values, "pauseWhenLeavingFullScreen", false),
            playing,
        );
        changed |= apply_playback_directive(&mut player, directive, &mut playing);
        let directive = lifecycle.observe_focus(
            focused,
            bool_preference(&preferences.values, "pauseWhenInactive", false),
            should_pause_when_unfocused && !minimized,
            playing,
        );
        changed |= apply_playback_directive(&mut player, directive, &mut playing);
        let pip_cleanup = if pip_active {
            PipWindowTransition::default()
        } else {
            lifecycle.finish_picture_in_picture()
        };
        let pip_toggle = if pip_cleanup == PipWindowTransition::default() {
            lifecycle.pip_toggle_for_minimized(
                minimized,
                bool_preference(&preferences.values, "togglePipByMinimizingWindow", false),
                pip_active,
            )
        } else {
            None
        };
        (
            changed,
            pip_toggle,
            pip_cleanup,
            minimized_event,
            screen_changed,
        )
    };

    apply_pip_window_transition(&window, pip_cleanup)?;

    #[cfg(target_os = "macos")]
    native_window_behavior::set_blackout(
        window.ns_window().map_err(|error| error.to_string())?,
        fullscreen
            && player_window_is_main
            && bool_preference(&preferences.values, "blackOutMonitor", false),
    )?;

    if changed {
        session.sync_mpv_executor_from_player()?;
        let snapshot = player_snapshot_for_session(&session)?;
        emit_player_state_for_session(app, session.label(), &snapshot);
    }
    if matches!(
        pip_toggle,
        Some(PipToggleDirective::Enter | PipToggleDirective::Exit)
    ) {
        let snapshot = toggle_picture_in_picture_for_session(app, state, label)?;
        emit_player_state_for_session(app, session.label(), &snapshot);
    }
    emit_player_window_status(app, &window);
    if let Some(minimized) = minimized_event {
        emit_plugin_host_event(
            app,
            label,
            if minimized {
                "iina.window-miniaturized"
            } else {
                "iina.window-deminiaturized"
            },
            serde_json::json!([]),
        );
    }
    if screen_changed {
        emit_plugin_host_event(
            app,
            label,
            "iina.window-screen.changed",
            serde_json::json!([]),
        );
    }
    Ok(())
}

pub(crate) fn remove_player_window_lifecycle<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    label: &str,
) {
    if let Some(window) = app.get_webview_window(label) {
        #[cfg(target_os = "macos")]
        if let Ok(native_window) = window.ns_window() {
            native_window_behavior::prepare_player_window_close(native_window);
            let _ = native_window_behavior::set_blackout(native_window, false);
        }
    }
    if let Ok(mut lifecycle) = state.player_window_lifecycle.lock() {
        lifecycle.remove(label);
    }
}

pub(crate) fn pause_all_players_for_sleep<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let state = app.state::<AppState>();
    let enabled = state
        .preferences
        .lock()
        .map(|preferences| bool_preference(&preferences.values, "pauseWhenGoesToSleep", true))
        .map_err(|error| error.to_string())?;
    if !enabled {
        return Ok(());
    }
    let labels = std::iter::once("main".to_string())
        .chain(state.player_session_labels()?)
        .collect::<Vec<_>>();
    for label in labels {
        let session = state.player_session_for_window(&label)?;
        let changed = session
            .player()
            .lock()
            .map(|mut player| {
                if player.current_url.is_some() && !player.paused {
                    player.apply(PlayerCommand::Pause);
                    true
                } else {
                    false
                }
            })
            .map_err(|error| error.to_string())?;
        if changed {
            session.sync_mpv_executor_from_player()?;
            let snapshot = player_snapshot_for_session(&session)?;
            emit_player_state_for_session(app, session.label(), &snapshot);
        }
    }
    Ok(())
}

pub(crate) fn set_player_window_fullscreen<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    window: &tauri::WebviewWindow<R>,
    fullscreen: bool,
) -> Result<bool, String> {
    if !is_player_window_label(window.label()) {
        return Err("Full screen is available only in a player window".to_string());
    }
    if player_window_is_fullscreen(window)? == fullscreen {
        emit_player_window_status(app, window);
        let session = state.player_session_for_window(window.label())?;
        let snapshot = session
            .player()
            .lock()
            .map(|player| player.clone())
            .map_err(|error| error.to_string())?;
        sync_player_window_title(window, &snapshot)?;
        crate::native_touch_bar::sync_session(app, state, session.label(), &snapshot)?;
        return Ok(fullscreen);
    }

    #[cfg(target_os = "macos")]
    let legacy_active = native_window_behavior::is_legacy_fullscreen(
        window.ns_window().map_err(|error| error.to_string())?,
    );
    #[cfg(not(target_os = "macos"))]
    let legacy_active = false;

    if fullscreen {
        let use_legacy = use_legacy_fullscreen_preference(state)?;
        configure_player_window_behavior(state, window)?;
        #[cfg(target_os = "macos")]
        if use_legacy {
            native_window_behavior::set_legacy_fullscreen(
                window.ns_window().map_err(|error| error.to_string())?,
                true,
                false,
                None,
            )?;
            observe_player_window_lifecycle(app, state, window.label(), Some(true))?;
        } else {
            window
                .set_fullscreen(true)
                .map_err(|error| error.to_string())?;
        }
        #[cfg(not(target_os = "macos"))]
        window
            .set_fullscreen(true)
            .map_err(|error| error.to_string())?;
    } else if legacy_active {
        #[cfg(target_os = "macos")]
        {
            let video_size = state
                .player_session_for_window(window.label())?
                .player()
                .lock()
                .map(|player| player.video_size_for_display())
                .map_err(|error| error.to_string())?;
            native_window_behavior::set_legacy_fullscreen(
                window.ns_window().map_err(|error| error.to_string())?,
                false,
                legacy_fullscreen_animation_preference(state)?,
                video_size,
            )?;
            observe_player_window_lifecycle(app, state, window.label(), Some(true))?;
        }
    } else {
        window
            .set_fullscreen(false)
            .map_err(|error| error.to_string())?;
    }
    let _ = menu::refresh_iina_menu(app);
    emit_player_window_status(app, window);
    let session = state.player_session_for_window(window.label())?;
    let snapshot = session
        .player()
        .lock()
        .map(|player| player.clone())
        .map_err(|error| error.to_string())?;
    // Re-evaluate the native fingerprint immediately: legacy fullscreen removes/restores
    // NSWindowStyleMaskTitled, which selects the safe lastPathComponent fallback.
    sync_player_window_title(window, &snapshot)?;
    crate::native_touch_bar::sync_session(app, state, session.label(), &snapshot)?;
    Ok(fullscreen)
}

pub(crate) fn refresh_native_general_preference<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    key: &str,
) -> Result<(), String> {
    if !matches!(
        key,
        "useLegacyFullScreen" | "blackOutMonitor" | "alwaysFloatOnTop" | "themeMaterial"
    ) {
        return Ok(());
    }
    let labels = std::iter::once("main".to_string())
        .chain(state.player_session_labels()?)
        .collect::<Vec<_>>();
    for label in labels {
        let Some(window) = app.get_webview_window(&label) else {
            continue;
        };
        if matches!(key, "useLegacyFullScreen" | "themeMaterial") {
            configure_player_window_behavior(state, &window)?;
        }
        if key == "alwaysFloatOnTop" {
            let session = state.player_session_for_window(&label)?;
            let snapshot = player_snapshot_for_session(&session)?;
            sync_player_window_always_on_top(app, state, &label, &snapshot)?;
        }
        observe_player_window_lifecycle(app, state, &label, None)?;
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_window_always_on_top(window: WebviewWindow) -> Result<bool, String> {
    if !is_player_window_label(window.label()) {
        return Err("Float on top is available only in a player window".to_string());
    }
    let always_on_top = !window
        .is_always_on_top()
        .map_err(|error| error.to_string())?;
    window
        .set_always_on_top(always_on_top)
        .map_err(|error| error.to_string())?;
    Ok(always_on_top)
}

#[tauri::command]
pub fn toggle_window_fullscreen(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<bool, String> {
    let target = shortcut_player_window(&app, state.inner(), &window)?;
    let fullscreen = !player_window_is_fullscreen(&target)?;
    set_player_window_fullscreen(&app, state.inner(), &target, fullscreen)
}

#[tauri::command]
pub fn resize_player_window_by_magnification(
    state: tauri::State<AppState>,
    window: WebviewWindow,
    magnification: f64,
) -> Result<crate::window_size::WindowResizeResult, String> {
    state.player_session_for_window(window.label())?;
    crate::window_size::resize_player_window_by_magnification(&window, magnification)
}

#[tauri::command]
pub fn start_player_window_drag(window: WebviewWindow) -> Result<(), String> {
    if !is_player_window_label(window.label()) {
        return Err("Window dragging is available only in a player window".to_string());
    }
    if player_window_is_fullscreen(&window)? {
        return Ok(());
    }
    window.start_dragging().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_window_fullscreen(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    fullscreen: bool,
) -> Result<bool, String> {
    let target = shortcut_player_window(&app, state.inner(), &window)?;
    set_player_window_fullscreen(&app, state.inner(), &target, fullscreen)
}

#[cfg(test)]
mod tests {
    use super::{
        abandon_rejected_opensubtitles_session, browser_extension_url, collect_plugin_file_entries,
        execute_playlist_trash_plan, format_localized_string_arguments,
        format_video_time_with_precision, http_auth_key_from_url, mini_player_label_for_session,
        mini_player_layout, mini_player_session_label, online_subtitle_preferences_for_request,
        open_media_batch_with_plan, open_url_submission_route, parse_video_time,
        percent_encode_query_component, playback_window_resize_action,
        player_session_creation_index, player_window_title_plan, player_window_url_disables_ui,
        player_window_url_is_plugin_managed, plugin_download_temporary_path,
        plugin_file_handle_bytes, plugin_http_body, plugin_http_method, plugin_http_reason,
        plugin_http_response, plugin_http_response_is_ok, plugin_keychain_service,
        plugin_track_file_path, read_plugin_file_handle_to_end, recreate_directory,
        remove_file_if_present, sanitized_config_filename, screenshot_options_from_preferences,
        select_reusable_idle_player_label, serialize_m3u8_playlist, service_open_url_route,
        service_player_controller_session_label, should_note_recent_document,
        should_open_in_new_player, should_quit_after_last_window_closes,
        should_record_recent_media_path, split_plugin_http_write_out, validate_plugin_http_header,
        validate_updater_preference, window_presentation, write_atomic_playlist,
        write_plugin_text_atomically, PlayerWindowTitlePlan, PluginFileHandleMode,
        PluginOpenFileHandle, RecentDocumentSource,
    };
    use crate::mpv::{mpv_command, set_property, MpvFormat};
    use crate::online_subtitles::OpenSubtitlesSession;
    use crate::player::{PlayerState, Track, TrackMetadata};
    use crate::playlist_actions::{IndexedPlaylistPath, PlaylistAutoAddPlan};
    use crate::preferences::{PreferenceChange, PreferenceStore};
    use crate::state::AppState;
    use crate::window_lifecycle::WindowResizeDirective;
    use crate::window_size::PlaybackWindowResizeAction;
    use serde_json::json;
    use std::cell::RefCell;
    use tauri::Url;

    #[test]
    fn command_n_reuses_the_first_non_loading_iina_owned_idle_player() {
        let mut creation_order = vec!["player-10", "player-2", "player-1"];
        creation_order.sort_by_key(|label| player_session_creation_index(label).unwrap());
        assert_eq!(creation_order, ["player-1", "player-2", "player-10"]);

        let selected = select_reusable_idle_player_label([
            ("main".to_string(), false, false),
            ("player-1".to_string(), true, true),
            ("player-2".to_string(), true, false),
            ("player-3".to_string(), true, false),
        ]);
        assert_eq!(selected.as_deref(), Some("player-2"));
        assert_eq!(
            select_reusable_idle_player_label([
                ("main".to_string(), false, false),
                ("player-1".to_string(), true, true),
            ]),
            None
        );

        assert!(player_window_url_is_plugin_managed(
            &Url::parse(
                "tauri://localhost/index.html?player-session=player-1&plugin-managed=io.test"
            )
            .unwrap()
        ));
        assert!(!player_window_url_is_plugin_managed(
            &Url::parse("tauri://localhost/index.html?player-session=player-2").unwrap()
        ));
    }

    #[test]
    fn player_window_title_tracks_local_network_and_idle_snapshots() {
        let mut snapshot = PlayerState::default();
        snapshot.current_url = Some("/tmp/Local Movie.mkv".to_string());
        snapshot.media_title = "Embedded title".to_string();
        assert_eq!(
            player_window_title_plan(&snapshot),
            PlayerWindowTitlePlan::RepresentedFile(std::borrow::Cow::Borrowed(
                "/tmp/Local Movie.mkv"
            ))
        );

        snapshot.current_url = Some("file:///tmp/Encoded%20Movie.mkv".to_string());
        assert_eq!(
            player_window_title_plan(&snapshot),
            PlayerWindowTitlePlan::RepresentedFile(std::borrow::Cow::Owned(
                "/tmp/Encoded Movie.mkv".to_string()
            ))
        );

        snapshot.current_url = Some("https://example.test/live".to_string());
        snapshot.media_title = "Live title".to_string();
        assert_eq!(
            player_window_title_plan(&snapshot),
            PlayerWindowTitlePlan::Plain("Live title")
        );
        snapshot.media_title = "Updated live title".to_string();
        assert_eq!(
            player_window_title_plan(&snapshot),
            PlayerWindowTitlePlan::Plain("Updated live title")
        );

        snapshot.current_url = None;
        snapshot.media_title = "Stale title".to_string();
        assert_eq!(
            player_window_title_plan(&snapshot),
            PlayerWindowTitlePlan::Plain("IINA")
        );
    }

    #[test]
    fn plugin_disable_ui_windows_do_not_receive_native_title_chrome() {
        assert!(player_window_url_disables_ui(
            &Url::parse("tauri://localhost/index.html?player-session=player-1&plugin-disable-ui=1")
                .unwrap()
        ));
        assert!(!player_window_url_disables_ui(
            &Url::parse("tauri://localhost/index.html?player-session=player-1&plugin-disable-ui=0")
                .unwrap()
        ));
        assert!(!player_window_url_disables_ui(
            &Url::parse("tauri://localhost/index.html?player-session=player-2").unwrap()
        ));

        let source = include_str!("commands.rs");
        assert!(
            source.contains("let plugin_disables_ui = is_plugin_disable_ui_player_window(window);")
        );
        assert!(source.contains("restored_initial_frame = if plugin_disables_ui"));
        let legacy_all_plugin_skip = ["restored_initial_frame = if ", "managed_by_plugin"].concat();
        assert!(!source.contains(&legacy_all_plugin_skip));
    }

    #[test]
    fn native_title_sync_stays_on_the_authoritative_owning_session_surface() {
        let source = include_str!("commands.rs");
        for contract in [
            "let window = app.get_webview_window(session_label);",
            "sync_player_window_title(window, snapshot)?;",
            "sync_player_window_surface(app, state.inner(), session_label, snapshot);",
            "sync_player_window_surface(&app, state.inner(), &target_label, &snapshot)?;",
            "is_plugin_disable_ui_player_window(window)",
        ] {
            assert!(
                source.contains(contract),
                "missing title sync contract: {contract}"
            );
        }
    }

    #[test]
    fn player_open_paths_make_native_surface_visible_before_consuming_loadfile_log() {
        let source = include_str!("commands.rs");
        for (start_marker, end_marker) in [
            (
                "pub(crate) fn open_media_paths_in_window",
                "#[tauri::command]\npub fn enqueue_media_paths",
            ),
            (
                "fn open_player_window_for_session",
                "#[tauri::command]\npub fn open_media_in_new_window",
            ),
        ] {
            let start = source.find(start_marker).unwrap();
            let remainder = &source[start..];
            let end = remainder.find(end_marker).unwrap();
            let body = &remainder[..end];
            let show = body.find("window.show()").unwrap();
            let reinstall = body[show..].find("native_video::install(").unwrap() + show;
            let syncs = body
                .match_indices("session.sync_mpv_executor_from_player()?")
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            assert_eq!(syncs.len(), 1, "{start_marker}");
            let sync = syncs[0];
            assert!(show < reinstall && reinstall < sync, "{start_marker}");
        }
    }

    #[test]
    fn online_subtitle_provider_override_is_request_scoped() {
        let preferences = PreferenceStore::default();
        let overridden =
            online_subtitle_preferences_for_request(&preferences, Some(":assrt")).unwrap();

        assert_eq!(
            preferences
                .values
                .get("onlineSubProvider")
                .and_then(serde_json::Value::as_str),
            Some(":opensubtitles")
        );
        assert_eq!(
            overridden
                .values
                .get("onlineSubProvider")
                .and_then(serde_json::Value::as_str),
            Some(":assrt")
        );
        assert!(online_subtitle_preferences_for_request(
            &preferences,
            Some("plugin:fixture:provider")
        )
        .is_err());
    }
    use std::collections::BTreeMap;
    use std::path::Path;

    #[test]
    fn plugin_managed_user_labels_are_safe_query_components() {
        assert_eq!(percent_encode_query_component("child one"), "child%20one");
        assert_eq!(
            percent_encode_query_component("播放器/100%"),
            "%E6%92%AD%E6%94%BE%E5%99%A8%2F100%25"
        );
        assert_eq!(percent_encode_query_component("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn media_open_plan_appends_auto_matched_subtitles_after_playlist_ordering() {
        let mut player = PlayerState::default();
        open_media_batch_with_plan(
            &mut player,
            PlaylistAutoAddPlan {
                paths: vec!["/tmp/current.mkv".into(), "/tmp/next.mkv".into()],
                preceding_sibling_indexes: vec![1],
            },
            vec!["/tmp/current.en.srt".into()],
            Err("probe unavailable".into()),
            false,
        );

        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("loadfile", ["/tmp/current.mkv", "replace"]),
                set_property("pause", MpvFormat::Flag, "false"),
                mpv_command("loadfile", ["/tmp/next.mkv", "append"]),
                mpv_command("playlist-move", ["1", "0"]),
                mpv_command("sub-add", ["/tmp/current.en.srt"]),
            ]
        );
    }

    #[test]
    fn menu_open_routing_matches_iina_active_or_new_semantics() {
        for always_open_in_new_window in [false, true] {
            for is_alternative_action in [false, true] {
                assert!(!should_open_in_new_player(
                    always_open_in_new_window,
                    is_alternative_action,
                    false
                ));
            }
        }

        assert!(should_open_in_new_player(true, false, true));
        assert!(!should_open_in_new_player(true, true, true));
        assert!(!should_open_in_new_player(false, false, true));
        assert!(should_open_in_new_player(false, true, true));
    }

    #[test]
    fn open_url_submission_uses_the_latest_active_session_not_the_window_owner() {
        let state = AppState::default();
        state
            .preferences
            .lock()
            .unwrap()
            .values
            .insert("alwaysOpenInNewWindow".into(), json!(true));
        let stale_owner = "main";
        state.note_player_session_active(stale_owner).unwrap();
        let (latest_label, latest) = state.create_player_session().unwrap();
        latest.player.lock().unwrap().current_url = Some("https://active.invalid/video".into());
        state.note_player_session_active(&latest_label).unwrap();

        let (target, open_new) = open_url_submission_route(&state, false, false).unwrap();
        assert_eq!(target, latest_label);
        assert_ne!(target, stale_owner);
        assert!(open_new);

        let (enqueue_target, enqueue_new) = open_url_submission_route(&state, true, true).unwrap();
        assert_eq!(enqueue_target, latest_label);
        assert!(!enqueue_new);
    }

    #[test]
    fn macos_service_url_uses_main_window_player_or_first_without_new_window_routing() {
        let state = AppState::default();
        state
            .preferences
            .lock()
            .unwrap()
            .values
            .insert("alwaysOpenInNewWindow".into(), json!(true));
        let (latest_label, _) = state.create_player_session().unwrap();
        state.note_player_session_active(&latest_label).unwrap();

        let (target, url) = service_open_url_route(
            &state,
            Some(&latest_label),
            "https://example.com/video%20path".into(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(target, latest_label);
        assert_eq!(url, "https://example.com/video%20path");

        let (fallback, whitespace) = service_open_url_route(&state, None, "%20%20".into())
            .unwrap()
            .unwrap();
        assert_eq!(fallback, "main");
        assert_eq!(whitespace, "%20%20");
        assert_eq!(
            service_open_url_route(&state, None, String::new()).unwrap(),
            None
        );

        let idle = PlayerState::default();
        assert_eq!(
            service_player_controller_session_label("player-12", &idle, false),
            None
        );
        assert_eq!(
            service_player_controller_session_label("player-12", &idle, true).as_deref(),
            Some("player-12")
        );
        assert_eq!(
            service_player_controller_session_label("mini-player-player-12", &idle, false)
                .as_deref(),
            Some("player-12")
        );
        let mut playing = PlayerState::default();
        playing.current_url = Some("/tmp/movie.mp4".into());
        assert_eq!(
            service_player_controller_session_label("player-12", &playing, false).as_deref(),
            Some("player-12")
        );
        assert_eq!(
            service_player_controller_session_label("preferences", &playing, false),
            None
        );
    }

    #[test]
    fn playlist_trash_finishes_all_file_operations_before_removing_only_successes() {
        let targets = vec![
            IndexedPlaylistPath {
                index: 1,
                path: "/tmp/one.mp4".to_string(),
            },
            IndexedPlaylistPath {
                index: 3,
                path: "/tmp/three.mp4".to_string(),
            },
        ];
        let events = RefCell::new(Vec::new());
        let (removed, successes, failures) = execute_playlist_trash_plan(
            &targets,
            |path| {
                events
                    .borrow_mut()
                    .push(format!("trash:{}", path.display()));
                if path.ends_with("three.mp4") {
                    Err("permission denied".to_string())
                } else {
                    Ok(())
                }
            },
            |indexes| {
                events.borrow_mut().push(format!("remove:{indexes:?}"));
                Ok(indexes)
            },
        )
        .unwrap();

        assert_eq!(successes, vec![1]);
        assert_eq!(removed, vec![1]);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].index, 3);
        assert_eq!(
            events.into_inner(),
            vec!["trash:/tmp/one.mp4", "trash:/tmp/three.mp4", "remove:[1]",]
        );
    }

    #[test]
    fn jump_to_default_value_matches_iina_video_time_precision() {
        assert_eq!(format_video_time_with_precision(0.0, 3), "00:00.000");
        assert_eq!(format_video_time_with_precision(65.1234, 3), "01:05.123");
        assert_eq!(
            format_video_time_with_precision(3_665.1234, 3),
            "1:01:05.123"
        );
    }

    #[test]
    fn jump_to_parser_matches_iina_video_time_component_rules() {
        for (input, expected) in [
            ("90", 90.0),
            (".5", 0.5),
            ("20:35", 1_235.0),
            ("1:02:03.5", 3_723.5),
            ("1::2", 62.0),
            ("ignored:1:02:03", 3_723.0),
            ("bad:02", 2.0),
            ("1:bad", 60.0),
        ] {
            let actual = parse_video_time(input).unwrap();
            assert!(
                (actual - expected).abs() < f64::EPSILON,
                "{input} parsed as {actual}, expected {expected}"
            );
        }
        for input in ["", "not-a-time", " 1", "nan", "inf", "-inf"] {
            assert_eq!(parse_video_time(input), None, "{input} should be invalid");
        }
    }

    #[test]
    fn playlist_serialization_preserves_order_and_trailing_newlines() {
        assert_eq!(
            serialize_m3u8_playlist([
                "/Users/example/电影 one.mp4",
                "https://example.com/live?id=2"
            ]),
            "/Users/example/电影 one.mp4\nhttps://example.com/live?id=2\n"
        );
        assert_eq!(serialize_m3u8_playlist(std::iter::empty()), "");
    }

    #[test]
    fn localized_string_arguments_follow_apple_placeholder_order() {
        assert_eq!(
            format_localized_string_arguments(
                "Error occurred when saving %@: %@",
                &["subtitle", "disk full"]
            ),
            "Error occurred when saving subtitle: disk full"
        );
    }

    #[test]
    fn playlist_write_atomically_replaces_destination_with_exact_utf8() {
        let root =
            std::env::temp_dir().join(format!("iima-playlist-atomic-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let destination = root.join("current.m3u8");
        std::fs::write(&destination, "old\n").unwrap();

        write_atomic_playlist(&destination, "电影.mp4\n".as_bytes()).unwrap();
        assert_eq!(std::fs::read_to_string(&destination).unwrap(), "电影.mp4\n");
        write_atomic_playlist(&destination, b"").unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), Vec::<u8>::new());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sanitizes_key_binding_export_filename() {
        assert_eq!(
            sanitized_config_filename("IINA Default"),
            "IINA-Default.conf"
        );
        assert_eq!(sanitized_config_filename("custom.conf"), "custom.conf");
        assert_eq!(sanitized_config_filename(""), "input.conf");
    }

    #[test]
    fn stdin_is_not_added_to_recent_documents() {
        assert!(!should_record_recent_media_path("-"));
        assert!(should_record_recent_media_path("/tmp/movie.mp4"));
        assert!(should_record_recent_media_path(
            "https://example.com/movie.mp4"
        ));
    }

    #[test]
    fn recent_document_recording_matches_open_panel_and_file_loaded_sources() {
        assert!(should_note_recent_document(
            true,
            false,
            RecentDocumentSource::OpenPanel
        ));
        assert!(!should_note_recent_document(
            true,
            false,
            RecentDocumentSource::FileLoaded
        ));
        assert!(should_note_recent_document(
            true,
            true,
            RecentDocumentSource::FileLoaded
        ));
        assert!(!should_note_recent_document(
            false,
            true,
            RecentDocumentSource::OpenPanel
        ));
    }

    #[test]
    fn updater_preferences_accept_only_the_reference_types_and_intervals() {
        for (key, value) in [
            ("receiveBetaUpdate", json!(true)),
            ("updaterAutomaticallyChecks", json!(false)),
            ("updaterCheckInterval", json!(3600.0)),
            ("updaterCheckInterval", json!(86400.0)),
            ("updaterCheckInterval", json!(604800.0)),
            ("updaterCheckInterval", json!(2_629_800.0)),
        ] {
            assert!(validate_updater_preference(&PreferenceChange {
                key: key.to_string(),
                value,
            })
            .is_ok());
        }

        for (key, value) in [
            ("receiveBetaUpdate", json!(1)),
            ("updaterAutomaticallyChecks", json!("yes")),
            ("updaterCheckInterval", json!(7200.0)),
        ] {
            assert!(validate_updater_preference(&PreferenceChange {
                key: key.to_string(),
                value,
            })
            .is_err());
        }
    }

    #[test]
    fn http_auth_key_matches_iina_host_and_optional_port() {
        assert_eq!(
            http_auth_key_from_url("https://User:Secret@Example.COM:8443/movie.mp4").unwrap(),
            ("example.com".to_string(), Some(8443))
        );
        assert_eq!(
            http_auth_key_from_url("http://example.com/movie.mp4").unwrap(),
            ("example.com".to_string(), None)
        );
        assert!(http_auth_key_from_url("file:///tmp/movie.mp4").is_err());
        assert!(http_auth_key_from_url("not a URL").is_err());
    }

    #[test]
    fn server_rejected_opensubtitles_token_abandons_app_session() {
        let state = AppState::default();
        *state.opensubtitles_session.lock().unwrap() = Some(OpenSubtitlesSession::for_test());
        let result: Result<(), String> =
            Err("OPEN_SUBTITLES_INVALID_TOKEN:invalid token".to_string());

        abandon_rejected_opensubtitles_session(&state, &result).unwrap();

        assert!(state.opensubtitles_session.lock().unwrap().is_none());
    }

    #[test]
    fn utility_paths_are_recreated_and_missing_history_is_allowed() {
        let root =
            std::env::temp_dir().join(format!("iima-utility-path-test-{}", std::process::id()));
        let watch_later = root.join("watch_later");
        std::fs::create_dir_all(&watch_later).unwrap();
        std::fs::write(watch_later.join("resume"), "data").unwrap();

        recreate_directory(&watch_later).unwrap();
        assert!(watch_later.is_dir());
        assert!(std::fs::read_dir(&watch_later).unwrap().next().is_none());
        remove_file_if_present(&root.join("history.plist")).unwrap();

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn browser_extension_urls_match_iina_135() {
        assert_eq!(
            browser_extension_url("chrome"),
            Some(
                "https://chrome.google.com/webstore/detail/open-in-iina/pdnojahnhpgmdhjdhgphgdcecehkbhfo"
            )
        );
        assert_eq!(
            browser_extension_url("firefox"),
            Some("https://addons.mozilla.org/addon/open-in-iina-x")
        );
        assert_eq!(browser_extension_url("safari"), None);
    }

    #[test]
    fn mini_player_labels_preserve_the_owning_player_session() {
        assert_eq!(mini_player_label_for_session("main"), "mini-player");
        assert_eq!(
            mini_player_label_for_session("player-7"),
            "mini-player-player-7"
        );
        assert_eq!(mini_player_session_label("mini-player"), Some("main"));
        assert_eq!(
            mini_player_session_label("mini-player-player-7"),
            Some("player-7")
        );
        assert_eq!(mini_player_session_label("player-7"), None);
    }

    #[test]
    fn mini_player_layout_matches_the_reference_xib_dimensions() {
        let album_art = mini_player_layout(300.0, true, false, 1.0);
        assert_eq!((album_art.width, album_art.height), (300.0, 372.0));
        assert_eq!(album_art.video_height, 300.0);
        assert!(!album_art.playlist_visible);

        let compact = mini_player_layout(300.0, false, false, 1.0);
        assert_eq!((compact.width, compact.height), (300.0, 72.0));

        let playlist = mini_player_layout(300.0, true, true, 16.0 / 9.0);
        assert_eq!(playlist.video_height, 169.0);
        assert_eq!(playlist.playlist_height, 300.0);
        assert_eq!(playlist.height, 541.0);
        assert!(playlist.playlist_visible);
    }

    #[test]
    fn initial_and_player_windows_use_the_reference_presentations() {
        let initial = window_presentation("initial").unwrap();
        assert_eq!((initial.width, initial.height), (640.0, 400.0));
        assert_eq!((initial.min_width, initial.min_height), (640.0, 400.0));
        assert!(!initial.resizable);

        let player = window_presentation("player").unwrap();
        assert_eq!((player.width, player.height), (640.0, 400.0));
        assert_eq!((player.min_width, player.min_height), (285.0, 120.0));
        assert!(player.resizable);
        assert!(window_presentation("unknown").is_err());
    }

    #[test]
    fn automatic_window_resize_routing_matches_iina_timing_rules() {
        assert_eq!(
            playback_window_resize_action(WindowResizeDirective::ManuallyOpenedFile, 1, 3,),
            PlaybackWindowResizeAction::Preference(3)
        );
        assert_eq!(
            playback_window_resize_action(WindowResizeDirective::AutomaticallyStartedFile, 0, 1,),
            PlaybackWindowResizeAction::Preference(1)
        );
        for timing in [1, 2] {
            assert_eq!(
                playback_window_resize_action(
                    WindowResizeDirective::AutomaticallyStartedFile,
                    timing,
                    2,
                ),
                PlaybackWindowResizeAction::PreserveWidth
            );
        }
        assert_eq!(
            playback_window_resize_action(WindowResizeDirective::ManuallyOpenedFile, 2, 2,),
            PlaybackWindowResizeAction::PreserveWidth
        );
        assert_eq!(
            playback_window_resize_action(WindowResizeDirective::VideoReconfigured, 2, 4),
            PlaybackWindowResizeAction::VideoReconfigured
        );
    }

    #[test]
    fn quit_after_last_window_requires_the_live_preference_and_a_visible_closing_window() {
        assert!(should_quit_after_last_window_closes(true, true, false));
        assert!(!should_quit_after_last_window_closes(false, true, false));
        assert!(!should_quit_after_last_window_closes(true, false, false));
        assert!(!should_quit_after_last_window_closes(true, true, true));
    }

    #[test]
    fn validates_plugin_http_download_request_options() {
        assert_eq!(plugin_http_method(&json!({})).unwrap(), "GET");
        assert_eq!(
            plugin_http_method(&json!({"method": "PATCH"})).unwrap(),
            "PATCH"
        );
        assert_eq!(
            plugin_http_method(&json!({"method": "HEAD"})).unwrap(),
            "HEAD"
        );
        assert_eq!(
            plugin_http_method(&json!({"method": "OPTIONS"})).unwrap(),
            "OPTIONS"
        );
        assert!(plugin_http_method(&json!({"method": "patch"})).is_err());
        assert!(plugin_http_method(&json!({"method": "TRACE"})).is_err());
        assert_eq!(
            plugin_http_body(&json!({"data": {"name": "IINA"}})).unwrap(),
            Some("{\"name\":\"IINA\"}".to_string())
        );
        assert!(validate_plugin_http_header("X-Test", "value").is_ok());
        assert!(validate_plugin_http_header("X-Test\nInjected", "value").is_err());
    }

    #[test]
    fn plugin_http_responses_match_iina_status_reason_and_camel_case_shape() {
        let (body, status_code) = split_plugin_http_write_out(b"{\"ok\":true}\n404");
        assert_eq!(body, b"{\"ok\":true}");
        assert_eq!(status_code, Some(404));
        let response = plugin_http_response(&body, status_code, "curl transport detail");
        assert_eq!(response.reason, "not found");
        assert_eq!(response.data, Some(json!({"ok": true})));
        assert_eq!(response.text.as_deref(), Some("{\"ok\":true}"));
        assert!(!plugin_http_response_is_ok(response.status_code));
        assert!(plugin_http_response_is_ok(Some(302)));
        assert_eq!(
            plugin_http_reason(Some(503), "ignored"),
            "service unavailable"
        );

        assert_eq!(
            serde_json::to_value(&response).unwrap(),
            json!({
                "statusCode": 404,
                "reason": "not found",
                "data": {"ok": true},
                "text": "{\"ok\":true}"
            })
        );

        let (body, status_code) = split_plugin_http_write_out(b"\n000");
        assert!(body.is_empty());
        assert_eq!(status_code, None);
        let transport = plugin_http_response(&[], status_code, "Could not connect");
        assert_eq!(transport.reason, "Could not connect");
        assert_eq!(transport.status_code, None);
        assert_eq!(transport.data, None);
        assert_eq!(transport.text, None);
        assert!(!plugin_http_response_is_ok(transport.status_code));

        let json_fragment = plugin_http_response(b"123", Some(200), "");
        assert_eq!(json_fragment.data, None);
        assert_eq!(json_fragment.text.as_deref(), Some("123"));
    }

    #[test]
    fn namespaces_plugin_keychain_services_like_iina() {
        assert_eq!(
            plugin_keychain_service("io.iina.demo", "Account").unwrap(),
            "io.iina.demo - Account"
        );
        assert!(plugin_keychain_service("io.iina.demo", "").is_err());
        assert!(plugin_keychain_service("io.iina.demo", &"x".repeat(257)).is_err());
        assert!(plugin_keychain_service("io.iina.demo", "bad\0service").is_err());
    }

    #[test]
    fn plugin_file_handles_accept_strings_and_byte_arrays_only() {
        assert_eq!(
            plugin_file_handle_bytes(json!("IINA")).unwrap(),
            b"IINA".to_vec()
        );
        assert_eq!(
            plugin_file_handle_bytes(json!([0, 127, 255])).unwrap(),
            vec![0, 127, 255]
        );
        assert!(plugin_file_handle_bytes(json!([256])).is_err());
        assert!(plugin_file_handle_bytes(json!({"byte": 1})).is_err());
    }

    #[test]
    fn plugin_file_handle_read_to_end_rejects_truncation() {
        let root = std::env::temp_dir().join(format!(
            "iima-plugin-file-handle-limit-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("bytes.bin");
        std::fs::write(&path, [0_u8, 1, 2, 3, 4]).unwrap();
        let mut handle = PluginOpenFileHandle {
            identifier: "io.iina.test".to_string(),
            window_label: "main".to_string(),
            mode: PluginFileHandleMode::Read,
            file: std::fs::File::open(&path).unwrap(),
        };

        let error = read_plugin_file_handle_to_end(&mut handle, 4).unwrap_err();
        assert!(error.contains("exceeds the 8 MiB limit"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plugin_file_paths_resolve_external_tracks_without_filesystem_permission() {
        let mut player = crate::player::PlayerState::default();
        player.tracks.subtitles.push(Track {
            id: 7,
            title: "Commentary".to_string(),
            selected: false,
            metadata: TrackMetadata {
                external: true,
                external_filename: Some("/tmp/commentary.srt".to_string()),
                ..TrackMetadata::default()
            },
        });

        assert_eq!(
            plugin_track_file_path(&player, "@sub/7").unwrap(),
            Some(std::path::PathBuf::from("/tmp/commentary.srt"))
        );
        assert!(plugin_track_file_path(&player, "@sub/not-an-id").is_err());
        assert!(plugin_track_file_path(&player, "@sub/8").is_err());
        assert!(plugin_track_file_path(&player, "@sub/7/nested").is_err());
        assert_eq!(
            plugin_track_file_path(&player, "@subtitle/7").unwrap(),
            Some(std::path::PathBuf::from("/tmp/commentary.srt"))
        );
        assert_eq!(plugin_track_file_path(&player, "@data/file").unwrap(), None);
    }

    #[test]
    fn builds_private_temporary_download_name() {
        let parent = std::env::temp_dir();
        let destination = parent.join("plugin-output.txt");
        let expected = format!(".plugin-output.txt.iima-download-{}", std::process::id());
        assert_eq!(
            plugin_download_temporary_path(&parent, &destination)
                .unwrap()
                .file_name()
                .and_then(|value| value.to_str()),
            Some(expected.as_str())
        );
        assert!(plugin_download_temporary_path(&parent, Path::new("/")).is_err());
    }

    #[test]
    fn maps_iina_screenshot_preferences_to_mpv_options() {
        let values = BTreeMap::from([
            ("screenshotSaveToFile".to_string(), json!(false)),
            ("screenshotCopyToClipboard".to_string(), json!(true)),
            ("screenShotIncludeSubtitle".to_string(), json!(false)),
            ("screenShotFolder".to_string(), json!("~/Pictures/IINA")),
            ("screenShotFormat".to_string(), json!(2)),
            ("screenShotTemplate".to_string(), json!("%F-%P")),
            ("screenshotShowPreview".to_string(), json!(false)),
        ]);

        let options = screenshot_options_from_preferences(&values);

        assert!(!options.save_to_file);
        assert!(options.copy_to_clipboard);
        assert!(!options.include_subtitles);
        assert_eq!(options.directory.as_deref(), Some("~/Pictures/IINA"));
        assert_eq!(options.format.extension(), "jpeg");
        assert_eq!(options.template, "%F-%P");
        assert!(!options.show_preview);
    }

    #[test]
    fn plugin_file_list_matches_reference_direct_children_even_when_option_is_true() {
        let root =
            std::env::temp_dir().join(format!("iima-plugin-file-list-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("top.txt"), "top").unwrap();
        std::fs::write(root.join("nested/child.txt"), "child").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("nested"), root.join("directory-link")).unwrap();
            std::os::unix::fs::symlink(root.join("missing"), root.join("dangling-link")).unwrap();
        }

        let mut shallow = Vec::new();
        collect_plugin_file_entries(&root, &root, false, &mut shallow).unwrap();
        shallow.sort_by(|left, right| left.path.cmp(&right.path));
        assert_eq!(
            shallow
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>(),
            {
                let mut expected = vec!["/nested", "/top.txt"];
                #[cfg(unix)]
                expected.extend(["/directory-link", "/dangling-link"]);
                expected.sort();
                expected
            }
        );
        #[cfg(unix)]
        {
            assert!(
                shallow
                    .iter()
                    .find(|entry| entry.path == "/directory-link")
                    .unwrap()
                    .is_dir
            );
            assert!(
                !shallow
                    .iter()
                    .find(|entry| entry.path == "/dangling-link")
                    .unwrap()
                    .is_dir
            );
        }
        let wire_entry = serde_json::to_value(
            shallow
                .iter()
                .find(|entry| entry.path == "/top.txt")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(wire_entry["isDir"], serde_json::Value::Bool(false));
        assert!(wire_entry.get("is_dir").is_none());

        let mut include_sub_dir = Vec::new();
        collect_plugin_file_entries(&root, &root, true, &mut include_sub_dir).unwrap();
        include_sub_dir.sort_by(|left, right| left.path.cmp(&right.path));
        assert_eq!(
            include_sub_dir
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>(),
            {
                let mut expected = vec!["/nested", "/top.txt"];
                #[cfg(unix)]
                expected.extend(["/directory-link", "/dangling-link"]);
                expected.sort();
                expected
            }
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn plugin_text_write_is_utf8_atomic_and_does_not_create_nested_parents() {
        let root = std::env::temp_dir().join(format!(
            "iima-plugin-atomic-write-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("value.txt");
        std::fs::write(&path, "old").unwrap();
        write_plugin_text_atomically(&path, "IINA 同步".as_bytes()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "IINA 同步");
        assert!(std::fs::read_dir(&root).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".iima-plugin-write-")));

        let missing = root.join("missing/value.txt");
        assert!(write_plugin_text_atomically(&missing, b"no parent").is_err());
        assert!(!root.join("missing").exists());
        let _ = std::fs::remove_dir_all(root);
    }
}
