use plist::{Dictionary as PlistDictionary, Value as PlistValue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Number as JsonNumber, Value};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const PREFERENCES_FILE_NAME: &str = "preferences.json";
pub const IINA_USER_DEFAULTS_DOMAIN: &str = "com.colliderli.iina";
const PLIST_TAG_CONTAINER: &str = "__iimaUserDefaultsPlistValue";
const PLIST_TAG_TYPE: &str = "type";
const PLIST_TAG_VALUE: &str = "value";
const KEY_BINDING_MODEL_VERSION_KEY: &str = "keyBindingModelVersion";
const CURRENT_KEY_BINDING_MODEL_VERSION: u64 = 2;
pub const SAVED_VIDEO_FILTERS_KEY: &str = "savedVideoFilters";
pub const SAVED_AUDIO_FILTERS_KEY: &str = "savedAudioFilters";

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFilter {
    pub name: String,
    pub filter_string: String,
    pub shortcut_key: String,
    pub shortcut_key_modifiers: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PreferenceStore {
    pub values: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PreferenceChange {
    pub key: String,
    pub value: Value,
}

fn tagged_plist_value(kind: &str, value: Value) -> Value {
    let mut tagged = JsonMap::new();
    tagged.insert(PLIST_TAG_TYPE.into(), Value::String(kind.into()));
    tagged.insert(PLIST_TAG_VALUE.into(), value);
    let mut outer = JsonMap::new();
    outer.insert(PLIST_TAG_CONTAINER.into(), Value::Object(tagged));
    Value::Object(outer)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn decode_hex(encoded: &str) -> Result<Vec<u8>, String> {
    if !encoded.is_ascii() || encoded.len() % 2 != 0 {
        return Err("invalid hexadecimal plist data payload".into());
    }
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).expect("ASCII was checked above");
            u8::from_str_radix(pair, 16)
                .map_err(|_| "invalid hexadecimal plist data payload".to_string())
        })
        .collect()
}

fn plist_value_to_json(value: PlistValue) -> Result<Value, String> {
    match value {
        PlistValue::Array(values) => values
            .into_iter()
            .map(plist_value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        PlistValue::Dictionary(values) => {
            let mut converted = JsonMap::new();
            for (key, value) in values {
                converted.insert(key, plist_value_to_json(value)?);
            }
            Ok(Value::Object(converted))
        }
        PlistValue::Boolean(value) => Ok(Value::Bool(value)),
        PlistValue::Data(value) => Ok(tagged_plist_value("data", json!(encode_hex(&value)))),
        PlistValue::Date(value) => Ok(tagged_plist_value("date", json!(value.to_xml_format()))),
        PlistValue::Real(value) if value.is_finite() => JsonNumber::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| "plist real could not be represented as JSON".to_string()),
        PlistValue::Real(value) => {
            let spelling = if value.is_nan() {
                "nan"
            } else if value.is_sign_positive() {
                "+infinity"
            } else {
                "-infinity"
            };
            Ok(tagged_plist_value("real", json!(spelling)))
        }
        PlistValue::Integer(value) => {
            if let Some(value) = value.as_signed() {
                Ok(Value::Number(JsonNumber::from(value)))
            } else if let Some(value) = value.as_unsigned() {
                Ok(Value::Number(JsonNumber::from(value)))
            } else {
                Err("plist integer could not be represented as JSON".into())
            }
        }
        PlistValue::String(value) => Ok(Value::String(value)),
        PlistValue::Uid(value) => Ok(tagged_plist_value("uid", json!(value.get()))),
        _ => Err("unsupported value in IINA UserDefaults plist".into()),
    }
}

fn decode_tagged_plist_value(value: &Value) -> Option<Result<PlistValue, String>> {
    let outer = value.as_object()?;
    if outer.len() != 1 {
        return None;
    }
    let tagged = outer.get(PLIST_TAG_CONTAINER)?.as_object()?;
    if tagged.len() != 2 {
        return None;
    }
    let kind = tagged.get(PLIST_TAG_TYPE)?.as_str()?;
    let payload = tagged.get(PLIST_TAG_VALUE)?;
    Some(match kind {
        "data" => payload
            .as_str()
            .ok_or_else(|| "plist data tag payload is not a string".to_string())
            .and_then(decode_hex)
            .map(PlistValue::Data),
        "date" => payload
            .as_str()
            .ok_or_else(|| "plist date tag payload is not a string".to_string())
            .and_then(|value| {
                plist::Date::from_xml_format(value)
                    .map_err(|error| format!("invalid plist date tag: {error}"))
            })
            .map(PlistValue::Date),
        "uid" => payload
            .as_u64()
            .ok_or_else(|| "plist uid tag payload is not an unsigned integer".to_string())
            .map(plist::Uid::new)
            .map(PlistValue::Uid),
        "real" => payload
            .as_str()
            .ok_or_else(|| "plist real tag payload is not a string".to_string())
            .and_then(|value| match value {
                "nan" => Ok(f64::NAN),
                "+infinity" => Ok(f64::INFINITY),
                "-infinity" => Ok(f64::NEG_INFINITY),
                _ => Err("invalid non-finite plist real tag".to_string()),
            })
            .map(PlistValue::Real),
        _ => return None,
    })
}

