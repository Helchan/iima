#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{c_char, CStr, CString};

    unsafe extern "C" {
        fn iima_native_prompt_text(
            title: *const c_char,
            message: *const c_char,
            initial_value: *const c_char,
            confirm_title: *const c_char,
            cancel_title: *const c_char,
        ) -> *mut c_char;
        fn iima_native_prompt_multiline_text(
            title: *const c_char,
            message: *const c_char,
            initial_value: *const c_char,
            confirm_title: *const c_char,
            cancel_title: *const c_char,
        ) -> *mut c_char;
        fn iima_native_prompt_free(value: *mut c_char);
        fn iima_native_confirm(
            title: *const c_char,
            confirm_title: *const c_char,
            cancel_title: *const c_char,
        ) -> i32;
        fn iima_native_show_error(title: *const c_char, message: *const c_char);
    }

    pub fn prompt_text(
        title: &str,
        message: &str,
        initial_value: &str,
        confirm_title: &str,
        cancel_title: &str,
    ) -> Result<Option<String>, String> {
        let title = c_string("prompt title", title)?;
        let message = c_string("prompt message", message)?;
        let initial_value = c_string("prompt initial value", initial_value)?;
        let confirm_title = c_string("prompt confirm title", confirm_title)?;
        let cancel_title = c_string("prompt cancel title", cancel_title)?;
        prompt_text_with(
            title,
            message,
            initial_value,
            confirm_title,
            cancel_title,
            iima_native_prompt_text,
        )
    }

    pub fn prompt_multiline_text(
        title: &str,
        message: &str,
        initial_value: &str,
        confirm_title: &str,
        cancel_title: &str,
    ) -> Result<Option<String>, String> {
        let title = c_string("prompt title", title)?;
        let message = c_string("prompt message", message)?;
        let initial_value = c_string("prompt initial value", initial_value)?;
        let confirm_title = c_string("prompt confirm title", confirm_title)?;
        let cancel_title = c_string("prompt cancel title", cancel_title)?;
        prompt_text_with(
            title,
            message,
            initial_value,
            confirm_title,
            cancel_title,
            iima_native_prompt_multiline_text,
        )
    }

    fn prompt_text_with(
        title: CString,
        message: CString,
        initial_value: CString,
        confirm_title: CString,
        cancel_title: CString,
        prompt: unsafe extern "C" fn(
            *const c_char,
            *const c_char,
            *const c_char,
            *const c_char,
            *const c_char,
        ) -> *mut c_char,
    ) -> Result<Option<String>, String> {
        let value = unsafe {
            prompt(
                title.as_ptr(),
                message.as_ptr(),
                initial_value.as_ptr(),
                confirm_title.as_ptr(),
                cancel_title.as_ptr(),
            )
        };
        if value.is_null() {
            return Ok(None);
        }
        let result = unsafe { CStr::from_ptr(value) }
            .to_str()
            .map(str::to_owned)
            .map_err(|error| format!("native prompt returned invalid UTF-8: {error}"));
        unsafe { iima_native_prompt_free(value) };
        result.map(Some)
    }

    fn c_string(label: &str, value: &str) -> Result<CString, String> {
        CString::new(value).map_err(|_| format!("{label} contains a NUL byte"))
    }

    pub fn confirm(title: &str, confirm_title: &str, cancel_title: &str) -> Result<bool, String> {
        let title = c_string("alert title", title)?;
        let confirm_title = c_string("alert confirm title", confirm_title)?;
        let cancel_title = c_string("alert cancel title", cancel_title)?;
        Ok(unsafe {
            iima_native_confirm(
                title.as_ptr(),
                confirm_title.as_ptr(),
                cancel_title.as_ptr(),
            ) != 0
        })
    }

    pub fn show_error(title: &str, message: &str) -> Result<(), String> {
        let title = c_string("alert title", title)?;
        let message = c_string("alert message", message)?;
        unsafe { iima_native_show_error(title.as_ptr(), message.as_ptr()) };
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn confirm(
        _title: &str,
        _confirm_title: &str,
        _cancel_title: &str,
    ) -> Result<bool, String> {
        Ok(false)
    }

    pub fn prompt_text(
        _title: &str,
        _message: &str,
        _initial_value: &str,
        _confirm_title: &str,
        _cancel_title: &str,
    ) -> Result<Option<String>, String> {
        Ok(None)
    }

    pub fn prompt_multiline_text(
        _title: &str,
        _message: &str,
        _initial_value: &str,
        _confirm_title: &str,
        _cancel_title: &str,
    ) -> Result<Option<String>, String> {
        Ok(None)
    }

    pub fn show_error(_title: &str, _message: &str) -> Result<(), String> {
        Ok(())
    }
}

pub use imp::{confirm, prompt_multiline_text, prompt_text, show_error};
