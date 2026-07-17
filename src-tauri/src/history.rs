use md5::{Digest, Md5};
use plist::{Date, Dictionary, Integer, Uid, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StoredPlaybackHistoryEntry {
    #[serde(rename = "IINAPHUrl")]
    path: String,
    #[serde(rename = "IINAPHNme")]
    name: String,
    #[serde(rename = "IINAPHMpvmd5")]
    mpv_md5: String,
    #[serde(rename = "IINAPHPlayed")]
    played: bool,
    #[serde(rename = "IINAPHDate")]
    added_date: Date,
    #[serde(rename = "IINAPHDuration")]
    duration_seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlaybackHistoryItem {
    pub id: String,
    pub path: String,
    pub name: String,
    pub played: bool,
    pub added_date: String,
    pub duration_seconds: f64,
    pub progress_seconds: Option<f64>,
    pub file_exists: bool,
}

#[derive(Debug, Default)]
pub struct PlaybackHistoryStore {
    entries: Vec<StoredPlaybackHistoryEntry>,
    preserve_unreadable_source: bool,
}

impl PlaybackHistoryStore {
    pub fn load_or_recover(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        match read_playback_history(path) {
            Ok(entries) => Self {
                entries,
                preserve_unreadable_source: false,
            },
            Err(_) => Self {
                entries: Vec::new(),
                preserve_unreadable_source: true,
            },
        }
    }

    pub fn record(
        &mut self,
        path: String,
        name: String,
        duration_seconds: f64,
    ) -> PlaybackHistoryItem {
        self.record_at(path, name, duration_seconds, SystemTime::now())
    }

    fn record_at(
        &mut self,
        path: String,
        name: String,
        duration_seconds: f64,
        added_date: SystemTime,
    ) -> PlaybackHistoryItem {
        let mpv_md5 = mpv_watch_later_md5(&path);
        self.entries.retain(|entry| entry.mpv_md5 != mpv_md5);
        let entry = StoredPlaybackHistoryEntry {
            path,
            name,
            mpv_md5,
            played: true,
            added_date: added_date.into(),
            duration_seconds: finite_nonnegative(duration_seconds),
        };
        let item = playback_history_item(&entry, None);
        self.entries.insert(0, entry);
        item
    }

    pub fn items(&self, watch_later_directory: &Path) -> Vec<PlaybackHistoryItem> {
        self.entries
            .iter()
            .map(|entry| {
                let progress =
                    playback_progress_from_watch_later(&watch_later_directory.join(&entry.mpv_md5));
                playback_history_item(entry, progress)
            })
            .collect()
    }

    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        if self.preserve_unreadable_source && path.exists() {
            let backup = unreadable_history_backup_path(path);
            if !backup.exists() {
                fs::copy(path, &backup).map_err(|error| {
                    format!(
                        "Unable to preserve unreadable playback history at {}: {error}",
                        backup.display()
                    )
                })?;
            }
        }
        let temporary_path = path.with_extension("plist.tmp");
        keyed_archive_value(&self.entries)
            .to_file_binary(&temporary_path)
            .map_err(|error| format!("Unable to serialize playback history: {error}"))?;
        fs::rename(&temporary_path, path)
            .map_err(|error| format!("Unable to save playback history: {error}"))?;
        self.preserve_unreadable_source = false;
        Ok(())
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.preserve_unreadable_source = false;
    }

    pub fn remove(&mut self, ids: &[String]) -> usize {
        let ids = ids.iter().map(String::as_str).collect::<HashSet<_>>();
        let original_len = self.entries.len();
        self.entries
            .retain(|entry| !ids.contains(entry.mpv_md5.as_str()));
        original_len - self.entries.len()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

const APPLE_REFERENCE_DATE_UNIX_SECONDS: f64 = 978_307_200.0;

fn read_playback_history(path: &Path) -> Result<Vec<StoredPlaybackHistoryEntry>, String> {
    let value = Value::from_file(path).map_err(|error| error.to_string())?;
    if let Some(entries) = decode_keyed_archive(&value) {
        return Ok(entries);
    }
    plist::from_file::<_, Vec<StoredPlaybackHistoryEntry>>(path).map_err(|error| error.to_string())
}

fn keyed_archive_value(entries: &[StoredPlaybackHistoryEntry]) -> Value {
    let date_class_index = 2 + entries.len() * 6;
    let history_class_index = date_class_index + 1;
    let url_class_index = history_class_index + 1;
    let array_class_index = url_class_index + 1;
    let mut objects = vec![Value::String("$null".to_string())];
    let entry_references = entries
        .iter()
        .enumerate()
        .map(|(index, _)| uid_value(2 + index * 6))
        .collect::<Vec<_>>();
    objects.push(dictionary_value([
        ("$class", uid_value(array_class_index)),
        ("NS.objects", Value::Array(entry_references)),
    ]));

    for (index, entry) in entries.iter().enumerate() {
        let entry_index = 2 + index * 6;
        let url_index = entry_index + 1;
        let url_string_index = entry_index + 2;
        let name_index = entry_index + 3;
        let md5_index = entry_index + 4;
        let date_index = entry_index + 5;
        objects.extend([
            dictionary_value([
                ("$class", uid_value(history_class_index)),
                ("IINAPHUrl", uid_value(url_index)),
                ("IINAPHNme", uid_value(name_index)),
                ("IINAPHMpvmd5", uid_value(md5_index)),
                ("IINAPHPlayed", Value::Boolean(entry.played)),
                ("IINAPHDate", uid_value(date_index)),
                ("IINAPHDuration", Value::Real(entry.duration_seconds)),
            ]),
            dictionary_value([
                ("$class", uid_value(url_class_index)),
                ("NS.base", uid_value(0)),
                ("NS.relative", uid_value(url_string_index)),
            ]),
            Value::String(archived_url_string(&entry.path)),
            Value::String(entry.name.clone()),
            Value::String(entry.mpv_md5.clone()),
            dictionary_value([
                ("$class", uid_value(date_class_index)),
                (
                    "NS.time",
                    Value::Real(seconds_since_apple_reference_date(entry.added_date)),
                ),
            ]),
        ]);
    }

    objects.extend([
        class_value("NSDate"),
        class_value("IINA.PlaybackHistory"),
        class_value("NSURL"),
        class_value("NSArray"),
    ]);
    dictionary_value([
        ("$archiver", Value::String("NSKeyedArchiver".to_string())),
        ("$objects", Value::Array(objects)),
        ("$top", dictionary_value([("root", uid_value(1))])),
        ("$version", Value::Integer(Integer::from(100_000_i64))),
    ])
}

fn decode_keyed_archive(value: &Value) -> Option<Vec<StoredPlaybackHistoryEntry>> {
    let archive = value.as_dictionary()?;
    if archive.get("$archiver")?.as_string()? != "NSKeyedArchiver" {
        return None;
    }
    let objects = archive.get("$objects")?.as_array()?;
    let root_reference = archive.get("$top")?.as_dictionary()?.get("root")?;
    let root = referenced_object(root_reference, objects)?.as_dictionary()?;
    root.get("NS.objects")?
        .as_array()?
        .iter()
        .map(|reference| decode_keyed_history_entry(reference, objects))
        .collect()
}

fn decode_keyed_history_entry(
    reference: &Value,
    objects: &[Value],
) -> Option<StoredPlaybackHistoryEntry> {
    let entry = referenced_object(reference, objects)?.as_dictionary()?;
    let archived_url = referenced_dictionary_value(entry, "IINAPHUrl", objects)?
        .as_dictionary()?
        .get("NS.relative")?;
    let path = decoded_archived_url(referenced_string(archived_url, objects)?);
    let name = referenced_string(entry.get("IINAPHNme")?, objects)?.to_string();
    let mpv_md5 = referenced_string(entry.get("IINAPHMpvmd5")?, objects)?.to_string();
    let played = entry.get("IINAPHPlayed")?.as_boolean()?;
    let date = referenced_dictionary_value(entry, "IINAPHDate", objects)?.as_dictionary()?;
    let seconds = numeric_value(date.get("NS.time")?)?;
    let duration_seconds = numeric_value(entry.get("IINAPHDuration")?)?;
    Some(StoredPlaybackHistoryEntry {
        path,
        name,
        mpv_md5,
        played,
        added_date: system_time_from_apple_reference_seconds(seconds).into(),
        duration_seconds,
    })
}

fn referenced_dictionary_value<'a>(
    dictionary: &'a Dictionary,
    key: &str,
    objects: &'a [Value],
) -> Option<&'a Value> {
    referenced_object(dictionary.get(key)?, objects)
}

fn referenced_string<'a>(value: &'a Value, objects: &'a [Value]) -> Option<&'a str> {
    value
        .as_string()
        .or_else(|| referenced_object(value, objects)?.as_string())
}

fn referenced_object<'a>(reference: &Value, objects: &'a [Value]) -> Option<&'a Value> {
    let index = usize::try_from(reference.as_uid()?.get()).ok()?;
    objects.get(index)
}

