#[cfg(target_os = "macos")]
mod imp {
    use crate::{commands, menu, state::AppState};
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::{Mutex, OnceLock};
    use tauri::{AppHandle, Manager};

    type DockOpenCallback = extern "C" fn();
    type ServiceOpenUrlCallback = extern "C" fn(*const c_char, *mut c_void);

    unsafe extern "C" {
        fn iima_native_application_bridge_install(
            dock_open_title: *const c_char,
            dock_open_callback: DockOpenCallback,
            service_open_url_callback: ServiceOpenUrlCallback,
        ) -> i32;
        fn iima_native_application_bridge_shutdown();
        fn iima_native_normalize_open_url_string(
            raw: *const c_char,
            output: *mut c_char,
            output_capacity: usize,
        ) -> isize;
    }

    fn installed_app() -> &'static Mutex<Option<AppHandle>> {
        static APP: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();
        APP.get_or_init(|| Mutex::new(None))
    }

    fn app_handle() -> Option<AppHandle> {
        installed_app()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    extern "C" fn dock_open_callback() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let Some(app) = app_handle() else {
                return;
            };
            // Keep the Dock item on the exact File > Open route so menu target resolution,
            // the native NSOpenPanel, alternate-window policy, and recent-document handling stay
            // owned by the existing shared implementation.
            menu::handle_iina_menu_event(&app, "iina.open");
        }));
    }

    extern "C" fn service_open_url_callback(url: *const c_char, main_window: *mut c_void) {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let Some(app) = app_handle() else {
                return;
            };
            let state = app.state::<AppState>();
            // AppDelegate.droppedText flips openFileCalled before openURLString attempts to parse
            // anything. Do the same even for an empty or otherwise invalid pasteboard string so
            // the delayed welcome/open-panel action cannot race a Services launch.
            state.note_external_open_request();
            if url.is_null() {
                return;
            }
            let Ok(raw) = (unsafe { CStr::from_ptr(url) }).to_str() else {
                return;
            };
            let normalized = match normalize_open_url_string(raw) {
                Ok(Some(normalized)) => normalized,
                Ok(None) => return,
                Err(error) => {
                    eprintln!("iima: unable to normalize URL from macOS Services: {error}");
                    return;
                }
            };
            if let Err(error) = commands::open_service_url_in_active_player(
                &app,
                state.inner(),
                main_window,
                normalized,
            ) {
                eprintln!("iima: unable to open URL from macOS Services: {error}");
            }
        }));
    }

    pub(crate) fn normalize_open_url_string(raw: &str) -> Result<Option<String>, String> {
        let raw = CString::new(raw)
            .map_err(|_| "IINA openURLString input contains a null byte".to_string())?;
        let length =
            unsafe { iima_native_normalize_open_url_string(raw.as_ptr(), std::ptr::null_mut(), 0) };
        if length == -1 {
            return Ok(None);
        }
        if length < 0 {
            return Err(format!(
                "Foundation openURLString sizing failed with status {length}"
            ));
        }
        let length = usize::try_from(length).map_err(|error| error.to_string())?;
        let mut output = vec![0_u8; length.saturating_add(1)];
        let written = unsafe {
            iima_native_normalize_open_url_string(
                raw.as_ptr(),
                output.as_mut_ptr().cast(),
                output.len(),
            )
        };
        if written != length as isize {
            return Err(format!(
                "Foundation openURLString write returned {written}, expected {length}"
            ));
        }
        output.truncate(length);
        String::from_utf8(output)
            .map(Some)
            .map_err(|error| format!("Foundation openURLString returned invalid UTF-8: {error}"))
    }

    pub fn install(app: &AppHandle) -> Result<(), String> {
        let title = CString::new(crate::localization::menu_title("Open..."))
            .map_err(|_| "localized Dock Open title contains a null byte".to_string())?;
        let previous = {
            let mut installed = installed_app()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            installed.replace(app.clone())
        };
        let installed = unsafe {
            iima_native_application_bridge_install(
                title.as_ptr(),
                dock_open_callback,
                service_open_url_callback,
            )
        };
        if installed == 0 {
            *installed_app()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = previous;
            return Err("failed to install the macOS Dock/Services bridge".to_string());
        }
        Ok(())
    }

    pub fn shutdown() {
        unsafe { iima_native_application_bridge_shutdown() };
        *installed_app()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn install(_app: &tauri::AppHandle) -> Result<(), String> {
        Ok(())
    }

    pub fn shutdown() {}

    pub(crate) fn normalize_open_url_string(raw: &str) -> Result<Option<String>, String> {
        Ok((!raw.is_empty()).then(|| raw.to_string()))
    }
}

#[cfg(test)]
pub(crate) use imp::normalize_open_url_string;
pub use imp::{install, shutdown};

#[cfg(test)]
mod tests {
    #[test]
    fn native_bridge_is_runtime_scoped_without_replacing_the_delegate_object() {
        let source = include_str!("native_application.m");
        assert!(source.contains("NSApp.servicesProvider = self"));
        assert!(source.contains("@selector(applicationDockMenu:)"));
        assert!(source.contains("@selector(openFile:)"));
        assert!(source.contains("stringForType:NSPasteboardTypeString"));
        assert!(source.contains("(__bridge void *)NSApp.mainWindow"));
        assert!(source.contains("IIMANormalizedOpenURLString"));
        assert!(source.contains("@available(macOS 14.0, *)"));
        assert!(source.contains("object_setClass(delegate, subclass)"));
        assert!(source.contains("object_setClass(delegate, self.originalDelegateClass)"));
        assert!(!source.contains("NSApp.delegate ="));
        assert!(!source.contains("setDelegate:"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn foundation_open_url_string_normalization_matches_iina_boundaries() {
        use super::normalize_open_url_string;

        assert_eq!(
            normalize_open_url_string("-").unwrap().as_deref(),
            Some("-")
        );
        assert_eq!(
            normalize_open_url_string("/tmp/a b/影片.mp4")
                .unwrap()
                .as_deref(),
            Some("/tmp/a b/影片.mp4")
        );
        assert_eq!(
            normalize_open_url_string("file:///tmp/a%20b/%E5%BD%B1%E7%89%87.mp4")
                .unwrap()
                .as_deref(),
            Some("/tmp/a b/影片.mp4")
        );
        assert_eq!(normalize_open_url_string("").unwrap(), None);
        assert_eq!(
            normalize_open_url_string("  ").unwrap().as_deref(),
            Some("%20%20")
        );
        assert_eq!(
            normalize_open_url_string("%").unwrap().as_deref(),
            Some("%25")
        );
        assert_eq!(normalize_open_url_string("file://%").unwrap(), None);
        assert_eq!(normalize_open_url_string("http://[").unwrap(), None);
        assert!(normalize_open_url_string("nul\0byte").is_err());
        let network = normalize_open_url_string("https://example.com/video path/影片.mp4?q=你 好")
            .unwrap()
            .unwrap();
        assert_eq!(
            network,
            "https://example.com/video%20path/%E5%BD%B1%E7%89%87.mp4?q=%E4%BD%A0%20%E5%A5%BD"
        );
    }
}
