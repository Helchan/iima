use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRemovalMode {
    Trash,
    Permanent,
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{FileRemovalMode, Path};
    use std::ffi::{c_char, CStr, CString};
    use std::ptr;

    unsafe extern "C" {
        fn iima_native_file_trash(path_utf8: *const c_char, error_out: *mut *mut c_char) -> i32;
        fn iima_native_file_remove(path_utf8: *const c_char, error_out: *mut *mut c_char) -> i32;
        fn iima_native_file_reveal_paths(
            paths_json_utf8: *const c_char,
            error_out: *mut *mut c_char,
        ) -> i32;
        fn iima_native_file_copy_text(text_utf8: *const c_char, error_out: *mut *mut c_char)
            -> i32;
        fn iima_native_file_free_string(value: *mut c_char);
    }

    pub fn reveal(paths: &[std::path::PathBuf]) -> Result<(), String> {
        if paths.is_empty() {
            return Ok(());
        }
        let paths = paths
            .iter()
            .map(|path| {
                path.to_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("File path is not valid UTF-8: {}", path.display()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let payload = serde_json::to_string(&paths).map_err(|error| error.to_string())?;
        call_string_operation(&payload, iima_native_file_reveal_paths)
    }

    pub fn copy_text(text: &str) -> Result<(), String> {
        call_string_operation(text, iima_native_file_copy_text)
    }

    fn call_string_operation(
        value: &str,
        operation: unsafe extern "C" fn(*const c_char, *mut *mut c_char) -> i32,
    ) -> Result<(), String> {
        let value = CString::new(value).map_err(|_| "Value contains a NUL byte".to_string())?;
        let mut error = ptr::null_mut();
        let status = unsafe { operation(value.as_ptr(), &mut error) };
        if status == 0 {
            return Ok(());
        }
        Err(take_string(error).unwrap_or_else(|| "Native file operation failed".to_string()))
    }

    pub fn remove(path: &Path, mode: FileRemovalMode) -> Result<(), String> {
        let path = path
            .to_str()
            .ok_or_else(|| format!("File path is not valid UTF-8: {}", path.display()))?;
        let path = CString::new(path).map_err(|_| "File path contains a NUL byte".to_string())?;
        let mut error = ptr::null_mut();
        let status = unsafe {
            match mode {
                FileRemovalMode::Trash => iima_native_file_trash(path.as_ptr(), &mut error),
                FileRemovalMode::Permanent => iima_native_file_remove(path.as_ptr(), &mut error),
            }
        };
        if status == 0 {
            return Ok(());
        }
        Err(take_string(error).unwrap_or_else(|| match mode {
            FileRemovalMode::Trash => "Unable to move the file to the Trash".to_string(),
            FileRemovalMode::Permanent => "Unable to delete the file".to_string(),
        }))
    }

    fn take_string(value: *mut c_char) -> Option<String> {
        if value.is_null() {
            return None;
        }
        let result = unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned();
        unsafe { iima_native_file_free_string(value) };
        Some(result)
    }
}

#[cfg(target_os = "macos")]
pub use platform::{copy_text, remove, reveal};

#[cfg(not(target_os = "macos"))]
pub fn remove(path: &Path, mode: FileRemovalMode) -> Result<(), String> {
    match mode {
        FileRemovalMode::Trash => {
            Err("Moving files to the Trash is available only on macOS".into())
        }
        FileRemovalMode::Permanent => std::fs::remove_file(path)
            .map_err(|error| format!("Unable to delete {}: {error}", path.display())),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn reveal(paths: &[std::path::PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        Ok(())
    } else {
        Err("Showing files in Finder is available only on macOS".into())
    }
}

#[cfg(not(target_os = "macos"))]
pub fn copy_text(_text: &str) -> Result<(), String> {
    Err("Copying playlist URLs is available only on macOS".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_file(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("iima-{name}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn native_permanent_removal_uses_file_manager_contract() {
        let path = temporary_file("hard-delete");
        fs::write(&path, b"fixture").unwrap();
        remove(&path, FileRemovalMode::Permanent).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn revealing_an_empty_batch_is_a_noop() {
        reveal(&[]).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_trash_bridge_reports_a_missing_item_without_crashing() {
        let path = temporary_file("missing-trash-item");
        let error = remove(&path, FileRemovalMode::Trash).unwrap_err();
        assert!(!error.trim().is_empty());
    }
}
