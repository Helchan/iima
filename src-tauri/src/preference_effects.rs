use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::app_logging;
use crate::mpv::{mpv_command, set_property, MpvClientOperation, MpvFormat, MpvStartupOption};
use crate::preferences::{PreferenceChange, PreferenceStore};
use crate::subtitle_color;

pub const ADVANCED_HELP_URL: &str = "https://github.com/iina/iina/wiki/MPV-Options-and-Properties";
const ADVANCED_LOG_IDENTIFIER: &str = "io.iima.player";

pub fn advanced_log_directory(home: &Path) -> PathBuf {
    app_logging::directory().unwrap_or_else(|_| {
        home.join("Library")
            .join("Logs")
            .join(ADVANCED_LOG_IDENTIFIER)
    })
}

const SUBTITLE_ENCODINGS: &[&str] = &[
    "auto",
    "UTF-8",
    "UTF-16",
    "UTF-16BE",
    "UTF-16LE",
    "ISO-8859-6",
    "WINDOWS-1256",
    "LATIN7",
    "WINDOWS-1257",
    "LATIN8",
    "WINDOWS-1250",
    "ISO-8859-5",
    "WINDOWS-1251",
    "ISO-8859-2",
    "WINDOWS-1252",
    "ISO-8859-7",
    "WINDOWS-1253",
    "ISO-8859-8",
    "WINDOWS-1255",
    "SHIFT-JIS",
    "ISO-2022-JP-2",
    "EUC-KR",
    "CP949",
    "ISO-2022-KR",
    "LATIN6",
    "LATIN4",
    "KOI8-R",
    "GBK",
    "GB18030",
    "ISO-2022-CN-EXT",
    "LATIN3",
    "LATIN10",
    "TIS-620",
    "WINDOWS-874",
    "EUC-TW",
    "BIG5",
    "BIG5-HKSCS",
    "LATIN5",
    "WINDOWS-1254",
    "KOI8-U",
    "WINDOWS-1258",
    "VISCII",
    "LATIN1",
    "LATIN-9",
];

/// Describes whether a preference is projected into a future mpv client and whether changing it
/// can also be applied to an already initialized client. Keeping this table separate from the
/// Tauri command prevents preference side effects from becoming a second collection of ad-hoc
/// `if` statements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferenceEffectClass {
    Unmodeled,
    StartupOnly,
    StartupAndLive,
    StartupAndApplicationLoggingLive,
    ApplicationLoggingLive,
    NativeSurfaceStartupOnly,
    NativeSurfaceLive,
    NativeColorLive,
    ApplicationOnNextMedia,
    ApplicationRestartOnly,
}

impl PreferenceEffectClass {
    pub const fn has_startup_effect(self) -> bool {
        matches!(
            self,
            Self::StartupOnly | Self::StartupAndLive | Self::StartupAndApplicationLoggingLive
        )
    }

    pub const fn has_live_effect(self) -> bool {
        matches!(self, Self::StartupAndLive)
    }

    pub const fn has_application_logging_effect(self) -> bool {
        matches!(
            self,
            Self::StartupAndApplicationLoggingLive | Self::ApplicationLoggingLive
        )
    }

    pub const fn has_native_color_effect(self) -> bool {
        matches!(self, Self::NativeColorLive)
    }
}

pub fn effect_class(key: &str) -> PreferenceEffectClass {
    match key {
        // IINA reads these only while constructing an mpv client. Applying them to an existing
        // client would incorrectly claim that its startup/config parsing has been repeated.
        "softVolume"
        | "enableInitialVolume"
        | "initialVolume"
        | "useMpvOsd"
        | "useUserDefinedConfDir"
        | "userDefinedConfDir"
        | "userOptions"
        | "ytdlSearchPath"
        | "httpProxy" => PreferenceEffectClass::StartupOnly,

        // These values alter both future mpv startup logging and the application logger that is
        // already running. Keeping the combined class explicit avoids reinitializing logging for
        // every unrelated preference save.
        "enableAdvancedSettings" | "enableLogging" => {
            PreferenceEffectClass::StartupAndApplicationLoggingLive
        }
        // IINA's mpv client always requests warning-level messages. This preference filters the
        // application logger only, so changing it must not rebuild the mpv startup configuration.
        "logLevel" => PreferenceEffectClass::ApplicationLoggingLive,

        // IINA labels this setting as requiring an application restart. The
        // persisted value is latched by AppState during setup and is not
        // projected into an already running player's playlist.
        "playlistAutoAdd" => PreferenceEffectClass::ApplicationRestartOnly,

        // The OpenGL pixel format is immutable after NSOpenGLView creation. A changed value is
        // consumed by future native video surfaces only; it is neither an mpv option nor a live
        // mutation of an existing view.
        "forceDedicatedGPU" => PreferenceEffectClass::NativeSurfaceStartupOnly,

        // IINA's VideoView owns these values, not MPVController. They reconfigure the existing
        // native renderer and are also read when a future surface is installed.
        "loadIccProfile"
        | "enableHdrSupport"
        | "enableToneMapping"
        | "toneMappingTargetPeak"
        | "toneMappingAlgorithm" => PreferenceEffectClass::NativeColorLive,

        // The local AutoFileMatcher equivalent reads these immediately before each explicit
        // single-file open. They do not mutate mpv directly and do not require an app restart.
        "subAutoLoadIINA" | "subAutoLoadPriorityString" | "subAutoLoadSearchPath" => {
            PreferenceEffectClass::ApplicationOnNextMedia
        }

        // These settings are consumed by the real Tauri/AppKit window event
        // path rather than by mpv properties.
        "useLegacyFullScreen"
        | "blackOutMonitor"
        | "themeMaterial"
        | "alwaysFloatOnTop"
        | "usePhysicalResolution"
        | "initialWindowSizePosition"
        | "resizeWindowTiming"
        | "resizeWindowOption"
        | "windowBehaviorWhenPip"
        | "pauseWhenPip"
        | "togglePipByMinimizingWindow"
        | "pauseWhenMinimized"
        | "pauseWhenInactive"
        | "playWhenEnteringFullScreen"
        | "pauseWhenLeavingFullScreen"
        | "pauseWhenGoesToSleep" => PreferenceEffectClass::NativeSurfaceLive,

        // MPVController.setUserOption observes these UserDefaults keys in IINA 1.3.5. The three
        // S/PDIF keys use one explicit shared update handler there and do the same here.
        "videoThreads"
        | "audioThreads"
        | "hardwareDecoder"
        | "audioLanguage"
        | "maxVolume"
        | "spdifAC3"
        | "spdifDTS"
        | "spdifDTSHD"
        | "audioDevice"
        | "subLang"
        | "defaultEncoding"
        | "ignoreAssStyles"
        | "subOverrideLevel"
        | "subTextFont"
        | "subTextSize"
        | "subTextColor"
        | "subBgColor"
        | "subBold"
        | "subItalic"
        | "subBlur"
        | "subSpacing"
        | "subBorderSize"
        | "subBorderColor"
        | "subShadowSize"
        | "subShadowColor"
        | "subAlignX"
        | "subAlignY"
        | "subMarginX"
        | "subMarginY"
        | "subPos"
        | "displayInLetterBox"
        | "subScaleWithWindow"
        | "enableCache"
        | "defaultCacheSize"
        | "secPrefech"
        | "userAgent"
        | "transportRTSPThrough"
        | "ytdlEnabled"
        | "ytdlRawOptions"
        | "keepOpenOnFileEnd"
        | "playlistAutoPlayNext" => PreferenceEffectClass::StartupAndLive,
        _ => PreferenceEffectClass::Unmodeled,
    }
}

