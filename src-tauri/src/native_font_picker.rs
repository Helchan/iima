#[cfg(target_os = "macos")]
mod imp {
    use crate::localization;
    use std::ffi::{c_char, CStr, CString};

    unsafe extern "C" {
        fn iima_native_font_picker_choose(localizations_json: *const c_char) -> *mut c_char;
        fn iima_native_font_picker_free(font: *mut c_char);
    }

    pub fn choose_font() -> Result<Option<String>, String> {
        let localizations = serde_json::json!({
            "windowTitle": localization::menu_title("Choose a Font"),
            "chooseLabel": localization::menu_title("Choose a font:"),
            "searchPlaceholder": localization::menu_title("Type to filter..."),
            "otherLabel": localization::menu_title("Or enter the font name:"),
            "cancel": localization::menu_title("Cancel"),
            "confirm": localization::menu_title("OK"),
        })
        .to_string();
        let localizations = CString::new(localizations)
            .map_err(|_| "native font picker localization contains a NUL byte".to_string())?;
        let font = unsafe { iima_native_font_picker_choose(localizations.as_ptr()) };
        if font.is_null() {
            return Ok(None);
        }
        let value = unsafe { CStr::from_ptr(font) }
            .to_str()
            .map(str::to_owned)
            .map_err(|error| format!("native font picker returned invalid UTF-8: {error}"));
        unsafe { iima_native_font_picker_free(font) };
        value.map(Some)
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn choose_font() -> Result<Option<String>, String> {
        Ok(None)
    }
}

pub use imp::choose_font;
