#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::c_int;

    unsafe extern "C" {
        fn iima_native_set_default_application(
            video: c_int,
            audio: c_int,
            playlist: c_int,
            success_count: *mut c_int,
            failed_count: *mut c_int,
        ) -> c_int;
    }

    pub fn set_default_application(
        video: bool,
        audio: bool,
        playlist: bool,
    ) -> Result<(i32, i32), String> {
        let mut success_count = 0;
        let mut failed_count = 0;
        let status = unsafe {
            iima_native_set_default_application(
                i32::from(video),
                i32::from(audio),
                i32::from(playlist),
                &mut success_count,
                &mut failed_count,
            )
        };
        if status == 0 {
            Ok((success_count, failed_count))
        } else {
            Err(format!(
                "Unable to read the bundled media type declarations (status {status})"
            ))
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn set_default_application(
        _video: bool,
        _audio: bool,
        _playlist: bool,
    ) -> Result<(i32, i32), String> {
        Err("Default application registration is only available on macOS".to_string())
    }
}

pub use imp::set_default_application;