fn boolean(preferences: &PreferenceStore, key: &str, fallback: bool) -> bool {
    preferences
        .values
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(fallback)
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn integer(preferences: &PreferenceStore, key: &str, fallback: i64) -> i64 {
    preferences
        .values
        .get(key)
        .and_then(Value::as_i64)
        .unwrap_or(fallback)
}

fn integer_in(
    preferences: &PreferenceStore,
    key: &str,
    fallback: i64,
    minimum: i64,
    maximum: Option<i64>,
) -> i64 {
    let value = integer(preferences, key, fallback);
    if value >= minimum && maximum.map_or(true, |maximum| value <= maximum) {
        value
    } else {
        fallback
    }
}

fn string<'a>(preferences: &'a PreferenceStore, key: &str, fallback: &'a str) -> &'a str {
    preferences
        .values
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.contains('\0'))
        .unwrap_or(fallback)
}

fn default_encoding(preferences: &PreferenceStore) -> &str {
    let value = string(preferences, "defaultEncoding", "auto");
    if SUBTITLE_ENCODINGS.contains(&value) {
        value
    } else {
        "auto"
    }
}

fn number(preferences: &PreferenceStore, key: &str, fallback: f64) -> f64 {
    preferences
        .values
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

fn number_in(
    preferences: &PreferenceStore,
    key: &str,
    fallback: f64,
    minimum: Option<f64>,
    maximum: Option<f64>,
) -> f64 {
    let value = number(preferences, key, fallback);
    if minimum.map_or(true, |minimum| value >= minimum)
        && maximum.map_or(true, |maximum| value <= maximum)
    {
        value
    } else {
        fallback
    }
}

fn format_mpv_number(value: f64) -> String {
    let mut value = format!("{value:.6}");
    while value.ends_with('0') {
        value.pop();
    }
    if value.ends_with('.') {
        value.pop();
    }
    if value == "-0" {
        value = "0".into();
    }
    value
}

fn subtitle_override(preferences: &PreferenceStore) -> &'static str {
    if !boolean(preferences, "ignoreAssStyles", false) {
        return "yes";
    }
    match integer(preferences, "subOverrideLevel", 2) {
        0 => "yes",
        1 => "force",
        _ => "strip",
    }
}

fn subtitle_align_x(preferences: &PreferenceStore) -> &'static str {
    match integer(preferences, "subAlignX", 1) {
        0 => "left",
        2 => "right",
        _ => "center",
    }
}

fn subtitle_align_y(preferences: &PreferenceStore) -> &'static str {
    match integer(preferences, "subAlignY", 2) {
        0 => "top",
        1 => "center",
        _ => "bottom",
    }
}

fn subtitle_color_option(
    preferences: &PreferenceStore,
    key: &str,
    fallback: &str,
) -> Option<String> {
    match preferences.values.get(key) {
        Some(value) if subtitle_color::is_preserved_iina_archive(value) => None,
        Some(value) => subtitle_color::mpv_color(value)
            .or_else(|| subtitle_color::mpv_color(&Value::String(fallback.into()))),
        None => subtitle_color::mpv_color(&Value::String(fallback.into())),
    }
}

fn hardware_decoder(preferences: &PreferenceStore) -> &'static str {
    match integer(preferences, "hardwareDecoder", 1) {
        0 => "no",
        2 => "auto-copy",
        _ => "auto",
    }
}

fn rtsp_transport(preferences: &PreferenceStore) -> &'static str {
    match integer(preferences, "transportRTSPThrough", 1) {
        0 => "lavf",
        2 => "udp",
        3 => "http",
        _ => "tcp",
    }
}

fn spdif(preferences: &PreferenceStore) -> String {
    let mut codecs = Vec::new();
    if boolean(preferences, "spdifAC3", false) {
        codecs.push("ac3");
    }
    if boolean(preferences, "spdifDTS", false) {
        codecs.push("dts");
    }
    if boolean(preferences, "spdifDTSHD", false) {
        codecs.push("dts-hd");
    }
    codecs.join(",")
}

fn keep_open(preferences: &PreferenceStore) -> &'static str {
    if !boolean(preferences, "playlistAutoPlayNext", true) {
        "always"
    } else if boolean(preferences, "keepOpenOnFileEnd", true) {
        "yes"
    } else {
        "no"
    }
}

fn standardize_user_path(raw: &str, home: Option<&Path>) -> Option<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() || raw.contains('\0') {
        return None;
    }
    let expanded = if raw == "~" {
        home.map(Path::to_path_buf)?
    } else if let Some(suffix) = raw.strip_prefix("~/") {
        home.map(|home| home.join(suffix))?
    } else {
        PathBuf::from(raw)
    };
    let absolute = expanded.is_absolute();
    let mut standardized = PathBuf::new();
    for component in expanded.components() {
        match component {
            Component::Prefix(prefix) => standardized.push(prefix.as_os_str()),
            Component::RootDir => standardized.push(Path::new("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = standardized
                    .file_name()
                    .is_some_and(|component| component != "..");
                if can_pop {
                    standardized.pop();
                } else if !absolute {
                    standardized.push("..");
                }
            }
            Component::Normal(component) => standardized.push(component),
        }
    }
    Some(standardized)
}

fn user_option_pairs(preferences: &PreferenceStore) -> Vec<(String, String)> {
    preferences
        .values
        .get("userOptions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .filter_map(|option| {
            let name = option.first()?.as_str()?;
            let value = option.get(1)?.as_str()?;
            (!name.is_empty() && !name.contains('\0') && !value.contains('\0'))
                .then(|| (name.to_string(), value.to_string()))
        })
        .collect()
}

/// Projects IINA's preference table into options for a newly created mpv client. The returned
/// order follows MPVController.mpvInit: normal options first, then the advanced config directory
/// and finally user-defined options so those options retain IINA's override precedence.
pub fn startup_options(preferences: &PreferenceStore) -> Vec<MpvStartupOption> {
    startup_options_with_home(
        preferences,
        std::env::var_os("HOME").as_deref().map(Path::new),
    )
}