/// Converts a JSON value to a property-list value. `None` means the JSON value contains `null`,
/// which has no UserDefaults representation; its owning top-level preference is omitted from the
/// compatibility mirror while remaining intact in the authoritative JSON file.
fn json_value_to_plist(value: &Value) -> Result<Option<PlistValue>, String> {
    if let Some(tagged) = decode_tagged_plist_value(value) {
        return tagged.map(Some);
    }

    match value {
        Value::Null => Ok(None),
        Value::Bool(value) => Ok(Some(PlistValue::Boolean(*value))),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(Some(PlistValue::Integer(value.into())))
            } else if let Some(value) = value.as_u64() {
                Ok(Some(PlistValue::Integer(value.into())))
            } else {
                value
                    .as_f64()
                    .map(PlistValue::Real)
                    .map(Some)
                    .ok_or_else(|| "JSON number could not be represented in a plist".to_string())
            }
        }
        Value::String(value) => Ok(Some(PlistValue::String(value.clone()))),
        Value::Array(values) => {
            let mut converted = Vec::with_capacity(values.len());
            for value in values {
                let Some(value) = json_value_to_plist(value)? else {
                    return Ok(None);
                };
                converted.push(value);
            }
            Ok(Some(PlistValue::Array(converted)))
        }
        Value::Object(values) => {
            let mut converted = PlistDictionary::new();
            for (key, value) in values {
                let Some(value) = json_value_to_plist(value)? else {
                    return Ok(None);
                };
                converted.insert(key.clone(), value);
            }
            Ok(Some(PlistValue::Dictionary(converted)))
        }
    }
}

/// Returns the flat binary-plist mirror beside the authoritative JSON store. The production name
/// matches IINA's UserDefaults domain; alternate JSON fixture names keep isolated sibling mirrors.
pub fn user_defaults_mirror_path(preferences_json_path: impl AsRef<Path>) -> PathBuf {
    let path = preferences_json_path.as_ref();
    if path.file_name() == Some(OsStr::new(PREFERENCES_FILE_NAME)) {
        path.with_file_name(format!("{IINA_USER_DEFAULTS_DOMAIN}.plist"))
    } else {
        path.with_extension("plist")
    }
}

/// Returns IINA 1.3.5's standard persistent-domain path for an explicit home directory.
pub fn iina_user_defaults_path(home_directory: impl AsRef<Path>) -> PathBuf {
    home_directory
        .as_ref()
        .join("Library")
        .join("Preferences")
        .join(format!("{IINA_USER_DEFAULTS_DOMAIN}.plist"))
}

pub fn detected_iina_user_defaults_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(iina_user_defaults_path)
}

