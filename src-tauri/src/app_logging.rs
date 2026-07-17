use md5::{Digest, Md5};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const LOG_IDENTIFIER: &str = "io.iima.player";
const TOKEN_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppLogRecord {
    pub subsystem: String,
    pub level: u8,
    pub message: String,
    pub date: String,
    pub log_string: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppLogSnapshot {
    pub revision: u64,
    pub records: Vec<AppLogRecord>,
    pub directory: String,
}

#[derive(Debug)]
struct AppLogStore {
    directory: PathBuf,
    enabled: bool,
    preferred_level: u8,
    revision: u64,
    records: Vec<AppLogRecord>,
}

impl AppLogStore {
    fn new(directory: PathBuf, enabled: bool, preferred_level: u8) -> Self {
        Self {
            directory,
            enabled,
            preferred_level: preferred_level.min(3),
            revision: 0,
            records: Vec::new(),
        }
    }

    fn append(&mut self, subsystem: &str, level: u8, message: &str) -> Result<(), String> {
        if level > 3 || level < self.preferred_level {
            return Ok(());
        }
        let (_, date) = current_date_and_time();
        let level_label = ["v", "d", "w", "e"][level as usize];
        let log_string = format!("{date} [{subsystem}][{level_label}] {message}\n");
        let record = AppLogRecord {
            subsystem: subsystem.to_string(),
            level,
            message: message.to_string(),
            date,
            log_string: log_string.clone(),
        };

        // IINA debug builds retain records for the built-in viewer even if file logging is off.
        // Release builds retain the same records only when the Advanced logging switch was enabled
        // at startup, matching Logger.enabled's startup-latched behavior.
        if cfg!(debug_assertions) || self.enabled {
            self.records.push(record);
            self.revision = self.revision.wrapping_add(1);
        }
        if self.enabled {
            fs::create_dir_all(&self.directory).map_err(|error| {
                format!(
                    "Unable to create log directory {}: {error}",
                    self.directory.display()
                )
            })?;
            let path = self.directory.join("iina.log");
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|error| format!("Unable to open {}: {error}", path.display()))?;
            file.write_all(log_string.as_bytes())
                .map_err(|error| format!("Unable to write {}: {error}", path.display()))?;
        }
        Ok(())
    }
}

static LOG_STORE: OnceLock<Mutex<AppLogStore>> = OnceLock::new();

pub fn initialize(home: &Path, enabled: bool, preferred_level: i64) -> Result<PathBuf, String> {
    let store = LOG_STORE.get_or_init(|| {
        Mutex::new(AppLogStore::new(
            make_session_directory(home),
            enabled,
            preferred_level.clamp(0, 3) as u8,
        ))
    });
    let directory = store
        .lock()
        .map_err(|error| error.to_string())?
        .directory
        .clone();
    if enabled {
        ensure_directory()?;
    }
    Ok(directory)
}

pub fn ensure_initialized(home: &Path) -> Result<PathBuf, String> {
    initialize(home, false, 1)
}

pub fn ensure_directory() -> Result<PathBuf, String> {
    let directory = directory()?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Unable to create {}: {error}", directory.display()))?;
    Ok(directory)
}

pub fn directory() -> Result<PathBuf, String> {
    LOG_STORE
        .get()
        .ok_or_else(|| "Application logger has not been initialized".to_string())?
        .lock()
        .map(|store| store.directory.clone())
        .map_err(|error| error.to_string())
}

pub fn mpv_log_path() -> Result<PathBuf, String> {
    directory().map(|directory| directory.join("mpv.log"))
}

pub fn log(subsystem: &str, level: u8, message: impl AsRef<str>) {
    let Some(store) = LOG_STORE.get() else {
        return;
    };
    if let Ok(mut store) = store.lock() {
        if let Err(error) = store.append(subsystem, level, message.as_ref()) {
            eprintln!("iima logger: {error}");
        }
    }
}

pub fn snapshot() -> Result<AppLogSnapshot, String> {
    let mut store = LOG_STORE
        .get()
        .ok_or_else(|| "Application logger has not been initialized".to_string())?
        .lock()
        .map_err(|error| error.to_string())?;
    ingest_mpv_log(&mut store);
    Ok(AppLogSnapshot {
        revision: store.revision,
        records: store.records.clone(),
        directory: store.directory.to_string_lossy().into_owned(),
    })
}

fn ingest_mpv_log(store: &mut AppLogStore) {
    let path = store.directory.join("mpv.log");
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };
    // The file belongs to this process session. Rebuilding the mpv portion on every viewer poll is
    // inexpensive for the bounded interactive log and avoids maintaining a byte cursor across mpv
    // truncation/reopen. App records are identified by their subsystem and retained separately.
    let app_records = store
        .records
        .iter()
        .filter(|record| !record.subsystem.starts_with("mpv"))
        .cloned()
        .collect::<Vec<_>>();
    let mut parsed = parse_mpv_log(&contents);
    parsed.splice(0..0, app_records);
    if parsed != store.records {
        store.records = parsed;
        store.revision = store.revision.wrapping_add(1);
    }
}

