use serde::Serialize;

use crate::preferences::PreferenceStore;

#[derive(Debug, Clone, Serialize)]
pub struct NativeVideoRendererStatus {
    pub installed: bool,
    pub attached: bool,
    pub pip_available: bool,
    pub pip_active: bool,
    pub backend: &'static str,
    pub render_scheduler: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVideoHdrStatus {
    pub available: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVideoColorSettings {
    pub load_icc_profile: bool,
    pub enable_hdr_support: bool,
    pub enable_tone_mapping: bool,
    pub tone_mapping_target_peak: i64,
    pub tone_mapping_algorithm: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVideoSurfaceSettings {
    pub force_dedicated_gpu: bool,
}

fn native_video_install_error(result: i32) -> String {
    match result {
        -1 => "failed to install native video surface: host view is null".to_string(),
        -2 => "failed to install native video surface: OpenGL view creation failed".to_string(),
        -3 => "native video surface is not ready: host view is not attached to an NSWindow (retryable)"
            .to_string(),
        _ => format!("failed to install native video surface ({result})"),
    }
}

fn native_video_install_readiness(result: i32) -> Result<bool, String> {
    match result {
        0 => Ok(true),
        -3 => Ok(false),
        _ => Err(native_video_install_error(result)),
    }
}

fn native_video_render_scheduler_name(value: i32) -> &'static str {
    match value {
        1 => "display-link",
        2 => "appkit-invalidation",
        _ => "unavailable",
    }
}

pub fn surface_settings_from_preferences(
    preferences: &PreferenceStore,
) -> NativeVideoSurfaceSettings {
    NativeVideoSurfaceSettings {
        force_dedicated_gpu: preference_bool(&preferences.values, "forceDedicatedGPU", false),
    }
}

pub fn color_settings_from_preferences(preferences: &PreferenceStore) -> NativeVideoColorSettings {
    let values = &preferences.values;
    NativeVideoColorSettings {
        load_icc_profile: preference_bool(values, "loadIccProfile", true),
        enable_hdr_support: preference_bool(values, "enableHdrSupport", true),
        enable_tone_mapping: preference_bool(values, "enableToneMapping", false),
        tone_mapping_target_peak: values
            .get("toneMappingTargetPeak")
            .and_then(|value| value.as_i64())
            .unwrap_or(0)
            .max(0),
        tone_mapping_algorithm: tone_mapping_algorithm(values.get("toneMappingAlgorithm")),
    }
}

fn preference_bool(
    values: &std::collections::BTreeMap<String, serde_json::Value>,
    key: &str,
    default: bool,
) -> bool {
    values
        .get(key)
        .and_then(|value| value.as_bool())
        .unwrap_or(default)
}

fn tone_mapping_algorithm(value: Option<&serde_json::Value>) -> String {
    if let Some(value) = value.and_then(|value| value.as_str()) {
        return value.to_string();
    }
    match value.and_then(|value| value.as_i64()).unwrap_or(0) {
        1 => "clip",
        2 => "mobius",
        3 => "reinhard",
        4 => "hable",
        5 => "bt.2390",
        6 => "gamma",
        7 => "linear",
        _ => "auto",
    }
    .to_string()
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{c_char, c_int, c_void, CStr, CString};
    use std::sync::{Arc, Mutex, OnceLock};

    use tauri::{AppHandle, Emitter, Runtime};

    use super::{
        NativeVideoColorSettings, NativeVideoHdrStatus, NativeVideoRendererStatus,
        NativeVideoSurfaceSettings,
    };

    unsafe extern "C" {
        fn iima_native_video_install(
            host_view: *mut c_void,
            session_label: *const c_char,
            force_dedicated_gpu: c_int,
        ) -> c_int;
        fn iima_native_video_attach_mpv_client(
            mpv_handle: *mut c_void,
            libmpv_path: *const c_char,
            session_label: *const c_char,
        ) -> c_int;
        fn iima_native_video_detach_mpv_client(session_label: *const c_char);
        fn iima_native_video_remove_session(session_label: *const c_char);
        fn iima_native_video_configure_color(
            session_label: *const c_char,
            load_icc_profile: c_int,
            enable_hdr_support: c_int,
            enable_tone_mapping: c_int,
            tone_mapping_target_peak: c_int,
            tone_mapping_algorithm: *const c_char,
        );
        fn iima_native_video_request_color_refresh(session_label: *const c_char);
        fn iima_native_video_set_hdr_enabled(session_label: *const c_char, enabled: c_int);
        fn iima_native_video_hdr_is_available(session_label: *const c_char) -> c_int;
        fn iima_native_video_hdr_is_enabled(session_label: *const c_char) -> c_int;
        fn iima_native_video_is_installed(session_label: *const c_char) -> c_int;
        fn iima_native_video_is_attached(session_label: *const c_char) -> c_int;
        fn iima_native_video_render_scheduler(session_label: *const c_char) -> c_int;
        fn iima_native_video_toggle_pip(
            session_label: *const c_char,
            playing: c_int,
            title: *const c_char,
            video_width: f64,
            video_height: f64,
            origin_fullscreen: c_int,
        ) -> c_int;
        #[cfg(test)]
        fn iima_native_video_plan_pip_replacement_rect(
            container_width: f64,
            container_height: f64,
            video_width: f64,
            video_height: f64,
            values: *mut f64,
        ) -> c_int;
        fn iima_native_video_pip_is_available() -> c_int;
        fn iima_native_video_pip_is_active() -> c_int;
        fn iima_native_video_pip_is_active_for_session(session_label: *const c_char) -> c_int;
        fn iima_native_video_set_pip_will_close_callback(
            callback: Option<unsafe extern "C" fn(*const c_char)>,
        );
        fn iima_native_window_center_after_delay(window: *mut c_void, delay_milliseconds: u64);
        fn iima_native_configure_mini_player_window(window: *mut c_void);
        fn iima_native_path_is_on_local_volume(path: *const c_char) -> c_int;
    }

    fn session_label(session_label: &str) -> Result<CString, String> {
        CString::new(session_label)
            .map_err(|_| "session label contains an interior nul byte".to_string())
    }

    type PipWillCloseEmitter = Arc<dyn Fn(&str) + Send + Sync>;

    fn pip_will_close_emitter() -> &'static Mutex<Option<PipWillCloseEmitter>> {
        static EMITTER: OnceLock<Mutex<Option<PipWillCloseEmitter>>> = OnceLock::new();
        EMITTER.get_or_init(|| Mutex::new(None))
    }

    unsafe extern "C" fn pip_will_close_callback(session_label: *const c_char) {
        if session_label.is_null() {
            return;
        }
        let Ok(session_label) = unsafe { CStr::from_ptr(session_label) }.to_str() else {
            return;
        };
        let emitter = pip_will_close_emitter()
            .lock()
            .ok()
            .and_then(|emitter| emitter.clone());
        if let Some(emitter) = emitter {
            emitter(session_label);
        }
    }

    pub fn register_pip_will_close_emitter<R: Runtime>(app: &AppHandle<R>) {
        let app = app.clone();
        let emitter: PipWillCloseEmitter = Arc::new(move |session_label| {
            let _ = app.emit_to(session_label, "iima-pip-will-close", ());
        });
        if let Ok(mut registered) = pip_will_close_emitter().lock() {
            *registered = Some(emitter);
        }
        unsafe {
            iima_native_video_set_pip_will_close_callback(Some(pip_will_close_callback));
        }
    }

    fn install_result(
        host_view: *mut c_void,
        session: &str,
        settings: &NativeVideoSurfaceSettings,
    ) -> Result<i32, String> {
        let session = session_label(session)?;
        Ok(unsafe {
            iima_native_video_install(
                host_view,
                session.as_ptr(),
                c_int::from(settings.force_dedicated_gpu),
            )
        })
    }

    pub fn install(
        host_view: *mut c_void,
        session: &str,
        settings: &NativeVideoSurfaceSettings,
    ) -> Result<(), String> {
        let result = install_result(host_view, session, settings)?;
        super::native_video_install_readiness(result)?
            .then_some(())
            .ok_or_else(|| super::native_video_install_error(result))
    }

    /// Attempts the early, hidden-window installation without treating AppKit's transient
    /// `host.window == nil` state as a permanent failure. Callers must perform a strict `install`
    /// after showing the window before they let libmpv consume queued playback operations.
    pub fn install_if_ready(
        host_view: *mut c_void,
        session: &str,
        settings: &NativeVideoSurfaceSettings,
    ) -> Result<bool, String> {
        super::native_video_install_readiness(install_result(host_view, session, settings)?)
    }

    pub fn center_window_after_delay(
        window: *mut c_void,
        delay_milliseconds: u64,
    ) -> Result<(), String> {
        if window.is_null() {
            return Err("native window pointer is null".to_string());
        }
        unsafe { iima_native_window_center_after_delay(window, delay_milliseconds) };
        Ok(())
    }

    pub fn configure_mini_player_window(window: *mut c_void) -> Result<(), String> {
        if window.is_null() {
            return Err("native Mini Player window pointer is null".to_string());
        }
        unsafe { iima_native_configure_mini_player_window(window) };
        Ok(())
    }

    pub fn path_is_on_local_volume(path: &str) -> bool {
        let Ok(path) = CString::new(path) else {
            return true;
        };
        unsafe { iima_native_path_is_on_local_volume(path.as_ptr()) != 0 }
    }

    pub fn attach_mpv_client(
        mpv_handle: *mut c_void,
        libmpv_path: &str,
        session: &str,
    ) -> Result<(), String> {
        let libmpv_path = CString::new(libmpv_path)
            .map_err(|_| "libmpv path contains an interior nul byte".to_string())?;
        let session = session_label(session)?;
        let result = unsafe {
            iima_native_video_attach_mpv_client(mpv_handle, libmpv_path.as_ptr(), session.as_ptr())
        };
        (result == 0)
            .then_some(())
            .ok_or_else(|| format!("failed to attach libmpv render context ({result})"))
    }

    pub fn detach_mpv_client(session: &str) {
        if let Ok(session) = session_label(session) {
            unsafe { iima_native_video_detach_mpv_client(session.as_ptr()) }
        }
    }

    pub fn remove_session(session: &str) {
        if let Ok(session) = session_label(session) {
            unsafe { iima_native_video_remove_session(session.as_ptr()) }
        }
    }

    pub fn configure_color(settings: &NativeVideoColorSettings, session: &str) {
        let algorithm = CString::new(settings.tone_mapping_algorithm.as_str())
            .unwrap_or_else(|_| CString::new("auto").expect("static algorithm is valid"));
        let Ok(session) = session_label(session) else {
            return;
        };
        unsafe {
            iima_native_video_configure_color(
                session.as_ptr(),
                c_int::from(settings.load_icc_profile),
                c_int::from(settings.enable_hdr_support),
                c_int::from(settings.enable_tone_mapping),
                settings
                    .tone_mapping_target_peak
                    .clamp(0, c_int::MAX as i64) as c_int,
                algorithm.as_ptr(),
            )
        }
    }

    pub fn request_color_refresh(session: &str) {
        if let Ok(session) = session_label(session) {
            unsafe { iima_native_video_request_color_refresh(session.as_ptr()) }
        }
    }

    pub fn set_hdr_enabled(enabled: bool, session: &str) {
        if let Ok(session) = session_label(session) {
            unsafe { iima_native_video_set_hdr_enabled(session.as_ptr(), c_int::from(enabled)) }
        }
    }

    pub fn toggle_pip(
        playing: bool,
        title: &str,
        session: &str,
        video_size: Option<(f64, f64)>,
        origin_fullscreen: bool,
    ) -> Result<(), String> {
        let title = CString::new(title)
            .map_err(|_| "media title contains an interior nul byte".to_string())?;
        let session = session_label(session)?;
        let (video_width, video_height) = video_size.unwrap_or((0.0, 0.0));
        let result = unsafe {
            iima_native_video_toggle_pip(
                session.as_ptr(),
                c_int::from(playing),
                title.as_ptr(),
                video_width,
                video_height,
                c_int::from(origin_fullscreen),
            )
        };
        (result == 0)
            .then_some(())
            .ok_or_else(|| format!("failed to toggle Picture in Picture ({result})"))
    }

    #[cfg(test)]
    pub fn plan_pip_replacement_rect(
        container_size: (f64, f64),
        video_size: (f64, f64),
    ) -> Result<(f64, f64, f64, f64), String> {
        let mut values = [0.0_f64; 4];
        let status = unsafe {
            iima_native_video_plan_pip_replacement_rect(
                container_size.0,
                container_size.1,
                video_size.0,
                video_size.1,
                values.as_mut_ptr(),
            )
        };
        (status == 0)
            .then_some((values[0], values[1], values[2], values[3]))
            .ok_or_else(|| format!("failed to plan Picture in Picture replacement rect ({status})"))
    }

    pub fn pip_is_active() -> bool {
        unsafe { iima_native_video_pip_is_active() != 0 }
    }

    pub fn pip_is_active_for_session(session: &str) -> bool {
        let Ok(session) = session_label(session) else {
            return false;
        };
        unsafe { iima_native_video_pip_is_active_for_session(session.as_ptr()) != 0 }
    }

    pub fn hdr_status(session: &str) -> NativeVideoHdrStatus {
        let Ok(session) = session_label(session) else {
            return NativeVideoHdrStatus {
                available: false,
                enabled: true,
            };
        };
        NativeVideoHdrStatus {
            available: unsafe { iima_native_video_hdr_is_available(session.as_ptr()) != 0 },
            enabled: unsafe { iima_native_video_hdr_is_enabled(session.as_ptr()) != 0 },
        }
    }

    pub fn status(session: &str) -> NativeVideoRendererStatus {
        let Ok(session) = session_label(session) else {
            return NativeVideoRendererStatus {
                installed: false,
                attached: false,
                pip_available: false,
                pip_active: false,
                backend: "opengl",
                render_scheduler: "unavailable",
            };
        };
        NativeVideoRendererStatus {
            installed: unsafe { iima_native_video_is_installed(session.as_ptr()) != 0 },
            attached: unsafe { iima_native_video_is_attached(session.as_ptr()) != 0 },
            pip_available: unsafe { iima_native_video_pip_is_available() != 0 },
            pip_active: pip_is_active_for_session(session.to_str().unwrap_or_default()),
            backend: "opengl",
            render_scheduler: super::native_video_render_scheduler_name(unsafe {
                iima_native_video_render_scheduler(session.as_ptr())
            }),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::ffi::c_void;

    use tauri::{AppHandle, Runtime};

    use super::{
        NativeVideoColorSettings, NativeVideoHdrStatus, NativeVideoRendererStatus,
        NativeVideoSurfaceSettings,
    };

    pub fn install(
        _host_view: *mut c_void,
        _session: &str,
        _settings: &NativeVideoSurfaceSettings,
    ) -> Result<(), String> {
        Ok(())
    }

    pub fn install_if_ready(
        _host_view: *mut c_void,
        _session: &str,
        _settings: &NativeVideoSurfaceSettings,
    ) -> Result<bool, String> {
        Ok(true)
    }

    pub fn center_window_after_delay(
        _window: *mut c_void,
        _delay_milliseconds: u64,
    ) -> Result<(), String> {
        Ok(())
    }

    pub fn configure_mini_player_window(_window: *mut c_void) -> Result<(), String> {
        Ok(())
    }

    pub fn path_is_on_local_volume(_path: &str) -> bool {
        true
    }

    pub fn attach_mpv_client(
        _mpv_handle: *mut c_void,
        _libmpv_path: &str,
        _session: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    pub fn detach_mpv_client(_session: &str) {}

    pub fn remove_session(_session: &str) {}

    pub fn configure_color(_settings: &NativeVideoColorSettings, _session: &str) {}

    pub fn request_color_refresh(_session: &str) {}

    pub fn set_hdr_enabled(_enabled: bool, _session: &str) {}

    pub fn toggle_pip(
        _playing: bool,
        _title: &str,
        _session: &str,
        _video_size: Option<(f64, f64)>,
        _origin_fullscreen: bool,
    ) -> Result<(), String> {
        Err("Picture in Picture is only available on macOS".to_string())
    }

    pub fn register_pip_will_close_emitter<R: Runtime>(_app: &AppHandle<R>) {}

    pub fn pip_is_active() -> bool {
        false
    }

    pub fn pip_is_active_for_session(_session: &str) -> bool {
        false
    }

    pub fn hdr_status(_session: &str) -> NativeVideoHdrStatus {
        NativeVideoHdrStatus {
            available: false,
            enabled: true,
        }
    }

    pub fn status(_session: &str) -> NativeVideoRendererStatus {
        NativeVideoRendererStatus {
            installed: false,
            attached: false,
            pip_available: false,
            pip_active: false,
            backend: "unavailable",
            render_scheduler: "unavailable",
        }
    }
}

pub use imp::{
    attach_mpv_client, center_window_after_delay, configure_color, configure_mini_player_window,
    detach_mpv_client, hdr_status, install, install_if_ready, path_is_on_local_volume,
    pip_is_active, pip_is_active_for_session, register_pip_will_close_emitter, remove_session,
    request_color_refresh, set_hdr_enabled, status, toggle_pip,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[cfg(target_os = "macos")]
    unsafe extern "C" {
        fn iima_native_video_hdr_color_space_kind(
            primaries: *const std::ffi::c_char,
            mac_major: std::ffi::c_int,
            mac_minor: std::ffi::c_int,
            mac_patch: std::ffi::c_int,
        ) -> std::ffi::c_int;
        fn iima_native_video_resolve_target_peak(
            configured_peak: std::ffi::c_int,
            reference_peak_hdr_luminance: std::ffi::c_int,
            display_backlight: std::ffi::c_int,
        ) -> std::ffi::c_int;
        fn iima_native_video_test_install_parent_result(
            parent_available: std::ffi::c_int,
        ) -> std::ffi::c_int;
        fn iima_native_video_test_render_scheduler(
            create_result: std::ffi::c_int,
            display_link_created: std::ffi::c_int,
            callback_result: std::ffi::c_int,
            start_result: std::ffi::c_int,
        ) -> std::ffi::c_int;
    }

    #[test]
    fn native_surface_errors_distinguish_retryable_window_readiness() {
        assert!(native_video_install_error(-1).contains("host view is null"));
        assert!(native_video_install_error(-2).contains("OpenGL view creation failed"));
        let retryable = native_video_install_error(-3);
        assert!(retryable.contains("not attached to an NSWindow"));
        assert!(retryable.contains("retryable"));
        assert_eq!(
            native_video_install_error(-99),
            "failed to install native video surface (-99)"
        );
        assert_eq!(native_video_install_readiness(0), Ok(true));
        assert_eq!(native_video_install_readiness(-3), Ok(false));
        assert!(native_video_install_readiness(-2).is_err());
    }

    #[test]
    fn native_render_scheduler_codes_have_stable_status_names() {
        assert_eq!(native_video_render_scheduler_name(0), "unavailable");
        assert_eq!(native_video_render_scheduler_name(1), "display-link");
        assert_eq!(native_video_render_scheduler_name(2), "appkit-invalidation");
        assert_eq!(native_video_render_scheduler_name(99), "unavailable");

        let status = NativeVideoRendererStatus {
            installed: true,
            attached: true,
            pip_available: true,
            pip_active: false,
            backend: "opengl",
            render_scheduler: native_video_render_scheduler_name(2),
        };
        let value = serde_json::to_value(status).expect("native renderer status should serialize");
        assert_eq!(value["render_scheduler"], "appkit-invalidation");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_surface_parent_and_display_scheduler_plans_are_retryable() {
        assert_eq!(
            unsafe { iima_native_video_test_install_parent_result(0) },
            -3
        );
        assert_eq!(
            unsafe { iima_native_video_test_install_parent_result(1) },
            0
        );

        let scheduler = |create_result, created, callback_result, start_result| unsafe {
            iima_native_video_test_render_scheduler(
                create_result,
                created,
                callback_result,
                start_result,
            )
        };
        assert_eq!(scheduler(0, 1, 0, 0), 1);
        assert_eq!(scheduler(-1, 0, 0, 0), 2);
        assert_eq!(scheduler(0, 0, 0, 0), 2);
        assert_eq!(scheduler(0, 1, -1, 0), 2);
        assert_eq!(scheduler(0, 1, 0, -1), 2);
    }

    #[test]
    fn color_settings_follow_iina_defaults() {
        let settings = color_settings_from_preferences(&PreferenceStore::default());

        assert_eq!(
            settings,
            NativeVideoColorSettings {
                load_icc_profile: true,
                enable_hdr_support: true,
                enable_tone_mapping: false,
                tone_mapping_target_peak: 0,
                tone_mapping_algorithm: "auto".to_string(),
            }
        );
    }

    #[test]
    fn surface_settings_follow_iina_gpu_switching_default() {
        let mut preferences = PreferenceStore::default();
        assert_eq!(
            surface_settings_from_preferences(&preferences),
            NativeVideoSurfaceSettings {
                force_dedicated_gpu: false,
            }
        );

        preferences
            .values
            .insert("forceDedicatedGPU".into(), json!(true));
        assert!(surface_settings_from_preferences(&preferences).force_dedicated_gpu);
    }

    #[test]
    fn color_settings_accept_iina_tone_mapping_menu_values() {
        let mut preferences = PreferenceStore::default();
        preferences
            .values
            .insert("enableToneMapping".into(), json!(true));
        preferences
            .values
            .insert("toneMappingTargetPeak".into(), json!(1000));
        preferences
            .values
            .insert("toneMappingAlgorithm".into(), json!(5));

        let settings = color_settings_from_preferences(&preferences);

        assert!(settings.enable_tone_mapping);
        assert_eq!(settings.tone_mapping_target_peak, 1000);
        assert_eq!(settings.tone_mapping_algorithm, "bt.2390");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn hdr_color_space_and_display_peak_resolution_match_iina_135() {
        fn color_space_kind(primaries: &[u8], major: i32, minor: i32, patch: i32) -> i32 {
            assert_eq!(primaries.last(), Some(&0));
            unsafe {
                iima_native_video_hdr_color_space_kind(
                    primaries.as_ptr().cast(),
                    major,
                    minor,
                    patch,
                )
            }
        }

        assert_eq!(color_space_kind(b"display-p3\0", 10, 15, 3), 2);
        assert_eq!(color_space_kind(b"display-p3\0", 10, 15, 4), 1);
        assert_eq!(color_space_kind(b"bt.2020\0", 10, 15, 3), 5);
        assert_eq!(color_space_kind(b"bt.2020\0", 10, 15, 4), 4);
        assert_eq!(color_space_kind(b"bt.2020\0", 11, 0, 0), 3);
        assert_eq!(color_space_kind(b"bt.709\0", 15, 0, 0), 0);
        assert_eq!(
            unsafe { iima_native_video_hdr_color_space_kind(std::ptr::null(), 15, 0, 0) },
            0
        );

        assert_eq!(
            unsafe { iima_native_video_resolve_target_peak(1_000, 1_600, 600) },
            1_000
        );
        assert_eq!(
            unsafe { iima_native_video_resolve_target_peak(0, 1_600, 600) },
            1_600
        );
        assert_eq!(
            unsafe { iima_native_video_resolve_target_peak(0, 0, 600) },
            600
        );
        assert_eq!(
            unsafe { iima_native_video_resolve_target_peak(0, 0, 0) },
            400
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_hdr_pipeline_preserves_iina_layer_peak_and_screen_refresh_contract() {
        let source = include_str!("native_video.m");
        let build = include_str!("../build.rs");

        for contract in [
            "#import <QuartzCore/CAOpenGLLayer.h>",
            "self.wantsExtendedDynamicRangeOpenGLSurface = YES;",
            "@selector(setColorspace:)",
            "openGLLayer.colorspace = colorSpace;",
            "setWantsExtendedDynamicRangeContent:extendedDynamicRange",
            "kCGColorSpaceDisplayP3_PQ",
            "kCGColorSpaceDisplayP3_PQ_EOTF",
            "kCGColorSpaceITUR_2100_PQ",
            "kCGColorSpaceITUR_2020_PQ",
            "kCGColorSpaceITUR_2020_PQ_EOTF",
            "CoreDisplay_DisplayCreateInfoDictionary",
            "@\"ReferencePeakHDRLuminance\"",
            "@\"DisplayBacklight\"",
            "NSWindowDidChangeScreenNotification",
            "NSApplicationDidChangeScreenParametersNotification",
            "NSScreenColorSpaceDidChangeNotification",
            "Moving the shared view between the player, Mini Player, and PIP",
            "maximumPotentialExtendedDynamicRangeColorComponentValue > 1.0",
            "_hasCurrentDisplay = NO;",
            "[self applyIccProfileForCurrentDisplay];",
        ] {
            assert!(
                source.contains(contract),
                "HDR contract is missing: {contract}"
            );
        }
        assert!(
            !source.contains("self.window.colorSpace"),
            "IINA 1.3.5 keeps output color space on CAOpenGLLayer and must not mutate NSWindow.colorSpace"
        );
        assert!(build.contains("cargo:rustc-link-lib=framework=QuartzCore"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_renderer_uses_child_window_and_pip_replacement_contract() {
        let source = include_str!("native_video.m");
        let commands = include_str!("commands.rs");

        assert!(source.contains("[parent addChildWindow:videoWindow ordered:NSWindowBelow];"));
        assert!(source.contains("parent.hasShadow = NO;"));
        assert!(source.contains("videoWindow.hasShadow = YES;"));
        assert!(source.contains("forKey:@\"replacementWindow\""));
        assert!(source.contains("forKey:@\"replacementRect\""));
        assert!(source.contains("dispatch_async(dispatch_get_main_queue()"));
        assert!(source.contains("void iima_native_window_center_after_delay"));
        assert!(source.contains("dispatch_after(dispatch_time(DISPATCH_TIME_NOW"));
        assert!(source.contains("dispatch_get_main_queue(), ^{"));
        assert!(source.contains("[window center];"));
        assert!(source.contains("NSOpenGLPFADoubleBuffer"));
        assert!(source.contains("NSOpenGLPFAAllowOfflineRenderers"));
        assert!(source.contains("NSOpenGLPFAColorFloat"));
        assert!(source.contains("NSOpenGLPFAColorSize, 64"));
        assert!(source.contains("NSOpenGLPFAOpenGLProfile, NSOpenGLProfileVersion3_2Core"));
        assert!(source.contains("NSOpenGLPFAAccelerated"));
        assert!(source.contains("NSOpenGLPixelFormatAttribute requestedAttributes[10]"));
        assert!(source.contains("NSInteger attributeCount = 8;"));
        assert!(source.contains("if (!forceDedicatedGPU)"));
        assert!(source.contains("kCGLPFASupportsAutomaticGraphicsSwitching"));
        assert!(source.contains("for (NSInteger length = attributeCount; length > 0; length--)"));
        assert!(source.contains("initWithFrame:host.bounds forceDedicatedGPU:forceDedicatedGPU"));
        assert!(commands.contains("native_video::center_window_after_delay("));
        assert!(commands.contains("window.ns_window().map_err(|error| error.to_string())?"));
        assert!(commands.contains("        120,"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_child_window_shape_preserves_native_corners_and_opengl_layer_ownership() {
        let source = include_str!("native_video.m");

        for contract in [
            "videoWindow.backgroundColor = NSColor.clearColor;",
            "videoWindow.opaque = NO;",
            "NSView *frameView = contentView.superview ?: contentView;",
            "frameLayer.backgroundColor = NSColor.blackColor.CGColor;",
            "frameLayer.cornerRadius = iima_native_video_corner_radius_for_style_mask(parent.styleMask);",
            "frameLayer.masksToBounds = YES;",
            "frameLayer.cornerCurve = kCACornerCurveContinuous;",
            "[videoWindow invalidateShadow];",
            "BOOL fullscreen = (styleMask & NSWindowStyleMaskFullScreen) != 0;",
            "BOOL titled = (styleMask & NSWindowStyleMaskTitled) != 0;",
            "return fullscreen || !titled ? 0.0 : 10.0;",
        ] {
            assert!(
                source.contains(contract),
                "native child-window shape contract is missing: {contract}"
            );
        }
        assert!(
            !source.contains("view.layer.cornerRadius"),
            "corner clipping must stay on the outer window frame, not the ICC/HDR CAOpenGLLayer"
        );

        let restore = source
            .split_once("static void iima_native_video_restore_from_pip")
            .and_then(|(_, suffix)| {
                suffix
                    .split_once("int iima_native_video_install")
                    .map(|(body, _)| body)
            })
            .expect("PIP restore segment");
        assert!(restore.contains("iima_native_video_sync_window_shape("));

        let apply = source
            .split_once("static BOOL iima_native_video_apply_window_frame")
            .and_then(|(_, suffix)| {
                suffix
                    .split_once("static void iima_native_video_schedule_window_frame_retry")
                    .map(|(body, _)| body)
            })
            .expect("child-window frame synchronization segment");
        assert!(apply.contains("iima_native_video_sync_window_shape(videoWindow, parent);"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_native_video_registry_access_is_main_thread_serialized() {
        let source = include_str!("native_video.m");

        assert!(source.contains("static void iima_native_video_run_on_main_sync"));
        assert!(source.contains("dispatch_sync(dispatch_get_main_queue(), block);"));
        assert!(source.contains(
            "static atomic_bool iima_native_video_sessions_initialized = ATOMIC_VAR_INIT(false);"
        ));
        assert!(source.contains("if (!iima_native_video_sessions_are_initialized())"));
        assert!(!source.contains(
            "iima_native_video_view_for_session(iima_native_video_session_key(sessionLabel))"
        ));

        for (start, end) in [
            (
                "int iima_native_video_attach_mpv_client",
                "void iima_native_video_detach_mpv_client",
            ),
            (
                "void iima_native_video_detach_mpv_client",
                "void iima_native_video_remove_session",
            ),
            (
                "void iima_native_video_configure_color",
                "void iima_native_video_request_color_refresh",
            ),
            (
                "void iima_native_video_set_hdr_enabled",
                "int iima_native_video_toggle_pip",
            ),
            (
                "int iima_native_video_toggle_pip",
                "int iima_native_video_pip_is_available",
            ),
            (
                "int iima_native_video_pip_is_available",
                "int iima_native_video_pip_is_active(void)",
            ),
            (
                "int iima_native_video_pip_is_active(void)",
                "int iima_native_video_pip_is_active_for_session",
            ),
            (
                "int iima_native_video_pip_is_active_for_session",
                "void iima_native_video_set_pip_will_close_callback",
            ),
            (
                "int iima_native_video_hdr_is_available",
                "int iima_native_video_hdr_is_enabled",
            ),
            (
                "int iima_native_video_hdr_is_enabled",
                "int iima_native_video_is_installed",
            ),
            (
                "int iima_native_video_is_installed",
                "int iima_native_video_is_attached",
            ),
            (
                "int iima_native_video_is_attached",
                "int iima_native_video_render_scheduler",
            ),
            (
                "int iima_native_video_render_scheduler",
                "void iima_native_window_center_after_delay",
            ),
        ] {
            let body = source
                .split_once(start)
                .and_then(|(_, suffix)| suffix.split_once(end).map(|(body, _)| body))
                .unwrap_or_else(|| panic!("missing native video ABI segment: {start}"));
            assert!(
                body.contains("iima_native_video_run_on_main_sync"),
                "native video ABI reads shared AppKit/session state outside the main-thread boundary: {start}"
            );
        }

        let refresh = source
            .split_once("void iima_native_video_request_color_refresh")
            .and_then(|(_, suffix)| {
                suffix
                    .split_once("void iima_native_video_set_hdr_enabled")
                    .map(|(body, _)| body)
            })
            .expect("request-color-refresh ABI segment");
        assert!(refresh.contains("dispatch_async(dispatch_get_main_queue(), refresh);"));
        assert!(refresh
            .contains("IIMANativeVideoView *view = iima_native_video_view_for_session(session);"));
        assert!(source.contains("iima_native_video_assert_main_thread();"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_renderer_install_and_scheduler_contract_is_retryable() {
        let source = include_str!("native_video.m");

        for contract in [
            "int parentResult = iima_native_video_install_parent_result(parent);",
            "result = parentResult;",
            "IIMANativeVideoInstallParentNotReady",
            "[self useAppKitInvalidationFallbackForStage:@\"create\" code:displayLinkResult];",
            "[self useAppKitInvalidationFallbackForStage:@\"callback\" code:callbackResult];",
            "[self useAppKitInvalidationFallbackForStage:@\"start\" code:result];",
            "_renderScheduler = IIMANativeVideoRenderSchedulerAppKitInvalidation;",
            "if (_displayLink == NULL || !CVDisplayLinkIsRunning(_displayLink))",
            "[self setNeedsDisplay:YES];",
            "int iima_native_video_render_scheduler(const char *sessionLabel)",
        ] {
            assert!(
                source.contains(contract),
                "retryable native renderer contract is missing: {contract}"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_child_window_live_resize_updates_each_main_queue_turn() {
        let source = include_str!("native_video.m");
        let observer_start = source
            .find("static void iima_native_video_observe_parent_window")
            .expect("parent-window observer should exist");
        let observer_end = source[observer_start..]
            .find("static IIMANativeVideoView *iima_native_video_view_for_session")
            .map(|offset| observer_start + offset)
            .expect("observer should end before the session-view helper");
        let observer = &source[observer_start..observer_end];

        for notification in [
            "NSWindowWillStartLiveResizeNotification",
            "NSWindowDidResizeNotification",
            "NSWindowDidEndLiveResizeNotification",
        ] {
            assert!(
                observer.contains(notification),
                "live-resize observer is missing {notification}"
            );
        }
        for contract in [
            "iima_native_video_live_resize_sessions",
            "iima_native_video_live_frame_updates",
            "iima_native_video_live_frame_update_generations",
            "iima_native_video_suspended_live_frame_updates",
            "dispatch_async(dispatch_get_main_queue()",
            "[videoWindow setFrame:targetFrame display:NO];",
            "[view requestRender];",
            // The end-live-resize path must use the full updater and force an OpenGL surface
            // refresh after the fast geometry-only turns.
            "[iima_native_video_force_surface_updates addObject:sessionKey];",
            "iima_native_video_schedule_final_window_frame_update(sessionKey);",
            "[context update];",
            // Non-interactive fullscreen/screen transitions retain the quiet-period path.
            "static const NSTimeInterval quietPeriod = 0.075;",
            "iima_native_video_next_frame_update_generation()",
            "iima_native_video_live_frame_update_generations[sessionKey] = @(generation);",
            "iima_native_video_live_frame_update_generations removeObjectForKey:sessionKey",
            "iima_native_video_frame_update_generations[sessionKey] = @(generation);",
            "pendingGeneration.unsignedLongLongValue != generation",
            "iima_native_video_frame_update_generations removeObjectForKey:sessionKey",
            "dispatch_after(",
            "iima_native_video_frame_retry_attempts[session] != nil",
            "@catch (NSException *exception)",
            "iima_native_video_schedule_window_frame_retry(session, 1);",
            "iima_native_video_apply_window_frame(sessionKey, forceSurfaceUpdate)",
            "static const NSUInteger maxAttempts = 3;",
        ] {
            assert!(
                source.contains(contract),
                "live/exception-safe frame synchronization contract is missing: {contract}"
            );
        }

        assert!(
            observer.contains("iima_native_video_schedule_live_window_frame_update(sessionKey);")
                || observer.contains("iima_native_video_schedule_live_frame_update(sessionKey);"),
            "DidResize must route live-resize events to a main-turn fast scheduler"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn fullscreen_pip_replacement_rect_centers_the_current_video_aspect() {
        let wide =
            super::imp::plan_pip_replacement_rect((1440.0, 900.0), (1920.0, 1080.0)).unwrap();
        assert_eq!(wide, (0.0, 45.0, 1440.0, 810.0));

        let portrait =
            super::imp::plan_pip_replacement_rect((1440.0, 900.0), (1080.0, 1920.0)).unwrap();
        assert_eq!(portrait, (466.875, 0.0, 506.25, 900.0));

        assert!(super::imp::plan_pip_replacement_rect((0.0, 900.0), (1920.0, 1080.0)).is_err());
    }
}