impl Default for PreferenceStore {
    fn default() -> Self {
        let mut values = BTreeMap::new();
        values.insert("actionAfterLaunch".into(), json!(0));
        values.insert("alwaysOpenInNewWindow".into(), json!(true));
        values.insert("enableCmdN".into(), json!(false));
        values.insert("receiveBetaUpdate".into(), json!(false));
        values.insert("recordPlaybackHistory".into(), json!(true));
        values.insert("recordRecentFiles".into(), json!(true));
        values.insert("trackAllFilesInRecentOpenMenu".into(), json!(true));
        values.insert("recentDocuments".into(), json!([]));
        values.insert("watchProperties".into(), json!([]));
        values.insert("iinaLastPlayedFilePath".into(), Value::Null);
        values.insert("iinaLastPlayedFilePosition".into(), json!(0.0));
        values.insert("suppressCannotPreventDisplaySleep".into(), json!(false));
        values.insert("quitWhenNoOpenedWindow".into(), json!(false));
        values.insert("themeMaterial".into(), json!(0));
        values.insert("softVolume".into(), json!(100));
        values.insert("maxVolume".into(), json!(100));
        values.insert("pauseWhenOpen".into(), json!(false));
        values.insert("fullScreenWhenOpen".into(), json!(false));
        values.insert("keepOpenOnFileEnd".into(), json!(true));
        values.insert("resumeLastPosition".into(), json!(true));
        values.insert("useLegacyFullScreen".into(), json!(false));
        values.insert("legacyFullScreenAnimation".into(), json!(false));
        values.insert("blackOutMonitor".into(), json!(false));
        values.insert("pauseWhenMinimized".into(), json!(false));
        values.insert("pauseWhenInactive".into(), json!(false));
        values.insert("playWhenEnteringFullScreen".into(), json!(false));
        values.insert("pauseWhenLeavingFullScreen".into(), json!(false));
        values.insert("pauseWhenGoesToSleep".into(), json!(true));
        values.insert("usePhysicalResolution".into(), json!(true));
        values.insert("initialWindowSizePosition".into(), json!(""));
        values.insert("resizeWindowTiming".into(), json!(1));
        values.insert("resizeWindowOption".into(), json!(2));
        values.insert("alwaysFloatOnTop".into(), json!(false));
        values.insert("alwaysShowOnTopIcon".into(), json!(false));
        values.insert("oscPosition".into(), json!(0));
        values.insert("controlBarToolbarButtons".into(), json!([2, 1, 0]));
        values.insert("controlBarPositionHorizontal".into(), json!(0.5));
        values.insert("controlBarPositionVertical".into(), json!(0.1));
        values.insert("controlBarAutoHideTimeout".into(), json!(2.5));
        values.insert("controlBarStickToCenter".into(), json!(true));
        values.insert("showChapterPos".into(), json!(false));
        values.insert("arrowBtnAction".into(), json!(0));
        values.insert("showRemainingTime".into(), json!(false));
        values.insert("timeDisplayPrecision".into(), json!(0));
        values.insert("touchbarShowRemainingTime".into(), json!(true));
        values.insert("enableOSD".into(), json!(true));
        values.insert("osdAutoHideTimeout".into(), json!(1.0));
        values.insert("osdTextSize".into(), json!(20.0));
        values.insert("displayTimeAndBatteryInFullScreen".into(), json!(false));
        values.insert("playlistWidth".into(), json!(270));
        values.insert("prefetchPlaylistVideoDuration".into(), json!(true));
        values.insert("playlistAutoAdd".into(), json!(true));
        values.insert("playlistAutoPlayNext".into(), json!(true));
        values.insert("playlistShowMetadata".into(), json!(true));
        values.insert("playlistShowMetadataInMusicMode".into(), json!(true));
        values.insert("autoSwitchToMusicMode".into(), json!(true));
        values.insert("musicModeShowPlaylist".into(), json!(false));
        values.insert("musicModeShowAlbumArt".into(), json!(true));
        values.insert("enableThumbnailPreview".into(), json!(true));
        values.insert("maxThumbnailPreviewCacheSize".into(), json!(500));
        values.insert("windowBehaviorWhenPip".into(), json!(0));
        values.insert("pauseWhenPip".into(), json!(false));
        values.insert("togglePipByMinimizingWindow".into(), json!(false));
        values.insert("enableThumbnailForRemoteFiles".into(), json!(false));
        values.insert("thumbnailWidth".into(), json!(240));
        values.insert("videoThreads".into(), json!(0));
        values.insert("hardwareDecoder".into(), json!(1));
        values.insert("forceDedicatedGPU".into(), json!(false));
        values.insert("loadIccProfile".into(), json!(true));
        values.insert("enableHdrSupport".into(), json!(true));
        values.insert("enableToneMapping".into(), json!(false));
        values.insert("toneMappingTargetPeak".into(), json!(0));
        // The AppKit popup binds selectedTag, so persisted values are 0...7 even
        // though older defaults tables spelled the first value as "auto".
        values.insert("toneMappingAlgorithm".into(), json!(0));
        values.insert("audioThreads".into(), json!(0));
        values.insert("audioLanguage".into(), json!(""));
        values.insert("spdifAC3".into(), json!(false));
        values.insert("spdifDTS".into(), json!(false));
        values.insert("spdifDTSHD".into(), json!(false));
        values.insert("audioDevice".into(), json!("auto"));
        values.insert("audioDeviceDesc".into(), json!("Autoselect device"));
        values.insert("enableInitialVolume".into(), json!(false));
        values.insert("initialVolume".into(), json!(100));
        values.insert("subAutoLoadIINA".into(), json!(2));
        values.insert("subAutoLoadPriorityString".into(), json!(""));
        values.insert("subAutoLoadSearchPath".into(), json!("./*"));
        values.insert("ignoreAssStyles".into(), json!(false));
        values.insert("subOverrideLevel".into(), json!(2));
        values.insert("subTextFont".into(), json!("sans-serif"));
        values.insert("subTextSize".into(), json!(55.0));
        // New values use a small, deterministic RGBA codec. Imported IINA
        // NSColor archives remain tagged plist Data and are never rewritten.
        values.insert("subTextColor".into(), json!("1/1/1/1"));
        values.insert("subBgColor".into(), json!("0/0/0/0"));
        values.insert("subBold".into(), json!(false));
        values.insert("subItalic".into(), json!(false));
        values.insert("subBlur".into(), json!(0.0));
        values.insert("subSpacing".into(), json!(0.0));
        values.insert("subBorderSize".into(), json!(3.0));
        values.insert("subBorderColor".into(), json!("0/0/0/1"));
        values.insert("subShadowSize".into(), json!(0.0));
        values.insert("subShadowColor".into(), json!("0/0/0/0"));
        values.insert("subAlignX".into(), json!(1));
        values.insert("subAlignY".into(), json!(2));
        values.insert("subMarginX".into(), json!(25.0));
        values.insert("subMarginY".into(), json!(22.0));
        values.insert("subPos".into(), json!(100.0));
        values.insert("displayInLetterBox".into(), json!(true));
        values.insert("subScaleWithWindow".into(), json!(true));
        values.insert("onlineSubProvider".into(), json!(":opensubtitles"));
        // IINA keeps this pre-provider-string key for UserDefaults compatibility.
        values.insert("onlineSubSource".into(), json!(1));
        values.insert("openSubUsername".into(), json!(""));
        values.insert("subLang".into(), json!(""));
        values.insert("assrtToken".into(), json!(""));
        values.insert("autoSearchOnlineSub".into(), json!(false));
        values.insert("autoSearchThreshold".into(), json!(20));
        values.insert("defaultEncoding".into(), json!("auto"));
        values.insert("enableCache".into(), json!(true));
        values.insert("defaultCacheSize".into(), json!(153_600));
        values.insert("cacheBufferSize".into(), json!(153_600));
        values.insert("secPrefech".into(), json!(36_000));
        values.insert("userAgent".into(), json!(""));
        values.insert("transportRTSPThrough".into(), json!(1));
        values.insert("ytdlEnabled".into(), json!(true));
        values.insert("ytdlSearchPath".into(), json!(""));
        values.insert("ytdlRawOptions".into(), json!(""));
        values.insert("httpProxy".into(), json!(""));
        values.insert("useExactSeek".into(), json!(0));
        values.insert("followGlobalSeekTypeWhenAdjustSlider".into(), json!(false));
        values.insert("verticalScrollAction".into(), json!(0));
        values.insert("horizontalScrollAction".into(), json!(1));
        values.insert("singleClickAction".into(), json!(3));
        values.insert("doubleClickAction".into(), json!(1));
        values.insert("rightClickAction".into(), json!(2));
        values.insert("middleClickAction".into(), json!(0));
        values.insert("pinchAction".into(), json!(0));
        values.insert("forceTouchAction".into(), json!(0));
        values.insert("videoViewAcceptsFirstMouse".into(), json!(false));
        values.insert("relativeSeekAmount".into(), json!(3));
        values.insert("volumeScrollAmount".into(), json!(3));
        values.insert("useMediaKeys".into(), json!(true));
        values.insert("useAppleRemote".into(), json!(false));
        // Retain the 1.3.5 legacy profile dictionary alongside the v2 model.
        values.insert("inputConfigs".into(), json!({}));
        values.insert("currentInputConfigName".into(), json!("IINA Default"));
        values.insert("displayKeyBindingRawValues".into(), json!(false));
        values.insert("modeledKeyBindings".into(), Value::Null);
        values.insert(
            KEY_BINDING_MODEL_VERSION_KEY.into(),
            json!(CURRENT_KEY_BINDING_MODEL_VERSION),
        );
        values.insert("enableAdvancedSettings".into(), json!(false));
        values.insert("useMpvOsd".into(), json!(false));
        values.insert("enableLogging".into(), json!(false));
        values.insert("logLevel".into(), json!(1));
        values.insert("userOptions".into(), json!([]));
        values.insert("useUserDefinedConfDir".into(), json!(false));
        values.insert("userDefinedConfDir".into(), json!("~/.config/mpv/"));
        values.insert("iinaEnablePluginSystem".into(), json!(false));
        values.insert("screenshotSaveToFile".into(), json!(true));
        values.insert("screenshotCopyToClipboard".into(), json!(false));
        values.insert("screenShotFolder".into(), json!("~/Pictures/Screenshots"));
        values.insert("screenShotIncludeSubtitle".into(), json!(true));
        values.insert("screenShotFormat".into(), json!(0));
        values.insert("screenShotTemplate".into(), json!("%F-%n"));
        values.insert("screenshotShowPreview".into(), json!(true));
        values.insert(SAVED_VIDEO_FILTERS_KEY.into(), json!([]));
        values.insert(SAVED_AUDIO_FILTERS_KEY.into(), json!([]));
        Self { values }
    }
}

