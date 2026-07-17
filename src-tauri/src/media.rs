use std::collections::HashSet;
use std::env;
use std::fs;
#[cfg(test)]
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use md5::{Digest, Md5};
use serde::Serialize;
#[cfg(test)]
use serde_json::Value;

use crate::mpv::{
    inspect_media, libmpv_runtime_status, libmpv_runtime_versions, MpvHeadlessMediaSession,
    MpvMediaInspection,
};

const THUMBNAIL_CACHE_VERSION: u8 = 2;
const THUMBNAIL_DEFAULT_WIDTH: u32 = 240;
const THUMBNAIL_DEFAULT_COUNT: usize = 100;
const THUMBNAIL_PARTIAL_BATCH_SIZE: usize = 10;
const THUMBNAIL_NOTIFICATION_MIN_INTERVAL: Duration = Duration::from_millis(200);
const THUMBNAIL_NOTIFICATION_MAX_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Serialize)]
pub struct MediaRuntime {
    pub ffmpeg: ToolStatus,
    pub ffprobe: ToolStatus,
    pub mpv: ToolStatus,
    pub libmpv: ToolStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolStatus {
    pub name: String,
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaProbe {
    pub path: String,
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub duration_seconds: Option<f64>,
    pub format_name: Option<String>,
    pub format_long_name: Option<String>,
    pub bit_rate: Option<u64>,
    pub streams: Vec<MediaStreamProbe>,
    pub chapters: Vec<MediaChapterProbe>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaStreamProbe {
    pub index: i64,
    pub codec_type: String,
    pub codec_name: Option<String>,
    pub codec_long_name: Option<String>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub channels: Option<u64>,
    pub sample_rate: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaChapterProbe {
    pub index: usize,
    pub title: String,
    pub start_time_seconds: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThumbnailSet {
    pub source_path: String,
    pub width: u32,
    pub requested_count: usize,
    pub thumbnails: Vec<MediaThumbnail>,
    pub progress: f64,
    pub ready: bool,
    pub cache_hit: bool,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThumbnailProgress {
    pub source_path: String,
    pub width: u32,
    pub requested_count: usize,
    pub progress_index: usize,
    pub progress: f64,
    pub thumbnails: Vec<MediaThumbnail>,
    pub complete: bool,
    pub cache_hit: bool,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScreenshotResult {
    pub source_path: String,
    pub time_seconds: f64,
    pub path: String,
    pub format: String,
    pub saved_to_file: bool,
    pub copied_to_clipboard: bool,
    pub show_preview: bool,
}

#[derive(Debug, Clone)]
pub struct ScreenshotOptions {
    pub save_to_file: bool,
    pub copy_to_clipboard: bool,
    pub include_subtitles: bool,
    pub directory: Option<String>,
    pub format: ScreenshotFormat,
    pub template: String,
    pub show_preview: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenshotFormat {
    Png,
    Jpg,
    Jpeg,
    Ppm,
    Pgm,
    Pgmyuv,
    Tga,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaThumbnail {
    pub index: usize,
    pub time_seconds: f64,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProbeStatus {
    Probed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaInfo {
    pub probe_status: ProbeStatus,
    pub probe_message: Option<String>,
    pub format: Option<String>,
    pub duration_seconds: Option<f64>,
    pub bit_rate: Option<u64>,
    pub video_summary: Option<String>,
    pub audio_summary: Option<String>,
    pub subtitle_count: usize,
}

impl ToolStatus {
    fn missing(name: &str) -> Self {
        Self {
            name: name.to_string(),
            available: false,
            path: None,
            version: None,
        }
    }

    fn available(name: &str, path: PathBuf, version: Option<String>) -> Self {
        Self {
            name: name.to_string(),
            available: true,
            path: Some(path.display().to_string()),
            version,
        }
    }
}

impl Default for ScreenshotOptions {
    fn default() -> Self {
        Self {
            save_to_file: true,
            copy_to_clipboard: false,
            include_subtitles: true,
            directory: None,
            format: ScreenshotFormat::Png,
            template: "%F-%n".to_string(),
            show_preview: true,
        }
    }
}

impl ScreenshotFormat {
    pub fn from_i64(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::Png),
            1 => Some(Self::Jpg),
            2 => Some(Self::Jpeg),
            3 => Some(Self::Ppm),
            4 => Some(Self::Pgm),
            5 => Some(Self::Pgmyuv),
            6 => Some(Self::Tga),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpg => "jpg",
            Self::Jpeg => "jpeg",
            Self::Ppm => "ppm",
            Self::Pgm => "pgm",
            Self::Pgmyuv => "pgmyuv",
            Self::Tga => "tga",
        }
    }
}

impl MediaInfo {
    pub fn from_probe(probe: &MediaProbe) -> Self {
        Self {
            probe_status: ProbeStatus::Probed,
            probe_message: None,
            format: probe
                .format_long_name
                .clone()
                .or_else(|| probe.format_name.clone()),
            duration_seconds: probe.duration_seconds,
            bit_rate: probe.bit_rate,
            video_summary: probe.video_summary(),
            audio_summary: probe.audio_summary(),
            subtitle_count: probe
                .streams
                .iter()
                .filter(|stream| stream.codec_type == "subtitle")
                .count(),
        }
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            probe_status: ProbeStatus::Unavailable,
            probe_message: Some(message.into()),
            format: None,
            duration_seconds: None,
            bit_rate: None,
            video_summary: None,
            audio_summary: None,
            subtitle_count: 0,
        }
    }
}

impl MediaProbe {
    #[cfg(test)]
    fn from_ffprobe_value(path: &str, value: &Value) -> Result<Self, String> {
        let format = value.get("format").unwrap_or(&Value::Null);
        let title = tag_string(format, "title");
        let album = tag_string(format, "album");
        let artist = tag_string(format, "artist");
        let duration_seconds = value_string(format, "duration").and_then(|text| text.parse().ok());
        let format_name = value_string(format, "format_name");
        let format_long_name = value_string(format, "format_long_name");
        let bit_rate = value_string(format, "bit_rate").and_then(|text| text.parse().ok());

        let streams: Vec<MediaStreamProbe> = value
            .get("streams")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(MediaStreamProbe::from_ffprobe_value)
                    .collect()
            })
            .unwrap_or_default();

        let chapters: Vec<MediaChapterProbe> = value
            .get("chapters")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .enumerate()
                    .filter_map(|(index, item)| MediaChapterProbe::from_ffprobe_value(index, item))
                    .collect()
            })
            .unwrap_or_default();

        if streams.is_empty() && duration_seconds.is_none() {
            return Err("ffprobe returned no playable streams or duration".to_string());
        }

        Ok(Self {
            path: path.to_string(),
            title,
            album,
            artist,
            duration_seconds,
            format_name,
            format_long_name,
            bit_rate,
            streams,
            chapters,
        })
    }

    fn from_mpv_inspection(path: &str, inspection: MpvMediaInspection) -> Result<Self, String> {
        let streams = inspection
            .tracks
            .into_iter()
            .map(|track| MediaStreamProbe {
                index: track.ff_index.unwrap_or(track.id),
                codec_type: match track.track_type.as_str() {
                    "sub" => "subtitle".to_string(),
                    other => other.to_string(),
                },
                codec_name: track.codec,
                codec_long_name: track.decoder_desc,
                language: track.lang,
                title: track.title,
                width: track.demux_w.and_then(|value| u64::try_from(value).ok()),
                height: track.demux_h.and_then(|value| u64::try_from(value).ok()),
                channels: track
                    .demux_channel_count
                    .and_then(|value| u64::try_from(value).ok()),
                sample_rate: track
                    .demux_samplerate
                    .and_then(|value| u64::try_from(value).ok()),
            })
            .collect::<Vec<_>>();
        let chapters = inspection
            .chapters
            .into_iter()
            .map(|chapter| MediaChapterProbe {
                index: chapter.index,
                title: chapter.title,
                start_time_seconds: chapter.time_seconds,
            })
            .collect::<Vec<_>>();

        if streams.is_empty() && inspection.duration_seconds.is_none() {
            return Err("bundled libmpv returned no playable streams or duration".to_string());
        }

        Ok(Self {
            path: path.to_string(),
            title: inspection.media_title,
            album: inspection.album,
            artist: inspection.artist,
            duration_seconds: inspection.duration_seconds,
            format_name: inspection.file_format,
            format_long_name: None,
            bit_rate: inspection.bit_rate,
            streams,
            chapters,
        })
    }

    pub fn video_summary(&self) -> Option<String> {
        self.streams
            .iter()
            .find(|stream| stream.codec_type == "video")
            .map(|stream| {
                let codec = stream.codec_name.as_deref().unwrap_or("video");
                match (stream.width, stream.height) {
                    (Some(width), Some(height)) => format!("{codec} {width}x{height}"),
                    _ => codec.to_string(),
                }
            })
    }

    pub fn audio_summary(&self) -> Option<String> {
        self.streams
            .iter()
            .find(|stream| stream.codec_type == "audio")
            .map(|stream| {
                let codec = stream.codec_name.as_deref().unwrap_or("audio");
                match stream.channels {
                    Some(channels) => format!("{codec} {channels}ch"),
                    None => codec.to_string(),
                }
            })
    }
}

impl MediaStreamProbe {
    #[cfg(test)]
    fn from_ffprobe_value(value: &Value) -> Option<Self> {
        let codec_type = value_string(value, "codec_type")?;
        Some(Self {
            index: value
                .get("index")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
            codec_type,
            codec_name: value_string(value, "codec_name"),
            codec_long_name: value_string(value, "codec_long_name"),
            language: tag_string(value, "language"),
            title: tag_string(value, "title"),
            width: value.get("width").and_then(Value::as_u64),
            height: value.get("height").and_then(Value::as_u64),
            channels: value.get("channels").and_then(Value::as_u64),
            sample_rate: value_string(value, "sample_rate").and_then(|text| text.parse().ok()),
        })
    }

    pub fn display_title(&self) -> String {
        let mut parts = Vec::new();
        if let Some(title) = &self.title {
            parts.push(title.clone());
        }
        if let Some(language) = &self.language {
            if language != "und" {
                parts.push(language.clone());
            }
        }
        if let Some(codec) = &self.codec_name {
            parts.push(codec.clone());
        }
        match self.codec_type.as_str() {
            "video" => {
                if let (Some(width), Some(height)) = (self.width, self.height) {
                    parts.push(format!("{width}x{height}"));
                }
            }
            "audio" => {
                if let Some(channels) = self.channels {
                    parts.push(format!("{channels}ch"));
                }
            }
            _ => {}
        }

        if parts.is_empty() {
            format!("{} track {}", self.codec_type, self.index)
        } else {
            parts.join(" - ")
        }
    }
}

impl MediaChapterProbe {
    #[cfg(test)]
    fn from_ffprobe_value(index: usize, value: &Value) -> Option<Self> {
        let start_time_seconds = value_string(value, "start_time")?.parse().ok()?;
        let title = tag_string(value, "title").unwrap_or_else(|| format!("Chapter {}", index + 1));
        Some(Self {
            index,
            title,
            start_time_seconds,
        })
    }
}

pub fn media_runtime() -> MediaRuntime {
    let (libmpv, ffmpeg_version) = libmpv_tool_status();
    let ffmpeg = embedded_ffmpeg_tool_status("ffmpeg", &libmpv, ffmpeg_version.clone());
    let ffprobe = embedded_ffmpeg_tool_status("ffprobe", &libmpv, ffmpeg_version);
    MediaRuntime {
        ffmpeg,
        ffprobe,
        mpv: discover_executable("mpv"),
        libmpv,
    }
}

pub fn probe_media(path: &str) -> Result<MediaProbe, String> {
    if is_remote_url(path) {
        return Err(
            "Remote URL probing is deferred until native libmpv network loading is connected"
                .to_string(),
        );
    }

    if !Path::new(path).exists() {
        return Err(format!("Media file does not exist: {path}"));
    }

    MediaProbe::from_mpv_inspection(path, inspect_media(path)?)
}

pub fn generate_cached_thumbnails<F, C>(
    path: &str,
    width: Option<u32>,
    count: Option<usize>,
    cache_directory: &Path,
    max_cache_size_bytes: u64,
    mut on_progress: F,
    is_cancelled: C,
) -> Result<ThumbnailSet, String>
where
    F: FnMut(ThumbnailProgress),
    C: Fn() -> bool,
{
    validate_thumbnail_source(path)?;
    let width = normalized_thumbnail_width(width);
    let count = normalized_thumbnail_count(count);
    fs::create_dir_all(cache_directory)
        .map_err(|error| format!("Failed to create thumbnail cache directory: {error}"))?;

    let cache_name = thumbnail_cache_name(path);
    let cache_path = cache_directory.join(&cache_name);
    if thumbnail_cache_matches_source(&cache_path, Path::new(path)) {
        match read_thumbnail_cache(&cache_path, path, width, count) {
            Ok(thumbnails) => {
                if is_cancelled() {
                    return Ok(ThumbnailSet {
                        source_path: path.to_string(),
                        width,
                        requested_count: count,
                        thumbnails: Vec::new(),
                        progress: 0.0,
                        ready: false,
                        cache_hit: true,
                        cancelled: true,
                    });
                }
                let result = ThumbnailSet {
                    source_path: path.to_string(),
                    width,
                    requested_count: count,
                    thumbnails,
                    progress: 1.0,
                    ready: true,
                    cache_hit: true,
                    cancelled: false,
                };
                on_progress(ThumbnailProgress {
                    source_path: path.to_string(),
                    width,
                    requested_count: count,
                    progress_index: count,
                    progress: 1.0,
                    thumbnails: result.thumbnails.clone(),
                    complete: true,
                    cache_hit: true,
                    cancelled: false,
                });
                return Ok(result);
            }
            Err(_) => {
                let _ = fs::remove_file(&cache_path);
            }
        }
    } else if cache_path.is_file() {
        let _ = fs::remove_file(&cache_path);
    }

    let (result, cached_thumbnails) =
        generate_thumbnail_frames(path, width, count, &mut on_progress, &is_cancelled)?;
    if result.ready && !is_cancelled() && max_cache_size_bytes > 0 {
        if thumbnail_cache_size(cache_directory) > max_cache_size_bytes {
            clear_old_thumbnail_cache(cache_directory, max_cache_size_bytes / 2);
        }
        let _ = write_thumbnail_cache(&cache_path, Path::new(path), &cached_thumbnails);
    }
    Ok(result)
}

fn generate_thumbnail_frames<F, C>(
    path: &str,
    width: u32,
    count: usize,
    on_progress: &mut F,
    is_cancelled: &C,
) -> Result<(ThumbnailSet, Vec<CachedThumbnail>), String>
where
    F: FnMut(ThumbnailProgress),
    C: Fn() -> bool,
{
    validate_thumbnail_source(path)?;
    let (duration, mut media_session) = thumbnail_generation_context(path)?;
    let output_dir = thumbnail_output_dir(path, width, count)?;
    let _ = fs::remove_dir_all(&output_dir);
    fs::create_dir_all(&output_dir)
        .map_err(|error| format!("Failed to create thumbnail output directory: {error}"))?;
    let times = thumbnail_times(duration, count);
    let mut thumbnails = Vec::with_capacity(times.len());
    let mut cached_thumbnails = Vec::with_capacity(times.len());
    let mut partial_result = Vec::new();
    let mut last_notification = Instant::now();
    let mut last_progress_index = 0;

    for (index, time_seconds) in times.into_iter().enumerate() {
        if is_cancelled() {
            let progress = thumbnail_progress(index.saturating_sub(1), count);
            on_progress(ThumbnailProgress {
                source_path: path.to_string(),
                width,
                requested_count: count,
                progress_index: index.saturating_sub(1),
                progress,
                thumbnails: std::mem::take(&mut partial_result),
                complete: false,
                cache_hit: false,
                cancelled: true,
            });
            return Ok((
                ThumbnailSet {
                    source_path: path.to_string(),
                    width,
                    requested_count: count,
                    thumbnails,
                    progress,
                    ready: false,
                    cache_hit: false,
                    cancelled: true,
                },
                cached_thumbnails,
            ));
        }

        let output_path = output_dir.join(format!("thumb-{index:03}.jpg"));
        media_session.capture_video_frame(
            time_seconds,
            &output_path,
            Some(width),
            Duration::from_secs(10),
        )?;

        let jpeg = fs::read(&output_path)
            .map_err(|error| format!("Failed to read generated thumbnail: {error}"))?;
        if !is_jpeg(&jpeg) {
            return Err("FFmpeg generated an invalid JPEG thumbnail".to_string());
        }
        let thumbnail = MediaThumbnail {
            index,
            time_seconds,
            path: output_path.display().to_string(),
        };
        thumbnails.push(thumbnail.clone());
        partial_result.push(thumbnail);
        cached_thumbnails.push(CachedThumbnail { time_seconds, jpeg });
        last_progress_index = index;

        let elapsed = last_notification.elapsed();
        if elapsed >= THUMBNAIL_NOTIFICATION_MIN_INTERVAL
            && (partial_result.len() >= THUMBNAIL_PARTIAL_BATCH_SIZE
                || elapsed >= THUMBNAIL_NOTIFICATION_MAX_INTERVAL)
        {
            on_progress(ThumbnailProgress {
                source_path: path.to_string(),
                width,
                requested_count: count,
                progress_index: index,
                progress: thumbnail_progress(index, count),
                thumbnails: std::mem::take(&mut partial_result),
                complete: false,
                cache_hit: false,
                cancelled: false,
            });
            last_notification = Instant::now();
        }
    }

    on_progress(ThumbnailProgress {
        source_path: path.to_string(),
        width,
        requested_count: count,
        progress_index: count,
        progress: 1.0,
        thumbnails: thumbnails.clone(),
        complete: true,
        cache_hit: false,
        cancelled: false,
    });
    Ok((
        ThumbnailSet {
            source_path: path.to_string(),
            width,
            requested_count: count,
            thumbnails,
            progress: thumbnail_progress(last_progress_index, count),
            ready: true,
            cache_hit: false,
            cancelled: false,
        },
        cached_thumbnails,
    ))
}

fn validate_thumbnail_source(path: &str) -> Result<(), String> {
    if is_remote_url(path) {
        return Err("IINA does not generate OSC thumbnails for network media".to_string());
    }

    if !Path::new(path).exists() {
        return Err(format!("Media file does not exist: {path}"));
    }
    Ok(())
}

fn thumbnail_generation_context(path: &str) -> Result<(f64, MpvHeadlessMediaSession), String> {
    let session = MpvHeadlessMediaSession::open(path, Duration::from_secs(10))?;
    let inspection = session.inspection();
    if !inspection
        .tracks
        .iter()
        .any(|track| track.track_type == "video")
    {
        return Err("Cannot generate thumbnails: no video stream".to_string());
    }
    let duration = inspection
        .duration_seconds
        .filter(|duration| *duration > 0.0)
        .ok_or_else(|| "Cannot generate thumbnails: media duration is unknown".to_string())?;
    Ok((duration, session))
}

#[cfg(test)]
pub fn capture_screenshot(
    path: &str,
    time_seconds: Option<f64>,
) -> Result<ScreenshotResult, String> {
    capture_screenshot_with_options(path, time_seconds, &ScreenshotOptions::default())
}

#[cfg(test)]
pub fn capture_screenshot_with_options(
    path: &str,
    time_seconds: Option<f64>,
    options: &ScreenshotOptions,
) -> Result<ScreenshotResult, String> {
    if !options.save_to_file && !options.copy_to_clipboard {
        return Err("Screenshot output is disabled".to_string());
    }

    if is_remote_url(path) {
        return Err(
            "Remote screenshot capture is disabled until network media loading is connected"
                .to_string(),
        );
    }

    if !Path::new(path).exists() {
        return Err(format!("Media file does not exist: {path}"));
    }

    let probe = probe_media(path).ok();
    if probe.as_ref().is_some_and(|probe| {
        !probe
            .streams
            .iter()
            .any(|stream| stream.codec_type == "video")
    }) {
        return Err("Cannot capture screenshot: no video stream".to_string());
    }

    let timestamp = screenshot_time(
        time_seconds,
        probe.as_ref().and_then(|probe| probe.duration_seconds),
    );
    let ffmpeg = discover_executable("ffmpeg");
    let ffmpeg_path = ffmpeg.path.ok_or_else(|| {
        "ffmpeg is not available; install FFmpeg or bundle it with the app".to_string()
    })?;
    let output_path = screenshot_output_path(path, timestamp, options)?;
    let timestamp_arg = format!("{timestamp:.3}");

    let output = Command::new(&ffmpeg_path)
        .args([
            "-v",
            "error",
            "-ss",
            &timestamp_arg,
            "-i",
            path,
            "-frames:v",
            "1",
            "-y",
        ])
        .arg(&output_path)
        .output()
        .map_err(|error| format!("Failed to run ffmpeg: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("ffmpeg exited with status {}", output.status)
        } else {
            stderr
        });
    }

    let copied_to_clipboard = if options.copy_to_clipboard {
        copy_image_to_clipboard(&output_path)?;
        true
    } else {
        false
    };

    Ok(ScreenshotResult {
        source_path: path.to_string(),
        time_seconds: timestamp,
        path: output_path.display().to_string(),
        format: options.format.extension().to_string(),
        saved_to_file: options.save_to_file,
        copied_to_clipboard,
        show_preview: options.show_preview,
    })
}

pub fn configured_screenshot_directory_for_options(options: &ScreenshotOptions) -> PathBuf {
    options
        .directory
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(expand_tilde_path)
        .unwrap_or_else(default_screenshot_directory)
}

pub fn screenshot_cache_directory(cache_root: &Path) -> PathBuf {
    cache_root.join("screenshot_cache")
}

pub fn finalize_mpv_screenshot(
    source_path: &str,
    time_seconds: f64,
    output_directory: &Path,
    options: &ScreenshotOptions,
) -> Result<ScreenshotResult, String> {
    let output_path = latest_screenshot_file(output_directory)?;
    let copied_to_clipboard = if options.copy_to_clipboard {
        copy_image_to_clipboard(&output_path)?;
        true
    } else {
        false
    };
    let result = ScreenshotResult {
        source_path: source_path.to_string(),
        time_seconds: time_seconds.max(0.0),
        path: output_path.display().to_string(),
        format: options.format.extension().to_string(),
        saved_to_file: options.save_to_file,
        copied_to_clipboard,
        show_preview: options.show_preview,
    };
    if !options.save_to_file && !options.show_preview {
        fs::remove_file(&output_path)
            .map_err(|error| format!("Failed to remove temporary screenshot: {error}"))?;
    }
    Ok(result)
}

#[cfg(target_os = "macos")]
fn copy_image_to_clipboard(path: &Path) -> Result<(), String> {
    macos_clipboard::copy_image_file(path)
}

#[cfg(not(target_os = "macos"))]
fn copy_image_to_clipboard(_path: &Path) -> Result<(), String> {
    Err("Screenshot clipboard copy is only implemented on macOS".to_string())
}

#[cfg(target_os = "macos")]
mod macos_clipboard {
    use std::ffi::{c_char, c_void, CString};
    use std::path::Path;

    type Id = *mut c_void;
    type Sel = *mut c_void;
    type Class = *mut c_void;
    type Bool = i8;

    #[link(name = "AppKit", kind = "framework")]
    extern "C" {}

    #[link(name = "Foundation", kind = "framework")]
    extern "C" {}

    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> Class;
        fn sel_registerName(name: *const c_char) -> Sel;
        fn objc_msgSend();
    }

    pub fn copy_image_file(path: &Path) -> Result<(), String> {
        let path = path
            .to_str()
            .ok_or_else(|| "Screenshot path is not valid UTF-8".to_string())?;
        let ns_path = ns_string(path)?;

        unsafe {
            let ns_image = class("NSImage")?;
            let image: Id = msg_id1(
                msg_id0(ns_image, "alloc")?,
                "initWithContentsOfFile:",
                ns_path,
            )?;
            if image.is_null() {
                return Err("Failed to load screenshot image for clipboard".to_string());
            }

            let ns_array = class("NSArray")?;
            let objects: Id = msg_id1(ns_array, "arrayWithObject:", image)?;
            if objects.is_null() {
                return Err("Failed to create clipboard image object array".to_string());
            }

            let pasteboard: Id = msg_id0(class("NSPasteboard")?, "generalPasteboard")?;
            if pasteboard.is_null() {
                return Err("Failed to access the macOS pasteboard".to_string());
            }

            let _: Id = msg_id0(pasteboard, "clearContents")?;
            let ok = msg_bool(pasteboard, "writeObjects:", objects)?;
            if !ok {
                return Err("Failed to write screenshot image to the macOS pasteboard".to_string());
            }
        }

        Ok(())
    }

    fn ns_string(value: &str) -> Result<Id, String> {
        let c_value = CString::new(value)
            .map_err(|_| "Screenshot path contains an embedded NUL byte".to_string())?;
        unsafe {
            msg_id_cstr(
                msg_id0(class("NSString")?, "alloc")?,
                "initWithUTF8String:",
                c_value.as_ptr(),
            )
        }
    }

    fn class(name: &str) -> Result<Class, String> {
        let c_name =
            CString::new(name).map_err(|_| format!("Invalid Objective-C class: {name}"))?;
        let class = unsafe { objc_getClass(c_name.as_ptr()) };
        if class.is_null() {
            Err(format!("Objective-C class is not available: {name}"))
        } else {
            Ok(class)
        }
    }

    fn selector(name: &str) -> Result<Sel, String> {
        let c_name =
            CString::new(name).map_err(|_| format!("Invalid Objective-C selector: {name}"))?;
        let selector = unsafe { sel_registerName(c_name.as_ptr()) };
        if selector.is_null() {
            Err(format!("Objective-C selector is not available: {name}"))
        } else {
            Ok(selector)
        }
    }

    unsafe fn msg_id0(receiver: Id, name: &str) -> Result<Id, String> {
        let selector = selector(name)?;
        let send: extern "C" fn(Id, Sel) -> Id = std::mem::transmute(objc_msgSend as *const ());
        Ok(send(receiver, selector))
    }

    unsafe fn msg_id1(receiver: Id, name: &str, argument: Id) -> Result<Id, String> {
        let selector = selector(name)?;
        let send: extern "C" fn(Id, Sel, Id) -> Id = std::mem::transmute(objc_msgSend as *const ());
        Ok(send(receiver, selector, argument))
    }

    unsafe fn msg_id_cstr(receiver: Id, name: &str, argument: *const c_char) -> Result<Id, String> {
        let selector = selector(name)?;
        let send: extern "C" fn(Id, Sel, *const c_char) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        Ok(send(receiver, selector, argument))
    }

    unsafe fn msg_bool(receiver: Id, name: &str, argument: Id) -> Result<bool, String> {
        let selector = selector(name)?;
        let send: extern "C" fn(Id, Sel, Id) -> Bool =
            std::mem::transmute(objc_msgSend as *const ());
        Ok(send(receiver, selector, argument) != 0)
    }
}

fn discover_executable(name: &str) -> ToolStatus {
    for candidate in executable_candidates(name) {
        if is_executable_file(&candidate) {
            let version = tool_version(&candidate, name);
            return ToolStatus::available(name, candidate, version);
        }
    }
    ToolStatus::missing(name)
}

fn libmpv_tool_status() -> (ToolStatus, Option<String>) {
    let runtime = libmpv_runtime_status();
    if !runtime.available {
        return (ToolStatus::missing("libmpv"), None);
    }
    let Some(path) = runtime.path.map(PathBuf::from) else {
        return (ToolStatus::missing("libmpv"), None);
    };
    let (mpv_version, ffmpeg_version) = libmpv_runtime_versions();
    (
        ToolStatus::available("libmpv", path, mpv_version),
        ffmpeg_version,
    )
}

fn embedded_ffmpeg_tool_status(
    name: &str,
    libmpv: &ToolStatus,
    version: Option<String>,
) -> ToolStatus {
    let Some(path) = libmpv.path.as_deref() else {
        return ToolStatus::missing(name);
    };
    ToolStatus::available(name, PathBuf::from(path), version)
}

fn executable_candidates(name: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    if let Some(paths) = env::var_os("PATH") {
        for path in env::split_paths(&paths) {
            push_candidate(&mut candidates, &mut seen, path.join(name));
        }
    }

    for path in [
        format!("src-tauri/bin/{name}"),
        format!("/opt/homebrew/bin/{name}"),
        format!("/usr/local/bin/{name}"),
        format!("/usr/bin/{name}"),
    ] {
        push_candidate(&mut candidates, &mut seen, PathBuf::from(path));
    }

    candidates
}

fn push_candidate(candidates: &mut Vec<PathBuf>, seen: &mut HashSet<String>, path: PathBuf) {
    let key = path.display().to_string();
    if seen.insert(key) {
        candidates.push(path);
    }
}

fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn tool_version(path: &Path, name: &str) -> Option<String> {
    let version_arg = if name == "mpv" {
        "--version"
    } else {
        "-version"
    };
    let output = Command::new(path).arg(version_arg).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
}

fn is_remote_url(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}

#[derive(Debug)]
struct CachedThumbnail {
    time_seconds: f64,
    jpeg: Vec<u8>,
}

fn normalized_thumbnail_width(width: Option<u32>) -> u32 {
    width.unwrap_or(THUMBNAIL_DEFAULT_WIDTH).clamp(64, 720)
}

fn normalized_thumbnail_count(count: Option<usize>) -> usize {
    count
        .unwrap_or(THUMBNAIL_DEFAULT_COUNT)
        .clamp(1, THUMBNAIL_DEFAULT_COUNT)
}

fn thumbnail_progress(index: usize, count: usize) -> f64 {
    (index as f64 / count.max(1) as f64).clamp(0.0, 1.0)
}

pub fn thumbnail_cache_name(path: &str) -> String {
    let mut digest = Md5::new();
    digest.update(path.as_bytes());
    format!("{:x}", digest.finalize())
}

pub fn thumbnail_cache_size(cache_directory: &Path) -> u64 {
    fs::read_dir(cache_directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
        .sum()
}

pub fn clear_thumbnail_cache(cache_directory: &Path) -> Result<(), String> {
    if cache_directory.exists() {
        fs::remove_dir_all(cache_directory)
            .map_err(|error| format!("Failed to clear thumbnail cache: {error}"))?;
    }
    fs::create_dir_all(cache_directory)
        .map_err(|error| format!("Failed to recreate thumbnail cache: {error}"))
}

fn clear_old_thumbnail_cache(cache_directory: &Path, bytes_to_delete: u64) {
    let mut entries = fs::read_dir(cache_directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            metadata.is_file().then(|| {
                let access_time = metadata
                    .accessed()
                    .or_else(|_| metadata.modified())
                    .unwrap_or(UNIX_EPOCH);
                (entry.path(), metadata.len(), access_time)
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|(_, _, access_time)| *access_time);

    let mut deleted = 0;
    for (path, size, _) in entries {
        if deleted >= bytes_to_delete {
            break;
        }
        if fs::remove_file(path).is_ok() {
            deleted += size;
        }
    }
}

fn thumbnail_cache_matches_source(cache_path: &Path, source_path: &Path) -> bool {
    let Ok((source_size, source_timestamp)) = thumbnail_source_signature(source_path) else {
        return false;
    };
    let Ok(mut file) = fs::File::open(cache_path) else {
        return false;
    };
    let mut version = [0_u8; 1];
    let mut size = [0_u8; 8];
    let mut timestamp = [0_u8; 8];
    file.read_exact(&mut version).is_ok()
        && file.read_exact(&mut size).is_ok()
        && file.read_exact(&mut timestamp).is_ok()
        && version[0] == THUMBNAIL_CACHE_VERSION
        && u64::from_ne_bytes(size) == source_size
        && i64::from_ne_bytes(timestamp) == source_timestamp
}

fn thumbnail_source_signature(path: &Path) -> Result<(u64, i64), String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("Cannot get video file attributes: {error}"))?;
    let modified = metadata
        .modified()
        .map_err(|error| format!("Cannot get video modification date: {error}"))?;
    let timestamp = match modified.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(error) => -(error.duration().as_secs() as i64),
    };
    Ok((metadata.len(), timestamp))
}

fn write_thumbnail_cache(
    cache_path: &Path,
    source_path: &Path,
    thumbnails: &[CachedThumbnail],
) -> Result<(), String> {
    let (source_size, source_timestamp) = thumbnail_source_signature(source_path)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary_path = cache_path.with_extension(format!("tmp-{nonce}"));
    let write_result = (|| -> Result<(), String> {
        let mut file = fs::File::create(&temporary_path)
            .map_err(|error| format!("Cannot create thumbnail cache file: {error}"))?;
        file.write_all(&[THUMBNAIL_CACHE_VERSION])
            .and_then(|_| file.write_all(&source_size.to_ne_bytes()))
            .and_then(|_| file.write_all(&source_timestamp.to_ne_bytes()))
            .map_err(|error| format!("Cannot write thumbnail cache metadata: {error}"))?;
        for thumbnail in thumbnails {
            let block_length = (std::mem::size_of::<f64>() + thumbnail.jpeg.len()) as i64;
            file.write_all(&block_length.to_ne_bytes())
                .and_then(|_| file.write_all(&thumbnail.time_seconds.to_ne_bytes()))
                .and_then(|_| file.write_all(&thumbnail.jpeg))
                .map_err(|error| format!("Cannot write thumbnail cache image: {error}"))?;
        }
        file.flush()
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("Cannot finalize thumbnail cache: {error}"))?;
        fs::rename(&temporary_path, cache_path)
            .map_err(|error| format!("Cannot publish thumbnail cache: {error}"))
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    write_result
}

fn read_thumbnail_cache(
    cache_path: &Path,
    source_path: &str,
    width: u32,
    count: usize,
) -> Result<Vec<MediaThumbnail>, String> {
    let bytes = fs::read(cache_path)
        .map_err(|error| format!("Cannot read thumbnail cache file: {error}"))?;
    let metadata_size = 1 + std::mem::size_of::<u64>() + std::mem::size_of::<i64>();
    if bytes.len() < metadata_size || bytes[0] != THUMBNAIL_CACHE_VERSION {
        return Err("Thumbnail cache metadata is invalid".to_string());
    }

    let output_dir = thumbnail_output_dir(source_path, width, count)?;
    let _ = fs::remove_dir_all(&output_dir);
    fs::create_dir_all(&output_dir)
        .map_err(|error| format!("Failed to materialize thumbnail cache: {error}"))?;
    let mut offset = metadata_size;
    let mut thumbnails = Vec::new();
    while offset < bytes.len() {
        let block_length = take_native_i64(&bytes, &mut offset)?;
        if block_length < std::mem::size_of::<f64>() as i64 {
            return Err("Thumbnail cache block length is invalid".to_string());
        }
        let time_seconds = take_native_f64(&bytes, &mut offset)?;
        let jpeg_length = block_length as usize - std::mem::size_of::<f64>();
        let end = offset
            .checked_add(jpeg_length)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| "Thumbnail cache image is truncated".to_string())?;
        let jpeg = &bytes[offset..end];
        if !is_jpeg(jpeg) {
            return Err("Thumbnail cache contains an invalid JPEG".to_string());
        }
        let index = thumbnails.len();
        let output_path = output_dir.join(format!("thumb-{index:03}.jpg"));
        fs::write(&output_path, jpeg)
            .map_err(|error| format!("Cannot materialize cached thumbnail: {error}"))?;
        thumbnails.push(MediaThumbnail {
            index,
            time_seconds,
            path: output_path.display().to_string(),
        });
        offset = end;
    }
    if thumbnails.is_empty() {
        return Err("Thumbnail cache does not contain images".to_string());
    }
    Ok(thumbnails)
}

fn take_native_i64(bytes: &[u8], offset: &mut usize) -> Result<i64, String> {
    let end = offset
        .checked_add(std::mem::size_of::<i64>())
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| "Thumbnail cache header is truncated".to_string())?;
    let value = i64::from_ne_bytes(
        bytes[*offset..end]
            .try_into()
            .map_err(|_| "Thumbnail cache header is invalid".to_string())?,
    );
    *offset = end;
    Ok(value)
}

fn take_native_f64(bytes: &[u8], offset: &mut usize) -> Result<f64, String> {
    let end = offset
        .checked_add(std::mem::size_of::<f64>())
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| "Thumbnail cache timestamp is truncated".to_string())?;
    let value = f64::from_ne_bytes(
        bytes[*offset..end]
            .try_into()
            .map_err(|_| "Thumbnail cache timestamp is invalid".to_string())?,
    );
    *offset = end;
    Ok(value)
}

fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes.starts_with(&[0xff, 0xd8]) && bytes.ends_with(&[0xff, 0xd9])
}

fn thumbnail_output_dir(path: &str, width: u32, count: usize) -> Result<PathBuf, String> {
    let cache_key = format!("{}-{width}-{count}", thumbnail_cache_name(path));
    let output_dir = env::temp_dir().join("iima-thumbnails").join(cache_key);
    fs::create_dir_all(&output_dir)
        .map_err(|error| format!("Failed to create thumbnail cache directory: {error}"))?;
    Ok(output_dir)
}

fn thumbnail_times(duration_seconds: f64, count: usize) -> Vec<f64> {
    let interval = duration_seconds / count as f64;
    (0..=count)
        .map(|index| {
            let time = interval * index as f64;
            if duration_seconds > 0.05 {
                time.min(duration_seconds - 0.05)
            } else {
                0.0
            }
        })
        .collect()
}

#[cfg(test)]
fn screenshot_output_path(
    path: &str,
    time_seconds: f64,
    options: &ScreenshotOptions,
) -> Result<PathBuf, String> {
    let output_dir = screenshot_directory(options);
    fs::create_dir_all(&output_dir)
        .map_err(|error| format!("Failed to create screenshot directory: {error}"))?;

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    time_seconds.to_bits().hash(&mut hasher);
    let cache_key = format!("{:016x}", hasher.finish());
    let stem = Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(sanitize_file_stem)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "screenshot".to_string());
    let template = if options.template.trim().is_empty() {
        "%F-%n"
    } else {
        options.template.trim()
    };
    let extension = options.format.extension();
    let has_number = template.contains("%n");

    for index in 1..10_000 {
        let base = render_screenshot_template(template, &stem, index, time_seconds);
        let base = sanitize_file_stem(&base);
        let base = if has_number {
            base
        } else {
            format!("{base}-{index:04}")
        };
        let candidate = output_dir.join(format!("{base}.{extension}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Ok(output_dir.join(format!("{stem}-{cache_key}.{extension}")))
}

#[cfg(test)]
fn screenshot_directory(options: &ScreenshotOptions) -> PathBuf {
    if !options.save_to_file {
        return env::temp_dir().join("iima-screenshots");
    }
    configured_screenshot_directory_for_options(options)
}

fn latest_screenshot_file(directory: &Path) -> Result<PathBuf, String> {
    fs::read_dir(directory)
        .map_err(|error| format!("Failed to read screenshot directory: {error}"))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            let timestamp = metadata
                .created()
                .or_else(|_| metadata.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            Some((timestamp, entry.path()))
        })
        .max_by_key(|(timestamp, _)| *timestamp)
        .map(|(_, path)| path)
        .ok_or_else(|| "mpv did not create a screenshot file".to_string())
}

fn default_screenshot_directory() -> PathBuf {
    if let Ok(path) = env::var("IIMA_SCREENSHOT_DIR") {
        if !path.trim().is_empty() {
            return expand_tilde_path(&path);
        }
    }

    #[cfg(test)]
    {
        env::temp_dir().join("iima-screenshots")
    }

    #[cfg(not(test))]
    {
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Pictures").join("Screenshots"))
            .unwrap_or_else(|| env::temp_dir().join("iima-screenshots"))
    }
}

#[cfg(test)]
fn sanitize_file_stem(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .take(64)
        .collect()
}

#[cfg(test)]
fn render_screenshot_template(
    template: &str,
    file_stem: &str,
    index: usize,
    time_seconds: f64,
) -> String {
    let total = time_seconds.max(0.0).floor() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    template
        .replace("%F", file_stem)
        .replace("%n", &format!("{index:04}"))
        .replace("%P", &format!("{hours:02}-{minutes:02}-{seconds:02}"))
}

fn expand_tilde_path(path: &str) -> PathBuf {
    if path == "~" {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(path));
    }

    if let Some(rest) = path.strip_prefix("~/") {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(rest))
            .unwrap_or_else(|| PathBuf::from(path));
    }

    PathBuf::from(path)
}

