use crate::mpv::MpvExecutorLifecycle;
use crate::player::{PlayerState, Track};
use crate::preferences::preference_file_path;
use crate::state::AppState;
use crate::{localization, native_video};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const INSPECTOR_WINDOW_LABEL: &str = "inspector";
pub const INSPECTOR_REFRESH_INTERVAL_MS: u64 = 1_000;
pub const MAX_WATCH_PROPERTIES: usize = 32;
pub const MAX_WATCH_PROPERTY_NAME_BYTES: usize = 96;
const MAX_PROPERTY_VALUE_CHARS: usize = 4_096;

const INSPECTOR_MPV_PROPERTIES: &[&str] = &[
    "path",
    "file-size",
    "file-format",
    "chapters",
    "editions",
    "duration",
    "video-format",
    "video-codec",
    "hwdec-current",
    "container-fps",
    "current-vo",
    "width",
    "height",
    "video-bitrate",
    "video-params/primaries",
    "video-params/gamma",
    "video-params/sig-peak",
    "video-params/colormatrix",
    "video-params/hw-pixelformat",
    "video-params/pixelformat",
    "audio-params/format",
    "audio-params/channels",
    "audio-bitrate",
    "audio-codec",
    "current-ao",
    "audio-params/samplerate",
    "avsync",
    "total-avsync-change",
    "frame-drop-count",
    "mistimed-frame-count",
    "display-fps",
    "estimated-vf-fps",
    "estimated-display-fps",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectorSnapshot {
    pub session_label: String,
    pub media_title: String,
    pub has_media: bool,
    pub general: InspectorGeneralSnapshot,
    pub tracks: Vec<InspectorTrackSnapshot>,
    pub file: InspectorFileSnapshot,
    pub status: InspectorStatusSnapshot,
    pub watch_properties: Vec<InspectorWatchProperty>,
    pub runtime: InspectorRuntimeSnapshot,
    pub refresh_interval_ms: u64,
    pub refreshed_at_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectorGeneralSnapshot {
    pub video: BTreeMap<String, Option<String>>,
    pub audio: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectorTrackSnapshot {
    pub key: String,
    pub kind: &'static str,
    pub readable_title: String,
    pub id: i64,
    pub default_track: bool,
    pub forced: bool,
    pub selected: bool,
    pub external: bool,
    pub source_id: Option<String>,
    pub title: Option<String>,
    pub language: Option<String>,
    pub file_path: Option<String>,
    pub codec: Option<String>,
    pub decoder: Option<String>,
    pub fps: Option<String>,
    pub channels: Option<String>,
    pub sample_rate: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectorFileSnapshot {
    pub path: Option<String>,
    pub size: Option<String>,
    pub format: Option<String>,
    pub duration: Option<String>,
    pub chapters: Option<String>,
    pub editions: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectorStatusSnapshot {
    pub av_sync_difference: Option<String>,
    pub total_av_sync: Option<String>,
    pub dropped_frames: Option<String>,
    pub mistimed_frames: Option<String>,
    pub display_fps: Option<String>,
    pub estimated_output_fps: Option<String>,
    pub estimated_display_fps: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectorWatchProperty {
    pub name: String,
    pub value: Option<String>,
    pub valid: bool,
    pub error: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectorRuntimeSnapshot {
    pub executor_lifecycle: &'static str,
    pub client_running: bool,
    pub executor_error: Option<String>,
    pub renderer_installed: bool,
    pub renderer_attached: bool,
    pub renderer_backend: &'static str,
}

/// Shows IINA's single reusable Inspector panel. Snapshot and watch-property commands are kept in
/// this module so the native menu does not need to know which player session is currently active.
pub fn show_inspector_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(INSPECTOR_WINDOW_LABEL) {
        window.unminimize().map_err(|error| error.to_string())?;
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(
        app,
        INSPECTOR_WINDOW_LABEL,
        WebviewUrl::App("inspector.html".into()),
    )
    .title(localization::menu_title_key(
        "InspectorWindowController",
        "F0z-JX-Cv5.title",
        "Inspector",
    ))
    .inner_size(350.0, 430.0)
    .min_inner_size(350.0, 430.0)
    .resizable(true)
    .decorations(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .center()
    .build()
    .map_err(|error| error.to_string())?;
    configure_native_inspector_panel(&window)?;
    Ok(())
}

#[tauri::command]
pub fn show_inspector(app: AppHandle) -> Result<(), String> {
    show_inspector_window(&app)
}

#[tauri::command]
pub fn get_inspector_snapshot(
    state: tauri::State<AppState>,
    window: WebviewWindow,
) -> Result<InspectorSnapshot, String> {
    require_inspector_window(&window)?;
    inspector_snapshot(state.inner())
}

#[tauri::command]
pub fn set_inspector_watch_properties(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    properties: Vec<String>,
) -> Result<Vec<String>, String> {
    require_inspector_window(&window)?;
    validate_watch_properties(&properties)?;

    let preferences = {
        let mut preferences = state
            .preferences
            .lock()
            .map_err(|error| error.to_string())?;
        preferences
            .values
            .insert("watchProperties".to_string(), serde_json::json!(properties));
        preferences.clone()
    };
    let config_directory = app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?;
    preferences.save_to_file(&preference_file_path(config_directory))?;
    read_watch_properties(state.inner())
}

fn require_inspector_window(window: &WebviewWindow) -> Result<(), String> {
    (window.label() == INSPECTOR_WINDOW_LABEL)
        .then_some(())
        .ok_or_else(|| "Inspector data is available only in the Inspector window".to_string())
}

#[cfg(target_os = "macos")]
fn configure_native_inspector_panel<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    use std::ffi::{c_int, c_void};

    unsafe extern "C" {
        fn iima_native_configure_inspector_panel(window: *mut c_void) -> c_int;
    }

    let status = unsafe {
        iima_native_configure_inspector_panel(
            window.ns_window().map_err(|error| error.to_string())?,
        )
    };
    (status == 0)
        .then_some(())
        .ok_or_else(|| format!("Unable to configure native Inspector panel ({status})"))
}

#[cfg(not(target_os = "macos"))]
fn configure_native_inspector_panel<R: Runtime>(_window: &WebviewWindow<R>) -> Result<(), String> {
    Ok(())
}

fn inspector_snapshot(state: &AppState) -> Result<InspectorSnapshot, String> {
    let session_label = state.last_active_player_session_label()?;
    let session = state.player_session_for_window(&session_label)?;
    let executor_status = session.mpv_executor_status()?;
    let player = session
        .player()
        .lock()
        .map(|player| player.clone())
        .map_err(|error| error.to_string())?;
    let watch_names = read_watch_properties(state)?;
    let valid_watch_names = watch_names
        .iter()
        .filter(|name| validate_watch_property_name(name).is_ok())
        .map(String::as_str);
    let requested_names = INSPECTOR_MPV_PROPERTIES
        .iter()
        .copied()
        .chain(valid_watch_names)
        .collect::<BTreeSet<_>>();
    let properties = session
        .mpv_executor()
        .lock()
        .map(|executor| executor.read_string_properties(requested_names.iter().copied()))
        .map_err(|error| error.to_string())?;
    let renderer = native_video::status(session.label());

    Ok(InspectorSnapshot {
        session_label,
        media_title: player.media_title.clone(),
        has_media: player.current_url.is_some(),
        general: general_snapshot(&player, &properties, renderer.backend),
        tracks: track_snapshots(&player),
        file: file_snapshot(&player, &properties),
        status: status_snapshot(&properties),
        watch_properties: watch_names
            .into_iter()
            .map(|name| watch_property_snapshot(name, &properties))
            .collect(),
        runtime: InspectorRuntimeSnapshot {
            executor_lifecycle: executor_lifecycle_name(executor_status.lifecycle),
            client_running: executor_status.client_running,
            executor_error: executor_status.last_error,
            renderer_installed: renderer.installed,
            renderer_attached: renderer.attached,
            renderer_backend: renderer.backend,
        },
        refresh_interval_ms: INSPECTOR_REFRESH_INTERVAL_MS,
        refreshed_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    })
}

fn executor_lifecycle_name(lifecycle: MpvExecutorLifecycle) -> &'static str {
    match lifecycle {
        MpvExecutorLifecycle::RuntimeUnavailable => "runtime-unavailable",
        MpvExecutorLifecycle::ClientNotStarted => "client-not-started",
        MpvExecutorLifecycle::ClientReady => "client-ready",
        MpvExecutorLifecycle::ClientError => "client-error",
    }
}

fn general_snapshot(
    player: &PlayerState,
    properties: &BTreeMap<String, Option<String>>,
    renderer_backend: &str,
) -> InspectorGeneralSnapshot {
    let selected_video = selected_or_first(&player.tracks.video);
    let selected_audio = selected_or_first(&player.tracks.audio);
    let width = value(properties, "width").or_else(|| {
        selected_video.and_then(|track| track.metadata.demux_width.map(|v| v.to_string()))
    });
    let height = value(properties, "height").or_else(|| {
        selected_video.and_then(|track| track.metadata.demux_height.map(|v| v.to_string()))
    });
    let video_size = width
        .zip(height)
        .map(|(width, height)| format!("{width}×{height}"));
    let video_bitrate = value(properties, "video-bitrate")
        .map(format_mpv_bit_rate)
        .or_else(|| {
            selected_video.and_then(|track| track.metadata.demux_bitrate.map(format_bit_rate))
        });
    let audio_bitrate = value(properties, "audio-bitrate")
        .map(format_mpv_bit_rate)
        .or_else(|| {
            selected_audio.and_then(|track| track.metadata.demux_bitrate.map(format_bit_rate))
        });
    let primaries = video_primaries(properties);
    let colorspace = value(properties, "video-params/colormatrix").map(|value| {
        format!(
            "{value} ({})",
            if player.quick_settings.hdr_available && player.quick_settings.hdr_enabled {
                "HDR"
            } else {
                "SDR"
            }
        )
    });
    let pixel_format = value(properties, "video-params/hw-pixelformat")
        .map(|value| format!("{value} (HW)"))
        .or_else(|| {
            value(properties, "video-params/pixelformat").map(|value| format!("{value} (SW)"))
        });

    InspectorGeneralSnapshot {
        video: BTreeMap::from([
            (
                "format".into(),
                value(properties, "video-format").or_else(|| {
                    player
                        .media_info
                        .as_ref()
                        .and_then(|info| info.video_summary.clone())
                }),
            ),
            ("size".into(), video_size),
            ("bitRate".into(), video_bitrate),
            (
                "codec".into(),
                value(properties, "video-codec")
                    .or_else(|| selected_video.and_then(|track| track.metadata.codec.clone())),
            ),
            ("hardwareDecoder".into(), value(properties, "hwdec-current")),
            (
                "driver".into(),
                value(properties, "current-vo").or_else(|| Some(renderer_backend.to_string())),
            ),
            ("primaries".into(), primaries),
            (
                "fps".into(),
                value(properties, "container-fps").or_else(|| {
                    selected_video.and_then(|track| track.metadata.demux_fps.map(format_number))
                }),
            ),
            ("colorspace".into(), colorspace),
            ("pixelFormat".into(), pixel_format),
        ]),
        audio: BTreeMap::from([
            ("format".into(), value(properties, "audio-params/format")),
            (
                "channels".into(),
                value(properties, "audio-params/channels").or_else(|| {
                    selected_audio.and_then(|track| track.metadata.demux_channels.clone())
                }),
            ),
            ("bitRate".into(), audio_bitrate),
            (
                "codec".into(),
                value(properties, "audio-codec")
                    .or_else(|| selected_audio.and_then(|track| track.metadata.codec.clone())),
            ),
            ("driver".into(), value(properties, "current-ao")),
            (
                "sampleRate".into(),
                value(properties, "audio-params/samplerate").or_else(|| {
                    selected_audio.and_then(|track| {
                        track
                            .metadata
                            .demux_samplerate
                            .map(|value| value.to_string())
                    })
                }),
            ),
        ]),
    }
}

fn selected_or_first(tracks: &[Track]) -> Option<&Track> {
    tracks
        .iter()
        .find(|track| track.selected)
        .or_else(|| tracks.first())
}

fn track_snapshots(player: &PlayerState) -> Vec<InspectorTrackSnapshot> {
    let mut result = Vec::new();
    result.extend(
        player
            .tracks
            .video
            .iter()
            .map(|track| track_snapshot("Video", track)),
    );
    result.extend(
        player
            .tracks
            .audio
            .iter()
            .map(|track| track_snapshot("Audio", track)),
    );
    result.extend(
        player
            .tracks
            .subtitles
            .iter()
            .map(|track| track_snapshot("Subtitle", track)),
    );
    result
}

fn track_snapshot(kind: &'static str, track: &Track) -> InspectorTrackSnapshot {
    let mut details = vec![format!("#{id}", id = track.id)];
    if !track.title.trim().is_empty() {
        details.push(track.title.clone());
    }
    if let Some(language) = track
        .metadata
        .language
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        details.push(language.to_string());
    }
    InspectorTrackSnapshot {
        key: format!("{}:{}", kind.to_ascii_lowercase(), track.id),
        kind,
        readable_title: format!("{kind} {}", details.join(" · ")),
        id: track.id,
        default_track: track.metadata.default_track,
        forced: track.metadata.forced,
        selected: track.selected,
        external: track.metadata.external,
        source_id: track.metadata.source_id.map(|value| value.to_string()),
        title: non_empty(track.title.clone()),
        language: track.metadata.language.clone().and_then(non_empty),
        file_path: track.metadata.external_filename.clone().and_then(non_empty),
        codec: track.metadata.codec.clone().and_then(non_empty),
        decoder: track
            .metadata
            .decoder_description
            .clone()
            .and_then(non_empty),
        fps: track.metadata.demux_fps.map(format_number),
        channels: track
            .metadata
            .demux_channels
            .clone()
            .or_else(|| track.metadata.audio_channels.clone())
            .and_then(non_empty),
        sample_rate: track
            .metadata
            .demux_samplerate
            .map(|value| value.to_string()),
    }
}

fn file_snapshot(
    player: &PlayerState,
    properties: &BTreeMap<String, Option<String>>,
) -> InspectorFileSnapshot {
    let path = value(properties, "path").or_else(|| player.current_url.clone());
    let metadata_size = path.as_deref().and_then(local_file_path).and_then(|path| {
        fs::metadata(path)
            .ok()
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.len())
    });
    let size = metadata_size.map(format_byte_count).or_else(|| {
        value(properties, "file-size")
            .and_then(|value| value.parse::<u64>().ok().map(format_byte_count))
    });
    let duration = value(properties, "duration")
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .or_else(|| {
            (player.duration_seconds.is_finite() && player.duration_seconds >= 0.0)
                .then_some(player.duration_seconds)
        })
        .map(format_duration);
    InspectorFileSnapshot {
        path,
        size,
        format: value(properties, "file-format").or_else(|| {
            player
                .media_info
                .as_ref()
                .and_then(|info| info.format.clone())
        }),
        duration,
        chapters: value(properties, "chapters").or_else(|| {
            player
                .current_url
                .as_ref()
                .map(|_| player.chapters.len().to_string())
        }),
        editions: value(properties, "editions"),
    }
}

fn status_snapshot(properties: &BTreeMap<String, Option<String>>) -> InspectorStatusSnapshot {
    InspectorStatusSnapshot {
        av_sync_difference: value(properties, "avsync"),
        total_av_sync: value(properties, "total-avsync-change"),
        dropped_frames: value(properties, "frame-drop-count"),
        mistimed_frames: value(properties, "mistimed-frame-count"),
        display_fps: value(properties, "display-fps"),
        estimated_output_fps: value(properties, "estimated-vf-fps"),
        estimated_display_fps: value(properties, "estimated-display-fps"),
    }
}

fn watch_property_snapshot(
    name: String,
    properties: &BTreeMap<String, Option<String>>,
) -> InspectorWatchProperty {
    if validate_watch_property_name(&name).is_err() {
        return InspectorWatchProperty {
            name,
            value: None,
            valid: false,
            error: Some("Invalid property name"),
        };
    }
    InspectorWatchProperty {
        value: value(properties, &name),
        name,
        valid: true,
        error: None,
    }
}

fn value(properties: &BTreeMap<String, Option<String>>, name: &str) -> Option<String> {
    properties
        .get(name)
        .and_then(Clone::clone)
        .and_then(non_empty)
        .map(bound_property_value)
}

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn bound_property_value(value: String) -> String {
    value.chars().take(MAX_PROPERTY_VALUE_CHARS).collect()
}

fn video_primaries(properties: &BTreeMap<String, Option<String>>) -> Option<String> {
    let peak = value(properties, "video-params/sig-peak")?
        .parse::<f64>()
        .ok()
        .filter(|peak| peak.is_finite() && *peak > 0.0)?;
    let primaries = value(properties, "video-params/primaries").unwrap_or_else(|| "?".into());
    let gamma = value(properties, "video-params/gamma").unwrap_or_else(|| "?".into());
    Some(format!(
        "{primaries} / {gamma} ({}DR)",
        if peak > 1.0 { "H" } else { "S" }
    ))
}

fn format_duration(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let hours = total / 3_600;
    let minutes = (total % 3_600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn format_decimal_count(value: u64) -> String {
    let (factor, prefix) = if value >= 1_000_000_000 {
        (1_000_000_000_u64, "G")
    } else if value >= 1_000_000 {
        (1_000_000_u64, "M")
    } else if value >= 1_000 {
        (1_000_u64, "K")
    } else {
        (1_u64, "")
    };
    let formatted = if factor == 1 {
        value.to_string()
    } else {
        let formatted = format!("{:.2}", value as f64 / factor as f64);
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    };
    format!("{formatted} {prefix}")
}

fn format_byte_count(bytes: u64) -> String {
    format!("{}B", format_decimal_count(bytes))
}

fn format_bit_rate(value: i64) -> String {
    format!("{}bps", format_decimal_count(value.max(0) as u64))
}

fn format_mpv_bit_rate(value: String) -> String {
    value
        .parse::<i64>()
        .map(format_bit_rate)
        .unwrap_or_else(|_| {
            if value.to_ascii_lowercase().ends_with("bps") {
                value
            } else {
                format!("{value}bps")
            }
        })
}

fn format_number(value: f64) -> String {
    let value = format!("{value:.3}");
    value
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn local_file_path(value: &str) -> Option<PathBuf> {
    if value.starts_with("file://") {
        return tauri::Url::parse(value).ok()?.to_file_path().ok();
    }
    (!value.contains("://")).then(|| PathBuf::from(value))
}

fn read_watch_properties(state: &AppState) -> Result<Vec<String>, String> {
    Ok(state
        .preferences
        .lock()
        .map_err(|error| error.to_string())?
        .values
        .get("watchProperties")
        .cloned()
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .take(MAX_WATCH_PROPERTIES)
        .collect())
}

fn validate_watch_properties(properties: &[String]) -> Result<(), String> {
    if properties.len() > MAX_WATCH_PROPERTIES {
        return Err(format!(
            "At most {MAX_WATCH_PROPERTIES} Inspector watch properties are allowed"
        ));
    }
    for property in properties {
        validate_watch_property_name(property)?;
    }
    Ok(())
}

fn validate_watch_property_name(name: &str) -> Result<(), String> {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_WATCH_PROPERTY_NAME_BYTES {
        return Err(format!(
            "mpv property names must contain 1 to {MAX_WATCH_PROPERTY_NAME_BYTES} bytes"
        ));
    }
    if !bytes[0].is_ascii_alphanumeric()
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-' | b'.' | b'/'))
        || name.contains("//")
        || name.contains("..")
        || name
            .as_bytes()
            .last()
            .is_some_and(|byte| matches!(*byte, b'/' | b'.' | b'-'))
    {
        return Err("Invalid mpv property name".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspector_window_and_refresh_match_reference_contract() {
        assert_eq!(INSPECTOR_WINDOW_LABEL, "inspector");
        assert_eq!(INSPECTOR_REFRESH_INTERVAL_MS, 1_000);
        let source = include_str!("../../src/inspector.html");
        let style = include_str!("../../src/inspector.css");
        let runtime = include_str!("../../src/inspector.js");
        for tab in ["General", "Tracks", "File", "Status"] {
            assert!(source.contains(tab));
        }
        assert!(style.contains("min-width: 350px"));
        assert!(style.contains("min-height: 430px"));
        assert!(runtime.contains("get_inspector_snapshot"));
        assert!(runtime.contains("set_inspector_watch_properties"));
        assert!(runtime.contains("window.setInterval(refresh, 1000)"));
        let native = include_str!("native_inspector.m");
        for contract in [
            "NSWindowStyleMaskUtilityWindow",
            "NSWindowStyleMaskHUDWindow",
            "window.hidesOnDeactivate = YES",
            "window.releasedWhenClosed = NO",
            "IINAInspectorPanel",
            "NSFloatingWindowLevel",
        ] {
            assert!(native.contains(contract));
        }
    }

    #[test]
    fn watch_property_names_are_strictly_bounded() {
        for valid in [
            "path",
            "video-params/primaries",
            "metadata/by-key/Artist",
            "track-list/0/codec",
        ] {
            assert!(validate_watch_property_name(valid).is_ok(), "{valid}");
        }
        for invalid in [
            "",
            "/path",
            "path/",
            "a//b",
            "a..b",
            "pause;quit",
            "\0path",
            " white-space",
        ] {
            assert!(
                validate_watch_property_name(invalid).is_err(),
                "{invalid:?}"
            );
        }
        assert!(validate_watch_property_name(&"a".repeat(MAX_WATCH_PROPERTY_NAME_BYTES)).is_ok());
        assert!(
            validate_watch_property_name(&"a".repeat(MAX_WATCH_PROPERTY_NAME_BYTES + 1)).is_err()
        );
    }

    #[test]
    fn watch_property_count_and_value_sizes_are_bounded() {
        let valid = vec!["path".to_string(); MAX_WATCH_PROPERTIES];
        assert!(validate_watch_properties(&valid).is_ok());
        let too_many = vec!["path".to_string(); MAX_WATCH_PROPERTIES + 1];
        assert!(validate_watch_properties(&too_many).is_err());
        assert_eq!(
            bound_property_value("x".repeat(MAX_PROPERTY_VALUE_CHARS + 20))
                .chars()
                .count(),
            MAX_PROPERTY_VALUE_CHARS
        );
    }

    #[test]
    fn file_and_number_formatters_match_inspector_display_shape() {
        assert_eq!(format_duration(5.2), "00:05");
        assert_eq!(format_duration(3_661.0), "1:01:01");
        assert_eq!(format_byte_count(2_500_000), "2.5 MB");
        assert_eq!(format_bit_rate(128_000), "128 Kbps");
        assert_eq!(format_mpv_bit_rate("128000".into()), "128 Kbps");
        assert_eq!(format_duration(5.9), "00:05");
        assert_eq!(format_number(23.976), "23.976");
    }
}