impl PreferenceStore {
    pub fn set(&mut self, change: PreferenceChange) {
        self.values.insert(change.key, change.value);
    }

    pub fn saved_filters(&self, key: &str) -> Vec<SavedFilter> {
        self.values
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|value| serde_json::from_value::<SavedFilter>(value.clone()).ok())
            .filter(|filter| {
                !filter.name.trim().is_empty()
                    && !filter.filter_string.trim().is_empty()
                    && !filter.name.contains('\0')
                    && !filter.filter_string.contains('\0')
            })
            .collect()
    }

    pub fn merged_with_defaults(self) -> Self {
        let mut merged = Self::default();
        for (key, value) in self.values {
            merged.values.insert(key, value);
        }
        merged
    }

    fn migrated_legacy_key_bindings(mut self) -> Self {
        let model_version = self
            .values
            .get(KEY_BINDING_MODEL_VERSION_KEY)
            .and_then(Value::as_u64)
            .unwrap_or(1);
        if model_version < CURRENT_KEY_BINDING_MODEL_VERSION {
            // Older builds used [] as a sentinel for the bundled IINA defaults,
            // so an empty array could not represent a deliberately empty profile.
            if self
                .values
                .get("modeledKeyBindings")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            {
                self.values.insert("modeledKeyBindings".into(), Value::Null);
            }
            self.values.insert(
                KEY_BINDING_MODEL_VERSION_KEY.into(),
                json!(CURRENT_KEY_BINDING_MODEL_VERSION),
            );
        }
        self
    }

    fn load_json_file(path: &Path) -> Result<Self, String> {
        let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
        serde_json::from_str::<Self>(&raw)
            .map(Self::migrated_legacy_key_bindings)
            .map(Self::merged_with_defaults)
            .map_err(|error| error.to_string())
    }

    pub fn load_from_user_defaults_plist(path: &Path) -> Result<Self, String> {
        let value = PlistValue::from_file(path)
            .map_err(|error| format!("failed to read IINA UserDefaults plist: {error}"))?;
        let dictionary = value
            .into_dictionary()
            .ok_or_else(|| "IINA UserDefaults plist root must be a flat dictionary".to_string())?;
        let mut values = BTreeMap::new();
        for (key, value) in dictionary {
            values.insert(key, plist_value_to_json(value)?);
        }
        Ok(Self { values }
            .migrated_legacy_key_bindings()
            .merged_with_defaults())
    }

    /// Loads the existing JSON store first, then its local UserDefaults-compatible mirror. This
    /// precedence protects all users of earlier Tauri builds from an older plist overriding JSON.
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        if path.exists() {
            return Self::load_json_file(path);
        }
        let mirror_path = user_defaults_mirror_path(path);
        if mirror_path.exists() {
            return Self::load_from_user_defaults_plist(&mirror_path);
        }
        Ok(Self::default())
    }

    /// Adds a one-time import source for the original IINA persistent domain. It is considered only
    /// when neither `preferences.json` nor this port's local plist mirror exists.
    pub fn load_with_iina_user_defaults_path(
        path: &Path,
        iina_user_defaults_path: Option<&Path>,
    ) -> Result<Self, String> {
        if path.exists() || user_defaults_mirror_path(path).exists() {
            return Self::load_from_file(path);
        }
        if let Some(iina_user_defaults_path) = iina_user_defaults_path.filter(|path| path.exists())
        {
            return Self::load_from_user_defaults_plist(iina_user_defaults_path);
        }
        Ok(Self::default())
    }

    /// Production compatibility loader. The original `com.colliderli.iina` domain is read-only:
    /// this port never writes into IINA's own `~/Library/Preferences` file.
    pub fn load_compatible(path: &Path) -> Result<Self, String> {
        let legacy_path = detected_iina_user_defaults_path();
        Self::load_with_iina_user_defaults_path(path, legacy_path.as_deref())
    }

    /// Dependency-injected form of [`Self::load_compatible`] used by deterministic migration tests.
    #[cfg(test)]
    fn load_compatible_from_home(
        path: &Path,
        home_directory: Option<&Path>,
    ) -> Result<Self, String> {
        let legacy_path = home_directory.map(iina_user_defaults_path);
        Self::load_with_iina_user_defaults_path(path, legacy_path.as_deref())
    }

    fn user_defaults_plist(&self) -> Result<PlistValue, String> {
        let mut values = PlistDictionary::new();
        for (key, value) in &self.values {
            if let Some(value) = json_value_to_plist(value)? {
                values.insert(key.clone(), value);
            }
        }
        Ok(PlistValue::Dictionary(values))
    }

    pub fn save_to_user_defaults_plist(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let temporary_path = path.with_extension("plist.tmp");
        self.user_defaults_plist()?
            .to_file_binary(&temporary_path)
            .map_err(|error| format!("failed to write UserDefaults plist mirror: {error}"))?;
        fs::rename(&temporary_path, path)
            .map_err(|error| format!("failed to install UserDefaults plist mirror: {error}"))
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let payload = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        // Commit the compatibility mirror first and JSON last. JSON remains authoritative, so a
        // failed final rename cannot cause an older JSON store to be silently overridden.
        self.save_to_user_defaults_plist(&user_defaults_mirror_path(path))?;
        let temporary_path = path.with_extension("json.tmp");
        fs::write(&temporary_path, payload).map_err(|error| error.to_string())?;
        fs::rename(&temporary_path, path).map_err(|error| error.to_string())
    }
}