fn numeric_value(value: &Value) -> Option<f64> {
    value
        .as_real()
        .or_else(|| value.as_signed_integer().map(|value| value as f64))
        .or_else(|| value.as_unsigned_integer().map(|value| value as f64))
        .filter(|value| value.is_finite())
}

fn dictionary_value<const N: usize>(entries: [(&str, Value); N]) -> Value {
    let mut dictionary = Dictionary::new();
    for (key, value) in entries {
        dictionary.insert(key.to_string(), value);
    }
    Value::Dictionary(dictionary)
}

fn class_value(name: &str) -> Value {
    dictionary_value([
        (
            "$classes",
            Value::Array(vec![
                Value::String(name.to_string()),
                Value::String("NSObject".to_string()),
            ]),
        ),
        ("$classname", Value::String(name.to_string())),
    ])
}

fn uid_value(index: usize) -> Value {
    Value::Uid(Uid::new(index as u64))
}

fn seconds_since_apple_reference_date(date: Date) -> f64 {
    let system_time: SystemTime = date.into();
    system_time_seconds_since_unix(system_time) - APPLE_REFERENCE_DATE_UNIX_SECONDS
}

fn system_time_from_apple_reference_seconds(seconds: f64) -> SystemTime {
    let unix_seconds = seconds + APPLE_REFERENCE_DATE_UNIX_SECONDS;
    if unix_seconds >= 0.0 {
        UNIX_EPOCH + Duration::from_secs_f64(unix_seconds)
    } else {
        UNIX_EPOCH - Duration::from_secs_f64(-unix_seconds)
    }
}