fn startup_options_with_home(
    preferences: &PreferenceStore,
    home: Option<&Path>,
) -> Vec<MpvStartupOption> {
    let mut options = Vec::new();
    let initial_volume = if boolean(preferences, "enableInitialVolume", false) {
        integer(preferences, "initialVolume", 100)
    } else {
        integer(preferences, "softVolume", 100)
    };
    options.push(MpvStartupOption::new("volume", initial_volume.to_string()));

    let advanced = boolean(preferences, "enableAdvancedSettings", false);
    // With mpv OSD enabled IINA deliberately does not set osd-level, preserving mpv's default
    // instead of guessing that the default is a particular numeric level.
    if !(advanced && boolean(preferences, "useMpvOsd", false)) {
        options.push(MpvStartupOption::new("osd-level", "0"));
    }
    if advanced && boolean(preferences, "enableLogging", false) {
        if let Some(home) = home {
            options.push(MpvStartupOption::new(
                "log-file",
                app_logging::mpv_log_path()
                    .unwrap_or_else(|_| advanced_log_directory(home).join("mpv.log"))
                    .to_string_lossy(),
            ));
        }
    }

    options.extend([
        MpvStartupOption::new("keep-open", keep_open(preferences)),
        MpvStartupOption::new(
            "vd-lavc-threads",
            integer_in(preferences, "videoThreads", 0, 0, None).to_string(),
        ),
        MpvStartupOption::new(
            "ad-lavc-threads",
            integer_in(preferences, "audioThreads", 0, 0, None).to_string(),
        ),
        MpvStartupOption::new("hwdec", hardware_decoder(preferences)),
        MpvStartupOption::new("alang", string(preferences, "audioLanguage", "")),
        MpvStartupOption::new(
            "volume-max",
            integer_in(preferences, "maxVolume", 100, 100, Some(1_000)).to_string(),
        ),
        MpvStartupOption::new("audio-spdif", spdif(preferences)),
        MpvStartupOption::new("audio-device", string(preferences, "audioDevice", "auto")),
        // IINA owns subtitle matching and disables mpv's built-in matcher.
        MpvStartupOption::new("sub-auto", "no"),
        MpvStartupOption::new("sub-codepage", default_encoding(preferences)),
        MpvStartupOption::new("sub-ass-override", subtitle_override(preferences)),
        MpvStartupOption::new("sub-font", string(preferences, "subTextFont", "sans-serif")),
        MpvStartupOption::new(
            "sub-font-size",
            format_mpv_number(number_in(preferences, "subTextSize", 55.0, Some(0.0), None)),
        ),
        MpvStartupOption::new("sub-bold", yes_no(boolean(preferences, "subBold", false))),
        MpvStartupOption::new(
            "sub-italic",
            yes_no(boolean(preferences, "subItalic", false)),
        ),
        MpvStartupOption::new(
            "sub-blur",
            format_mpv_number(number_in(
                preferences,
                "subBlur",
                0.0,
                Some(0.0),
                Some(20.0),
            )),
        ),
        MpvStartupOption::new(
            "sub-spacing",
            format_mpv_number(number(preferences, "subSpacing", 0.0)),
        ),
        MpvStartupOption::new(
            "sub-border-size",
            format_mpv_number(number_in(
                preferences,
                "subBorderSize",
                3.0,
                Some(0.0),
                None,
            )),
        ),
        MpvStartupOption::new(
            "sub-shadow-offset",
            format_mpv_number(number_in(
                preferences,
                "subShadowSize",
                0.0,
                Some(0.0),
                None,
            )),
        ),
        MpvStartupOption::new("sub-align-x", subtitle_align_x(preferences)),
        MpvStartupOption::new("sub-align-y", subtitle_align_y(preferences)),
        MpvStartupOption::new(
            "sub-margin-x",
            (number(preferences, "subMarginX", 25.0) as i64).to_string(),
        ),
        MpvStartupOption::new(
            "sub-margin-y",
            (number(preferences, "subMarginY", 22.0) as i64).to_string(),
        ),
        MpvStartupOption::new(
            "sub-pos",
            (number_in(preferences, "subPos", 100.0, Some(0.0), Some(100.0)) as i64).to_string(),
        ),
        MpvStartupOption::new(
            "sub-use-margins",
            yes_no(boolean(preferences, "displayInLetterBox", true)),
        ),
        MpvStartupOption::new(
            "sub-ass-force-margins",
            yes_no(boolean(preferences, "displayInLetterBox", true)),
        ),
        MpvStartupOption::new(
            "sub-scale-by-window",
            yes_no(boolean(preferences, "subScaleWithWindow", true)),
        ),
        MpvStartupOption::new("slang", string(preferences, "subLang", "")),
    ]);

    // Imported IINA NSColor values are opaque NSArchiver Data. Preserve them in
    // preferences and let mpv keep its matching default instead of projecting a
    // guessed color. Values created by this port use the independent RGBA codec.
    for (name, key, fallback) in [
        ("sub-color", "subTextColor", "1/1/1/1"),
        ("sub-back-color", "subBgColor", "0/0/0/0"),
        ("sub-border-color", "subBorderColor", "0/0/0/1"),
        ("sub-shadow-color", "subShadowColor", "0/0/0/0"),
    ] {
        if let Some(value) = subtitle_color_option(preferences, key, fallback) {
            options.push(MpvStartupOption::new(name, value));
        }
    }

    // IINA leaves mpv's automatic cache policy untouched when caching is enabled.
    if !boolean(preferences, "enableCache", true) {
        options.push(MpvStartupOption::new("cache", "no"));
    }
    options.extend([
        MpvStartupOption::new(
            "demuxer-max-bytes",
            format!(
                "{}KiB",
                integer_in(preferences, "defaultCacheSize", 153_600, 0, None)
            ),
        ),
        MpvStartupOption::new(
            "cache-secs",
            integer_in(preferences, "secPrefech", 36_000, 0, None).to_string(),
        ),
    ]);
    let user_agent = string(preferences, "userAgent", "");
    if !user_agent.is_empty() {
        options.push(MpvStartupOption::new("user-agent", user_agent));
    }
    options.extend([
        MpvStartupOption::new("rtsp-transport", rtsp_transport(preferences)),
        MpvStartupOption::new(
            "ytdl",
            if boolean(preferences, "ytdlEnabled", true) {
                "yes"
            } else {
                "no"
            },
        ),
        MpvStartupOption::new(
            "ytdl-raw-options",
            string(preferences, "ytdlRawOptions", ""),
        ),
    ]);

    if advanced && boolean(preferences, "useUserDefinedConfDir", false) {
        if let Some(directory) = standardize_user_path(
            string(preferences, "userDefinedConfDir", "~/.config/mpv/"),
            home,
        ) {
            options.push(MpvStartupOption::new("config", "yes"));
            options.push(MpvStartupOption::best_effort(
                "config-dir",
                directory.to_string_lossy(),
            ));
        }
    }
    if advanced {
        options.extend(
            user_option_pairs(preferences)
                .into_iter()
                .map(|(name, value)| MpvStartupOption::best_effort(name, value)),
        );
    }
    options
}

