use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlaylistPasteboardKind {
    Filenames,
    Urls,
    String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaylistPasteboardPayload {
    pub kind: PlaylistPasteboardKind,
    pub values: Vec<String>,
}

fn decode_payload(value: &str) -> Result<Option<PlaylistPasteboardPayload>, String> {
    if value == "null" {
        return Ok(None);
    }
    serde_json::from_str(value)
        .map(Some)
        .map_err(|error| format!("native playlist pasteboard returned invalid JSON: {error}"))
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{decode_payload, PlaylistPasteboardPayload};
    use std::ffi::{c_char, CStr, CString};
    use std::ptr;

    unsafe extern "C" {
        fn iima_native_playlist_pasteboard_write(
            indexes_json_utf8: *const c_char,
            paths_json_utf8: *const c_char,
            error_out: *mut *mut c_char,
        ) -> i32;
        fn iima_native_playlist_pasteboard_read(error_out: *mut *mut c_char) -> *mut c_char;
        fn iima_native_playlist_pasteboard_has_filenames() -> i32;
        fn iima_native_playlist_pasteboard_free(value: *mut c_char);
    }

    pub fn write(indexes: &[usize], paths: &[String]) -> Result<(), String> {
        let indexes =
            CString::new(serde_json::to_string(indexes).map_err(|error| error.to_string())?)
                .map_err(|_| "playlist indexes contain a NUL byte".to_string())?;
        let paths = CString::new(serde_json::to_string(paths).map_err(|error| error.to_string())?)
            .map_err(|_| "playlist paths contain a NUL byte".to_string())?;
        let mut error = ptr::null_mut();
        let status = unsafe {
            iima_native_playlist_pasteboard_write(indexes.as_ptr(), paths.as_ptr(), &mut error)
        };
        if status == 0 {
            Ok(())
        } else {
            Err(take_string(error).unwrap_or_else(|| "Unable to copy playlist items".to_string()))
        }
    }

    pub fn read() -> Result<Option<PlaylistPasteboardPayload>, String> {
        let mut error = ptr::null_mut();
        let value = unsafe { iima_native_playlist_pasteboard_read(&mut error) };
        if value.is_null() {
            return if error.is_null() {
                Ok(None)
            } else {
                Err(take_string(error)
                    .unwrap_or_else(|| "Unable to read playlist pasteboard".to_string()))
            };
        }
        let json = unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned();
        unsafe { iima_native_playlist_pasteboard_free(value) };
        decode_payload(&json)
    }

    pub fn has_filenames() -> bool {
        unsafe { iima_native_playlist_pasteboard_has_filenames() != 0 }
    }

    fn take_string(value: *mut c_char) -> Option<String> {
        if value.is_null() {
            return None;
        }
        let result = unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned();
        unsafe { iima_native_playlist_pasteboard_free(value) };
        Some(result)
    }
}

#[cfg(target_os = "macos")]
pub use platform::{has_filenames, read, write};

#[cfg(not(target_os = "macos"))]
pub fn write(_indexes: &[usize], _paths: &[String]) -> Result<(), String> {
    Err("Playlist pasteboard integration is available only on macOS".into())
}

#[cfg(not(target_os = "macos"))]
pub fn read() -> Result<Option<PlaylistPasteboardPayload>, String> {
    Ok(None)
}

#[cfg(not(target_os = "macos"))]
pub fn has_filenames() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_each_reference_pasteboard_payload_kind() {
        for (kind, expected_kind, value) in [
            ("filenames", PlaylistPasteboardKind::Filenames, "/tmp/a.mp4"),
            (
                "urls",
                PlaylistPasteboardKind::Urls,
                "https://example.com/a",
            ),
            (
                "string",
                PlaylistPasteboardKind::String,
                "rtsp://example.com/live",
            ),
        ] {
            let payload = format!(r#"{{"kind":"{kind}","values":["{value}"]}}"#);
            assert_eq!(
                decode_payload(&payload).unwrap(),
                Some(PlaylistPasteboardPayload {
                    kind: expected_kind,
                    values: vec![value.into()],
                })
            );
        }
        assert_eq!(decode_payload("null").unwrap(), None);
        assert!(decode_payload("not-json").is_err());
    }

    #[test]
    fn appkit_bridge_preserves_iina_type_priority_and_dual_copy_types() {
        let source = include_str!("native_pasteboard.m");
        let filenames = source
            .find("propertyListForType:IIMALegacyFilenamesType")
            .unwrap();
        let urls = source
            .find("propertyListForType:IIMALegacyURLType")
            .unwrap();
        let string = source.find("stringForType:NSPasteboardTypeString").unwrap();
        assert!(filenames < urls && urls < string);
        assert!(source.contains("IIMAPlaylistItemType, IIMALegacyFilenamesType"));
        assert!(source.contains("URLByResolvingAliasFileAtURL"));
    }
}