fn parse_mpv_log(contents: &str) -> Vec<AppLogRecord> {
    contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (level, message) = mpv_level_and_message(line);
            AppLogRecord {
                subsystem: "mpv0".to_string(),
                level,
                message: message.to_string(),
                date: mpv_time(line).unwrap_or_default().to_string(),
                log_string: format!("{line}\n"),
            }
        })
        .collect()
}

fn mpv_level_and_message(line: &str) -> (u8, &str) {
    for (marker, level) in [
        ("[v]", 0),
        ("[d]", 1),
        ("[w]", 2),
        ("[e]", 3),
        ("[fatal]", 3),
    ] {
        if let Some(index) = line.find(marker) {
            return (level, line[index + marker.len()..].trim_start());
        }
    }
    (1, line)
}

fn mpv_time(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let end = trimmed.find(']')?;
    trimmed.starts_with('[').then(|| trimmed[1..end].trim())
}

fn make_session_directory(home: &Path) -> PathBuf {
    let (directory_time, _) = current_date_and_time();
    let entropy = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        ^ u128::from(std::process::id());
    let mut hasher = Md5::new();
    hasher.update(entropy.to_le_bytes());
    let digest = hasher.finalize();
    let token = digest
        .iter()
        .take(6)
        .map(|byte| TOKEN_ALPHABET[*byte as usize % TOKEN_ALPHABET.len()] as char)
        .collect::<String>();
    session_directory(home, &directory_time, &token)
}

fn session_directory(home: &Path, directory_time: &str, token: &str) -> PathBuf {
    home.join("Library")
        .join("Logs")
        .join(LOG_IDENTIFIER)
        .join(format!("{directory_time}_{token}"))
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LocalTime {
    second: i32,
    minute: i32,
    hour: i32,
    day: i32,
    month: i32,
    year: i32,
    week_day: i32,
    year_day: i32,
    daylight_saving: i32,
    utc_offset: i64,
    zone: *const std::ffi::c_char,
}

#[cfg(unix)]
unsafe extern "C" {
    fn localtime_r(time: *const i64, result: *mut LocalTime) -> *mut LocalTime;
}

fn current_date_and_time() -> (String, String) {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = elapsed.as_secs() as i64;
    let millis = elapsed.subsec_millis();

    #[cfg(unix)]
    {
        let mut local = LocalTime {
            second: 0,
            minute: 0,
            hour: 0,
            day: 1,
            month: 0,
            year: 70,
            week_day: 0,
            year_day: 0,
            daylight_saving: 0,
            utc_offset: 0,
            zone: std::ptr::null(),
        };
        let converted = unsafe { localtime_r(&seconds, &mut local) };
        if !converted.is_null() {
            return (
                format!(
                    "{:04}-{:02}-{:02}-{:02}-{:02}-{:02}",
                    local.year + 1900,
                    local.month + 1,
                    local.day,
                    local.hour,
                    local.minute,
                    local.second
                ),
                format!(
                    "{:02}:{:02}:{:02}.{:03}",
                    local.hour, local.minute, local.second, millis
                ),
            );
        }
    }

    (format!("unix-{seconds}"), format!("{seconds}.{millis:03}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_directory_matches_iina_timestamp_and_six_character_token_shape() {
        assert_eq!(
            session_directory(Path::new("/Users/tester"), "2026-07-15-09-41-18", "aB3xY9"),
            PathBuf::from("/Users/tester/Library/Logs/io.iima.player/2026-07-15-09-41-18_aB3xY9")
        );
    }

    #[test]
    fn store_applies_iina_level_threshold_and_message_format() {
        let mut store = AppLogStore::new(PathBuf::from("/tmp/unused"), false, 1);
        store.append("iina", 0, "hidden").unwrap();
        store.append("iina", 1, "visible").unwrap();
        if cfg!(debug_assertions) {
            assert_eq!(store.records.len(), 1);
            assert_eq!(store.records[0].message, "visible");
            assert!(store.records[0].log_string.contains("[iina][d] visible\n"));
        }
    }

    #[test]
    fn parses_mpv_levels_and_time_without_losing_original_lines() {
        let records =
            parse_mpv_log("[   0.123][v][cplayer] verbose line\n[   0.456][e][ffmpeg] broken\n");
        assert_eq!(records.len(), 2);
        assert_eq!((records[0].level, records[0].date.as_str()), (0, "0.123"));
        assert_eq!(records[1].level, 3);
        assert_eq!(records[1].log_string, "[   0.456][e][ffmpeg] broken\n");
    }
}
