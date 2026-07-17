#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{c_char, CString};

    unsafe extern "C" {
        fn iima_native_decode_text(
            bytes: *const u8,
            length: usize,
            encoding_name: *const c_char,
            output: *mut *mut u8,
            output_length: *mut usize,
        ) -> i32;
        fn iima_native_decode_text_free(output: *mut u8);
    }

    pub fn decode(bytes: &[u8], encoding_name: &str) -> Result<String, String> {
        let encoding = CString::new(encoding_name)
            .map_err(|_| format!("Unknown encoding \"{encoding_name}\""))?;
        let mut output = std::ptr::null_mut();
        let mut output_length = 0_usize;
        let status = unsafe {
            iima_native_decode_text(
                bytes.as_ptr(),
                bytes.len(),
                encoding.as_ptr(),
                &mut output,
                &mut output_length,
            )
        };
        match status {
            0 => {
                let decoded = if output_length == 0 {
                    Ok(String::new())
                } else if output.is_null() {
                    Err("Cannot decode file: native text output is unavailable".to_string())
                } else {
                    let data = unsafe { std::slice::from_raw_parts(output, output_length) };
                    String::from_utf8(data.to_vec())
                        .map_err(|error| format!("Cannot decode file: {error}"))
                };
                if !output.is_null() {
                    unsafe { iima_native_decode_text_free(output) };
                }
                decoded
            }
            1 => Err(format!("Unknown encoding \"{encoding_name}\"")),
            2 => Err(format!(
                "Cannot decode file using encoding \"{encoding_name}\""
            )),
            _ => Err("Cannot decode file: native text conversion failed".to_string()),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn decode(bytes: &[u8], encoding_name: &str) -> Result<String, String> {
        if encoding_name != "utf8" {
            return Err(format!("Unknown encoding \"{encoding_name}\""));
        }
        String::from_utf8(bytes.to_vec()).map_err(|error| format!("Cannot decode file: {error}"))
    }
}

pub use imp::decode;

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn decodes_reference_utf8_name_case_sensitively() {
        assert_eq!(decode("IINA 同步".as_bytes(), "utf8").unwrap(), "IINA 同步");
        assert!(decode(b"IINA", "UTF8").is_err());
        assert!(decode(b"IINA", "utf-8").is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn decodes_representative_foundation_and_corefoundation_names() {
        assert_eq!(decode(&[0x49, 0x49, 0x4e, 0x41], "ascii").unwrap(), "IINA");
        assert_eq!(
            decode(&[0x63, 0x61, 0x66, 0xe9], "windowsCP1252").unwrap(),
            "café"
        );
        assert_eq!(
            decode(&[0xc4, 0xe3, 0xba, 0xc3], "GB_18030_2000").unwrap(),
            "你好"
        );
    }
}
