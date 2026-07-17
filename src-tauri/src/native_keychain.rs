use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HttpAuthCredentials {
    pub username: String,
    pub password: String,
}

#[cfg(target_os = "macos")]
mod platform {
    use super::HttpAuthCredentials;
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int};
    use std::ptr;

    unsafe extern "C" {
        fn iima_keychain_read_http_auth(
            server: *const c_char,
            port: c_int,
            username_out: *mut *mut c_char,
            password_out: *mut *mut c_char,
            error_out: *mut *mut c_char,
        ) -> c_int;
        fn iima_keychain_write_http_auth(
            server: *const c_char,
            port: c_int,
            username: *const c_char,
            password: *const c_char,
            error_out: *mut *mut c_char,
        ) -> c_int;
        fn iima_keychain_read_opensubtitles(
            username: *const c_char,
            password_out: *mut *mut c_char,
            error_out: *mut *mut c_char,
        ) -> c_int;
        fn iima_keychain_write_opensubtitles(
            username: *const c_char,
            password: *const c_char,
            error_out: *mut *mut c_char,
        ) -> c_int;
        fn iima_keychain_read_generic(
            service: *const c_char,
            account: *const c_char,
            password_out: *mut *mut c_char,
            error_out: *mut *mut c_char,
        ) -> c_int;
        fn iima_keychain_write_generic(
            service: *const c_char,
            account: *const c_char,
            password: *const c_char,
            error_out: *mut *mut c_char,
        ) -> c_int;
        fn iima_keychain_free_string(value: *mut c_char);
    }

    pub fn read(server: &str, port: Option<u16>) -> Result<Option<HttpAuthCredentials>, String> {
        let server =
            CString::new(server).map_err(|_| "Keychain server contains NUL".to_string())?;
        let mut username = ptr::null_mut();
        let mut password = ptr::null_mut();
        let mut error = ptr::null_mut();
        let status = unsafe {
            iima_keychain_read_http_auth(
                server.as_ptr(),
                port.map(i32::from).unwrap_or(0),
                &mut username,
                &mut password,
                &mut error,
            )
        };
        if status == 0 {
            return Ok(None);
        }
        if status < 0 {
            return Err(take_string(error).unwrap_or_else(|| "Unable to read Keychain".to_string()));
        }
        let username = take_string(username);
        let password = take_string(password);
        let username =
            username.ok_or_else(|| "Keychain did not return an HTTP username".to_string())?;
        let password =
            password.ok_or_else(|| "Keychain did not return an HTTP password".to_string())?;
        Ok(Some(HttpAuthCredentials { username, password }))
    }

    pub fn write(
        server: &str,
        port: Option<u16>,
        username: &str,
        password: &str,
    ) -> Result<(), String> {
        let server =
            CString::new(server).map_err(|_| "Keychain server contains NUL".to_string())?;
        let username =
            CString::new(username).map_err(|_| "Keychain username contains NUL".to_string())?;
        let password =
            CString::new(password).map_err(|_| "Keychain password contains NUL".to_string())?;
        let mut error = ptr::null_mut();
        let status = unsafe {
            iima_keychain_write_http_auth(
                server.as_ptr(),
                port.map(i32::from).unwrap_or(0),
                username.as_ptr(),
                password.as_ptr(),
                &mut error,
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err(take_string(error).unwrap_or_else(|| "Unable to write Keychain".to_string()))
        }
    }

    pub fn read_opensubtitles_password(username: &str) -> Result<Option<String>, String> {
        let username =
            CString::new(username).map_err(|_| "Keychain username contains NUL".to_string())?;
        let mut password = ptr::null_mut();
        let mut error = ptr::null_mut();
        let status = unsafe {
            iima_keychain_read_opensubtitles(username.as_ptr(), &mut password, &mut error)
        };
        if status == 0 {
            return Ok(None);
        }
        if status < 0 {
            return Err(take_string(error)
                .unwrap_or_else(|| "Unable to read OpenSubtitles Keychain account".to_string()));
        }
        take_string(password)
            .map(Some)
            .ok_or_else(|| "Keychain did not return an OpenSubtitles password".to_string())
    }

    pub fn write_opensubtitles_password(username: &str, password: &str) -> Result<(), String> {
        let username =
            CString::new(username).map_err(|_| "Keychain username contains NUL".to_string())?;
        let password =
            CString::new(password).map_err(|_| "Keychain password contains NUL".to_string())?;
        let mut error = ptr::null_mut();
        let status = unsafe {
            iima_keychain_write_opensubtitles(username.as_ptr(), password.as_ptr(), &mut error)
        };
        if status == 0 {
            Ok(())
        } else {
            Err(take_string(error)
                .unwrap_or_else(|| "Unable to write OpenSubtitles Keychain account".to_string()))
        }
    }

    pub fn read_generic_password(service: &str, account: &str) -> Result<Option<String>, String> {
        let service =
            CString::new(service).map_err(|_| "Keychain service contains NUL".to_string())?;
        let account =
            CString::new(account).map_err(|_| "Keychain account contains NUL".to_string())?;
        let mut password = ptr::null_mut();
        let mut error = ptr::null_mut();
        let status = unsafe {
            iima_keychain_read_generic(
                service.as_ptr(),
                account.as_ptr(),
                &mut password,
                &mut error,
            )
        };
        if status == 0 {
            return Ok(None);
        }
        if status < 0 {
            return Err(take_string(error)
                .unwrap_or_else(|| "Unable to read generic Keychain password".to_string()));
        }
        take_string(password)
            .map(Some)
            .ok_or_else(|| "Keychain did not return a generic password".to_string())
    }

    pub fn write_generic_password(
        service: &str,
        account: &str,
        password: &str,
    ) -> Result<(), String> {
        let service =
            CString::new(service).map_err(|_| "Keychain service contains NUL".to_string())?;
        let account =
            CString::new(account).map_err(|_| "Keychain account contains NUL".to_string())?;
        let password =
            CString::new(password).map_err(|_| "Keychain password contains NUL".to_string())?;
        let mut error = ptr::null_mut();
        let status = unsafe {
            iima_keychain_write_generic(
                service.as_ptr(),
                account.as_ptr(),
                password.as_ptr(),
                &mut error,
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err(take_string(error)
                .unwrap_or_else(|| "Unable to write generic Keychain password".to_string()))
        }
    }

    fn take_string(value: *mut c_char) -> Option<String> {
        if value.is_null() {
            return None;
        }
        let result = unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned();
        unsafe { iima_keychain_free_string(value) };
        Some(result)
    }
}

#[cfg(target_os = "macos")]
pub use platform::{
    read, read_generic_password, read_opensubtitles_password, write, write_generic_password,
    write_opensubtitles_password,
};

#[cfg(not(target_os = "macos"))]
pub fn read(_server: &str, _port: Option<u16>) -> Result<Option<HttpAuthCredentials>, String> {
    Err("HTTP credential Keychain is available only on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn write(
    _server: &str,
    _port: Option<u16>,
    _username: &str,
    _password: &str,
) -> Result<(), String> {
    Err("HTTP credential Keychain is available only on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn read_opensubtitles_password(_username: &str) -> Result<Option<String>, String> {
    Err("OpenSubtitles Keychain account is available only on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn write_opensubtitles_password(_username: &str, _password: &str) -> Result<(), String> {
    Err("OpenSubtitles Keychain account is available only on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn read_generic_password(_service: &str, _account: &str) -> Result<Option<String>, String> {
    Err("Generic Keychain passwords are available only on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn write_generic_password(
    _service: &str,
    _account: &str,
    _password: &str,
) -> Result<(), String> {
    Err("Generic Keychain passwords are available only on macOS".to_string())
}
