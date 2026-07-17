use serde::{Deserialize, Serialize};

#[cfg(any(test, not(target_os = "macos")))]
pub const STABLE_APPCAST_URL: &str = "https://www.iina.io/appcast.xml";
#[cfg(test)]
pub const BETA_APPCAST_URL: &str = "https://www.iina.io/appcast-beta.xml";
pub const UPDATE_CHECK_INTERVALS: [f64; 4] = [3600.0, 86400.0, 604800.0, 2_629_800.0];

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct UpdaterStatus {
    pub available: bool,
    pub can_check_for_updates: bool,
    pub automatically_checks_for_updates: bool,
    pub update_check_interval: f64,
    pub receive_beta_updates: bool,
    pub feed_url: String,
    pub framework_version: String,
    pub error: Option<String>,
}

impl UpdaterStatus {
    #[cfg(not(target_os = "macos"))]
    fn unavailable(error: impl Into<String>) -> Self {
        Self {
            available: false,
            can_check_for_updates: false,
            automatically_checks_for_updates: false,
            update_check_interval: 86400.0,
            receive_beta_updates: false,
            feed_url: STABLE_APPCAST_URL.to_string(),
            framework_version: String::new(),
            error: Some(error.into()),
        }
    }
}

pub fn validated_update_interval(value: f64) -> Result<f64, String> {
    UPDATE_CHECK_INTERVALS
        .into_iter()
        .find(|candidate| (candidate - value).abs() < f64::EPSILON)
        .ok_or_else(|| {
            "Update check interval must be Hourly, Daily, Weekly, or Monthly".to_string()
        })
}

fn validated_feed_url(value: &str) -> Result<(), String> {
    let url =
        tauri::Url::parse(value).map_err(|_| "Updater returned an invalid feed URL".to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("Updater feed must use HTTPS".to_string());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{validated_feed_url, UpdaterStatus};
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_double, c_int};
    use std::ptr;

    unsafe extern "C" {
        fn iima_updater_initialize(
            receive_beta_updates: c_int,
            error_out: *mut *mut c_char,
        ) -> c_int;
        fn iima_updater_set_receive_beta(
            receive_beta_updates: c_int,
            error_out: *mut *mut c_char,
        ) -> c_int;
        fn iima_updater_set_automatic_checks(enabled: c_int, error_out: *mut *mut c_char) -> c_int;
        fn iima_updater_set_check_interval(
            interval: c_double,
            error_out: *mut *mut c_char,
        ) -> c_int;
        fn iima_updater_check_for_updates(error_out: *mut *mut c_char) -> c_int;
        fn iima_updater_status_json(error_out: *mut *mut c_char) -> *mut c_char;
        fn iima_updater_free_string(value: *mut c_char);
    }

    pub fn initialize(receive_beta_updates: bool) -> Result<(), String> {
        let mut error = ptr::null_mut();
        let status =
            unsafe { iima_updater_initialize(i32::from(receive_beta_updates), &mut error) };
        status_result(status, error, "Unable to initialize Sparkle")
    }

    pub fn set_receive_beta(receive_beta_updates: bool) -> Result<(), String> {
        let mut error = ptr::null_mut();
        let status =
            unsafe { iima_updater_set_receive_beta(i32::from(receive_beta_updates), &mut error) };
        status_result(status, error, "Unable to change the update channel")
    }

    pub fn set_automatic_checks(enabled: bool) -> Result<(), String> {
        let mut error = ptr::null_mut();
        let status = unsafe { iima_updater_set_automatic_checks(i32::from(enabled), &mut error) };
        status_result(status, error, "Unable to change automatic update checks")
    }

    pub fn set_check_interval(interval: f64) -> Result<(), String> {
        let mut error = ptr::null_mut();
        let status = unsafe { iima_updater_set_check_interval(interval, &mut error) };
        status_result(status, error, "Unable to change the update interval")
    }

    pub fn check_for_updates() -> Result<(), String> {
        let mut error = ptr::null_mut();
        let status = unsafe { iima_updater_check_for_updates(&mut error) };
        status_result(status, error, "Unable to check for updates")
    }

    pub fn status() -> Result<UpdaterStatus, String> {
        let mut error = ptr::null_mut();
        let json = unsafe { iima_updater_status_json(&mut error) };
        if json.is_null() {
            return Err(
                take_string(error).unwrap_or_else(|| "Unable to read updater status".to_string())
            );
        }
        let json = take_string(json).ok_or_else(|| "Updater status was empty".to_string())?;
        let status: UpdaterStatus = serde_json::from_str(&json)
            .map_err(|error| format!("Unable to decode updater status: {error}"))?;
        validated_feed_url(&status.feed_url)?;
        Ok(status)
    }

    fn status_result(status: c_int, error: *mut c_char, fallback: &str) -> Result<(), String> {
        if status == 0 {
            Ok(())
        } else {
            Err(take_string(error).unwrap_or_else(|| fallback.to_string()))
        }
    }

    fn take_string(value: *mut c_char) -> Option<String> {
        if value.is_null() {
            return None;
        }
        let result = unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned();
        unsafe { iima_updater_free_string(value) };
        Some(result)
    }
}

#[cfg(target_os = "macos")]
pub use platform::{
    check_for_updates, initialize, set_automatic_checks, set_check_interval, set_receive_beta,
    status,
};

#[cfg(not(target_os = "macos"))]
pub fn initialize(_receive_beta_updates: bool) -> Result<(), String> {
    Err("Sparkle is available only on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn set_receive_beta(_receive_beta_updates: bool) -> Result<(), String> {
    Err("Sparkle is available only on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn set_automatic_checks(_enabled: bool) -> Result<(), String> {
    Err("Sparkle is available only on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn set_check_interval(_interval: f64) -> Result<(), String> {
    Err("Sparkle is available only on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn check_for_updates() -> Result<(), String> {
    Err("Sparkle is available only on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn status() -> Result<UpdaterStatus, String> {
    Ok(UpdaterStatus::unavailable(
        "Sparkle is available only on macOS",
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        validated_feed_url, validated_update_interval, BETA_APPCAST_URL, STABLE_APPCAST_URL,
    };

    #[test]
    fn accepts_only_the_four_reference_update_intervals() {
        for interval in [3600.0, 86400.0, 604800.0, 2_629_800.0] {
            assert_eq!(validated_update_interval(interval).unwrap(), interval);
        }
        assert!(validated_update_interval(0.0).is_err());
        assert!(validated_update_interval(7200.0).is_err());
    }

    #[test]
    fn uses_the_reference_stable_and_beta_appcasts() {
        assert_eq!(STABLE_APPCAST_URL, "https://www.iina.io/appcast.xml");
        assert_eq!(BETA_APPCAST_URL, "https://www.iina.io/appcast-beta.xml");
    }

    #[test]
    fn accepts_only_https_update_feeds() {
        assert!(validated_feed_url("https://updates.example.test/appcast.xml").is_ok());
        assert!(validated_feed_url("http://updates.example.test/appcast.xml").is_err());
        assert!(
            validated_feed_url("https://user:password@updates.example.test/appcast.xml").is_err()
        );
        assert!(validated_feed_url("https://updates.example.test/appcast.xml#stable").is_err());
        assert!(validated_feed_url("not a url").is_err());
    }
}
