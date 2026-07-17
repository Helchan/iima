use crate::{localization, state::AppState};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::{
    AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

pub(crate) const OPEN_URL_WINDOW_LABEL: &str = "open-url";
pub(crate) const VIDEO_FILTER_WINDOW_LABEL: &str = "video-filter";
pub(crate) const AUDIO_FILTER_WINDOW_LABEL: &str = "audio-filter";
pub(crate) const PREFERENCES_WINDOW_LABEL: &str = "preferences";
pub(crate) const PLAYBACK_HISTORY_WINDOW_LABEL: &str = "playback-history";
pub(crate) const LOG_VIEWER_WINDOW_LABEL: &str = "log-viewer";
pub(crate) const AUXILIARY_CONTEXT_EVENT: &str = "iima-auxiliary-window-context";
pub(crate) const PLAYER_PLUGIN_RUNTIME_REFRESH_EVENT: &str = "iima-plugin-runtime-refresh";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuxiliaryWindowKind {
    OpenUrl,
    VideoFilter,
    AudioFilter,
    Preferences,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AuxiliaryWindowSpec {
    label: &'static str,
    role: &'static str,
    width: f64,
    height: f64,
    min_width: Option<f64>,
    min_height: Option<f64>,
    full_size_content: bool,
    hidden_title: bool,
    hide_standard_buttons: bool,
}

impl AuxiliaryWindowKind {
    const fn spec(self) -> AuxiliaryWindowSpec {
        match self {
            Self::OpenUrl => AuxiliaryWindowSpec {
                label: OPEN_URL_WINDOW_LABEL,
                role: "open-url",
                width: 576.0,
                height: 270.0,
                min_width: None,
                min_height: None,
                full_size_content: true,
                hidden_title: true,
                hide_standard_buttons: true,
            },
            Self::VideoFilter => AuxiliaryWindowSpec {
                label: VIDEO_FILTER_WINDOW_LABEL,
                role: "video-filter",
                width: 480.0,
                height: 382.0,
                min_width: None,
                min_height: None,
                full_size_content: false,
                hidden_title: false,
                hide_standard_buttons: false,
            },
            Self::AudioFilter => AuxiliaryWindowSpec {
                label: AUDIO_FILTER_WINDOW_LABEL,
                role: "audio-filter",
                width: 480.0,
                height: 382.0,
                min_width: None,
                min_height: None,
                full_size_content: false,
                hidden_title: false,
                hide_standard_buttons: false,
            },
            Self::Preferences => AuxiliaryWindowSpec {
                label: PREFERENCES_WINDOW_LABEL,
                role: "preferences",
                width: 820.0,
                height: 480.0,
                min_width: Some(820.0),
                min_height: Some(320.0),
                full_size_content: true,
                hidden_title: true,
                hide_standard_buttons: false,
            },
        }
    }

    fn title(self) -> String {
        match self {
            Self::OpenUrl => {
                localization::menu_title_key("Localizable", "alert.open_url.title", "Open URL")
            }
            Self::VideoFilter => {
                localization::menu_title_key("Localizable", "filter.video_filters", "Video Filters")
            }
            Self::AudioFilter => {
                localization::menu_title_key("Localizable", "filter.audio_filters", "Audio Filters")
            }
            Self::Preferences => localization::menu_title_key(
                "PreferenceWindowController",
                "F0z-JX-Cv5.title",
                "Preferences",
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuxiliaryWindowContext {
    role: &'static str,
    owner_label: Option<String>,
    is_alternative_action: bool,
    enqueue: bool,
    pane: Option<String>,
    selected_plugin_identifier: Option<String>,
    drain_pending_plugin_installs: bool,
}

impl AuxiliaryWindowContext {
    fn new(kind: AuxiliaryWindowKind, owner_label: Option<String>) -> Self {
        Self {
            role: kind.spec().role,
            owner_label,
            is_alternative_action: false,
            enqueue: false,
            pane: None,
            selected_plugin_identifier: None,
            drain_pending_plugin_installs: false,
        }
    }
}

fn contexts() -> &'static Mutex<HashMap<&'static str, AuxiliaryWindowContext>> {
    static CONTEXTS: OnceLock<Mutex<HashMap<&'static str, AuxiliaryWindowContext>>> =
        OnceLock::new();
    CONTEXTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn set_context(kind: AuxiliaryWindowKind, context: AuxiliaryWindowContext) -> Result<(), String> {
    contexts()
        .lock()
        .map_err(|error| error.to_string())?
        .insert(kind.spec().label, context);
    Ok(())
}

fn show_window<R: Runtime>(
    app: &AppHandle<R>,
    kind: AuxiliaryWindowKind,
    context: AuxiliaryWindowContext,
) -> Result<(), String> {
    let spec = kind.spec();
    set_context(kind, context.clone())?;
    if let Some(window) = app.get_webview_window(spec.label) {
        window.unminimize().map_err(|error| error.to_string())?;
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        let _ = window.emit(AUXILIARY_CONTEXT_EVENT, context);
        return Ok(());
    }

    let url = format!("index.html?window-role={}", spec.role);
    let mut builder = WebviewWindowBuilder::new(app, spec.label, WebviewUrl::App(url.into()))
        .title(kind.title())
        .inner_size(spec.width, spec.height)
        .resizable(true)
        .maximizable(true)
        .minimizable(true)
        .decorations(true)
        .center();
    if let (Some(width), Some(height)) = (spec.min_width, spec.min_height) {
        builder = builder.min_inner_size(width, height);
    }
    #[cfg(target_os = "macos")]
    if spec.full_size_content {
        builder = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(spec.hidden_title);
    }
    let window = builder.build().map_err(|error| error.to_string())?;
    configure_native_auxiliary_window(&window, kind)?;
    let _ = window.emit(AUXILIARY_CONTEXT_EVENT, context);
    Ok(())
}

fn player_owner_for_window(state: &AppState, window: &WebviewWindow) -> Result<String, String> {
    state.shortcut_player_session_label(window.label())
}

pub(crate) fn show_open_url_for_owner<R: Runtime>(
    app: &AppHandle<R>,
    owner_label: &str,
    is_alternative_action: bool,
    enqueue: bool,
) -> Result<(), String> {
    app.state::<AppState>()
        .player_session_for_window(owner_label)?;
    let mut context =
        AuxiliaryWindowContext::new(AuxiliaryWindowKind::OpenUrl, Some(owner_label.to_string()));
    context.is_alternative_action = is_alternative_action;
    context.enqueue = enqueue;
    show_window(app, AuxiliaryWindowKind::OpenUrl, context)
}

pub(crate) fn show_filter_for_owner<R: Runtime>(
    app: &AppHandle<R>,
    owner_label: &str,
    kind: &str,
) -> Result<(), String> {
    app.state::<AppState>()
        .player_session_for_window(owner_label)?;
    let kind = match kind {
        "video" => AuxiliaryWindowKind::VideoFilter,
        "audio" => AuxiliaryWindowKind::AudioFilter,
        value => return Err(format!("unsupported filter window kind: {value}")),
    };
    show_window(
        app,
        kind,
        AuxiliaryWindowContext::new(kind, Some(owner_label.to_string())),
    )
}

pub(crate) fn show_preferences_for_pane<R: Runtime>(
    app: &AppHandle<R>,
    pane: Option<String>,
    selected_plugin_identifier: Option<String>,
    drain_pending_plugin_installs: bool,
) -> Result<(), String> {
    const PANES: &[&str] = &[
        "general",
        "ui",
        "video_audio",
        "subtitle",
        "network",
        "control",
        "keybindings",
        "plugins",
        "advanced",
        "utilities",
    ];
    if pane.as_deref().is_some_and(|pane| !PANES.contains(&pane)) {
        return Err("unsupported Preferences pane".to_string());
    }
    let mut context = AuxiliaryWindowContext::new(AuxiliaryWindowKind::Preferences, None);
    context.pane = pane;
    context.selected_plugin_identifier = selected_plugin_identifier
        .map(|identifier| identifier.trim().to_string())
        .filter(|identifier| !identifier.is_empty());
    if context
        .selected_plugin_identifier
        .as_ref()
        .is_some_and(|identifier| identifier.len() > 255)
    {
        return Err("selected plugin identifier is too long".to_string());
    }
    context.drain_pending_plugin_installs = drain_pending_plugin_installs;
    show_window(app, AuxiliaryWindowKind::Preferences, context)
}

#[tauri::command]
pub fn show_open_url_window(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    is_alternative_action: bool,
    enqueue: bool,
) -> Result<(), String> {
    let owner = player_owner_for_window(state.inner(), &window)?;
    show_open_url_for_owner(&app, &owner, is_alternative_action, enqueue)
}

#[tauri::command]
pub fn show_filter_window(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    kind: String,
) -> Result<(), String> {
    let owner = player_owner_for_window(state.inner(), &window)?;
    show_filter_for_owner(&app, &owner, &kind)
}

#[tauri::command]
pub fn show_preferences_window(
    app: AppHandle,
    pane: Option<String>,
    selected_plugin_identifier: Option<String>,
    drain_pending_plugin_installs: bool,
) -> Result<(), String> {
    show_preferences_for_pane(
        &app,
        pane,
        selected_plugin_identifier,
        drain_pending_plugin_installs,
    )
}

#[tauri::command]
pub fn request_player_plugin_runtime_refresh(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<usize, String> {
    if window.label() != PREFERENCES_WINDOW_LABEL {
        return Err("Only the Preferences window can request a player plugin refresh".to_string());
    }
    let mut refreshed = 0;
    for (label, _) in app.webview_windows() {
        if !is_player_plugin_runtime_host(&label) {
            continue;
        }
        app.emit_to(&label, PLAYER_PLUGIN_RUNTIME_REFRESH_EVENT, ())
            .map_err(|error| error.to_string())?;
        refreshed += 1;
    }
    Ok(refreshed)
}

#[tauri::command]
pub fn get_auxiliary_window_context(
    window: WebviewWindow,
) -> Result<AuxiliaryWindowContext, String> {
    let label = window.label();
    contexts()
        .lock()
        .map_err(|error| error.to_string())?
        .get(label)
        .cloned()
        .ok_or_else(|| format!("{label} is not a managed IINA auxiliary window"))
}

#[tauri::command]
pub fn hide_auxiliary_window(window: WebviewWindow) -> Result<(), String> {
    if !is_reusable_auxiliary_window_label(window.label()) {
        return Err("Only a managed IINA auxiliary window can hide itself".to_string());
    }
    window.hide().map_err(|error| error.to_string())
}

pub(crate) fn is_reusable_auxiliary_window_label(label: &str) -> bool {
    matches!(
        label,
        OPEN_URL_WINDOW_LABEL
            | VIDEO_FILTER_WINDOW_LABEL
            | AUDIO_FILTER_WINDOW_LABEL
            | PREFERENCES_WINDOW_LABEL
            | PLAYBACK_HISTORY_WINDOW_LABEL
            | LOG_VIEWER_WINDOW_LABEL
    )
}

fn is_player_plugin_runtime_host(label: &str) -> bool {
    label == "main" || label.starts_with("player-")
}

#[cfg(target_os = "macos")]
fn configure_native_auxiliary_window<R: Runtime>(
    window: &WebviewWindow<R>,
    kind: AuxiliaryWindowKind,
) -> Result<(), String> {
    use std::ffi::{c_int, c_void};

    unsafe extern "C" {
        fn iima_native_configure_auxiliary_window(window: *mut c_void, kind: c_int) -> c_int;
    }
    let native_kind = match kind {
        AuxiliaryWindowKind::OpenUrl => 1,
        AuxiliaryWindowKind::Preferences => 2,
        AuxiliaryWindowKind::VideoFilter | AuxiliaryWindowKind::AudioFilter => 3,
    };
    let status = unsafe {
        iima_native_configure_auxiliary_window(
            window.ns_window().map_err(|error| error.to_string())?,
            native_kind,
        )
    };
    (status == 0)
        .then_some(())
        .ok_or_else(|| format!("Unable to configure native auxiliary window ({status})"))
}

#[cfg(not(target_os = "macos"))]
fn configure_native_auxiliary_window<R: Runtime>(
    _window: &WebviewWindow<R>,
    _kind: AuxiliaryWindowKind,
) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn configure_retained_window<R: Runtime>(
    window: &WebviewWindow<R>,
    frame_autosave_name: &str,
) -> Result<(), String> {
    use std::ffi::{c_char, c_int, c_void, CString};

    unsafe extern "C" {
        fn iima_native_configure_retained_window(
            window: *mut c_void,
            frame_autosave_name: *const c_char,
        ) -> c_int;
    }
    let frame_autosave_name =
        CString::new(frame_autosave_name).map_err(|error| error.to_string())?;
    let status = unsafe {
        iima_native_configure_retained_window(
            window.ns_window().map_err(|error| error.to_string())?,
            frame_autosave_name.as_ptr(),
        )
    };
    (status == 0)
        .then_some(())
        .ok_or_else(|| format!("Unable to configure retained window ({status})"))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn configure_retained_window<R: Runtime>(
    _window: &WebviewWindow<R>,
    _frame_autosave_name: &str,
) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_url_window_matches_iina_135_xib_geometry_and_hidden_traffic_lights() {
        let spec = AuxiliaryWindowKind::OpenUrl.spec();
        assert_eq!(spec.label, "open-url");
        assert_eq!((spec.width, spec.height), (576.0, 270.0));
        assert!(spec.full_size_content);
        assert!(spec.hidden_title);
        assert!(spec.hide_standard_buttons);
    }

    #[test]
    fn filter_windows_are_independent_reusable_480_by_382_surfaces() {
        let video = AuxiliaryWindowKind::VideoFilter.spec();
        let audio = AuxiliaryWindowKind::AudioFilter.spec();
        assert_eq!(video.label, "video-filter");
        assert_eq!(audio.label, "audio-filter");
        assert_eq!((video.width, video.height), (480.0, 382.0));
        assert_eq!((audio.width, audio.height), (480.0, 382.0));
        assert_ne!(video.label, audio.label);
        assert!(!video.full_size_content);
        assert!(!audio.full_size_content);
    }

    #[test]
    fn preferences_window_matches_iina_135_xib_geometry_and_minimum() {
        let spec = AuxiliaryWindowKind::Preferences.spec();
        assert_eq!(spec.label, "preferences");
        assert_eq!((spec.width, spec.height), (820.0, 480.0));
        assert_eq!(
            (spec.min_width, spec.min_height),
            (Some(820.0), Some(320.0))
        );
        assert!(spec.full_size_content);
        assert!(spec.hidden_title);
        assert!(!spec.hide_standard_buttons);
    }

    #[test]
    fn contexts_keep_action_owner_and_filter_concurrency_separate() {
        let mut open =
            AuxiliaryWindowContext::new(AuxiliaryWindowKind::OpenUrl, Some("player-2".into()));
        open.is_alternative_action = true;
        open.enqueue = true;
        assert_eq!(open.owner_label.as_deref(), Some("player-2"));
        assert!(open.is_alternative_action);
        assert!(open.enqueue);
        assert_ne!(
            AuxiliaryWindowKind::VideoFilter.spec().label,
            AuxiliaryWindowKind::AudioFilter.spec().label
        );
    }

    #[test]
    fn plugin_runtime_refresh_targets_only_real_player_hosts() {
        assert!(is_player_plugin_runtime_host("main"));
        assert!(is_player_plugin_runtime_host("player-2"));
        assert!(!is_player_plugin_runtime_host("mini-player"));
        assert!(!is_player_plugin_runtime_host("mini-player-player-2"));
        assert!(!is_player_plugin_runtime_host("preferences"));
        assert!(!is_player_plugin_runtime_host("video-filter"));
    }

    #[test]
    fn preferences_context_can_select_a_plugin_in_a_reused_window() {
        let mut context = AuxiliaryWindowContext::new(AuxiliaryWindowKind::Preferences, None);
        context.pane = Some("plugins".into());
        context.selected_plugin_identifier = Some("io.iina.example".into());
        context.drain_pending_plugin_installs = true;
        assert_eq!(context.pane.as_deref(), Some("plugins"));
        assert_eq!(
            context.selected_plugin_identifier.as_deref(),
            Some("io.iina.example")
        );
        assert!(context.drain_pending_plugin_installs);
    }

    #[test]
    fn history_and_log_windows_are_retained_on_close() {
        assert!(is_reusable_auxiliary_window_label("playback-history"));
        assert!(is_reusable_auxiliary_window_label("log-viewer"));
        let native = include_str!("native_window.m");
        assert!(native.contains("iima_native_configure_retained_window"));
        assert!(native.contains("window.releasedWhenClosed = NO"));
        assert!(native.contains("IIMAConfigureFrameAutosave(window, autosaveName)"));
        assert!(native.contains("setFrameUsingName:autosaveName force:NO"));
        assert!(native.contains("NSWindow Frame %@"));
        assert!(include_str!("commands.rs").contains("PlaybackHistoryWindow"));
        assert!(include_str!("auxiliary_windows.rs").contains("IINALogViewer"));
    }

    #[test]
    fn preferences_restore_a_saved_appkit_frame_without_overriding_the_default() {
        let native = include_str!("native_window.m");
        let preferences = native
            .split("int iima_native_configure_auxiliary_window(")
            .nth(1)
            .and_then(|source| {
                source
                    .split("int iima_native_configure_retained_window(")
                    .next()
            })
            .expect("auxiliary native window source");
        assert!(
            preferences.contains("IIMAConfigureFrameAutosave(window, @\"IINAPreferenceWindow\")")
        );
        let restore = native
            .split("static BOOL IIMAConfigureFrameAutosave(")
            .nth(1)
            .and_then(|source| {
                source
                    .split("int iima_native_configure_auxiliary_window(")
                    .next()
            })
            .expect("shared frame restore helper");
        let saved_check = restore
            .find("objectForKey:")
            .expect("saved AppKit frame check");
        let restore_call = restore
            .find("setFrameUsingName:")
            .expect("saved AppKit frame restore");
        let autosave_call = restore
            .find("setFrameAutosaveName:")
            .expect("AppKit frame autosave registration");
        assert!(saved_check < restore_call && restore_call < autosave_call);
        assert!(restore.contains("if (hasSavedFrame)"));
    }
}