/// Returns the property writes that IINA can safely apply to already initialized player cores.
/// Startup-only advanced/config and initial-volume preferences intentionally return no operation.
pub fn live_operations(key: &str, preferences: &PreferenceStore) -> Vec<MpvClientOperation> {
    if !effect_class(key).has_live_effect() {
        return Vec::new();
    }
    let string_property =
        |name: &str, value: String| vec![set_property(name, MpvFormat::String, value)];
    match key {
        "keepOpenOnFileEnd" | "playlistAutoPlayNext" => {
            string_property("keep-open", keep_open(preferences).into())
        }
        "videoThreads" => vec![set_property(
            "vd-lavc-threads",
            MpvFormat::Int64,
            integer_in(preferences, key, 0, 0, None).to_string(),
        )],
        "audioThreads" => vec![set_property(
            "ad-lavc-threads",
            MpvFormat::Int64,
            integer_in(preferences, key, 0, 0, None).to_string(),
        )],
        "hardwareDecoder" => string_property("hwdec", hardware_decoder(preferences).into()),
        "audioLanguage" => {
            string_property("alang", string(preferences, "audioLanguage", "").into())
        }
        "maxVolume" => vec![set_property(
            "volume-max",
            MpvFormat::Int64,
            integer_in(preferences, key, 100, 100, Some(1_000)).to_string(),
        )],
        "spdifAC3" | "spdifDTS" | "spdifDTSHD" => {
            string_property("audio-spdif", spdif(preferences))
        }
        "audioDevice" => string_property(
            "audio-device",
            string(preferences, "audioDevice", "auto").into(),
        ),
        "subLang" => string_property("slang", string(preferences, "subLang", "").into()),
        "defaultEncoding" => {
            subtitle_encoding_live_operations(preferences, std::iter::empty::<i64>())
        }
        "ignoreAssStyles" | "subOverrideLevel" => {
            string_property("sub-ass-override", subtitle_override(preferences).into())
        }
        "subTextFont" => string_property(
            "sub-font",
            string(preferences, "subTextFont", "sans-serif").into(),
        ),
        "subTextSize" => vec![set_property(
            "sub-font-size",
            MpvFormat::Double,
            format_mpv_number(number_in(preferences, key, 55.0, Some(0.0), None)),
        )],
        "subTextColor" | "subBgColor" | "subBorderColor" | "subShadowColor" => {
            let (name, fallback) = match key {
                "subTextColor" => ("sub-color", "1/1/1/1"),
                "subBgColor" => ("sub-back-color", "0/0/0/0"),
                "subBorderColor" => ("sub-border-color", "0/0/0/1"),
                _ => ("sub-shadow-color", "0/0/0/0"),
            };
            subtitle_color_option(preferences, key, fallback)
                .map(|value| set_property(name, MpvFormat::String, value))
                .into_iter()
                .collect()
        }
        "subBold" => vec![set_property(
            "sub-bold",
            MpvFormat::Flag,
            boolean(preferences, key, false).to_string(),
        )],
        "subItalic" => vec![set_property(
            "sub-italic",
            MpvFormat::Flag,
            boolean(preferences, key, false).to_string(),
        )],
        "subBlur" => vec![set_property(
            "sub-blur",
            MpvFormat::Double,
            format_mpv_number(number_in(preferences, key, 0.0, Some(0.0), Some(20.0))),
        )],
        "subSpacing" => vec![set_property(
            "sub-spacing",
            MpvFormat::Double,
            format_mpv_number(number(preferences, key, 0.0)),
        )],
        "subBorderSize" => vec![set_property(
            "sub-border-size",
            MpvFormat::Double,
            format_mpv_number(number_in(preferences, key, 3.0, Some(0.0), None)),
        )],
        "subShadowSize" => vec![set_property(
            "sub-shadow-offset",
            MpvFormat::Double,
            format_mpv_number(number_in(preferences, key, 0.0, Some(0.0), None)),
        )],
        "subAlignX" => string_property("sub-align-x", subtitle_align_x(preferences).into()),
        "subAlignY" => string_property("sub-align-y", subtitle_align_y(preferences).into()),
        "subMarginX" => vec![set_property(
            "sub-margin-x",
            MpvFormat::Int64,
            (number(preferences, key, 25.0) as i64).to_string(),
        )],
        "subMarginY" => vec![set_property(
            "sub-margin-y",
            MpvFormat::Int64,
            (number(preferences, key, 22.0) as i64).to_string(),
        )],
        "subPos" => vec![set_property(
            "sub-pos",
            MpvFormat::Int64,
            (number_in(preferences, key, 100.0, Some(0.0), Some(100.0)) as i64).to_string(),
        )],
        "displayInLetterBox" => {
            let value = boolean(preferences, key, true).to_string();
            vec![
                set_property("sub-use-margins", MpvFormat::Flag, value.clone()),
                set_property("sub-ass-force-margins", MpvFormat::Flag, value),
            ]
        }
        "subScaleWithWindow" => vec![set_property(
            "sub-scale-by-window",
            MpvFormat::Flag,
            boolean(preferences, key, true).to_string(),
        )],
        // IINA's transformer deliberately emits only the disabling write. Re-enabling restores
        // the omitted/default policy for future clients and does not pretend that a running client
        // has reconstructed mpv's version-specific automatic cache decision.
        "enableCache" => (!boolean(preferences, "enableCache", true))
            .then(|| set_property("cache", MpvFormat::String, "no"))
            .into_iter()
            .collect(),
        "defaultCacheSize" => string_property(
            "demuxer-max-bytes",
            format!("{}KiB", integer_in(preferences, key, 153_600, 0, None)),
        ),
        "secPrefech" => vec![set_property(
            "cache-secs",
            MpvFormat::Int64,
            integer_in(preferences, key, 36_000, 0, None).to_string(),
        )],
        "userAgent" => {
            let value = string(preferences, "userAgent", "");
            // An empty preference means "leave mpv's version-specific default alone". There is no
            // stable value with which to reconstruct that default on a running client.
            (!value.is_empty())
                .then(|| set_property("user-agent", MpvFormat::String, value))
                .into_iter()
                .collect()
        }
        "transportRTSPThrough" => {
            string_property("rtsp-transport", rtsp_transport(preferences).into())
        }
        "ytdlEnabled" => vec![set_property(
            "ytdl",
            MpvFormat::Flag,
            boolean(preferences, "ytdlEnabled", true).to_string(),
        )],
        "ytdlRawOptions" => string_property(
            "ytdl-raw-options",
            string(preferences, "ytdlRawOptions", "").into(),
        ),
        _ => Vec::new(),
    }
}

/// IINA writes `sub-codepage` before reloading every currently known subtitle
/// track. The caller supplies per-player track ids so multi-window sessions do
/// not incorrectly share one player's list.
pub fn subtitle_encoding_live_operations(
    preferences: &PreferenceStore,
    subtitle_track_ids: impl IntoIterator<Item = i64>,
) -> Vec<MpvClientOperation> {
    let mut operations = vec![set_property(
        "sub-codepage",
        MpvFormat::String,
        default_encoding(preferences),
    )];
    operations.extend(
        subtitle_track_ids
            .into_iter()
            .map(|id| mpv_command("sub-reload", [id.to_string()])),
    );
    operations
}