pub fn preference_file_path(config_dir: impl AsRef<Path>) -> PathBuf {
    config_dir.as_ref().join(PREFERENCES_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_preference_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "iima-preferences-{}-{name}.json",
            std::process::id()
        ))
    }

    fn remove_preference_fixture(path: &Path) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(user_defaults_mirror_path(path));
        let _ = fs::remove_file(path.with_extension("json.tmp"));
        let _ = fs::remove_file(path.with_extension("plist.tmp"));
    }

    #[test]
    fn loads_missing_file_as_defaults() {
        let path = temp_preference_path("missing");
        remove_preference_fixture(&path);

        let preferences = PreferenceStore::load_from_file(&path).expect("missing file should load");

        assert_eq!(
            preferences.values.get("screenShotFolder"),
            Some(&json!("~/Pictures/Screenshots"))
        );
        assert_eq!(
            preferences.values.get("currentInputConfigName"),
            Some(&json!("IINA Default"))
        );
        assert_eq!(
            preferences.values.get("modeledKeyBindings"),
            Some(&Value::Null)
        );
        assert_eq!(
            preferences.values.get(KEY_BINDING_MODEL_VERSION_KEY),
            Some(&json!(CURRENT_KEY_BINDING_MODEL_VERSION))
        );
        assert_eq!(preferences.values.get("recentDocuments"), Some(&json!([])));
        assert_eq!(preferences.values.get("actionAfterLaunch"), Some(&json!(0)));
        assert_eq!(
            preferences.values.get("alwaysOpenInNewWindow"),
            Some(&json!(true))
        );
        assert_eq!(
            preferences.values.get("touchbarShowRemainingTime"),
            Some(&json!(true))
        );
        assert_eq!(
            preferences.values.get(SAVED_VIDEO_FILTERS_KEY),
            Some(&json!([]))
        );
        assert_eq!(
            preferences.values.get(SAVED_AUDIO_FILTERS_KEY),
            Some(&json!([]))
        );
    }

    #[test]
    fn codec_network_and_layout_defaults_match_iina_135() {
        let preferences = PreferenceStore::default();
        for (key, expected) in [
            ("controlBarAutoHideTimeout", json!(2.5)),
            ("playlistWidth", json!(270)),
            ("prefetchPlaylistVideoDuration", json!(true)),
            ("playlistShowMetadata", json!(true)),
            ("playlistShowMetadataInMusicMode", json!(true)),
            ("videoThreads", json!(0)),
            ("forceDedicatedGPU", json!(false)),
            ("audioThreads", json!(0)),
            ("audioLanguage", json!("")),
            ("maxVolume", json!(100)),
            ("audioDevice", json!("auto")),
            ("enableInitialVolume", json!(false)),
            ("initialVolume", json!(100)),
            ("toneMappingAlgorithm", json!(0)),
            ("subAutoLoadIINA", json!(2)),
            ("subAutoLoadPriorityString", json!("")),
            ("subAutoLoadSearchPath", json!("./*")),
            ("ignoreAssStyles", json!(false)),
            ("subOverrideLevel", json!(2)),
            ("subTextFont", json!("sans-serif")),
            ("subTextSize", json!(55.0)),
            ("subTextColor", json!("1/1/1/1")),
            ("subBgColor", json!("0/0/0/0")),
            ("subBold", json!(false)),
            ("subItalic", json!(false)),
            ("subBlur", json!(0.0)),
            ("subSpacing", json!(0.0)),
            ("subBorderSize", json!(3.0)),
            ("subBorderColor", json!("0/0/0/1")),
            ("subShadowSize", json!(0.0)),
            ("subShadowColor", json!("0/0/0/0")),
            ("subAlignX", json!(1)),
            ("subAlignY", json!(2)),
            ("subMarginX", json!(25.0)),
            ("subMarginY", json!(22.0)),
            ("subPos", json!(100.0)),
            ("subLang", json!("")),
            ("displayInLetterBox", json!(true)),
            ("subScaleWithWindow", json!(true)),
            ("defaultEncoding", json!("auto")),
            ("enableCache", json!(true)),
            ("defaultCacheSize", json!(153_600)),
            ("cacheBufferSize", json!(153_600)),
            ("secPrefech", json!(36_000)),
            ("transportRTSPThrough", json!(1)),
            ("ytdlEnabled", json!(true)),
            ("ytdlSearchPath", json!("")),
            ("httpProxy", json!("")),
            ("enableAdvancedSettings", json!(false)),
            ("useMpvOsd", json!(false)),
            ("enableLogging", json!(false)),
            ("logLevel", json!(1)),
            ("userOptions", json!([])),
            ("useUserDefinedConfDir", json!(false)),
            ("userDefinedConfDir", json!("~/.config/mpv/")),
        ] {
            assert_eq!(preferences.values.get(key), Some(&expected), "{key}");
        }
    }

    #[test]
    fn general_defaults_match_iina_135() {
        let preferences = PreferenceStore::default();
        for (key, expected) in [
            ("useLegacyFullScreen", json!(false)),
            ("blackOutMonitor", json!(false)),
            ("pauseWhenMinimized", json!(false)),
            ("pauseWhenInactive", json!(false)),
            ("playWhenEnteringFullScreen", json!(false)),
            ("pauseWhenLeavingFullScreen", json!(false)),
            ("pauseWhenGoesToSleep", json!(true)),
            ("playlistAutoAdd", json!(true)),
            ("playlistAutoPlayNext", json!(true)),
            ("playlistShowMetadata", json!(true)),
            ("playlistShowMetadataInMusicMode", json!(true)),
            ("recordPlaybackHistory", json!(true)),
            ("recordRecentFiles", json!(true)),
            ("trackAllFilesInRecentOpenMenu", json!(true)),
        ] {
            assert_eq!(preferences.values.get(key), Some(&expected), "{key}");
        }
    }

    #[test]
    fn ui_defaults_match_iina_135() {
        let preferences = PreferenceStore::default();
        for (key, expected) in [
            ("themeMaterial", json!(0)),
            ("usePhysicalResolution", json!(true)),
            ("initialWindowSizePosition", json!("")),
            ("resizeWindowTiming", json!(1)),
            ("resizeWindowOption", json!(2)),
            ("alwaysFloatOnTop", json!(false)),
            ("alwaysShowOnTopIcon", json!(false)),
            ("oscPosition", json!(0)),
            ("controlBarToolbarButtons", json!([2, 1, 0])),
            ("controlBarStickToCenter", json!(true)),
            ("showChapterPos", json!(false)),
            ("showRemainingTime", json!(false)),
            ("arrowBtnAction", json!(0)),
            ("controlBarAutoHideTimeout", json!(2.5)),
            ("enableOSD", json!(true)),
            ("osdAutoHideTimeout", json!(1.0)),
            ("osdTextSize", json!(20.0)),
            ("displayTimeAndBatteryInFullScreen", json!(false)),
            ("enableThumbnailPreview", json!(true)),
            ("maxThumbnailPreviewCacheSize", json!(500)),
            ("windowBehaviorWhenPip", json!(0)),
            ("pauseWhenPip", json!(false)),
            ("togglePipByMinimizingWindow", json!(false)),
        ] {
            assert_eq!(preferences.values.get(key), Some(&expected), "{key}");
        }
    }

    #[test]
    fn compatibility_and_control_edge_defaults_match_iina_135() {
        let preferences = PreferenceStore::default();
        for (key, expected) in [
            ("enableCmdN", json!(false)),
            ("legacyFullScreenAnimation", json!(false)),
            ("useAppleRemote", json!(false)),
            ("onlineSubSource", json!(1)),
            ("inputConfigs", json!({})),
            ("followGlobalSeekTypeWhenAdjustSlider", json!(false)),
            ("videoViewAcceptsFirstMouse", json!(false)),
        ] {
            assert_eq!(preferences.values.get(key), Some(&expected), "{key}");
        }
    }

    #[test]
    fn saves_and_loads_preferences() {
        let path = temp_preference_path("roundtrip");
        remove_preference_fixture(&path);
        let mut preferences = PreferenceStore::default();
        preferences.set(PreferenceChange {
            key: "screenShotFormat".into(),
            value: json!(2),
        });
        preferences
            .save_to_file(&path)
            .expect("preference file should save");

        let loaded = PreferenceStore::load_from_file(&path).expect("preference file should load");

        assert_eq!(loaded.values.get("screenShotFormat"), Some(&json!(2)));
        assert_eq!(
            loaded.values.get("screenshotShowPreview"),
            Some(&json!(true))
        );
        remove_preference_fixture(&path);
    }

    #[test]
    fn merges_saved_values_with_new_defaults_and_preserves_unknown_keys() {
        let path = temp_preference_path("merge");
        remove_preference_fixture(&path);
        fs::write(
            &path,
            r#"{"values":{"screenShotFormat":1,"futurePreference":"kept"}}"#,
        )
        .expect("fixture should write");

        let loaded = PreferenceStore::load_from_file(&path).expect("preference file should load");

        assert_eq!(loaded.values.get("screenShotFormat"), Some(&json!(1)));
        assert_eq!(loaded.values.get("futurePreference"), Some(&json!("kept")));
        assert_eq!(
            loaded.values.get("screenshotSaveToFile"),
            Some(&json!(true))
        );
        assert_eq!(
            loaded.values.get("usePhysicalResolution"),
            Some(&json!(true))
        );
        remove_preference_fixture(&path);
    }

    #[test]
    fn migrates_the_legacy_empty_key_binding_default_without_losing_new_empty_profiles() {
        let legacy_path = temp_preference_path("legacy-empty-key-bindings");
        let current_path = temp_preference_path("current-empty-key-bindings");
        remove_preference_fixture(&legacy_path);
        remove_preference_fixture(&current_path);
        fs::write(
            &legacy_path,
            r#"{"values":{"modeledKeyBindings":[],"currentInputConfigName":"IINA Default"}}"#,
        )
        .expect("legacy fixture should write");
        fs::write(
            &current_path,
            format!(
                r#"{{"values":{{"modeledKeyBindings":[],"{KEY_BINDING_MODEL_VERSION_KEY}":{CURRENT_KEY_BINDING_MODEL_VERSION}}}}}"#
            ),
        )
        .expect("current fixture should write");

        let legacy = PreferenceStore::load_from_file(&legacy_path)
            .expect("legacy preferences should migrate");
        let current = PreferenceStore::load_from_file(&current_path)
            .expect("current preferences should preserve an empty profile");

        assert_eq!(legacy.values.get("modeledKeyBindings"), Some(&Value::Null));
        assert_eq!(current.values.get("modeledKeyBindings"), Some(&json!([])));
        assert_eq!(
            legacy.values.get(KEY_BINDING_MODEL_VERSION_KEY),
            Some(&json!(CURRENT_KEY_BINDING_MODEL_VERSION))
        );
        remove_preference_fixture(&legacy_path);
        remove_preference_fixture(&current_path);
    }

    #[test]
    fn imports_flat_iina_user_defaults_plists_without_losing_native_value_types() {
        let json_path = temp_preference_path("plist-import");
        let plist_path = json_path.with_extension("legacy.plist");
        let round_trip_path = json_path.with_extension("roundtrip.plist");
        remove_preference_fixture(&json_path);
        let _ = fs::remove_file(&plist_path);
        let _ = fs::remove_file(&round_trip_path);

        let date = plist::Date::from_xml_format("2024-01-02T03:04:05Z").unwrap();
        let mut nested = PlistDictionary::new();
        nested.insert("enabled".into(), PlistValue::Boolean(true));
        nested.insert("count".into(), PlistValue::Integer(3_i64.into()));
        let mut source = PlistDictionary::new();
        source.insert("softVolume".into(), PlistValue::Integer(76_i64.into()));
        source.insert("futureDictionary".into(), PlistValue::Dictionary(nested));
        source.insert(
            "archivedColor".into(),
            PlistValue::Data(vec![0, 1, 254, 255]),
        );
        source.insert("subTextColor".into(), PlistValue::Data(vec![4, 3, 2, 1]));
        source.insert("futureDate".into(), PlistValue::Date(date));
        source.insert("futureUid".into(), PlistValue::Uid(plist::Uid::new(42)));
        source.insert("futureNonFinite".into(), PlistValue::Real(f64::INFINITY));
        PlistValue::Dictionary(source)
            .to_file_binary(&plist_path)
            .expect("fixture plist should write");

        let imported = PreferenceStore::load_from_user_defaults_plist(&plist_path)
            .expect("IINA UserDefaults plist should import");
        assert_eq!(imported.values.get("softVolume"), Some(&json!(76)));
        assert_eq!(
            imported.values.get("futureDictionary"),
            Some(&json!({ "enabled": true, "count": 3 }))
        );
        assert_eq!(
            imported.values.get("screenshotSaveToFile"),
            Some(&json!(true)),
            "registered defaults should still merge after importing the persistent domain"
        );

        imported
            .save_to_user_defaults_plist(&round_trip_path)
            .expect("imported native plist values should export");
        let round_trip = PlistValue::from_file(&round_trip_path)
            .unwrap()
            .into_dictionary()
            .unwrap();
        assert_eq!(
            round_trip.get("archivedColor"),
            Some(&PlistValue::Data(vec![0, 1, 254, 255]))
        );
        assert_eq!(
            round_trip.get("subTextColor"),
            Some(&PlistValue::Data(vec![4, 3, 2, 1])),
            "an imported archived NSColor must not be replaced by the RGBA default"
        );
        assert_eq!(round_trip.get("futureDate"), Some(&PlistValue::Date(date)));
        assert_eq!(
            round_trip.get("futureUid"),
            Some(&PlistValue::Uid(plist::Uid::new(42)))
        );
        assert!(matches!(
            round_trip.get("futureNonFinite"),
            Some(PlistValue::Real(value)) if value.is_infinite() && value.is_sign_positive()
        ));

        let _ = fs::remove_file(&plist_path);
        let _ = fs::remove_file(&round_trip_path);
        remove_preference_fixture(&json_path);
    }

    #[test]
    fn every_json_save_writes_a_flat_binary_user_defaults_mirror() {
        let path = temp_preference_path("plist-mirror");
        remove_preference_fixture(&path);
        let mirror_path = user_defaults_mirror_path(&path);
        let mut preferences = PreferenceStore::default();
        preferences.values.insert("softVolume".into(), json!(64));

        preferences
            .save_to_file(&path)
            .expect("JSON and plist should save");

        assert!(fs::read(&mirror_path)
            .expect("mirror should exist")
            .starts_with(b"bplist00"));
        let mirror = PlistValue::from_file(&mirror_path)
            .unwrap()
            .into_dictionary()
            .unwrap();
        assert_eq!(
            mirror.get("softVolume"),
            Some(&PlistValue::Integer(64_i64.into()))
        );
        assert!(!mirror.contains_key("values"));
        assert!(
            !mirror.contains_key("modeledKeyBindings"),
            "JSON null has no UserDefaults representation and must stay JSON-only"
        );

        remove_preference_fixture(&path);
    }

    #[test]
    fn existing_json_is_authoritative_over_any_plist_mirror() {
        let path = temp_preference_path("json-precedence");
        remove_preference_fixture(&path);
        fs::write(&path, r#"{"values":{"softVolume":31}}"#).unwrap();
        let mut mirror = PlistDictionary::new();
        mirror.insert("softVolume".into(), PlistValue::Integer(92_i64.into()));
        PlistValue::Dictionary(mirror)
            .to_file_binary(user_defaults_mirror_path(&path))
            .unwrap();

        let loaded = PreferenceStore::load_from_file(&path).unwrap();

        assert_eq!(loaded.values.get("softVolume"), Some(&json!(31)));
        remove_preference_fixture(&path);
    }

    #[test]
    fn malformed_existing_json_is_not_hidden_by_a_valid_plist() {
        let path = temp_preference_path("malformed-json-precedence");
        remove_preference_fixture(&path);
        fs::write(&path, "not JSON").unwrap();
        let mut mirror = PlistDictionary::new();
        mirror.insert("softVolume".into(), PlistValue::Integer(92_i64.into()));
        PlistValue::Dictionary(mirror)
            .to_file_binary(user_defaults_mirror_path(&path))
            .unwrap();

        assert!(PreferenceStore::load_from_file(&path).is_err());
        remove_preference_fixture(&path);
    }

    #[test]
    fn compatibility_loader_imports_the_original_iina_domain_only_when_local_stores_are_absent() {
        let path = temp_preference_path("legacy-domain");
        let fake_home = path.with_extension("home");
        let original_iina_path = iina_user_defaults_path(&fake_home);
        remove_preference_fixture(&path);
        let _ = fs::remove_dir_all(&fake_home);
        fs::create_dir_all(original_iina_path.parent().unwrap()).unwrap();
        let mut original = PlistDictionary::new();
        original.insert("softVolume".into(), PlistValue::Integer(43_i64.into()));
        PlistValue::Dictionary(original)
            .to_file_binary(&original_iina_path)
            .unwrap();

        let imported = PreferenceStore::load_compatible_from_home(&path, Some(&fake_home)).unwrap();
        assert_eq!(imported.values.get("softVolume"), Some(&json!(43)));

        fs::write(&path, r#"{"values":{"softVolume":27}}"#).unwrap();
        let existing = PreferenceStore::load_compatible_from_home(&path, Some(&fake_home)).unwrap();
        assert_eq!(existing.values.get("softVolume"), Some(&json!(27)));

        let _ = fs::remove_dir_all(&fake_home);
        remove_preference_fixture(&path);
    }

    #[test]
    fn missing_json_loads_the_local_user_defaults_mirror() {
        let path = temp_preference_path("local-plist-fallback");
        remove_preference_fixture(&path);
        let mut mirror = PlistDictionary::new();
        mirror.insert("softVolume".into(), PlistValue::Integer(58_i64.into()));
        PlistValue::Dictionary(mirror)
            .to_file_binary(user_defaults_mirror_path(&path))
            .unwrap();

        let loaded = PreferenceStore::load_from_file(&path).unwrap();

        assert_eq!(loaded.values.get("softVolume"), Some(&json!(58)));
        remove_preference_fixture(&path);
    }

    #[test]
    fn derives_the_exact_iina_135_user_defaults_domain_path() {
        assert_eq!(IINA_USER_DEFAULTS_DOMAIN, "com.colliderli.iina");
        assert_eq!(
            iina_user_defaults_path("/Users/example"),
            PathBuf::from("/Users/example/Library/Preferences/com.colliderli.iina.plist")
        );
    }

    #[test]
    fn decodes_iina_saved_filter_dictionaries_and_skips_invalid_entries() {
        let mut preferences = PreferenceStore::default();
        preferences.values.insert(
            SAVED_VIDEO_FILTERS_KEY.into(),
            json!([
                {
                    "name": "Mirror",
                    "filterString": "hflip",
                    "shortcutKey": "m",
                    "shortcutKeyModifiers": "s"
                },
                { "name": "Incomplete" }
            ]),
        );

        assert_eq!(
            preferences.saved_filters(SAVED_VIDEO_FILTERS_KEY),
            vec![SavedFilter {
                name: "Mirror".to_string(),
                filter_string: "hflip".to_string(),
                shortcut_key: "m".to_string(),
                shortcut_key_modifiers: "s".to_string(),
            }]
        );
    }
}