#[cfg(test)]
fn screenshot_time(time_seconds: Option<f64>, duration_seconds: Option<f64>) -> f64 {
    let requested = time_seconds
        .filter(|seconds| seconds.is_finite())
        .unwrap_or_default()
        .max(0.0);

    match duration_seconds.filter(|duration| duration.is_finite() && *duration > 0.0) {
        Some(duration) if duration > 0.05 => requested.min(duration - 0.05),
        Some(_) => 0.0,
        None => requested,
    }
}

#[cfg(test)]
fn value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
fn tag_string(value: &Value, key: &str) -> Option<String> {
    value
        .get("tags")
        .and_then(Value::as_object)
        .and_then(|tags| {
            tags.iter()
                .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
                .map(|(_, value)| value)
        })
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ffprobe_streams_and_chapters() {
        let value: Value = serde_json::json!({
            "streams": [
                {
                    "index": 0,
                    "codec_type": "video",
                    "codec_name": "h264",
                    "width": 1920,
                    "height": 1080,
                    "tags": { "language": "und" }
                },
                {
                    "index": 1,
                    "codec_type": "audio",
                    "codec_name": "aac",
                    "channels": 2,
                    "sample_rate": "48000",
                    "tags": { "language": "eng", "title": "Stereo" }
                }
            ],
            "chapters": [
                { "start_time": "12.5", "tags": { "title": "Intro" } }
            ],
            "format": {
                "duration": "61.25",
                "format_name": "mov,mp4,m4a,3gp,3g2,mj2",
                "format_long_name": "QuickTime / MOV",
                "bit_rate": "900000",
                "tags": { "TITLE": "Sample", "album": "Example Album", "ARTIST": "Example Artist" }
            }
        });

        let probe = MediaProbe::from_ffprobe_value("/tmp/sample.mp4", &value).unwrap();

        assert_eq!(probe.title.as_deref(), Some("Sample"));
        assert_eq!(probe.album.as_deref(), Some("Example Album"));
        assert_eq!(probe.artist.as_deref(), Some("Example Artist"));
        assert_eq!(probe.duration_seconds, Some(61.25));
        assert_eq!(probe.video_summary().as_deref(), Some("h264 1920x1080"));
        assert_eq!(probe.audio_summary().as_deref(), Some("aac 2ch"));
        assert_eq!(probe.chapters[0].title, "Intro");
    }

    #[test]
    fn probes_real_fixture_when_requested() {
        let Ok(path) = env::var("IIMA_FFPROBE_FIXTURE") else {
            return;
        };

        let probe = probe_media(&path).unwrap();

        assert!(probe.duration_seconds.unwrap_or_default() > 0.0);
        assert!(probe
            .streams
            .iter()
            .any(|stream| stream.codec_type == "video"));
    }

    #[test]
    fn calculates_iina_style_thumbnail_times() {
        let times = thumbnail_times(10.0, 4);

        assert_eq!(times, vec![0.0, 2.5, 5.0, 7.5, 9.95]);
    }

    #[test]
    fn uses_mpv_path_md5_for_iina_thumbnail_cache_names() {
        assert_eq!(
            thumbnail_cache_name("abc"),
            "900150983cd24fb0d6963f7d28e17f72"
        );
    }

    #[test]
    fn round_trips_iina_v2_thumbnail_cache_and_invalidates_source_changes() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("iima-thumbnail-cache-test-{nonce}"));
        let source = root.join("fixture.mp4");
        let cache_directory = root.join("thumb_cache");
        fs::create_dir_all(&cache_directory).unwrap();
        fs::write(&source, b"media").unwrap();
        let cache_path = cache_directory.join(thumbnail_cache_name(source.to_str().unwrap()));
        let cached = vec![
            CachedThumbnail {
                time_seconds: 0.0,
                jpeg: vec![0xff, 0xd8, 0xff, 0xd9],
            },
            CachedThumbnail {
                time_seconds: 2.5,
                jpeg: vec![0xff, 0xd8, 0x01, 0xff, 0xd9],
            },
        ];

        write_thumbnail_cache(&cache_path, &source, &cached).unwrap();

        let bytes = fs::read(&cache_path).unwrap();
        assert_eq!(bytes[0], THUMBNAIL_CACHE_VERSION);
        assert_eq!(
            u64::from_ne_bytes(bytes[1..9].try_into().unwrap()),
            fs::metadata(&source).unwrap().len()
        );
        assert!(thumbnail_cache_matches_source(&cache_path, &source));
        let materialized =
            read_thumbnail_cache(&cache_path, source.to_str().unwrap(), 240, 100).unwrap();
        assert_eq!(
            materialized
                .iter()
                .map(|thumbnail| thumbnail.time_seconds)
                .collect::<Vec<_>>(),
            vec![0.0, 2.5]
        );
        assert!(materialized
            .iter()
            .all(|thumbnail| Path::new(&thumbnail.path).is_file()));
        assert_eq!(thumbnail_cache_size(&cache_directory), bytes.len() as u64);

        fs::write(&source, b"media changed").unwrap();
        assert!(!thumbnail_cache_matches_source(&cache_path, &source));
        clear_thumbnail_cache(&cache_directory).unwrap();
        assert_eq!(thumbnail_cache_size(&cache_directory), 0);

        let output_directory = thumbnail_output_dir(source.to_str().unwrap(), 240, 100).unwrap();
        let _ = fs::remove_dir_all(output_directory);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clamps_screenshot_time_to_media_bounds() {
        assert_eq!(screenshot_time(Some(-3.0), Some(10.0)), 0.0);
        assert_eq!(screenshot_time(Some(f64::NAN), Some(10.0)), 0.0);
        assert_eq!(screenshot_time(Some(12.0), Some(10.0)), 9.95);
        assert_eq!(screenshot_time(Some(2.5), None), 2.5);
    }

    #[test]
    fn builds_screenshot_output_paths_under_temp() {
        let path =
            screenshot_output_path("/tmp/My Movie!.mkv", 1.25, &ScreenshotOptions::default())
                .unwrap();

        assert!(path.starts_with(env::temp_dir().join("iima-screenshots")));
        assert!(path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("My_Movie_-0001") && name.ends_with(".png")));
    }

    #[test]
    fn rejects_screenshot_when_outputs_are_disabled() {
        let options = ScreenshotOptions {
            save_to_file: false,
            copy_to_clipboard: false,
            ..ScreenshotOptions::default()
        };

        let error = capture_screenshot_with_options("/tmp/not-needed.mp4", Some(0.0), &options)
            .unwrap_err();

        assert_eq!(error, "Screenshot output is disabled");
    }

    #[test]
    fn maps_iina_screenshot_format_values() {
        assert_eq!(ScreenshotFormat::from_i64(0), Some(ScreenshotFormat::Png));
        assert_eq!(ScreenshotFormat::from_i64(1), Some(ScreenshotFormat::Jpg));
        assert_eq!(ScreenshotFormat::from_i64(6), Some(ScreenshotFormat::Tga));
        assert_eq!(ScreenshotFormat::from_i64(99), None);
    }

    #[test]
    fn expands_tilde_for_screenshot_folder_preferences() {
        let Some(home) = env::var_os("HOME") else {
            return;
        };

        assert_eq!(
            expand_tilde_path("~/Pictures/Screenshots"),
            PathBuf::from(home).join("Pictures").join("Screenshots")
        );
    }

    #[test]
    fn finalizes_the_latest_mpv_screenshot_file() {
        let directory = unique_test_directory("latest-mpv-screenshot");
        fs::create_dir_all(&directory).unwrap();
        let older = directory.join("older.png");
        let latest = directory.join("latest.png");
        fs::write(&older, b"older").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&latest, b"latest").unwrap();

        let result = finalize_mpv_screenshot(
            "/tmp/current.mp4",
            2.5,
            &directory,
            &ScreenshotOptions::default(),
        )
        .unwrap();

        assert_eq!(Path::new(&result.path), latest);
        assert_eq!(result.time_seconds, 2.5);
        assert!(result.saved_to_file);
        assert!(!result.copied_to_clipboard);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn removes_unpersisted_mpv_screenshot_when_preview_is_disabled() {
        let directory = unique_test_directory("temporary-mpv-screenshot");
        fs::create_dir_all(&directory).unwrap();
        let screenshot = directory.join("temporary.png");
        fs::write(&screenshot, b"temporary").unwrap();
        let options = ScreenshotOptions {
            save_to_file: false,
            copy_to_clipboard: false,
            show_preview: false,
            ..ScreenshotOptions::default()
        };

        let result =
            finalize_mpv_screenshot("/tmp/current.mp4", 0.0, &directory, &options).unwrap();

        assert_eq!(Path::new(&result.path), screenshot);
        assert!(!screenshot.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn generates_real_thumbnails_when_requested() {
        let Ok(path) = env::var("IIMA_FFMPEG_THUMBNAIL_FIXTURE") else {
            return;
        };

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let cache_directory =
            env::temp_dir().join(format!("iima-real-thumbnail-cache-test-{nonce}"));
        let mut generation_events = Vec::new();
        let thumbnails = generate_cached_thumbnails(
            &path,
            Some(120),
            Some(2),
            &cache_directory,
            1024 * 1024,
            |event| generation_events.push(event),
            || false,
        )
        .unwrap();

        assert_eq!(thumbnails.thumbnails.len(), 3);
        assert!(thumbnails.ready);
        assert_eq!(thumbnails.progress, 1.0);
        assert!(!thumbnails.cache_hit);
        assert!(generation_events.last().unwrap().complete);
        assert!(thumbnails
            .thumbnails
            .iter()
            .all(|thumbnail| Path::new(&thumbnail.path).is_file()));

        let mut cache_events = Vec::new();
        let cached = generate_cached_thumbnails(
            &path,
            Some(120),
            Some(2),
            &cache_directory,
            1024 * 1024,
            |event| cache_events.push(event),
            || false,
        )
        .unwrap();
        assert!(cached.cache_hit);
        assert!(cached.ready);
        assert_eq!(cached.thumbnails.len(), 3);
        assert!(cache_events.last().unwrap().cache_hit);
        assert!(cache_directory.join(thumbnail_cache_name(&path)).is_file());
        fs::remove_dir_all(cache_directory).unwrap();
    }

    #[test]
    fn captures_real_screenshot_when_requested() {
        let Ok(path) = env::var("IIMA_FFMPEG_SCREENSHOT_FIXTURE") else {
            return;
        };

        let screenshot = capture_screenshot(&path, Some(0.0)).unwrap();

        assert_eq!(screenshot.source_path, path);
        assert!(Path::new(&screenshot.path).is_file());
        assert!(screenshot.saved_to_file);
        assert!(!screenshot.copied_to_clipboard);
        assert!(screenshot.show_preview);
    }

    #[test]
    fn copies_real_screenshot_to_clipboard_when_explicitly_requested() {
        if env::var("IIMA_TEST_MACOS_CLIPBOARD").ok().as_deref() != Some("1") {
            return;
        }
        let Ok(path) = env::var("IIMA_FFMPEG_SCREENSHOT_FIXTURE") else {
            return;
        };
        let options = ScreenshotOptions {
            save_to_file: false,
            copy_to_clipboard: true,
            show_preview: false,
            ..ScreenshotOptions::default()
        };

        let screenshot = capture_screenshot_with_options(&path, Some(0.0), &options).unwrap();

        assert!(!screenshot.saved_to_file);
        assert!(screenshot.copied_to_clipboard);
        assert!(!screenshot.show_preview);
    }

    fn unique_test_directory(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("iima-{label}-{}-{nonce}", std::process::id()))
    }
}