pub fn validate_change(change: &PreferenceChange) -> Result<(), String> {
    let boolean_value = || {
        change
            .value
            .as_bool()
            .map(|_| ())
            .ok_or_else(|| format!("{} must be a boolean", change.key))
    };
    let integer_in = |minimum: i64, maximum: Option<i64>| {
        let value = change
            .value
            .as_i64()
            .ok_or_else(|| format!("{} must be an integer", change.key))?;
        if value < minimum || maximum.is_some_and(|maximum| value > maximum) {
            let range = maximum.map_or_else(
                || format!("{minimum} or greater"),
                |maximum| format!("between {minimum} and {maximum}"),
            );
            return Err(format!("{} must be {range}", change.key));
        }
        Ok(())
    };
    let string_value = || {
        change
            .value
            .as_str()
            .filter(|value| !value.contains('\0'))
            .map(|_| ())
            .ok_or_else(|| format!("{} must be a string without NUL bytes", change.key))
    };
    let number_in = |minimum: Option<f64>, maximum: Option<f64>| {
        let value = change
            .value
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("{} must be a finite number", change.key))?;
        if minimum.is_some_and(|minimum| value < minimum)
            || maximum.is_some_and(|maximum| value > maximum)
        {
            return Err(format!(
                "{} is outside the reference numeric range",
                change.key
            ));
        }
        Ok(())
    };

    match change.key.as_str() {
        "enableInitialVolume"
        | "spdifAC3"
        | "spdifDTS"
        | "spdifDTSHD"
        | "loadIccProfile"
        | "enableHdrSupport"
        | "enableToneMapping"
        | "enableCache"
        | "ytdlEnabled"
        | "forceDedicatedGPU"
        | "ignoreAssStyles"
        | "subBold"
        | "subItalic"
        | "displayInLetterBox"
        | "subScaleWithWindow"
        | "autoSearchOnlineSub"
        | "enableAdvancedSettings"
        | "useMpvOsd"
        | "enableLogging"
        | "useUserDefinedConfDir"
        | "alwaysOpenInNewWindow"
        | "quitWhenNoOpenedWindow"
        | "keepOpenOnFileEnd"
        | "resumeLastPosition"
        | "pauseWhenOpen"
        | "fullScreenWhenOpen"
        | "useLegacyFullScreen"
        | "blackOutMonitor"
        | "autoSwitchToMusicMode"
        | "pauseWhenMinimized"
        | "pauseWhenInactive"
        | "playWhenEnteringFullScreen"
        | "pauseWhenLeavingFullScreen"
        | "pauseWhenGoesToSleep"
        | "usePhysicalResolution"
        | "alwaysFloatOnTop"
        | "alwaysShowOnTopIcon"
        | "controlBarStickToCenter"
        | "showChapterPos"
        | "showRemainingTime"
        | "touchbarShowRemainingTime"
        | "enableOSD"
        | "displayTimeAndBatteryInFullScreen"
        | "enableThumbnailPreview"
        | "pauseWhenPip"
        | "togglePipByMinimizingWindow"
        | "recordPlaybackHistory"
        | "recordRecentFiles"
        | "trackAllFilesInRecentOpenMenu"
        | "playlistAutoAdd"
        | "playlistAutoPlayNext"
        | "playlistShowMetadata"
        | "playlistShowMetadataInMusicMode"
        | "screenshotSaveToFile"
        | "screenshotCopyToClipboard"
        | "screenShotIncludeSubtitle"
        | "screenshotShowPreview" => boolean_value(),
        "actionAfterLaunch"
        | "screenShotFormat"
        | "resizeWindowTiming"
        | "oscPosition"
        | "arrowBtnAction"
        | "windowBehaviorWhenPip" => integer_in(0, Some(2)),
        "resizeWindowOption" => integer_in(0, Some(4)),
        "themeMaterial" => {
            let value = change
                .value
                .as_i64()
                .ok_or_else(|| "themeMaterial must be an integer".to_string())?;
            if matches!(value, 0 | 2 | 4) {
                Ok(())
            } else {
                Err("themeMaterial must match a theme available on modern macOS".to_string())
            }
        }
        "videoThreads"
        | "audioThreads"
        | "defaultCacheSize"
        | "cacheBufferSize"
        | "secPrefech"
        | "maxThumbnailPreviewCacheSize" => integer_in(0, None),
        "toneMappingTargetPeak" => integer_in(0, None),
        "hardwareDecoder" => integer_in(0, Some(2)),
        "toneMappingAlgorithm" => integer_in(0, Some(7)),
        "maxVolume" => integer_in(100, Some(1_000)),
        "initialVolume" | "softVolume" => change
            .value
            .as_i64()
            .map(|_| ())
            .ok_or_else(|| format!("{} must be an integer", change.key)),
        "transportRTSPThrough" => integer_in(0, Some(3)),
        "timeDisplayPrecision" => integer_in(0, Some(3)),
        "subAutoLoadIINA" => integer_in(0, Some(2)),
        "subOverrideLevel" | "subAlignX" | "subAlignY" => integer_in(0, Some(2)),
        "autoSearchThreshold" => integer_in(1, Some(240)),
        "logLevel" => integer_in(0, Some(3)),
        "subTextSize" | "subBorderSize" | "subShadowSize" => number_in(Some(0.0), None),
        "osdTextSize" => number_in(Some(5.0), None),
        "osdAutoHideTimeout" | "controlBarAutoHideTimeout" => number_in(Some(0.0), None),
        "controlBarPositionHorizontal" | "controlBarPositionVertical" => {
            number_in(Some(0.0), Some(1.0))
        }
        "subBlur" => number_in(Some(0.0), Some(20.0)),
        "subPos" => number_in(Some(0.0), Some(100.0)),
        "subSpacing" | "subMarginX" | "subMarginY" => number_in(None, None),
        "subTextColor" | "subBgColor" | "subBorderColor" | "subShadowColor" => {
            subtitle_color::validate(&change.value)
                .map_err(|error| format!("{} {error}", change.key))
        }
        "defaultEncoding" => {
            let value = change
                .value
                .as_str()
                .ok_or_else(|| "defaultEncoding must be a reference encoding code".to_string())?;
            if SUBTITLE_ENCODINGS.contains(&value) {
                Ok(())
            } else {
                Err(format!("unsupported subtitle encoding: {value}"))
            }
        }
        "audioLanguage"
        | "audioDevice"
        | "audioDeviceDesc"
        | "subLang"
        | "subAutoLoadPriorityString"
        | "subAutoLoadSearchPath"
        | "subTextFont"
        | "openSubUsername"
        | "assrtToken"
        | "userAgent"
        | "ytdlSearchPath"
        | "ytdlRawOptions"
        | "httpProxy"
        | "userDefinedConfDir"
        | "screenShotFolder"
        | "screenShotTemplate" => string_value(),
        "onlineSubProvider" => {
            let provider = change
                .value
                .as_str()
                .filter(|value| !value.contains('\0'))
                .ok_or_else(|| {
                    "onlineSubProvider must be a non-empty provider identifier".to_string()
                })?;
            let built_in = matches!(provider, ":opensubtitles" | ":assrt" | ":shooter");
            let plugin = provider
                .strip_prefix("plugin:")
                .and_then(|provider| provider.split_once(':'))
                .is_some_and(|(identifier, provider)| {
                    !identifier.is_empty() && !provider.is_empty()
                });
            if built_in || plugin {
                Ok(())
            } else {
                Err(
                    "onlineSubProvider must identify a built-in or plugin subtitle provider"
                        .to_string(),
                )
            }
        }
        "initialWindowSizePosition" => {
            let value = change
                .value
                .as_str()
                .ok_or_else(|| "initialWindowSizePosition must be a string".to_string())?;
            if crate::window_size::valid_iina_geometry(value) {
                Ok(())
            } else {
                Err("initialWindowSizePosition must use IINA/mpv geometry syntax".to_string())
            }
        }
        "controlBarToolbarButtons" => {
            let buttons = change
                .value
                .as_array()
                .ok_or_else(|| "controlBarToolbarButtons must be an array".to_string())?;
            if buttons.len() > 5 {
                return Err("controlBarToolbarButtons supports at most 5 items".to_string());
            }
            let mut unique = BTreeSet::new();
            for button in buttons {
                let value = button
                    .as_i64()
                    .filter(|value| (0..=6).contains(value))
                    .ok_or_else(|| {
                        "controlBarToolbarButtons contains an unknown item".to_string()
                    })?;
                if !unique.insert(value) {
                    return Err("controlBarToolbarButtons items must be unique".to_string());
                }
            }
            Ok(())
        }
        "userOptions" => {
            let options = change
                .value
                .as_array()
                .ok_or_else(|| "userOptions must be an array".to_string())?;
            for (index, option) in options.iter().enumerate() {
                let pair = option
                    .as_array()
                    .ok_or_else(|| format!("userOptions[{index}] must be a name/value pair"))?;
                if pair.len() != 2
                    || pair[0]
                        .as_str()
                        .filter(|value| !value.is_empty() && !value.contains('\0'))
                        .is_none()
                    || pair[1]
                        .as_str()
                        .filter(|value| !value.is_empty() && !value.contains('\0'))
                        .is_none()
                {
                    return Err(format!(
                        "userOptions[{index}] must contain two non-empty NUL-free strings"
                    ));
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn option_value<'a>(options: &'a [MpvStartupOption], name: &str) -> Option<&'a str> {
        options
            .iter()
            .rev()
            .find(|option| option.name == name)
            .map(|option| option.value.as_str())
    }

    #[test]
    fn default_projection_matches_iina_135_codec_subtitle_network_and_advanced_values() {
        let options = startup_options_with_home(
            &PreferenceStore::default(),
            Some(Path::new("/Users/tester")),
        );
        for (name, expected) in [
            ("volume", "100"),
            ("osd-level", "0"),
            ("vd-lavc-threads", "0"),
            ("ad-lavc-threads", "0"),
            ("hwdec", "auto"),
            ("alang", ""),
            ("volume-max", "100"),
            ("audio-spdif", ""),
            ("audio-device", "auto"),
            ("sub-auto", "no"),
            ("sub-codepage", "auto"),
            ("sub-ass-override", "yes"),
            ("sub-font", "sans-serif"),
            ("sub-font-size", "55"),
            ("sub-color", "1/1/1/1"),
            ("sub-back-color", "0/0/0/0"),
            ("sub-bold", "no"),
            ("sub-italic", "no"),
            ("sub-blur", "0"),
            ("sub-spacing", "0"),
            ("sub-border-size", "3"),
            ("sub-border-color", "0/0/0/1"),
            ("sub-shadow-offset", "0"),
            ("sub-shadow-color", "0/0/0/0"),
            ("sub-align-x", "center"),
            ("sub-align-y", "bottom"),
            ("sub-margin-x", "25"),
            ("sub-margin-y", "22"),
            ("sub-pos", "100"),
            ("sub-use-margins", "yes"),
            ("sub-ass-force-margins", "yes"),
            ("sub-scale-by-window", "yes"),
            ("slang", ""),
            ("demuxer-max-bytes", "153600KiB"),
            ("cache-secs", "36000"),
            ("rtsp-transport", "tcp"),
            ("ytdl", "yes"),
            ("ytdl-raw-options", ""),
        ] {
            assert_eq!(option_value(&options, name), Some(expected), "{name}");
        }
        assert_eq!(option_value(&options, "cache"), None);
        assert_eq!(option_value(&options, "user-agent"), None);
        assert_eq!(option_value(&options, "config-dir"), None);
        assert_eq!(option_value(&options, "log-file"), None);
    }

    #[test]
    fn advanced_projection_expands_config_dir_and_keeps_user_options_last() {
        let mut preferences = PreferenceStore::default();
        preferences
            .values
            .insert("enableAdvancedSettings".into(), json!(true));
        preferences.values.insert("useMpvOsd".into(), json!(true));
        preferences
            .values
            .insert("enableLogging".into(), json!(true));
        preferences
            .values
            .insert("useUserDefinedConfDir".into(), json!(true));
        preferences.values.insert(
            "userDefinedConfDir".into(),
            json!("~/.config/../custom-mpv"),
        );
        preferences.values.insert(
            "userOptions".into(),
            json!([["hwdec", "no"], ["demuxer-max-bytes", "2MiB"]]),
        );

        let options = startup_options_with_home(&preferences, Some(Path::new("/Users/tester")));
        assert_eq!(option_value(&options, "osd-level"), None);
        assert_eq!(
            option_value(&options, "log-file"),
            Some("/Users/tester/Library/Logs/io.iima.player/mpv.log")
        );
        assert_eq!(option_value(&options, "config"), Some("yes"));
        assert_eq!(
            option_value(&options, "config-dir"),
            Some("/Users/tester/custom-mpv")
        );
        assert_eq!(option_value(&options, "hwdec"), Some("no"));
        assert_eq!(option_value(&options, "demuxer-max-bytes"), Some("2MiB"));
        for name in ["config-dir", "hwdec", "demuxer-max-bytes"] {
            assert!(
                options
                    .iter()
                    .rev()
                    .find(|option| option.name == name)
                    .is_some_and(|option| option.best_effort),
                "{name} should not prevent an otherwise valid mpv client from starting"
            );
        }
    }

    #[test]
    fn live_projection_reuses_shared_spdif_and_exact_enum_transformers() {
        let mut preferences = PreferenceStore::default();
        preferences.values.insert("spdifAC3".into(), json!(true));
        preferences.values.insert("spdifDTSHD".into(), json!(true));
        preferences
            .values
            .insert("hardwareDecoder".into(), json!(2));
        preferences
            .values
            .insert("transportRTSPThrough".into(), json!(3));

        assert_eq!(
            live_operations("spdifDTSHD", &preferences),
            vec![set_property("audio-spdif", MpvFormat::String, "ac3,dts-hd")]
        );
        assert_eq!(
            live_operations("hardwareDecoder", &preferences),
            vec![set_property("hwdec", MpvFormat::String, "auto-copy")]
        );
        assert_eq!(
            live_operations("transportRTSPThrough", &preferences),
            vec![set_property("rtsp-transport", MpvFormat::String, "http")]
        );
        assert!(live_operations("enableCache", &preferences).is_empty());
        preferences
            .values
            .insert("enableCache".into(), json!(false));
        assert_eq!(
            live_operations("enableCache", &preferences),
            vec![set_property("cache", MpvFormat::String, "no")]
        );
    }

    #[test]
    fn subtitle_live_projection_uses_reference_transformers_and_reload_order() {
        let mut preferences = PreferenceStore::default();
        preferences
            .values
            .insert("ignoreAssStyles".into(), json!(true));
        preferences
            .values
            .insert("subOverrideLevel".into(), json!(1));
        preferences.values.insert("subAlignX".into(), json!(2));
        preferences.values.insert("subAlignY".into(), json!(0));
        preferences
            .values
            .insert("displayInLetterBox".into(), json!(false));
        preferences
            .values
            .insert("defaultEncoding".into(), json!("GB18030"));

        assert_eq!(
            live_operations("subOverrideLevel", &preferences),
            vec![set_property("sub-ass-override", MpvFormat::String, "force")]
        );
        assert_eq!(
            live_operations("subAlignX", &preferences),
            vec![set_property("sub-align-x", MpvFormat::String, "right")]
        );
        assert_eq!(
            live_operations("subAlignY", &preferences),
            vec![set_property("sub-align-y", MpvFormat::String, "top")]
        );
        assert_eq!(
            live_operations("displayInLetterBox", &preferences),
            vec![
                set_property("sub-use-margins", MpvFormat::Flag, "false"),
                set_property("sub-ass-force-margins", MpvFormat::Flag, "false"),
            ]
        );
        assert_eq!(
            subtitle_encoding_live_operations(&preferences, [7, 12]),
            vec![
                set_property("sub-codepage", MpvFormat::String, "GB18030"),
                mpv_command("sub-reload", ["7"]),
                mpv_command("sub-reload", ["12"]),
            ]
        );
    }

    #[test]
    fn imported_iina_color_data_remains_opaque_and_is_not_guessed_at_runtime() {
        let archive = json!({
            "__iimaUserDefaultsPlistValue": {
                "type": "data",
                "value": "0001feff"
            }
        });
        let mut preferences = PreferenceStore::default();
        preferences
            .values
            .insert("subTextColor".into(), archive.clone());
        let options = startup_options_with_home(&preferences, None);

        assert_eq!(preferences.values.get("subTextColor"), Some(&archive));
        assert_eq!(option_value(&options, "sub-color"), None);
        assert!(live_operations("subTextColor", &preferences).is_empty());
    }

    #[test]
    fn startup_only_preferences_never_emit_hot_apply_operations() {
        let preferences = PreferenceStore::default();
        for key in [
            "enableInitialVolume",
            "initialVolume",
            "useMpvOsd",
            "useUserDefinedConfDir",
            "userDefinedConfDir",
            "userOptions",
            "ytdlSearchPath",
            "httpProxy",
        ] {
            assert_eq!(effect_class(key), PreferenceEffectClass::StartupOnly);
            assert!(live_operations(key, &preferences).is_empty(), "{key}");
        }
        for key in ["enableAdvancedSettings", "enableLogging"] {
            let class = effect_class(key);
            assert_eq!(
                class,
                PreferenceEffectClass::StartupAndApplicationLoggingLive
            );
            assert!(class.has_startup_effect(), "{key}");
            assert!(class.has_application_logging_effect(), "{key}");
            assert!(!class.has_live_effect(), "{key}");
            assert!(live_operations(key, &preferences).is_empty(), "{key}");
        }
        let log_level_class = effect_class("logLevel");
        assert_eq!(
            log_level_class,
            PreferenceEffectClass::ApplicationLoggingLive
        );
        assert!(!log_level_class.has_startup_effect());
        assert!(!log_level_class.has_live_effect());
        assert!(log_level_class.has_application_logging_effect());
        assert!(live_operations("logLevel", &preferences).is_empty());
        assert_eq!(
            effect_class("forceDedicatedGPU"),
            PreferenceEffectClass::NativeSurfaceStartupOnly
        );
        assert!(live_operations("forceDedicatedGPU", &preferences).is_empty());
        assert_eq!(
            effect_class("playlistAutoAdd"),
            PreferenceEffectClass::ApplicationRestartOnly
        );
        assert!(live_operations("playlistAutoAdd", &preferences).is_empty());
        for key in [
            "useLegacyFullScreen",
            "blackOutMonitor",
            "pauseWhenMinimized",
            "pauseWhenInactive",
            "playWhenEnteringFullScreen",
            "pauseWhenLeavingFullScreen",
            "pauseWhenGoesToSleep",
        ] {
            assert_eq!(effect_class(key), PreferenceEffectClass::NativeSurfaceLive);
            assert!(live_operations(key, &preferences).is_empty(), "{key}");
        }
        for key in [
            "loadIccProfile",
            "enableHdrSupport",
            "enableToneMapping",
            "toneMappingTargetPeak",
            "toneMappingAlgorithm",
        ] {
            let class = effect_class(key);
            assert_eq!(class, PreferenceEffectClass::NativeColorLive, "{key}");
            assert!(class.has_native_color_effect(), "{key}");
            assert!(!class.has_startup_effect(), "{key}");
            assert!(!class.has_live_effect(), "{key}");
            assert!(live_operations(key, &preferences).is_empty(), "{key}");
        }
        assert_eq!(
            effect_class("autoSearchOnlineSub"),
            PreferenceEffectClass::Unmodeled,
            "frontend automatic-search behavior must not masquerade as an mpv startup/live effect"
        );
        for key in [
            "subAutoLoadIINA",
            "subAutoLoadPriorityString",
            "subAutoLoadSearchPath",
        ] {
            assert_eq!(
                effect_class(key),
                PreferenceEffectClass::ApplicationOnNextMedia,
                "{key}"
            );
            assert!(live_operations(key, &preferences).is_empty(), "{key}");
        }
    }

    #[test]
    fn playlist_auto_next_and_keep_open_share_the_reference_mpv_projection() {
        for (auto_next, keep_on_end, expected) in [
            (false, false, "always"),
            (false, true, "always"),
            (true, false, "no"),
            (true, true, "yes"),
        ] {
            let mut preferences = PreferenceStore::default();
            preferences
                .values
                .insert("playlistAutoPlayNext".into(), json!(auto_next));
            preferences
                .values
                .insert("keepOpenOnFileEnd".into(), json!(keep_on_end));
            let options = startup_options_with_home(&preferences, None);
            assert_eq!(option_value(&options, "keep-open"), Some(expected));
            for key in ["playlistAutoPlayNext", "keepOpenOnFileEnd"] {
                assert_eq!(effect_class(key), PreferenceEffectClass::StartupAndLive);
                assert_eq!(
                    live_operations(key, &preferences),
                    vec![set_property("keep-open", MpvFormat::String, expected)]
                );
            }
        }
    }

    #[test]
    fn general_values_reject_wrong_types_and_invalid_popup_tags() {
        for key in [
            "useLegacyFullScreen",
            "blackOutMonitor",
            "pauseWhenMinimized",
            "pauseWhenInactive",
            "playWhenEnteringFullScreen",
            "pauseWhenLeavingFullScreen",
            "pauseWhenGoesToSleep",
            "playlistAutoAdd",
            "playlistAutoPlayNext",
            "playlistShowMetadata",
            "playlistShowMetadataInMusicMode",
            "recordPlaybackHistory",
            "recordRecentFiles",
            "trackAllFilesInRecentOpenMenu",
        ] {
            assert!(validate_change(&PreferenceChange {
                key: key.into(),
                value: json!(true),
            })
            .is_ok());
            assert!(validate_change(&PreferenceChange {
                key: key.into(),
                value: json!(1),
            })
            .is_err());
        }
        for (key, value) in [
            ("actionAfterLaunch", json!(-1)),
            ("actionAfterLaunch", json!(3)),
            ("screenShotFormat", json!(-1)),
            ("screenShotFormat", json!(3)),
        ] {
            assert!(validate_change(&PreferenceChange {
                key: key.into(),
                value,
            })
            .is_err());
        }
    }

    #[test]
    fn reference_numeric_ranges_and_user_option_shape_are_validated() {
        for (key, value) in [
            ("videoThreads", json!(0)),
            ("hardwareDecoder", json!(2)),
            ("maxVolume", json!(1000)),
            ("transportRTSPThrough", json!(3)),
            ("toneMappingTargetPeak", json!(0)),
            ("toneMappingAlgorithm", json!(7)),
            ("subOverrideLevel", json!(2)),
            ("subTextSize", json!(55.5)),
            ("subBlur", json!(20)),
            ("subPos", json!(100)),
            ("subTextColor", json!("#80402080")),
            ("defaultEncoding", json!("BIG5-HKSCS")),
            ("enableLogging", json!(true)),
            ("logLevel", json!(1)),
            ("forceDedicatedGPU", json!(false)),
            ("loadIccProfile", json!(true)),
            ("enableHdrSupport", json!(true)),
            ("enableToneMapping", json!(false)),
            ("autoSearchOnlineSub", json!(false)),
            ("onlineSubProvider", json!(":opensubtitles")),
            (
                "onlineSubProvider",
                json!("plugin:io.iina.example:provider"),
            ),
            ("openSubUsername", json!("viewer")),
            ("assrtToken", json!("token")),
            ("ytdlSearchPath", json!("/opt/homebrew/bin")),
            ("httpProxy", json!("http://127.0.0.1:8080")),
            ("userOptions", json!([["profile", "gpu-hq"]])),
        ] {
            assert!(validate_change(&PreferenceChange {
                key: key.into(),
                value,
            })
            .is_ok());
        }
        for (key, value) in [
            ("videoThreads", json!(-1)),
            ("hardwareDecoder", json!(3)),
            ("maxVolume", json!(99)),
            ("maxVolume", json!(1001)),
            ("transportRTSPThrough", json!(4)),
            ("toneMappingTargetPeak", json!(-1)),
            ("toneMappingAlgorithm", json!("auto")),
            ("subOverrideLevel", json!(3)),
            ("subTextSize", json!(-0.1)),
            ("subBlur", json!(20.1)),
            ("subPos", json!(101)),
            ("subTextColor", json!("#bad")),
            ("defaultEncoding", json!("definitely-not-an-encoding")),
            ("enableLogging", json!(1)),
            ("logLevel", json!(4)),
            ("forceDedicatedGPU", json!(1)),
            ("loadIccProfile", json!(1)),
            ("enableHdrSupport", json!(1)),
            ("enableToneMapping", json!(1)),
            ("autoSearchOnlineSub", json!(1)),
            ("onlineSubProvider", json!("unknown")),
            ("onlineSubProvider", json!("plugin:missing-provider")),
            ("openSubUsername", json!(false)),
            ("assrtToken", json!(false)),
            ("httpProxy", json!(false)),
            ("userOptions", json!([["only-name"]])),
            ("userOptions", json!([["profile", ""]])),
            ("userOptions", json!([["", "gpu-hq"]])),
        ] {
            assert!(validate_change(&PreferenceChange {
                key: key.into(),
                value,
            })
            .is_err());
        }
    }

    #[test]
    fn ui_values_preserve_iina_135_ranges_geometry_and_toolbar_invariants() {
        for (key, value) in [
            ("themeMaterial", json!(4)),
            ("resizeWindowTiming", json!(1)),
            ("resizeWindowOption", json!(4)),
            ("oscPosition", json!(2)),
            ("controlBarAutoHideTimeout", json!(2.5)),
            ("controlBarPositionHorizontal", json!(0.5)),
            ("osdTextSize", json!(5)),
            ("maxThumbnailPreviewCacheSize", json!(500)),
            ("windowBehaviorWhenPip", json!(2)),
            ("initialWindowSizePosition", json!("x720%+20-30%")),
            ("controlBarToolbarButtons", json!([2, 1, 0, 6, 5])),
        ] {
            assert!(
                validate_change(&PreferenceChange {
                    key: key.into(),
                    value,
                })
                .is_ok(),
                "{key} should accept its reference value"
            );
        }
        for (key, value) in [
            ("themeMaterial", json!(1)),
            ("resizeWindowTiming", json!(3)),
            ("resizeWindowOption", json!(5)),
            ("oscPosition", json!(-1)),
            ("controlBarAutoHideTimeout", json!(-0.1)),
            ("controlBarPositionHorizontal", json!(1.1)),
            ("osdTextSize", json!(4.9)),
            ("maxThumbnailPreviewCacheSize", json!(-1)),
            ("windowBehaviorWhenPip", json!(3)),
            ("initialWindowSizePosition", json!("not geometry")),
            ("controlBarToolbarButtons", json!([2, 2])),
            ("controlBarToolbarButtons", json!([0, 1, 2, 3, 4, 5])),
            ("controlBarToolbarButtons", json!([7])),
        ] {
            assert!(
                validate_change(&PreferenceChange {
                    key: key.into(),
                    value,
                })
                .is_err(),
                "{key} should reject an invalid value"
            );
        }
    }

    #[test]
    fn imported_out_of_range_values_are_preserved_but_not_projected_to_mpv() {
        let mut preferences = PreferenceStore::default();
        preferences.values.insert("maxVolume".into(), json!(5_000));
        preferences.values.insert("videoThreads".into(), json!(-4));
        preferences
            .values
            .insert("defaultCacheSize".into(), json!(-1));

        let options = startup_options_with_home(&preferences, None);
        assert_eq!(preferences.values.get("maxVolume"), Some(&json!(5_000)));
        assert_eq!(option_value(&options, "volume-max"), Some("100"));
        assert_eq!(option_value(&options, "vd-lavc-threads"), Some("0"));
        assert_eq!(
            option_value(&options, "demuxer-max-bytes"),
            Some("153600KiB")
        );
    }
}
