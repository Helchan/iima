fn decode_media_paths(payload: &str) -> Result<Vec<String>, String> {
    serde_json::from_str::<Vec<String>>(payload)
        .map_err(|error| format!("native media open panel returned invalid paths: {error}"))
}

#[cfg(target_os = "macos")]
mod imp {
    use super::decode_media_paths;
    use std::ffi::{c_char, CStr, CString};

    unsafe extern "C" {
        fn iima_native_open_media_panel(title: *const c_char) -> *mut c_char;
        fn iima_native_open_media_panel_free(paths: *mut c_char);
    }

    pub fn choose_media_paths(title: &str) -> Result<Option<Vec<String>>, String> {
        let title = CString::new(title)
            .map_err(|_| "native media open panel title contains a null byte".to_string())?;
        let paths = unsafe { iima_native_open_media_panel(title.as_ptr()) };
        if paths.is_null() {
            return Ok(None);
        }
        let result = unsafe { CStr::from_ptr(paths) }
            .to_str()
            .map_err(|error| format!("native media open panel returned invalid UTF-8: {error}"))
            .and_then(decode_media_paths);
        unsafe { iima_native_open_media_panel_free(paths) };
        result.map(Some)
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn choose_media_paths(_title: &str) -> Result<Option<Vec<String>>, String> {
        Err("Media file and directory selection is only available on macOS".to_string())
    }
}

pub use imp::choose_media_paths;

#[cfg(test)]
mod tests {
    use super::decode_media_paths;

    #[test]
    fn decodes_native_paths_without_delimiter_assumptions() {
        assert_eq!(
            decode_media_paths(r#"["/tmp/Folder","/tmp/line\nfeed.mp4"]"#).unwrap(),
            vec!["/tmp/Folder", "/tmp/line\nfeed.mp4"]
        );
        assert!(decode_media_paths("not-json").is_err());
    }
}
