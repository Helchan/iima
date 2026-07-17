use crate::player::RecentDocument;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct NativeRecentDocument {
    pub url: String,
    pub path: String,
    pub title: String,
    #[serde(default)]
    pub bookmark: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RestoreReport {
    pub restored: bool,
    pub found_stale_bookmark: bool,
}

pub fn player_documents(entries: &[NativeRecentDocument]) -> Vec<RecentDocument> {
    let mut seen = HashSet::new();
    entries
        .iter()
        .filter(|entry| !entry.path.trim().is_empty() && seen.insert(entry.path.clone()))
        .take(10)
        .enumerate()
        .map(|(index, entry)| RecentDocument {
            id: index + 1,
            path: entry.path.clone(),
            title: entry.title.clone(),
        })
        .collect()
}

pub fn persistence_value(entries: &[NativeRecentDocument]) -> Value {
    Value::Array(
        entries
            .iter()
            .map(|entry| match entry.bookmark.as_deref() {
                Some(bookmark) if !bookmark.is_empty() => json!({ "bookmark": bookmark }),
                _ => json!({ "url": entry.url }),
            })
            .collect(),
    )
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{NativeRecentDocument, RestoreReport};
    use serde_json::Value;
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int};
    use std::ptr;

    unsafe extern "C" {
        fn iima_recent_documents_is_sonoma_or_newer() -> c_int;
        fn iima_recent_documents_snapshot_json(error_out: *mut *mut c_char) -> *mut c_char;
        fn iima_recent_documents_note(value: *const c_char, error_out: *mut *mut c_char) -> c_int;
        fn iima_recent_documents_clear(error_out: *mut *mut c_char) -> c_int;
        fn iima_recent_documents_restore_if_empty(
            json: *const c_char,
            restored_out: *mut c_int,
            stale_out: *mut c_int,
            error_out: *mut *mut c_char,
        ) -> c_int;
        fn iima_recent_documents_free_string(value: *mut c_char);
    }

    pub fn is_sonoma_or_newer() -> bool {
        unsafe { iima_recent_documents_is_sonoma_or_newer() != 0 }
    }

    pub fn snapshot() -> Result<Vec<NativeRecentDocument>, String> {
        let mut error = ptr::null_mut();
        let json = unsafe { iima_recent_documents_snapshot_json(&mut error) };
        if json.is_null() {
            return Err(
                take_string(error).unwrap_or_else(|| "Unable to read recent documents".to_string())
            );
        }
        let json =
            take_string(json).ok_or_else(|| "Recent document snapshot was empty".to_string())?;
        serde_json::from_str(&json)
            .map_err(|error| format!("Unable to decode recent documents: {error}"))
    }

    pub fn note(path_or_url: &str) -> Result<(), String> {
        let value = CString::new(path_or_url)
            .map_err(|_| "Recent document path contains NUL".to_string())?;
        let mut error = ptr::null_mut();
        let status = unsafe { iima_recent_documents_note(value.as_ptr(), &mut error) };
        if status == 0 {
            Ok(())
        } else {
            Err(take_string(error)
                .unwrap_or_else(|| "Unable to record recent document".to_string()))
        }
    }

    pub fn clear() -> Result<(), String> {
        let mut error = ptr::null_mut();
        let status = unsafe { iima_recent_documents_clear(&mut error) };
        if status == 0 {
            Ok(())
        } else {
            Err(take_string(error)
                .unwrap_or_else(|| "Unable to clear recent documents".to_string()))
        }
    }

    pub fn restore_if_empty(persisted: &Value) -> Result<RestoreReport, String> {
        let json = CString::new(
            serde_json::to_string(persisted)
                .map_err(|error| format!("Unable to encode recent document backup: {error}"))?,
        )
        .map_err(|_| "Recent document backup contains NUL".to_string())?;
        let mut restored = 0;
        let mut stale = 0;
        let mut error = ptr::null_mut();
        let status = unsafe {
            iima_recent_documents_restore_if_empty(
                json.as_ptr(),
                &mut restored,
                &mut stale,
                &mut error,
            )
        };
        if status == 0 {
            Ok(RestoreReport {
                restored: restored != 0,
                found_stale_bookmark: stale != 0,
            })
        } else {
            Err(take_string(error)
                .unwrap_or_else(|| "Unable to restore recent documents".to_string()))
        }
    }

    fn take_string(value: *mut c_char) -> Option<String> {
        if value.is_null() {
            return None;
        }
        let result = unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned();
        unsafe { iima_recent_documents_free_string(value) };
        Some(result)
    }
}

#[cfg(target_os = "macos")]
pub use platform::{clear, is_sonoma_or_newer, note, restore_if_empty, snapshot};

#[cfg(not(target_os = "macos"))]
pub fn is_sonoma_or_newer() -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn snapshot() -> Result<Vec<NativeRecentDocument>, String> {
    Err("NSDocumentController is available only on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn note(_path_or_url: &str) -> Result<(), String> {
    Err("NSDocumentController is available only on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn clear() -> Result<(), String> {
    Err("NSDocumentController is available only on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn restore_if_empty(_persisted: &Value) -> Result<RestoreReport, String> {
    Ok(RestoreReport::default())
}

#[cfg(test)]
mod tests {
    use super::{persistence_value, player_documents, NativeRecentDocument};
    use serde_json::json;

    #[test]
    fn converts_native_recent_documents_to_the_shared_player_model() {
        let entries = vec![
            NativeRecentDocument {
                url: "file:///tmp/new.mp4".to_string(),
                path: "/tmp/new.mp4".to_string(),
                title: "new.mp4".to_string(),
                bookmark: Some("bookmark-a".to_string()),
            },
            NativeRecentDocument {
                url: "file:///tmp/new.mp4".to_string(),
                path: "/tmp/new.mp4".to_string(),
                title: "duplicate.mp4".to_string(),
                bookmark: None,
            },
            NativeRecentDocument {
                url: "https://example.com/live".to_string(),
                path: "https://example.com/live".to_string(),
                title: "live".to_string(),
                bookmark: None,
            },
        ];

        let documents = player_documents(&entries);
        assert_eq!(documents.len(), 2);
        assert_eq!(documents[0].id, 1);
        assert_eq!(documents[0].path, "/tmp/new.mp4");
        assert_eq!(documents[1].id, 2);
        assert_eq!(documents[1].path, "https://example.com/live");
    }

    #[test]
    fn persists_bookmarks_with_url_fallbacks() {
        let entries = vec![
            NativeRecentDocument {
                url: "file:///tmp/movie.mp4".to_string(),
                path: "/tmp/movie.mp4".to_string(),
                title: "movie.mp4".to_string(),
                bookmark: Some("bookmark-data".to_string()),
            },
            NativeRecentDocument {
                url: "https://example.com/live".to_string(),
                path: "https://example.com/live".to_string(),
                title: "live".to_string(),
                bookmark: None,
            },
        ];

        assert_eq!(
            persistence_value(&entries),
            json!([
                { "bookmark": "bookmark-data" },
                { "url": "https://example.com/live" }
            ])
        );
    }
}