fn system_time_seconds_since_unix(time: SystemTime) -> f64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs_f64(),
        Err(error) => -error.duration().as_secs_f64(),
    }
}

fn archived_url_string(path: &str) -> String {
    if path.contains("://") {
        path.to_string()
    } else {
        format!("file://{}", percent_encode_path(path))
    }
}

fn decoded_archived_url(url: &str) -> String {
    let path = url
        .strip_prefix("file://localhost")
        .or_else(|| url.strip_prefix("file://"));
    path.map(percent_decode_path)
        .unwrap_or_else(|| url.to_string())
}

fn percent_encode_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b':' | b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

fn percent_decode_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let value = std::str::from_utf8(&bytes[index + 1..index + 3])
                .ok()
                .and_then(|value| u8::from_str_radix(value, 16).ok());
            if let Some(value) = value {
                decoded.push(value);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

pub fn mpv_watch_later_md5(path: &str) -> String {
    let mut digest = Md5::new();
    digest.update(path.as_bytes());
    format!("{:x}", digest.finalize())
}

pub fn playback_progress_from_watch_later(path: &Path) -> Option<f64> {
    let contents = fs::read_to_string(path).ok()?;
    let progress = contents
        .lines()
        .next()?
        .strip_prefix("start=")?
        .parse::<f64>()
        .ok()?;
    progress.is_finite().then_some(progress.max(0.0))
}

fn playback_history_item(
    entry: &StoredPlaybackHistoryEntry,
    progress_seconds: Option<f64>,
) -> PlaybackHistoryItem {
    PlaybackHistoryItem {
        id: entry.mpv_md5.clone(),
        path: entry.path.clone(),
        name: entry.name.clone(),
        played: entry.played,
        added_date: entry.added_date.to_xml_format(),
        duration_seconds: finite_nonnegative(entry.duration_seconds),
        progress_seconds,
        file_exists: entry.path.contains("://") || Path::new(&entry.path).exists(),
    }
}

fn finite_nonnegative(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn unreadable_history_backup_path(path: &Path) -> PathBuf {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("history.plist");
    path.with_file_name(format!("{filename}.iina-backup"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "iima-playback-history-{}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn records_deduplicates_and_roundtrips_iina_keyed_archive() {
        let root = temp_root("roundtrip");
        let history_path = root.join("history.plist");
        let watch_later = root.join("watch_later");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&watch_later).unwrap();
        let mut store = PlaybackHistoryStore::default();
        store.record_at(
            "/tmp/first.mp4".to_string(),
            "First".to_string(),
            120.0,
            UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        );
        store.record_at(
            "/tmp/first.mp4".to_string(),
            "First again".to_string(),
            121.0,
            UNIX_EPOCH + Duration::from_secs(1_700_000_001),
        );
        store.record_at(
            "/tmp/second.mkv".to_string(),
            "Second".to_string(),
            f64::NAN,
            UNIX_EPOCH + Duration::from_secs(1_700_000_002),
        );

        assert_eq!(store.len(), 2);
        store.save(&history_path).unwrap();
        if let Some(output) = std::env::var_os("IIMA_HISTORY_ARCHIVE_OUTPUT") {
            fs::copy(&history_path, output).unwrap();
        }
        assert!(fs::read(&history_path).unwrap().starts_with(b"bplist00"));
        let archive = Value::from_file(&history_path).unwrap();
        let objects = archive
            .as_dictionary()
            .unwrap()
            .get("$objects")
            .unwrap()
            .as_array()
            .unwrap();
        assert!(objects.iter().any(|object| object
            .as_dictionary()
            .and_then(|dictionary| dictionary.get("$classname"))
            .and_then(Value::as_string)
            == Some("IINA.PlaybackHistory")));
        assert!(objects.iter().any(|object| object
            .as_dictionary()
            .is_some_and(|dictionary| dictionary.contains_key("IINAPHUrl")
                && dictionary.contains_key("IINAPHMpvmd5"))));

        let loaded = PlaybackHistoryStore::load_or_recover(&history_path);
        let items = loaded.items(&watch_later);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].path, "/tmp/second.mkv");
        assert_eq!(items[0].duration_seconds, 0.0);
        assert_eq!(items[1].name, "First again");
        assert_eq!(items[1].duration_seconds, 121.0);
        assert_eq!(items[1].added_date, "2023-11-14T22:13:21Z");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_mpv_watch_later_progress_from_the_first_start_line() {
        let root = temp_root("progress");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = "/tmp/current.mp4";
        let resume_path = root.join(mpv_watch_later_md5(path));
        fs::write(&resume_path, "start=42.75\npause=yes\n").unwrap();
        let mut store = PlaybackHistoryStore::default();
        store.record(path.to_string(), "Current".to_string(), 100.0);

        assert_eq!(store.items(&root)[0].progress_seconds, Some(42.75));
        assert_eq!(
            mpv_watch_later_md5(path),
            "44a9edfe5e383b42439aeb711e4cb688"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preserves_an_unreadable_iina_archive_before_writing_new_history() {
        let root = temp_root("backup");
        let history_path = root.join("history.plist");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(&history_path, b"legacy keyed archive").unwrap();

        let mut store = PlaybackHistoryStore::load_or_recover(&history_path);
        store.record("/tmp/current.mp4".into(), "Current".into(), 10.0);
        store.save(&history_path).unwrap();

        assert_eq!(
            fs::read(root.join("history.plist.iina-backup")).unwrap(),
            b"legacy keyed archive"
        );
        assert!(fs::read(history_path).unwrap().starts_with(b"bplist00"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migrates_the_previous_plain_plist_and_preserves_file_url_characters() {
        let root = temp_root("plain-migration");
        let history_path = root.join("history.plist");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let mut original = PlaybackHistoryStore::default();
        original.record("/tmp/Some Folder/影片.mp4".into(), "影片.mp4".into(), 30.0);
        plist::to_file_xml(&history_path, &original.entries).unwrap();

        let mut migrated = PlaybackHistoryStore::load_or_recover(&history_path);
        assert_eq!(migrated.entries[0].path, "/tmp/Some Folder/影片.mp4");
        migrated.save(&history_path).unwrap();

        let reloaded = PlaybackHistoryStore::load_or_recover(&history_path);
        assert_eq!(reloaded.entries[0].path, "/tmp/Some Folder/影片.mp4");
        assert!(fs::read(history_path).unwrap().starts_with(b"bplist00"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn removes_selected_history_entries_by_stable_mpv_id() {
        let mut store = PlaybackHistoryStore::default();
        let first = store.record("/tmp/first.mp4".into(), "First".into(), 10.0);
        let second = store.record("/tmp/second.mp4".into(), "Second".into(), 20.0);

        assert_eq!(store.remove(std::slice::from_ref(&first.id)), 1);
        assert_eq!(store.len(), 1);
        assert_eq!(store.entries[0].mpv_md5, second.id);
        assert_eq!(store.remove(&["missing".to_string()]), 0);
    }

    #[test]
    fn reads_foundation_keyed_archive_when_requested() {
        let Some(path) = std::env::var_os("IIMA_HISTORY_ARCHIVE_INPUT") else {
            return;
        };
        let store = PlaybackHistoryStore::load_or_recover(Path::new(&path));

        assert_eq!(store.len(), 1);
        assert_eq!(store.entries[0].path, "/tmp/movie.mp4");
        assert_eq!(store.entries[0].name, "movie.mp4");
        assert_eq!(store.entries[0].mpv_md5, "abc");
        assert_eq!(store.entries[0].duration_seconds, 123.5);
    }
}
