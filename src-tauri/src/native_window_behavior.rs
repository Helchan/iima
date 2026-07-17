#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryStatus {
    pub capacity: u8,
    pub charging: bool,
}

pub const NATIVE_PLAYER_INPUT_EVENT: &str = "iima-native-player-input";
pub const NATIVE_MINI_PLAYER_LAYOUT_EVENT: &str = "iima-native-mini-player-layout";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeMiniPlayerLayout {
    pub width: f64,
    pub height: f64,
    pub video_height: f64,
    pub playlist_height: f64,
    pub playlist_visible: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NativeMiniPlayerLayoutEvent {
    pub video_visible: bool,
    pub playlist_visible: bool,
    pub width: f64,
    pub height: f64,
    pub video_height: f64,
    pub playlist_height: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum NativePlayerInputEvent {
    Scroll {
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
        precise: bool,
        natural: bool,
        phase: u64,
        momentum_phase: u64,
    },
    Pressure {
        x: f64,
        y: f64,
        stage: i32,
    },
    Magnify {
        x: f64,
        y: f64,
        magnification: f64,
        phase: u64,
    },
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{c_char, c_int, c_void, CStr, CString};
    use std::sync::OnceLock;

    use tauri::{AppHandle, Emitter, Runtime};

    use super::{
        BatteryStatus, NativeMiniPlayerLayout, NativeMiniPlayerLayoutEvent, NativePlayerInputEvent,
        NATIVE_MINI_PLAYER_LAYOUT_EVENT, NATIVE_PLAYER_INPUT_EVENT,
    };

    unsafe extern "C" {
        fn iima_native_configure_fullscreen_mode(window: *mut c_void, use_legacy: c_int);
        fn iima_native_configure_player_presentation(window: *mut c_void, initial: c_int) -> c_int;
        fn iima_native_sync_player_window_title(
            window: *mut c_void,
            represented_path: *const c_char,
            plain_title: *const c_char,
        ) -> c_int;
        fn iima_native_set_window_theme(window: *mut c_void, theme: c_int);
        fn iima_native_window_is_legacy_fullscreen(window: *mut c_void) -> c_int;
        fn iima_native_set_legacy_fullscreen(
            window: *mut c_void,
            enabled: c_int,
            animate_exit: c_int,
            video_width: f64,
            video_height: f64,
        ) -> c_int;
        fn iima_native_prepare_player_window_close(window: *mut c_void);
        fn iima_native_set_blackout_other_monitors(window: *mut c_void, enabled: c_int);
        fn iima_native_application_is_active() -> c_int;
        fn iima_native_read_battery_status(capacity: *mut c_int, charging: *mut c_int) -> c_int;
        fn iima_native_install_system_sleep_observer(
            callback: unsafe extern "C" fn(*mut c_void),
            context: *mut c_void,
        );
        fn iima_native_install_player_input_monitor(
            window: *mut c_void,
            label: *const c_char,
            callback: unsafe extern "C" fn(
                *const c_char,
                c_int,
                f64,
                f64,
                f64,
                f64,
                c_int,
                c_int,
                u64,
                u64,
                c_int,
                f64,
                *mut c_void,
            ),
            context: *mut c_void,
        );
        fn iima_native_remove_player_input_monitor(window: *mut c_void);
        fn iima_native_remove_all_player_input_monitors();
        fn iima_native_install_mini_player_layout_observer(
            window: *mut c_void,
            label: *const c_char,
            callback: unsafe extern "C" fn(
                *const c_char,
                c_int,
                c_int,
                f64,
                f64,
                f64,
                f64,
                *mut c_void,
            ),
            context: *mut c_void,
        );
        fn iima_native_apply_mini_player_layout(
            window: *mut c_void,
            video_visible: c_int,
            playlist_visible: c_int,
            video_aspect: f64,
            values: *mut f64,
        ) -> c_int;
        #[cfg(test)]
        fn iima_native_plan_mini_player_live_resize(
            origin_y: f64,
            current_height: f64,
            normal_height: f64,
            values: *mut f64,
        ) -> c_int;
        #[cfg(test)]
        fn iima_native_plan_legacy_exit_aspect_frame(
            frame_x: f64,
            frame_y: f64,
            frame_width: f64,
            frame_height: f64,
            video_width: f64,
            video_height: f64,
            values: *mut f64,
        ) -> c_int;
    }

    static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

    unsafe extern "C" fn system_will_sleep(_context: *mut c_void) {
        if let Some(app) = APP_HANDLE.get() {
            if let Err(error) = crate::commands::pause_all_players_for_sleep(app) {
                eprintln!("failed to apply pauseWhenGoesToSleep: {error}");
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    unsafe extern "C" fn player_input(
        label: *const c_char,
        kind: c_int,
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
        precise: c_int,
        natural: c_int,
        phase: u64,
        momentum_phase: u64,
        stage: c_int,
        magnification: f64,
        _context: *mut c_void,
    ) {
        if label.is_null() {
            return;
        }
        let Ok(label) = CStr::from_ptr(label).to_str() else {
            return;
        };
        let Some(app) = APP_HANDLE.get() else {
            return;
        };
        let event = match kind {
            1 => NativePlayerInputEvent::Scroll {
                x,
                y,
                delta_x,
                delta_y,
                precise: precise != 0,
                natural: natural != 0,
                phase,
                momentum_phase,
            },
            2 => NativePlayerInputEvent::Pressure { x, y, stage },
            3 => NativePlayerInputEvent::Magnify {
                x,
                y,
                magnification,
                phase,
            },
            _ => return,
        };
        let _ = app.emit_to(label, NATIVE_PLAYER_INPUT_EVENT, event);
    }

    #[allow(clippy::too_many_arguments)]
    unsafe extern "C" fn mini_player_layout_changed(
        label: *const c_char,
        video_visible: c_int,
        playlist_visible: c_int,
        width: f64,
        height: f64,
        video_height: f64,
        playlist_height: f64,
        _context: *mut c_void,
    ) {
        if label.is_null() {
            return;
        }
        let Ok(label) = CStr::from_ptr(label).to_str() else {
            return;
        };
        let Some(app) = APP_HANDLE.get() else {
            return;
        };
        let _ = app.emit_to(
            label,
            NATIVE_MINI_PLAYER_LAYOUT_EVENT,
            NativeMiniPlayerLayoutEvent {
                video_visible: video_visible != 0,
                playlist_visible: playlist_visible != 0,
                width,
                height,
                video_height,
                playlist_height,
            },
        );
    }

    fn require_window(window: *mut c_void) -> Result<*mut c_void, String> {
        (!window.is_null())
            .then_some(window)
            .ok_or_else(|| "native window pointer is null".to_string())
    }

    pub fn configure_fullscreen_mode(window: *mut c_void, use_legacy: bool) -> Result<(), String> {
        let window = require_window(window)?;
        unsafe { iima_native_configure_fullscreen_mode(window, c_int::from(use_legacy)) };
        Ok(())
    }

    pub fn configure_player_presentation(
        window: *mut c_void,
        initial: bool,
    ) -> Result<bool, String> {
        let window = require_window(window)?;
        let status =
            unsafe { iima_native_configure_player_presentation(window, c_int::from(initial)) };
        match status {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(format!(
                "failed to configure retained player presentation ({value})"
            )),
        }
    }

    pub fn sync_player_window_title(
        window: *mut c_void,
        represented_path: Option<&str>,
        plain_title: &str,
    ) -> Result<(), String> {
        let window = require_window(window)?;
        let represented_path = represented_path
            .map(|path| CString::new(path.replace('\0', "\u{fffd}")))
            .transpose()
            .map_err(|error| error.to_string())?;
        let plain_title = CString::new(plain_title.replace('\0', "\u{fffd}"))
            .map_err(|error| error.to_string())?;
        let represented_path = represented_path
            .as_ref()
            .map_or(std::ptr::null(), |path| path.as_ptr());
        let status = unsafe {
            iima_native_sync_player_window_title(window, represented_path, plain_title.as_ptr())
        };
        match status {
            0 => Ok(()),
            value => Err(format!(
                "failed to synchronize the native player window title ({value})"
            )),
        }
    }

    pub fn set_window_theme(window: *mut c_void, theme: i64) -> Result<(), String> {
        let window = require_window(window)?;
        unsafe { iima_native_set_window_theme(window, theme as c_int) };
        Ok(())
    }

    pub fn is_legacy_fullscreen(window: *mut c_void) -> bool {
        require_window(window)
            .map(|window| unsafe { iima_native_window_is_legacy_fullscreen(window) != 0 })
            .unwrap_or(false)
    }

    pub fn set_legacy_fullscreen(
        window: *mut c_void,
        enabled: bool,
        animate_exit: bool,
        video_size: Option<(f64, f64)>,
    ) -> Result<(), String> {
        let window = require_window(window)?;
        let (video_width, video_height) = video_size.unwrap_or((0.0, 0.0));
        let status = unsafe {
            iima_native_set_legacy_fullscreen(
                window,
                c_int::from(enabled),
                c_int::from(animate_exit),
                video_width,
                video_height,
            )
        };
        (status == 0)
            .then_some(())
            .ok_or_else(|| format!("failed to set legacy fullscreen ({status})"))
    }

    pub fn set_blackout(window: *mut c_void, enabled: bool) -> Result<(), String> {
        let window = require_window(window)?;
        unsafe { iima_native_set_blackout_other_monitors(window, c_int::from(enabled)) };
        Ok(())
    }

    pub fn prepare_player_window_close(window: *mut c_void) {
        if !window.is_null() {
            unsafe { iima_native_prepare_player_window_close(window) };
        }
    }

    pub fn application_is_active() -> bool {
        unsafe { iima_native_application_is_active() != 0 }
    }

    pub fn battery_status() -> Option<BatteryStatus> {
        let mut capacity = 0;
        let mut charging = 0;
        let status = unsafe { iima_native_read_battery_status(&mut capacity, &mut charging) };
        (status == 0).then_some(BatteryStatus {
            capacity: capacity.clamp(0, 100) as u8,
            charging: charging != 0,
        })
    }

    pub fn install_system_sleep_observer(app: &AppHandle) {
        let _ = APP_HANDLE.set(app.clone());
        unsafe {
            iima_native_install_system_sleep_observer(system_will_sleep, std::ptr::null_mut())
        };
    }

    pub fn install_player_input_monitor<R: Runtime>(
        _app: &AppHandle<R>,
        window: *mut c_void,
        label: &str,
    ) -> Result<(), String> {
        let window = require_window(window)?;
        let label = CString::new(label).map_err(|_| "window label contains NUL".to_string())?;
        unsafe {
            iima_native_install_player_input_monitor(
                window,
                label.as_ptr(),
                player_input,
                std::ptr::null_mut(),
            )
        };
        Ok(())
    }

    pub fn install_mini_player_layout_observer<R: Runtime>(
        _app: &AppHandle<R>,
        window: *mut c_void,
        label: &str,
    ) -> Result<(), String> {
        let window = require_window(window)?;
        let label = CString::new(label).map_err(|_| "window label contains NUL".to_string())?;
        unsafe {
            iima_native_install_mini_player_layout_observer(
                window,
                label.as_ptr(),
                mini_player_layout_changed,
                std::ptr::null_mut(),
            )
        };
        Ok(())
    }

    pub fn apply_mini_player_layout(
        window: *mut c_void,
        video_visible: bool,
        playlist_visible: bool,
        video_aspect: f64,
    ) -> Result<NativeMiniPlayerLayout, String> {
        let window = require_window(window)?;
        let mut values = [0.0_f64; 5];
        let status = unsafe {
            iima_native_apply_mini_player_layout(
                window,
                c_int::from(video_visible),
                c_int::from(playlist_visible),
                video_aspect,
                values.as_mut_ptr(),
            )
        };
        (status == 0)
            .then_some(NativeMiniPlayerLayout {
                width: values[0],
                height: values[1],
                video_height: values[2],
                playlist_height: values[3],
                playlist_visible: values[4] != 0.0,
            })
            .ok_or_else(|| format!("failed to apply native Mini Player layout ({status})"))
    }

    #[cfg(test)]
    pub fn plan_mini_player_live_resize(
        origin_y: f64,
        current_height: f64,
        normal_height: f64,
    ) -> Result<(f64, f64, bool), String> {
        let mut values = [0.0_f64; 3];
        let status = unsafe {
            iima_native_plan_mini_player_live_resize(
                origin_y,
                current_height,
                normal_height,
                values.as_mut_ptr(),
            )
        };
        (status == 0)
            .then_some((values[0], values[1], values[2] != 0.0))
            .ok_or_else(|| format!("failed to plan native Mini Player live resize ({status})"))
    }

    #[cfg(test)]
    pub fn plan_legacy_exit_aspect_frame(
        frame: (f64, f64, f64, f64),
        video_size: (f64, f64),
    ) -> Result<(f64, f64, f64, f64), String> {
        let mut values = [0.0_f64; 4];
        let status = unsafe {
            iima_native_plan_legacy_exit_aspect_frame(
                frame.0,
                frame.1,
                frame.2,
                frame.3,
                video_size.0,
                video_size.1,
                values.as_mut_ptr(),
            )
        };
        (status == 0)
            .then_some((values[0], values[1], values[2], values[3]))
            .ok_or_else(|| format!("failed to plan legacy fullscreen exit aspect ({status})"))
    }

    pub fn remove_player_input_monitor(window: *mut c_void) {
        if !window.is_null() {
            unsafe { iima_native_remove_player_input_monitor(window) };
        }
    }

    pub fn remove_all_player_input_monitors() {
        unsafe { iima_native_remove_all_player_input_monitors() };
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::ffi::c_void;

    use tauri::{AppHandle, Runtime};

    use super::NativeMiniPlayerLayout;

    pub fn configure_fullscreen_mode(
        _window: *mut c_void,
        _use_legacy: bool,
    ) -> Result<(), String> {
        Ok(())
    }

    pub fn configure_player_presentation(
        _window: *mut c_void,
        _initial: bool,
    ) -> Result<bool, String> {
        Ok(false)
    }

    pub fn sync_player_window_title(
        _window: *mut c_void,
        _represented_path: Option<&str>,
        _plain_title: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    pub fn is_legacy_fullscreen(_window: *mut c_void) -> bool {
        false
    }

    pub fn set_window_theme(_window: *mut c_void, _theme: i64) -> Result<(), String> {
        Ok(())
    }

    pub fn set_legacy_fullscreen(
        _window: *mut c_void,
        _enabled: bool,
        _animate_exit: bool,
        _video_size: Option<(f64, f64)>,
    ) -> Result<(), String> {
        Err("legacy fullscreen is available only on macOS".to_string())
    }

    pub fn set_blackout(_window: *mut c_void, _enabled: bool) -> Result<(), String> {
        Ok(())
    }

    pub fn prepare_player_window_close(_window: *mut c_void) {}

    pub fn application_is_active() -> bool {
        true
    }

    pub fn battery_status() -> Option<super::BatteryStatus> {
        None
    }

    pub fn install_system_sleep_observer(_app: &AppHandle) {}

    pub fn install_player_input_monitor<R: Runtime>(
        _app: &AppHandle<R>,
        _window: *mut c_void,
        _label: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    pub fn install_mini_player_layout_observer<R: Runtime>(
        _app: &AppHandle<R>,
        _window: *mut c_void,
        _label: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    pub fn apply_mini_player_layout(
        _window: *mut c_void,
        _video_visible: bool,
        playlist_visible: bool,
        _video_aspect: f64,
    ) -> Result<NativeMiniPlayerLayout, String> {
        Ok(NativeMiniPlayerLayout {
            width: 0.0,
            height: 0.0,
            video_height: 0.0,
            playlist_height: 0.0,
            playlist_visible,
        })
    }

    pub fn remove_player_input_monitor(_window: *mut c_void) {}

    pub fn remove_all_player_input_monitors() {}
}

pub use imp::*;

#[cfg(test)]
mod tests {
    #[test]
    fn native_contract_uses_real_appkit_window_and_sleep_surfaces() {
        let source = include_str!("native_window.m");
        for contract in [
            "NSWindowStyleMaskBorderless",
            "NSAppearanceNameDarkAqua",
            "NSApplicationPresentationAutoHideMenuBar",
            "NSWindowCollectionBehaviorFullScreenAuxiliary",
            "accessibilityDisplayShouldReduceMotion",
            "iima_native_plan_legacy_exit_aspect_frame",
            "iima_native_prepare_player_window_close",
            "iima_native_configure_player_presentation",
            "iima_native_sync_player_window_title",
            "window.releasedWhenClosed = NO;",
            "IINAWelcomeWindow",
            "window.representedURL = representedURL;",
            "[window setTitleWithRepresentedFilename:path];",
            "window.representedURL = nil;",
            "IIMALegacyState(window) != nil",
            "path.lastPathComponent",
            "window.titleVisibility = initial != 0 ? NSWindowTitleHidden : NSWindowTitleVisible;",
            "NSApp.presentationOptions = state.presentationOptions;",
            "[window setFrame:state.frame display:YES animate:shouldAnimate]",
            "NSScreen.screens",
            "safeAreaInsets.top",
            "NSMainMenuWindowLevel + 1",
            "NSWorkspaceWillSleepNotification",
            "IOPSCopyPowerSourcesInfo",
            "kIOPSInternalBatteryType",
            "NSEventMaskScrollWheel | NSEventMaskPressure | NSEventMaskMagnify",
            "event.hasPreciseScrollingDeltas",
            "event.isDirectionInvertedFromDevice",
            "NSWindowWillStartLiveResizeNotification",
            "NSWindowDidEndLiveResizeNotification",
            "normalHeight + IIMAMiniPlayerAutoHidePlaylistThreshold",
            "frame.origin.y += frame.size.height - targetHeight",
            "[window setFrame:frame display:YES animate:YES]",
        ] {
            assert!(
                source.contains(contract),
                "missing native contract: {contract}"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mini_player_live_resize_uses_strict_threshold_and_preserves_top_edge() {
        let collapsed = super::plan_mini_player_live_resize(100.0, 449.0, 250.0).unwrap();
        assert_eq!(collapsed, (299.0, 250.0, false));

        let expanded = super::plan_mini_player_live_resize(100.0, 450.0, 250.0).unwrap();
        assert_eq!(expanded, (100.0, 450.0, true));

        assert!(super::plan_mini_player_live_resize(0.0, 0.0, 250.0).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn legacy_fullscreen_exit_centers_the_current_display_aspect() {
        let wide =
            super::plan_legacy_exit_aspect_frame((100.0, 200.0, 1200.0, 900.0), (1920.0, 1080.0))
                .unwrap();
        assert_eq!(wide, (100.0, 312.5, 1200.0, 675.0));

        let portrait =
            super::plan_legacy_exit_aspect_frame((100.0, 200.0, 1200.0, 900.0), (1080.0, 1920.0))
                .unwrap();
        assert_eq!(portrait, (446.875, 200.0, 506.25, 900.0));

        assert!(
            super::plan_legacy_exit_aspect_frame((0.0, 0.0, 0.0, 900.0), (1920.0, 1080.0),)
                .is_err()
        );
    }
}
