use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CStr, CString, OsStr, OsString};
use std::os::raw::{c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::native_video;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MpvFormat {
    None,
    Flag,
    Int64,
    Double,
    String,
}

/// Lossless JSON-safe representation of a libmpv node used by IINA's JavaScript API.
///
/// Integer and floating-point payloads are strings on purpose: JavaScript's synchronous
/// transport is JSON based, while libmpv can return the complete i64 range and non-finite
/// doubles. The frontend converts these tagged values back to JavaScript numbers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum MpvPluginValue {
    Null,
    Flag(bool),
    Int64(String),
    Double(String),
    String(String),
    Array(Vec<MpvPluginValue>),
    Map(BTreeMap<String, MpvPluginValue>),
    ByteArray(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MpvPluginGetKind {
    Flag,
    Number,
    String,
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MpvObservedProperty {
    pub name: &'static str,
    pub format: MpvFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MpvPlaybackSessionPlan {
    pub initialization: Vec<MpvClientOperation>,
    pub rendering: Vec<MpvClientOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum MpvClientOperation {
    CreateClient,
    SetOption {
        name: String,
        value: String,
    },
    RequestLogMessages {
        level: String,
    },
    SetWakeupCallback,
    ObserveProperty {
        name: String,
        format: MpvFormat,
    },
    Initialize,
    SetProperty {
        name: String,
        format: MpvFormat,
        value: String,
    },
    SetPropertyNode {
        name: String,
        value: MpvPluginValue,
    },
    Command {
        command: String,
        args: Vec<String>,
    },
    CommandString {
        action: String,
    },
    RemoveFilterAt {
        name: String,
        index: usize,
    },
    CreateRenderContext {
        api: String,
    },
    SetRenderUpdateCallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MpvFilter {
    pub name: String,
    pub label: Option<String>,
    pub params: BTreeMap<String, String>,
    pub string_format: String,
}

impl MpvFilter {
    pub fn from_raw(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        if raw.is_empty() || raw.len() > 8_192 || raw.contains('\0') {
            return None;
        }
        let (head, raw_params) = raw
            .split_once('=')
            .map_or((raw, None), |(head, params)| (head, Some(params)));
        let (label, name) = if let Some(labeled) = head.strip_prefix('@') {
            let (label, name) = labeled.split_once(':')?;
            if label.is_empty() || name.is_empty() {
                return None;
            }
            (Some(label.to_string()), name.to_string())
        } else {
            if head.is_empty() {
                return None;
            }
            (None, head.to_string())
        };
        if !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return None;
        }

        Some(Self {
            params: parsed_filter_params(&name, raw_params),
            name,
            label,
            string_format: raw.to_string(),
        })
    }

    pub fn matches_raw(&self, raw: &str) -> bool {
        let Some(candidate) = Self::from_raw(raw) else {
            return false;
        };
        if self.name != candidate.name || self.label != candidate.label {
            return false;
        }
        if !self.params.is_empty() && !candidate.params.is_empty() {
            return self.params == candidate.params;
        }
        self.string_format == candidate.string_format
    }
}

pub const IINA_OBSERVED_PROPERTIES: &[MpvObservedProperty] = &[
    observed("track-list", MpvFormat::None),
    observed("vf", MpvFormat::None),
    observed("af", MpvFormat::None),
    observed("vid", MpvFormat::Int64),
    observed("aid", MpvFormat::Int64),
    observed("sid", MpvFormat::Int64),
    observed("secondary-sid", MpvFormat::Int64),
    observed("pause", MpvFormat::Flag),
    observed("loop-playlist", MpvFormat::String),
    observed("loop-file", MpvFormat::String),
    observed("chapter", MpvFormat::Int64),
    observed("deinterlace", MpvFormat::Flag),
    observed("hwdec", MpvFormat::String),
    observed("video-rotate", MpvFormat::Int64),
    observed("mute", MpvFormat::Flag),
    observed("volume", MpvFormat::Double),
    observed("audio-delay", MpvFormat::Double),
    observed("speed", MpvFormat::Double),
    observed("sub-delay", MpvFormat::Double),
    observed("sub-scale", MpvFormat::Double),
    observed("sub-pos", MpvFormat::Double),
    observed("contrast", MpvFormat::Int64),
    observed("brightness", MpvFormat::Int64),
    observed("gamma", MpvFormat::Int64),
    observed("hue", MpvFormat::Int64),
    observed("saturation", MpvFormat::Int64),
    observed("fullscreen", MpvFormat::Flag),
    observed("ontop", MpvFormat::Flag),
    observed("window-scale", MpvFormat::Double),
    observed("media-title", MpvFormat::String),
    observed("video-params/rotate", MpvFormat::Int64),
    observed("video-params/primaries", MpvFormat::String),
    observed("video-params/gamma", MpvFormat::String),
    observed("idle-active", MpvFormat::Flag),
];

pub const IINA_POLLED_PROPERTIES: &[MpvObservedProperty] = &[
    observed("path", MpvFormat::String),
    observed("media-title", MpvFormat::String),
    observed("metadata/by-key/album", MpvFormat::String),
    observed("metadata/by-key/artist", MpvFormat::String),
    observed("chapter-metadata/by-key/title", MpvFormat::String),
    observed("chapter-metadata/by-key/performer", MpvFormat::String),
    observed("duration", MpvFormat::Double),
    observed("time-pos", MpvFormat::Double),
    observed("percent-pos", MpvFormat::Double),
    observed("pause", MpvFormat::Flag),
    observed("volume", MpvFormat::Double),
    observed("speed", MpvFormat::Double),
    observed("mute", MpvFormat::Flag),
    observed("ab-loop-a", MpvFormat::Double),
    observed("ab-loop-b", MpvFormat::Double),
    observed("ab-loop-count", MpvFormat::String),
    observed("audio-device", MpvFormat::String),
    observed("chapter", MpvFormat::Int64),
    observed("chapters", MpvFormat::Int64),
    observed("playlist-count", MpvFormat::Int64),
    observed("playlist-pos", MpvFormat::Int64),
    observed("track-list/count", MpvFormat::Int64),
    observed("vid", MpvFormat::Int64),
    observed("aid", MpvFormat::Int64),
    observed("sid", MpvFormat::Int64),
    observed("dwidth", MpvFormat::Int64),
    observed("dheight", MpvFormat::Int64),
    observed("sub-codepage", MpvFormat::String),
    observed("idle-active", MpvFormat::Flag),
];

pub const REQUIRED_LIBMPV_SYMBOLS: &[&str] = &[
    "mpv_create",
    "mpv_client_name",
    "mpv_initialize",
    "mpv_destroy",
    "mpv_terminate_destroy",
    "mpv_command",
    "mpv_command_string",
    "mpv_command_async",
    "mpv_set_option_string",
    "mpv_set_property",
    "mpv_set_property_string",
    "mpv_set_property_async",
    "mpv_get_property",
    "mpv_get_property_string",
    "mpv_observe_property",
    "mpv_hook_add",
    "mpv_hook_continue",
    "mpv_wait_event",
    "mpv_request_log_messages",
    "mpv_set_wakeup_callback",
    "mpv_free",
    "mpv_free_node_contents",
    "mpv_error_string",
    "mpv_render_context_create",
    "mpv_render_context_set_update_callback",
    "mpv_render_context_update",
    "mpv_render_context_render",
    "mpv_render_context_report_swap",
    "mpv_render_context_free",
];

#[derive(Debug, Clone, Serialize)]
pub struct LibmpvRuntimeStatus {
    pub available: bool,
    pub path: Option<String>,
    pub load_error: Option<String>,
    pub missing_symbols: Vec<String>,
    pub symbols: Vec<LibmpvSymbolStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LibmpvSymbolStatus {
    pub name: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LibmpvClientSmokeReport {
    pub available: bool,
    pub path: Option<String>,
    pub steps: Vec<LibmpvClientSmokeStep>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LibmpvClientSmokeStep {
    pub name: String,
    pub ok: bool,
    pub code: Option<i32>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MpvClientEvent {
    pub event_id: i32,
    pub name: String,
    pub error: i32,
    pub reply_userdata: u64,
    pub property: Option<MpvPropertyChange>,
    pub start_file: Option<MpvStartFileEvent>,
    pub end_file: Option<MpvEndFileEvent>,
    pub hook: Option<MpvHookEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MpvHookEvent {
    pub name: String,
    pub id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MpvPropertyChange {
    pub name: String,
    pub format: MpvFormat,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MpvStartFileEvent {
    pub playlist_entry_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MpvEndFileEvent {
    pub reason: MpvEndFileReason,
    pub reason_code: i32,
    pub error: i32,
    pub error_message: Option<String>,
    pub playlist_entry_id: i64,
    pub playlist_insert_id: i64,
    pub playlist_insert_num_entries: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MpvEndFileReason {
    Eof,
    Stop,
    Quit,
    Error,
    Redirect,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct MpvWakeupHandle {
    signal: Arc<MpvWakeupSignal>,
}

#[derive(Debug)]
struct MpvWakeupSignal {
    pending: AtomicBool,
    callback_count: AtomicU64,
    wait_lock: Mutex<()>,
    wait_condvar: Condvar,
}

impl MpvWakeupSignal {
    fn notify(&self) {
        self.callback_count.fetch_add(1, Ordering::Relaxed);
        let guard = match self.wait_lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        self.pending.store(true, Ordering::Release);
        self.wait_condvar.notify_one();
        drop(guard);
    }
}

impl Default for MpvWakeupHandle {
    fn default() -> Self {
        Self {
            signal: Arc::new(MpvWakeupSignal {
                pending: AtomicBool::new(false),
                callback_count: AtomicU64::new(0),
                wait_lock: Mutex::new(()),
                wait_condvar: Condvar::new(),
            }),
        }
    }
}

impl MpvWakeupHandle {
    pub fn wait_timeout(&self, timeout: Duration) -> bool {
        if self.signal.pending.swap(false, Ordering::AcqRel) {
            return true;
        }
        let Ok(guard) = self.signal.wait_lock.lock() else {
            return false;
        };
        if self.signal.pending.swap(false, Ordering::AcqRel) {
            return true;
        }
        let _ = self
            .signal
            .wait_condvar
            .wait_timeout_while(guard, timeout, |_| {
                !self.signal.pending.load(Ordering::Acquire)
            });
        self.signal.pending.swap(false, Ordering::AcqRel)
    }

    pub fn callback_count(&self) -> u64 {
        self.signal.callback_count.load(Ordering::Acquire)
    }

    #[cfg(test)]
    fn notify(&self) {
        self.signal.notify();
    }

    fn callback_context(&self) -> *mut c_void {
        Arc::as_ptr(&self.signal).cast_mut().cast::<c_void>()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MpvTrackListItem {
    pub index: usize,
    pub id: i64,
    #[serde(rename = "type")]
    pub track_type: String,
    pub src_id: Option<i64>,
    pub title: Option<String>,
    pub lang: Option<String>,
    pub image: bool,
    pub albumart: bool,
    pub default_track: bool,
    pub forced: bool,
    pub codec: Option<String>,
    pub external: bool,
    pub external_filename: Option<String>,
    pub selected: bool,
    pub main_selection: bool,
    pub ff_index: Option<i64>,
    pub decoder_desc: Option<String>,
    pub demux_w: Option<i64>,
    pub demux_h: Option<i64>,
    pub demux_channel_count: Option<i64>,
    pub demux_channels: Option<String>,
    pub demux_samplerate: Option<i64>,
    pub demux_fps: Option<f64>,
    pub demux_bitrate: Option<i64>,
    pub demux_rotation: Option<i64>,
    pub demux_par: Option<String>,
    pub audio_channels: Option<String>,
}

/// Metadata collected from an isolated, headless client backed by IINA's bundled libmpv.
///
/// `media_title` deliberately contains only the media's `title` metadata tag. mpv's
/// `media-title` property falls back to the filename, so it is kept separately as
/// `display_title` to preserve the existing probe/fallback boundary in `media.rs`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct MpvMediaInspection {
    pub runtime_path: String,
    pub duration_seconds: Option<f64>,
    pub media_title: Option<String>,
    pub display_title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub file_format: Option<String>,
    pub bit_rate: Option<u64>,
    pub tracks: Vec<MpvTrackListItem>,
    pub chapters: Vec<MpvMediaChapter>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct MpvMediaChapter {
    pub index: usize,
    pub title: String,
    pub time_seconds: f64,
}

/// A short-lived libmpv client used for probing and thumbnail extraction without a render view.
///
/// Keeping the loaded client alive lets `media.rs` seek and capture a thumbnail batch without
/// reopening an 800 MB media file for every frame.
pub(crate) struct MpvHeadlessMediaSession {
    client: LibmpvClient,
    inspection: MpvMediaInspection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MpvPlaylistItem {
    pub index: usize,
    pub id: Option<i64>,
    pub filename: String,
    pub current: bool,
    pub playing: bool,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MpvAudioDevice {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MpvExecutorLifecycle {
    RuntimeUnavailable,
    ClientNotStarted,
    ClientReady,
    ClientError,
}

#[derive(Debug, Clone, Serialize)]
pub struct MpvExecutorStatus {
    pub lifecycle: MpvExecutorLifecycle,
    pub runtime_available: bool,
    pub runtime_path: Option<String>,
    pub runtime_load_error: Option<String>,
    pub runtime_missing_symbols: Vec<String>,
    pub client_running: bool,
    pub wakeup_callback_registered: bool,
    pub wakeup_count: u64,
    pub accepted_operation_count: usize,
    pub pending_operation_count: usize,
    pub executed_operation_count: usize,
    pub startup_operation_count: usize,
    pub drained_event_count: usize,
    pub seen_player_operation_sequence: u64,
    pub last_error: Option<String>,
    pub last_operations: Vec<MpvClientOperation>,
    /// Events drained since the previous status handoff. Unlike `last_events`, this is a lossless
    /// handoff queue for the player reducer and plugin Event API, not a diagnostic tail.
    pub new_events: Vec<MpvClientEvent>,
    pub last_events: Vec<MpvClientEvent>,
    pub polled_properties: Vec<MpvPropertyChange>,
    pub track_list: Vec<MpvTrackListItem>,
    pub playlist: Vec<MpvPlaylistItem>,
    pub audio_devices: Vec<MpvAudioDevice>,
    pub video_filters: Vec<MpvFilter>,
    pub audio_filters: Vec<MpvFilter>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpvStartupOption {
    pub name: String,
    pub value: String,
    pub best_effort: bool,
}

/// The process environment inherited before IINA starts any mpv client.
///
/// IINA mutates the process-wide `PATH` and `http_proxy` immediately before `mpvInit`. Keeping an
/// immutable baseline lets every future player build the same result instead of repeatedly
/// prefixing a `PATH` that a previous player already changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpvProcessEnvironmentBaseline {
    pub path: Option<OsString>,
    pub http_proxy: Option<OsString>,
    pub executable_directory: PathBuf,
}

impl MpvProcessEnvironmentBaseline {
    pub fn capture() -> Result<Self, String> {
        let executable = std::env::current_exe()
            .map_err(|error| format!("Unable to resolve the IINA executable path: {error}"))?;
        let executable_directory = executable
            .parent()
            .filter(|directory| !directory.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .ok_or_else(|| "The IINA executable path has no parent directory".to_string())?;
        Ok(Self {
            path: std::env::var_os("PATH"),
            http_proxy: std::env::var_os("http_proxy"),
            executable_directory,
        })
    }
}

/// Final process environment to install before creating a libmpv client.
///
/// `http_proxy: None` deliberately means remove the variable. This restores an absent captured
/// baseline after a prior player used a configured proxy instead of leaking that player setting
/// into a future client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpvProcessEnvironmentPlan {
    pub path: OsString,
    pub http_proxy: Option<OsString>,
}

/// Pure IINA `PlayerCore.startMPV` environment projection.
///
/// The reference order is custom youtube-dl directory, application executable directory, then the
/// inherited `PATH`. Repeated entries are removed so applying or rebuilding the plan is stable.
/// A non-empty proxy is prefixed unconditionally with `http://`, matching IINA 1.3.5.
pub fn build_mpv_process_environment_plan(
    baseline: &MpvProcessEnvironmentBaseline,
    custom_ytdl_path: &str,
    http_proxy: &str,
) -> Result<MpvProcessEnvironmentPlan, String> {
    if custom_ytdl_path.contains('\0') {
        return Err("ytdlSearchPath contains an interior NUL byte".to_string());
    }
    if http_proxy.contains('\0') {
        return Err("httpProxy contains an interior NUL byte".to_string());
    }
    validate_environment_value(
        baseline.executable_directory.as_os_str(),
        "executable directory",
    )?;
    if let Some(path) = baseline.path.as_deref() {
        validate_environment_value(path, "inherited PATH")?;
    }
    if let Some(proxy) = baseline.http_proxy.as_deref() {
        validate_environment_value(proxy, "inherited http_proxy")?;
    }

    let mut path_entries = Vec::<PathBuf>::new();
    let mut append_unique = |entry: PathBuf| {
        if !entry.as_os_str().is_empty() && !path_entries.contains(&entry) {
            path_entries.push(entry);
        }
    };
    if !custom_ytdl_path.is_empty() {
        for entry in std::env::split_paths(OsStr::new(custom_ytdl_path)) {
            append_unique(entry);
        }
    }
    append_unique(baseline.executable_directory.clone());
    if let Some(inherited_path) = baseline.path.as_deref() {
        for entry in std::env::split_paths(inherited_path) {
            append_unique(entry);
        }
    }
    let path = std::env::join_paths(path_entries)
        .map_err(|error| format!("Unable to assemble the mpv PATH: {error}"))?;
    validate_environment_value(&path, "projected PATH")?;

    let http_proxy = if http_proxy.is_empty() {
        baseline.http_proxy.clone()
    } else {
        Some(OsString::from(format!("http://{http_proxy}")))
    };
    if let Some(proxy) = http_proxy.as_deref() {
        validate_environment_value(proxy, "projected http_proxy")?;
    }
    Ok(MpvProcessEnvironmentPlan { path, http_proxy })
}

#[cfg(unix)]
fn validate_environment_value(value: &OsStr, label: &str) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt;

    (!value.as_bytes().contains(&0))
        .then_some(())
        .ok_or_else(|| format!("{label} contains an interior NUL byte"))
}

#[cfg(not(unix))]
fn validate_environment_value(value: &OsStr, label: &str) -> Result<(), String> {
    (!value.to_string_lossy().contains('\0'))
        .then_some(())
        .ok_or_else(|| format!("{label} contains an interior NUL byte"))
}

impl MpvStartupOption {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            best_effort: false,
        }
    }

    pub fn best_effort(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            best_effort: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpvStartupConfiguration {
    pub watch_later_directory: Option<PathBuf>,
    pub resume_last_position: bool,
    pub input_config_path: Option<PathBuf>,
    pub preference_options: Vec<MpvStartupOption>,
    pub process_environment: Option<MpvProcessEnvironmentPlan>,
}

type MpvRendererAttachCallback =
    dyn Fn(*mut c_void, &str, &str) -> Result<(), String> + Send + Sync;
type MpvRendererStatusCallback = dyn Fn(&str) -> Result<bool, String> + Send + Sync;

#[derive(Clone)]
struct MpvRendererBridge {
    attach: Arc<MpvRendererAttachCallback>,
    is_attached: Arc<MpvRendererStatusCallback>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MpvRendererAttachmentState {
    Detached,
    Attaching,
    Attached,
}

struct MpvRendererAttachment {
    state: MpvRendererAttachmentState,
    bridge: Option<MpvRendererBridge>,
}

struct MpvRendererAttachRequest {
    bridge: MpvRendererBridge,
    mpv_handle: *mut c_void,
    libmpv_path: String,
    session: String,
}

struct MpvPlayerOperationSyncPreparation {
    attachment: Option<MpvRendererAttachRequest>,
    errors: Vec<String>,
}

impl MpvRendererAttachRequest {
    /// Runs the AppKit-facing renderer attachment without an `MpvExecutor` mutex guard.
    ///
    /// On macOS the native bridge synchronously hops to the main queue. Keeping this request
    /// separate from executor mutation prevents the main thread and the executor poll thread from
    /// waiting on each other in opposite lock order.
    fn perform(self) -> Result<(), String> {
        (self.bridge.attach)(self.mpv_handle, &self.libmpv_path, &self.session)?;
        if !(self.bridge.is_attached)(&self.session)? {
            return Err(format!(
                "native video renderer for session {} did not report an attached render context",
                self.session
            ));
        }
        Ok(())
    }
}

impl MpvRendererAttachment {
    fn for_player() -> Self {
        #[cfg(target_os = "macos")]
        {
            return Self::required(MpvRendererBridge {
                attach: Arc::new(native_video::attach_mpv_client),
                is_attached: Arc::new(|session| Ok(native_video::status(session).attached)),
            });
        }
        #[cfg(not(target_os = "macos"))]
        {
            Self {
                state: MpvRendererAttachmentState::Attached,
                bridge: None,
            }
        }
    }

    fn required(bridge: MpvRendererBridge) -> Self {
        Self {
            state: MpvRendererAttachmentState::Detached,
            bridge: Some(bridge),
        }
    }

    fn begin_attach(
        &mut self,
        mpv_handle: *mut c_void,
        libmpv_path: &str,
        session: &str,
    ) -> Result<Option<MpvRendererAttachRequest>, String> {
        match self.state {
            MpvRendererAttachmentState::Attached | MpvRendererAttachmentState::Attaching => {
                return Ok(None)
            }
            MpvRendererAttachmentState::Detached => {}
        }
        let bridge = self
            .bridge
            .as_ref()
            .ok_or_else(|| "native video renderer bridge is unavailable".to_string())?;
        let request = MpvRendererAttachRequest {
            bridge: bridge.clone(),
            mpv_handle,
            libmpv_path: libmpv_path.to_string(),
            session: session.to_string(),
        };
        self.state = MpvRendererAttachmentState::Attaching;
        Ok(Some(request))
    }

    fn complete_attach(&mut self, result: Result<(), String>) -> Result<(), String> {
        if self.state != MpvRendererAttachmentState::Attaching {
            return Err("native video renderer attachment completion was not pending".to_string());
        }
        match result {
            Ok(()) => {
                self.state = MpvRendererAttachmentState::Attached;
                Ok(())
            }
            Err(error) => {
                self.state = MpvRendererAttachmentState::Detached;
                Err(error)
            }
        }
    }
}

impl Default for MpvStartupConfiguration {
    fn default() -> Self {
        Self {
            watch_later_directory: None,
            resume_last_position: true,
            input_config_path: None,
            preference_options: vec![MpvStartupOption::new("osd-level", "0")],
            process_environment: None,
        }
    }
}

pub struct MpvExecutor {
    native_video_session: String,
    runtime_status: LibmpvRuntimeStatus,
    wakeup_handle: MpvWakeupHandle,
    startup_configuration: MpvStartupConfiguration,
    client: Option<LibmpvClient>,
    renderer_attachment: MpvRendererAttachment,
    pending_operations: Vec<MpvClientOperation>,
    accepted_operation_count: usize,
    executed_operation_count: usize,
    startup_operation_count: usize,
    drained_event_count: usize,
    seen_player_operation_sequence: u64,
    last_error: Option<String>,
    recent_events: Vec<MpvClientEvent>,
    pending_state_events: Vec<MpvClientEvent>,
    pending_hook_events: Vec<MpvClientEvent>,
    dynamically_observed_properties: BTreeSet<String>,
    last_polled_properties: Vec<MpvPropertyChange>,
    last_track_list: Vec<MpvTrackListItem>,
    last_playlist: Vec<MpvPlaylistItem>,
    last_audio_devices: Vec<MpvAudioDevice>,
    last_video_filters: Vec<MpvFilter>,
    last_audio_filters: Vec<MpvFilter>,
}

const MAX_MPV_EXECUTOR_PENDING_OPERATIONS: usize = 500;
const MAX_MPV_EXECUTOR_RECENT_OPERATIONS: usize = 20;
const MAX_MPV_EXECUTOR_DRAIN_EVENTS_PER_SYNC: usize = 100;
const MAX_MPV_EXECUTOR_RECENT_EVENTS: usize = 50;
const MAX_MPV_TRACK_LIST_ITEMS: usize = 200;
const MAX_MPV_PLAYLIST_ITEMS: usize = 2_000;
const MAX_MPV_AUDIO_DEVICES: usize = 128;
const MAX_MPV_FILTERS: usize = 256;
const MAX_MPV_FILTER_PARAMS: usize = 128;
const MAX_MPV_MEDIA_CHAPTERS: usize = 10_000;
const IINA_SCREENSHOT_REPLY_USERDATA: u64 = 1_000_000;
const HEADLESS_MEDIA_TIMEOUT: Duration = Duration::from_secs(10);
const HEADLESS_MEDIA_OPTIONS: &[(&str, &str)] = &[
    ("config", "no"),
    ("terminal", "no"),
    ("input-default-bindings", "no"),
    ("vo", "null"),
    ("ao", "null"),
    ("pause", "yes"),
];
static MPV_PROCESS_ENVIRONMENT_LOCK: Mutex<()> = Mutex::new(());

impl Default for MpvExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl MpvExecutor {
    pub fn new() -> Self {
        Self::with_runtime_status_for_session(libmpv_runtime_status(), "main")
    }

    #[cfg(test)]
    pub fn with_runtime_status(runtime_status: LibmpvRuntimeStatus) -> Self {
        Self::with_runtime_status_for_session(runtime_status, "main")
    }

    pub fn with_runtime_status_for_session(
        runtime_status: LibmpvRuntimeStatus,
        native_video_session: impl Into<String>,
    ) -> Self {
        Self::with_runtime_status_and_wakeup_for_session(
            runtime_status,
            native_video_session,
            MpvWakeupHandle::default(),
        )
    }

    pub fn with_runtime_status_and_wakeup_for_session(
        runtime_status: LibmpvRuntimeStatus,
        native_video_session: impl Into<String>,
        wakeup_handle: MpvWakeupHandle,
    ) -> Self {
        Self {
            native_video_session: native_video_session.into(),
            runtime_status,
            wakeup_handle,
            startup_configuration: MpvStartupConfiguration::default(),
            client: None,
            renderer_attachment: MpvRendererAttachment::for_player(),
            pending_operations: Vec::new(),
            accepted_operation_count: 0,
            executed_operation_count: 0,
            startup_operation_count: 0,
            drained_event_count: 0,
            seen_player_operation_sequence: 0,
            last_error: None,
            recent_events: Vec::new(),
            pending_state_events: Vec::new(),
            pending_hook_events: Vec::new(),
            dynamically_observed_properties: BTreeSet::new(),
            last_polled_properties: Vec::new(),
            last_track_list: Vec::new(),
            last_playlist: Vec::new(),
            last_audio_devices: Vec::new(),
            last_video_filters: Vec::new(),
            last_audio_filters: Vec::new(),
        }
    }

    pub fn configure_startup(&mut self, configuration: MpvStartupConfiguration) -> bool {
        if self.client.is_some() {
            return false;
        }
        self.startup_configuration = configuration;
        true
    }

    #[cfg(test)]
    fn submit_player_operation_log(
        &mut self,
        first_sequence: u64,
        next_sequence: u64,
        operations: &[MpvClientOperation],
    ) -> MpvExecutorStatus {
        let mut preparation =
            self.begin_player_operation_log_sync(first_sequence, next_sequence, operations);
        if let Some(attachment) = preparation.attachment.take() {
            let result = attachment.perform();
            self.complete_renderer_attachment(result, &mut preparation.errors);
        }
        self.finish_player_operation_log_sync(preparation.errors)
    }

    fn begin_player_operation_log_sync(
        &mut self,
        first_sequence: u64,
        next_sequence: u64,
        operations: &[MpvClientOperation],
    ) -> MpvPlayerOperationSyncPreparation {
        let (start_index, sync_error) =
            self.next_operation_index(first_sequence, next_sequence, operations.len());
        let new_operations = operations.get(start_index..).unwrap_or_default().to_vec();
        self.seen_player_operation_sequence = next_sequence;

        let mut errors = sync_error.into_iter().collect::<Vec<_>>();
        if !new_operations.is_empty() {
            self.accepted_operation_count += new_operations.len();
            self.pending_operations.extend(new_operations);
            if self.pending_operations.len() > MAX_MPV_EXECUTOR_PENDING_OPERATIONS {
                let overflow = self.pending_operations.len() - MAX_MPV_EXECUTOR_PENDING_OPERATIONS;
                self.pending_operations.drain(0..overflow);
                errors.push(format!(
                    "mpv executor pending queue dropped {overflow} oldest operation(s)"
                ));
            }
        }

        let mut attachment = None;
        if !self.pending_operations.is_empty() || !self.runtime_status.available {
            if !self.runtime_status.available {
                errors.push(self.pending_blocker_message());
            } else if let Err(error) = self.ensure_client() {
                errors.push(error);
            } else {
                match self.begin_renderer_attachment() {
                    Ok(request) => attachment = request,
                    Err(error) => errors.push(error),
                }
            }
        } else {
            self.drain_client_events();
        }
        self.poll_client_properties();

        MpvPlayerOperationSyncPreparation { attachment, errors }
    }

    fn complete_renderer_attachment(
        &mut self,
        result: Result<(), String>,
        errors: &mut Vec<String>,
    ) {
        if let Err(error) = self.renderer_attachment.complete_attach(result) {
            errors.push(error);
        }
    }

    fn finish_player_operation_log_sync(&mut self, mut errors: Vec<String>) -> MpvExecutorStatus {
        if !self.pending_operations.is_empty() {
            match self.renderer_attachment.state {
                MpvRendererAttachmentState::Attached => {
                    if let Err(error) = self.drain_pending_operations() {
                        errors.push(error);
                    }
                }
                MpvRendererAttachmentState::Detached => {
                    if errors.is_empty() {
                        errors.push("native video renderer is not attached".to_string());
                    }
                }
                MpvRendererAttachmentState::Attaching => {}
            }
        } else {
            self.drain_client_events();
        }
        self.poll_client_properties();
        self.last_error = (!errors.is_empty()).then(|| errors.join("; "));

        self.status()
    }

    pub fn poll_status(&mut self) -> MpvExecutorStatus {
        self.drain_client_events();
        self.poll_client_properties();
        self.status()
    }

    /// Registers a libmpv hook on this player's real client. Hook registration is intentionally
    /// kept outside the player operation log: mpv hook userdata belongs to one concrete client
    /// and its continuation must be routed back to that same client.
    pub fn add_hook(
        &mut self,
        name: &str,
        priority: i32,
        reply_userdata: u64,
    ) -> Result<(), String> {
        if !self.runtime_status.available {
            return Err(self.pending_blocker_message());
        }
        self.ensure_client()?;
        self.drain_client_events();
        self.client
            .as_ref()
            .ok_or_else(|| "libmpv client was not initialized".to_string())?
            .add_hook(name, priority, reply_userdata)
    }

    /// Continues one concrete hook event. libmpv hook IDs are event instances, not registration
    /// userdata, so callers must preserve the ID delivered in `mpv_event_hook`.
    pub fn continue_hook(&mut self, hook_id: u64) -> Result<(), String> {
        self.client
            .as_ref()
            .ok_or_else(|| "libmpv client was not initialized".to_string())?
            .continue_hook(hook_id)
    }

    /// Mirrors `JavascriptAPIEvent.on("mpv.<property>.changed", ...)`: IINA asks libmpv to
    /// observe a non-core property as a double when the listener is registered. Core properties
    /// are already observed with their exact native format during client startup.
    pub fn observe_plugin_property(&mut self, name: &str) -> Result<bool, String> {
        validate_plugin_observed_property_name(name)?;
        if IINA_OBSERVED_PROPERTIES
            .iter()
            .any(|property| property.name == name)
            || self.dynamically_observed_properties.contains(name)
        {
            return Ok(false);
        }
        if !self.runtime_status.available {
            return Err(self.pending_blocker_message());
        }
        self.ensure_client()?;
        self.drain_client_events();
        self.client
            .as_mut()
            .ok_or_else(|| "libmpv client was not initialized".to_string())?
            .execute_operation(&MpvClientOperation::ObserveProperty {
                name: name.to_string(),
                format: MpvFormat::Double,
            })?;
        self.dynamically_observed_properties
            .insert(name.to_string());
        self.drain_client_events();
        Ok(true)
    }

    pub fn take_pending_hook_events(&mut self) -> Vec<MpvClientEvent> {
        std::mem::take(&mut self.pending_hook_events)
    }

    pub fn capture_screenshot(
        &mut self,
        directory: &Path,
        format: &str,
        template: &str,
        include_subtitles: bool,
        timeout: Duration,
    ) -> Result<(), String> {
        if !self.runtime_status.available {
            return Err(self.pending_blocker_message());
        }
        let directory = directory
            .to_str()
            .ok_or_else(|| "Screenshot directory is not valid UTF-8".to_string())?;
        self.ensure_client()?;
        self.drain_client_events();

        let (events, result) = {
            let client = self
                .client
                .as_mut()
                .ok_or_else(|| "libmpv client was not initialized".to_string())?;
            client.set_property("screenshot-directory", MpvFormat::String, directory)?;
            client.set_property("screenshot-format", MpvFormat::String, format)?;
            client.set_property("screenshot-template", MpvFormat::String, template)?;
            client.command_async(
                IINA_SCREENSHOT_REPLY_USERDATA,
                "screenshot",
                &[if include_subtitles {
                    "subtitles".to_string()
                } else {
                    "video".to_string()
                }],
            )?;
            client.wait_for_command_reply(IINA_SCREENSHOT_REPLY_USERDATA, timeout)
        };
        self.record_client_events(events);
        self.poll_client_properties();
        result
    }

    pub fn status(&mut self) -> MpvExecutorStatus {
        let lifecycle = if !self.runtime_status.available {
            MpvExecutorLifecycle::RuntimeUnavailable
        } else if self.client.is_some() {
            MpvExecutorLifecycle::ClientReady
        } else if self.last_error.is_some() {
            MpvExecutorLifecycle::ClientError
        } else {
            MpvExecutorLifecycle::ClientNotStarted
        };
        let last_error = self
            .last_error
            .clone()
            .or_else(|| (!self.runtime_status.available).then(|| self.pending_blocker_message()));

        let new_events = std::mem::take(&mut self.pending_state_events);
        MpvExecutorStatus {
            lifecycle,
            runtime_available: self.runtime_status.available,
            runtime_path: self.runtime_status.path.clone(),
            runtime_load_error: self.runtime_status.load_error.clone(),
            runtime_missing_symbols: self.runtime_status.missing_symbols.clone(),
            client_running: self.client.is_some(),
            accepted_operation_count: self.accepted_operation_count,
            pending_operation_count: self.pending_operations.len(),
            executed_operation_count: self.executed_operation_count,
            startup_operation_count: self.startup_operation_count,
            drained_event_count: self.drained_event_count,
            seen_player_operation_sequence: self.seen_player_operation_sequence,
            last_error,
            last_operations: recent_operations(&self.pending_operations),
            new_events,
            last_events: self.recent_events.clone(),
            polled_properties: self.last_polled_properties.clone(),
            track_list: self.last_track_list.clone(),
            playlist: self.last_playlist.clone(),
            audio_devices: self.last_audio_devices.clone(),
            video_filters: self.last_video_filters.clone(),
            audio_filters: self.last_audio_filters.clone(),
            wakeup_callback_registered: self
                .client
                .as_ref()
                .is_some_and(|client| client.wakeup_callback_registered),
            wakeup_count: self.wakeup_handle.callback_count(),
        }
    }

    /// Reads a bounded caller-owned list of mpv properties as display strings.
    ///
    /// This is intentionally a crate-private primitive rather than a Tauri command. The
    /// Inspector owns the only webview-facing call site and validates every user-supplied watch
    /// name before it reaches this method, so the executor never becomes an arbitrary mpv IPC
    /// proxy.
    pub(crate) fn read_string_properties<'a>(
        &self,
        names: impl IntoIterator<Item = &'a str>,
    ) -> BTreeMap<String, Option<String>> {
        names
            .into_iter()
            .map(|name| {
                let live = self.client.as_ref().and_then(|client| {
                    client
                        .get_property_value(name, MpvFormat::String)
                        .ok()
                        .flatten()
                });
                let cached = self
                    .last_polled_properties
                    .iter()
                    .rev()
                    .find(|property| property.name == name)
                    .and_then(|property| property.value.clone());
                (name.to_string(), live.or(cached))
            })
            .collect()
    }

    /// Synchronously reads an arbitrary property from the concrete libmpv client, matching
    /// `JavascriptAPIMpv` rather than the bounded observer cache used by the player UI.
    pub(crate) fn plugin_property(
        &mut self,
        name: &str,
        kind: MpvPluginGetKind,
    ) -> Result<MpvPluginValue, String> {
        validate_plugin_mpv_name(name)?;
        let fallback = plugin_property_fallback(kind);
        if !self.runtime_status.available {
            return Ok(fallback);
        }
        if self.ensure_client().is_err() {
            return Ok(fallback);
        }
        self.drain_client_events();
        let value = self
            .client
            .as_ref()
            .and_then(|client| client.plugin_property(name, kind).ok())
            .unwrap_or(fallback);
        Ok(value)
    }

    fn drain_pending_operations(&mut self) -> Result<(), String> {
        if !self.runtime_status.available {
            return Err(self.pending_blocker_message());
        }
        if self.pending_operations.is_empty() {
            return Ok(());
        }
        self.ensure_client()?;
        let renderer_readiness = match self.renderer_attachment.state {
            MpvRendererAttachmentState::Attached => Ok(()),
            MpvRendererAttachmentState::Attaching => {
                Err("native video renderer attachment is still in progress".to_string())
            }
            MpvRendererAttachmentState::Detached => {
                Err("native video renderer is not attached".to_string())
            }
        };
        self.drain_client_events();

        let mut pending_operations = std::mem::take(&mut self.pending_operations);
        let mut executed_operation_count = 0;
        let result = drain_pending_operation_queue(
            &mut pending_operations,
            &mut executed_operation_count,
            renderer_readiness,
            |operation| {
                self.client
                    .as_mut()
                    .ok_or_else(|| "libmpv client was not initialized".to_string())?
                    .execute_operation(operation)?;
                self.drain_client_events();
                Ok(())
            },
        );
        self.pending_operations = pending_operations;
        self.executed_operation_count += executed_operation_count;
        result
    }

    fn ensure_client(&mut self) -> Result<(), String> {
        if self.client.is_some() {
            return Ok(());
        }
        let Some(path) = self.runtime_status.path.as_deref().map(PathBuf::from) else {
            return Err("libmpv runtime status did not include a path".to_string());
        };
        let library = DynamicLibrary::open(&path).map_err(|error| {
            format!(
                "failed to load libmpv client from {}: {error}",
                path.display()
            )
        })?;
        let api = unsafe { LibmpvApi::load(library) }?;
        if let Some(environment) = &self.startup_configuration.process_environment {
            apply_mpv_process_environment_plan(environment)?;
        }
        let mut client = LibmpvClient::create(api, self.wakeup_handle.clone())?;
        let startup_operations = iina_mpv_executor_client_startup_operations_with_configuration(
            &self.startup_configuration,
        );
        for operation in &startup_operations {
            if let Err(error) = client.execute_operation(operation) {
                if is_best_effort_startup_operation(operation, &self.startup_configuration) {
                    eprintln!("iima: optional mpv startup preference was rejected: {error}");
                } else {
                    return Err(error);
                }
            }
            self.startup_operation_count += 1;
        }
        self.client = Some(client);
        self.drain_client_events();
        self.poll_client_properties();
        Ok(())
    }

    fn begin_renderer_attachment(&mut self) -> Result<Option<MpvRendererAttachRequest>, String> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| "libmpv client was not initialized".to_string())?;
        let path = self
            .runtime_status
            .path
            .as_deref()
            .ok_or_else(|| "libmpv runtime status did not include a path".to_string())?;
        self.renderer_attachment.begin_attach(
            client.handle.cast::<c_void>(),
            path,
            &self.native_video_session,
        )
    }

    fn drain_client_events(&mut self) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        let events = client.drain_events(MAX_MPV_EXECUTOR_DRAIN_EVENTS_PER_SYNC);
        self.record_client_events(events);
    }

    fn record_client_events(&mut self, events: Vec<MpvClientEvent>) {
        if events.is_empty() {
            return;
        }
        self.drained_event_count += events.len();
        self.pending_state_events.extend(events.iter().cloned());
        self.pending_hook_events
            .extend(events.iter().filter(|event| event.hook.is_some()).cloned());
        self.recent_events.extend(events);
        if self.recent_events.len() > MAX_MPV_EXECUTOR_RECENT_EVENTS {
            let overflow = self.recent_events.len() - MAX_MPV_EXECUTOR_RECENT_EVENTS;
            self.recent_events.drain(0..overflow);
        }
    }

    fn poll_client_properties(&mut self) {
        let Some(client) = self.client.as_mut() else {
            self.last_polled_properties.clear();
            self.last_track_list.clear();
            self.last_playlist.clear();
            self.last_audio_devices.clear();
            self.last_video_filters.clear();
            self.last_audio_filters.clear();
            return;
        };
        self.last_polled_properties = client.poll_properties(IINA_POLLED_PROPERTIES);
        self.last_track_list = client.poll_track_list();
        self.last_playlist = client.poll_playlist();
        self.last_audio_devices = client.poll_audio_devices();
        self.last_video_filters = client.poll_filters("vf");
        self.last_audio_filters = client.poll_filters("af");
    }

    fn next_operation_index(
        &self,
        first_sequence: u64,
        next_sequence: u64,
        operation_count: usize,
    ) -> (usize, Option<String>) {
        if operation_count == 0 {
            return (0, None);
        }
        if first_sequence > self.seen_player_operation_sequence {
            return (
                0,
                Some(format!(
                    "player mpv operation log was trimmed before executor sync; replaying {operation_count} visible operation(s)"
                )),
            );
        }
        if next_sequence < self.seen_player_operation_sequence {
            return (
                0,
                Some(
                    "player mpv operation sequence moved backwards; replaying visible operation(s)"
                        .to_string(),
                ),
            );
        }

        let offset = (self.seen_player_operation_sequence - first_sequence) as usize;
        if offset > operation_count {
            (
                0,
                Some(
                    "player mpv operation log metadata was inconsistent; replaying visible operation(s)"
                        .to_string(),
                ),
            )
        } else {
            (offset, None)
        }
    }

    fn pending_blocker_message(&self) -> String {
        if !self.runtime_status.available {
            libmpv_unavailable_message(&self.runtime_status)
        } else {
            "libmpv client is not initialized".to_string()
        }
    }
}

/// Synchronizes one player operation log without holding the executor mutex across native UI
/// attachment. The attachment bridge performs a synchronous main-queue hop on macOS, so this
/// lock boundary is the shared deadlock-prevention contract for every player session.
pub fn sync_mpv_executor_from_player_log(
    executor: &Mutex<MpvExecutor>,
    first_sequence: u64,
    next_sequence: u64,
    operations: &[MpvClientOperation],
) -> Result<MpvExecutorStatus, String> {
    let mut preparation = executor
        .lock()
        .map_err(|error| error.to_string())?
        .begin_player_operation_log_sync(first_sequence, next_sequence, operations);

    if let Some(attachment) = preparation.attachment.take() {
        perform_renderer_attachment_without_executor_lock(
            executor,
            attachment,
            &mut preparation.errors,
        )?;
    }

    Ok(executor
        .lock()
        .map_err(|error| error.to_string())?
        .finish_player_operation_log_sync(preparation.errors))
}

fn perform_renderer_attachment_without_executor_lock(
    executor: &Mutex<MpvExecutor>,
    attachment: MpvRendererAttachRequest,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    // Deliberately no `MutexGuard<MpvExecutor>` here: the native renderer may synchronously wait
    // for AppKit's main thread, which is itself allowed to enter executor-backed commands.
    let result = attachment.perform();
    executor
        .lock()
        .map_err(|error| error.to_string())?
        .complete_renderer_attachment(result, errors);
    Ok(())
}

fn apply_mpv_process_environment_plan(plan: &MpvProcessEnvironmentPlan) -> Result<(), String> {
    let _guard = MPV_PROCESS_ENVIRONMENT_LOCK
        .lock()
        .map_err(|error| format!("Unable to lock the mpv process environment: {error}"))?;
    validate_environment_value(&plan.path, "projected PATH")?;
    if let Some(proxy) = plan.http_proxy.as_deref() {
        validate_environment_value(proxy, "projected http_proxy")?;
    }

    // SAFETY: all environment writes owned by the mpv executor are serialized by the process-wide
    // lock above and happen immediately before `mpv_create`, matching IINA's startup boundary.
    // The application does not expose another Rust environment writer. libmpv may read these
    // values after creation, but this exact process-wide contract is required for youtube-dl and
    // proxy child-process inheritance.
    unsafe {
        std::env::set_var("PATH", &plan.path);
        if let Some(proxy) = &plan.http_proxy {
            std::env::set_var("http_proxy", proxy);
        } else {
            std::env::remove_var("http_proxy");
        }
    }
    Ok(())
}

fn is_best_effort_startup_operation(
    operation: &MpvClientOperation,
    configuration: &MpvStartupConfiguration,
) -> bool {
    let MpvClientOperation::SetOption { name, value } = operation else {
        return false;
    };
    configuration
        .preference_options
        .iter()
        .any(|option| option.best_effort && option.name == *name && option.value == *value)
}

fn drain_pending_operation_queue<F>(
    pending_operations: &mut Vec<MpvClientOperation>,
    executed_operation_count: &mut usize,
    renderer_readiness: Result<(), String>,
    mut execute: F,
) -> Result<(), String>
where
    F: FnMut(&MpvClientOperation) -> Result<(), String>,
{
    renderer_readiness?;
    while let Some(operation) = pending_operations.first().cloned() {
        execute(&operation)?;
        pending_operations.remove(0);
        *executed_operation_count += 1;
    }
    Ok(())
}

impl Drop for MpvExecutor {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        native_video::detach_mpv_client(&self.native_video_session);
    }
}

pub fn iina_observed_properties() -> Vec<MpvObservedProperty> {
    IINA_OBSERVED_PROPERTIES.to_vec()
}

pub fn iina_mpv_playback_session_plan() -> MpvPlaybackSessionPlan {
    let mut initialization = vec![
        MpvClientOperation::CreateClient,
        set_option("volume", "<initialVolume|softVolume preference>"),
        set_option("osd-level", "0"),
        set_option("input-media-keys", "no"),
        set_option(
            "screenshot-directory",
            "<screenshotFolder or screenshot cache>",
        ),
        set_option("screenshot-format", "<screenShotFormat preference>"),
        set_option("screenshot-template", "<screenShotTemplate preference>"),
        set_option("keep-open", "<keepOpenOnFileEnd + playlistAutoPlayNext>"),
        set_option("watch-later-directory", "<IINA watch-later directory>"),
        set_option("save-position-on-quit", "<resumeLastPosition preference>"),
        set_option("resume-playback", "<resumeLastPosition preference>"),
        set_option("sub-auto", "no"),
        set_option("sub-codepage", "<defaultEncoding preference>"),
        set_option("reset-on-next-file", "ab-loop-a,ab-loop-b"),
        set_option("input-conf", "<current input.conf path>"),
        MpvClientOperation::RequestLogMessages {
            level: "warn".to_string(),
        },
        MpvClientOperation::SetWakeupCallback,
    ];
    initialization.extend(IINA_OBSERVED_PROPERTIES.iter().map(|property| {
        MpvClientOperation::ObserveProperty {
            name: property.name.to_string(),
            format: property.format,
        }
    }));
    initialization.extend([
        MpvClientOperation::Initialize,
        set_property("vo", MpvFormat::String, "libmpv"),
        set_property("keepaspect", MpvFormat::String, "no"),
        set_property("gpu-hwdec-interop", MpvFormat::String, "auto"),
    ]);

    MpvPlaybackSessionPlan {
        initialization,
        rendering: vec![
            MpvClientOperation::CreateRenderContext {
                api: "opengl".to_string(),
            },
            MpvClientOperation::SetRenderUpdateCallback,
        ],
    }
}

#[cfg(test)]
fn iina_mpv_executor_client_startup_operations() -> Vec<MpvClientOperation> {
    iina_mpv_executor_client_startup_operations_with_configuration(
        &MpvStartupConfiguration::default(),
    )
}

fn iina_mpv_executor_client_startup_operations_with_configuration(
    configuration: &MpvStartupConfiguration,
) -> Vec<MpvClientOperation> {
    let mut operations = vec![
        MpvClientOperation::CreateClient,
        set_option("config", "no"),
        set_option("idle", "yes"),
        set_option("terminal", "no"),
        set_option("input-media-keys", "no"),
    ];
    if let Some(directory) = &configuration.watch_later_directory {
        operations.push(set_option(
            "watch-later-directory",
            directory.to_string_lossy(),
        ));
    }
    let resume = if configuration.resume_last_position {
        "yes"
    } else {
        "no"
    };
    operations.extend([
        set_option("save-position-on-quit", resume),
        set_option("resume-playback", resume),
    ]);
    // IINA applies Advanced > user options after its normal preference projection. This is
    // intentionally after the watch-later/resume defaults so an explicit user option can override
    // them, while the selected key-binding profile remains final below.
    operations.extend(
        configuration
            .preference_options
            .iter()
            .map(|option| set_option(&option.name, &option.value)),
    );
    if let Some(path) = &configuration.input_config_path {
        operations.push(set_option("input-conf", path.to_string_lossy()));
    }
    operations.extend([
        MpvClientOperation::RequestLogMessages {
            level: "warn".to_string(),
        },
        MpvClientOperation::SetWakeupCallback,
    ]);
    operations.extend(IINA_OBSERVED_PROPERTIES.iter().map(|property| {
        MpvClientOperation::ObserveProperty {
            name: property.name.to_string(),
            format: property.format,
        }
    }));
    operations.extend([
        MpvClientOperation::Initialize,
        set_property("vo", MpvFormat::String, "libmpv"),
        set_property("keepaspect", MpvFormat::String, "no"),
        set_property("gpu-hwdec-interop", MpvFormat::String, "auto"),
    ]);
    operations
}

pub fn libmpv_runtime_status() -> LibmpvRuntimeStatus {
    libmpv_runtime_status_for_candidates(libmpv_candidates())
}

/// Inspects a local media file through the same dynamically loaded libmpv used by playback.
///
/// This deliberately does not invoke the `mpv`, `ffmpeg`, or `ffprobe` executables. In a packaged
/// app `libmpv_runtime_status()` resolves `Contents/Frameworks/libmpv.2.dylib` first, so probing
/// remains self-contained on machines without Homebrew.
pub(crate) fn inspect_media(path: &str) -> Result<MpvMediaInspection, String> {
    let session = MpvHeadlessMediaSession::open(path, HEADLESS_MEDIA_TIMEOUT)?;
    Ok(session.inspection)
}

impl MpvHeadlessMediaSession {
    pub(crate) fn open(path: &str, timeout: Duration) -> Result<Self, String> {
        Self::open_with_runtime_status(libmpv_runtime_status(), path, timeout)
    }

    #[cfg(test)]
    pub(crate) fn open_with_runtime_path(
        path: &str,
        runtime_path: &Path,
        timeout: Duration,
    ) -> Result<Self, String> {
        Self::open_with_runtime_status(
            libmpv_runtime_status_for_candidates(vec![runtime_path.to_path_buf()]),
            path,
            timeout,
        )
    }

    fn open_with_runtime_status(
        runtime: LibmpvRuntimeStatus,
        path: &str,
        timeout: Duration,
    ) -> Result<Self, String> {
        if path.is_empty() {
            return Err("Media path must not be empty".to_string());
        }
        if !runtime.available {
            return Err(runtime.load_error.unwrap_or_else(|| {
                format!(
                    "libmpv is unavailable; missing symbols: {}",
                    runtime.missing_symbols.join(", ")
                )
            }));
        }
        let runtime_path = runtime
            .path
            .ok_or_else(|| "libmpv runtime status did not include a path".to_string())?;
        let library = DynamicLibrary::open(Path::new(&runtime_path))
            .map_err(|error| format!("Unable to load libmpv at {runtime_path}: {error}"))?;
        let api = unsafe { LibmpvApi::load(library) }
            .map_err(|error| format!("Unable to resolve libmpv at {runtime_path}: {error}"))?;
        let mut client = LibmpvClient::create(api, MpvWakeupHandle::default())?;

        for &(name, value) in HEADLESS_MEDIA_OPTIONS {
            client
                .execute_operation(&set_option(name, value))
                .map_err(|error| {
                    format!("Unable to set headless mpv option {name}={value}: {error}")
                })?;
        }
        client
            .execute_operation(&MpvClientOperation::Initialize)
            .map_err(|error| format!("Unable to initialize headless libmpv: {error}"))?;
        client
            .command("loadfile", &[path.to_string(), "replace".to_string()])
            .map_err(|error| format!("Unable to load media through libmpv: {error}"))?;
        client.wait_for_event(MPV_EVENT_FILE_LOADED, timeout, "loading media metadata")?;

        let inspection = inspect_loaded_media(&client, runtime_path);
        if inspection.tracks.is_empty() && inspection.duration_seconds.is_none() {
            return Err("libmpv returned no playable tracks or duration".to_string());
        }
        Ok(Self { client, inspection })
    }

    pub(crate) fn inspection(&self) -> &MpvMediaInspection {
        &self.inspection
    }

    /// Seeks the already-loaded file and writes a filtered video-only JPEG through libmpv.
    pub(crate) fn capture_video_frame(
        &mut self,
        time_seconds: f64,
        output_path: &Path,
        scale_width: Option<u32>,
        timeout: Duration,
    ) -> Result<(), String> {
        if !time_seconds.is_finite() || time_seconds < 0.0 {
            return Err("Thumbnail time must be a finite non-negative value".to_string());
        }
        if scale_width == Some(0) {
            return Err("Thumbnail width must be greater than zero".to_string());
        }
        if !self
            .inspection
            .tracks
            .iter()
            .any(|track| track.track_type == "video")
        {
            return Err("Cannot capture thumbnail: no video track".to_string());
        }
        let output_path = output_path
            .to_str()
            .ok_or_else(|| "Thumbnail output path is not valid UTF-8".to_string())?;

        let video_filter = scale_width
            .map(|width| format!("lavfi=[scale={width}:-2]"))
            .unwrap_or_default();
        self.client
            .set_property("vf", MpvFormat::String, &video_filter)
            .map_err(|error| format!("Unable to configure thumbnail scaling: {error}"))?;
        self.client
            .set_property("screenshot-format", MpvFormat::String, "jpg")?;
        self.client
            .set_property("screenshot-jpeg-quality", MpvFormat::Int64, "80")?;

        // A playback-restart from initial loading or a prior seek must not satisfy this seek.
        self.client
            .drain_events(MAX_MPV_EXECUTOR_DRAIN_EVENTS_PER_SYNC);
        self.client.command(
            "seek",
            &[format_mpv_float(time_seconds), "absolute+exact".to_string()],
        )?;
        self.client.wait_for_event(
            MPV_EVENT_PLAYBACK_RESTART,
            timeout,
            "decoding the exact thumbnail frame",
        )?;
        self.client.command(
            "screenshot-to-file",
            &[output_path.to_string(), "video".to_string()],
        )?;

        let jpeg = std::fs::read(output_path)
            .map_err(|error| format!("Unable to read libmpv thumbnail {output_path}: {error}"))?;
        if jpeg.len() < 4 || jpeg[..2] != [0xff, 0xd8] || jpeg[jpeg.len() - 2..] != [0xff, 0xd9] {
            return Err("libmpv generated an invalid JPEG thumbnail".to_string());
        }
        Ok(())
    }
}

fn inspect_loaded_media(client: &LibmpvClient, runtime_path: String) -> MpvMediaInspection {
    let duration_seconds = client.get_f64_property("duration");
    let tracks = client.poll_track_list();
    let track_bit_rate = tracks
        .iter()
        .filter_map(|track| track.demux_bitrate)
        .filter_map(|value| u64::try_from(value).ok())
        .filter(|value| *value > 0)
        .try_fold(0_u64, u64::checked_add)
        .filter(|value| *value > 0);
    let file_bit_rate = client
        .get_i64_property("file-size")
        .and_then(|value| u64::try_from(value).ok())
        .zip(duration_seconds.filter(|duration| *duration > 0.0))
        .and_then(|(bytes, duration)| {
            let bits_per_second = bytes as f64 * 8.0 / duration;
            bits_per_second
                .is_finite()
                .then_some(bits_per_second.round() as u64)
        })
        .filter(|value| *value > 0);

    MpvMediaInspection {
        runtime_path,
        duration_seconds,
        // `media-title` falls back to filename; only the real tag belongs in MediaProbe.title.
        media_title: client.get_string_property("metadata/by-key/title"),
        display_title: client.get_string_property("media-title"),
        album: client.get_string_property("metadata/by-key/album"),
        artist: client.get_string_property("metadata/by-key/artist"),
        file_format: client.get_string_property("file-format"),
        // Match ffprobe's format-level bitrate when local file size and duration are known.
        bit_rate: file_bit_rate.or(track_bit_rate),
        chapters: poll_media_chapters(client),
        tracks,
    }
}

fn poll_media_chapters(client: &LibmpvClient) -> Vec<MpvMediaChapter> {
    let count = client
        .get_i64_property("chapter-list/count")
        .unwrap_or_default()
        .clamp(0, MAX_MPV_MEDIA_CHAPTERS as i64) as usize;
    (0..count)
        .filter_map(|index| {
            let time_seconds = client.get_f64_property(&format!("chapter-list/{index}/time"))?;
            Some(MpvMediaChapter {
                index,
                title: client
                    .get_string_property(&format!("chapter-list/{index}/title"))
                    .unwrap_or_else(|| format!("Chapter {}", index + 1)),
                time_seconds,
            })
        })
        .collect()
}

/// Reads the versions reported by the same bundled libmpv/FFmpeg runtime used by playback.
///
/// AboutWindowController reads these properties from its active MPVController. This isolated
/// probe avoids substituting an unrelated Homebrew command-line executable when the playback
/// client has not been initialized yet, and it never attaches a render surface or loads media.
pub fn libmpv_runtime_versions() -> (Option<String>, Option<String>) {
    libmpv_runtime_versions_for_candidates(libmpv_candidates())
}

fn libmpv_runtime_versions_for_candidates(
    candidates: Vec<PathBuf>,
) -> (Option<String>, Option<String>) {
    let runtime = libmpv_runtime_status_for_candidates(candidates);
    let Some(path) = runtime.path.as_deref() else {
        return (None, None);
    };
    let Ok(library) = DynamicLibrary::open(Path::new(path)) else {
        return (None, None);
    };
    let Ok(api) = (unsafe { LibmpvApi::load(library) }) else {
        return (None, None);
    };
    let Ok(mut client) = LibmpvClient::create(api, MpvWakeupHandle::default()) else {
        return (None, None);
    };
    for (name, value) in [
        ("config", "no"),
        ("terminal", "no"),
        ("input-default-bindings", "no"),
        ("vo", "null"),
        ("ao", "null"),
    ] {
        if client
            .execute_operation(&MpvClientOperation::SetOption {
                name: name.to_string(),
                value: value.to_string(),
            })
            .is_err()
        {
            return (None, None);
        }
    }
    if client
        .execute_operation(&MpvClientOperation::Initialize)
        .is_err()
    {
        return (None, None);
    }
    (
        client.get_string_property("mpv-version"),
        client.get_string_property("ffmpeg-version"),
    )
}

pub fn smoke_libmpv_client_session() -> LibmpvClientSmokeReport {
    smoke_libmpv_client_session_for_candidates(libmpv_candidates())
}

pub fn mpv_command(
    command: impl Into<String>,
    args: impl IntoIterator<Item = impl Into<String>>,
) -> MpvClientOperation {
    MpvClientOperation::Command {
        command: command.into(),
        args: args.into_iter().map(Into::into).collect(),
    }
}

pub fn mpv_command_string(action: impl Into<String>) -> MpvClientOperation {
    MpvClientOperation::CommandString {
        action: action.into(),
    }
}

pub fn set_property(
    name: impl Into<String>,
    format: MpvFormat,
    value: impl Into<String>,
) -> MpvClientOperation {
    MpvClientOperation::SetProperty {
        name: name.into(),
        format,
        value: value.into(),
    }
}

pub fn set_plugin_property(name: impl Into<String>, value: MpvPluginValue) -> MpvClientOperation {
    let name = name.into();
    match value {
        MpvPluginValue::Flag(value) => {
            set_property(name, MpvFormat::Flag, if value { "true" } else { "false" })
        }
        MpvPluginValue::Int64(value) => set_property(name, MpvFormat::Int64, value),
        MpvPluginValue::Double(value) => set_property(name, MpvFormat::Double, value),
        MpvPluginValue::String(value) => set_property(name, MpvFormat::String, value),
        value => MpvClientOperation::SetPropertyNode { name, value },
    }
}

const fn observed(name: &'static str, format: MpvFormat) -> MpvObservedProperty {
    MpvObservedProperty { name, format }
}

fn validate_plugin_observed_property_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= 512
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'/'));
    if valid {
        Ok(())
    } else {
        Err(
            "Plugin-observed mpv properties must use 1-512 ASCII letters, digits, _, -, or /"
                .to_string(),
        )
    }
}

fn validate_plugin_mpv_name(name: &str) -> Result<(), String> {
    if !name.trim().is_empty() && name.len() <= 8_192 && !name.contains('\0') {
        Ok(())
    } else {
        Err(
            "Plugin mpv names must be non-empty, at most 8192 bytes, and contain no nul byte"
                .to_string(),
        )
    }
}

fn plugin_property_fallback(kind: MpvPluginGetKind) -> MpvPluginValue {
    match kind {
        MpvPluginGetKind::Flag => MpvPluginValue::Flag(false),
        MpvPluginGetKind::Number => MpvPluginValue::Double("0".to_string()),
        MpvPluginGetKind::String | MpvPluginGetKind::Native => MpvPluginValue::Null,
    }
}

fn set_option(name: impl Into<String>, value: impl Into<String>) -> MpvClientOperation {
    MpvClientOperation::SetOption {
        name: name.into(),
        value: value.into(),
    }
}

fn libmpv_runtime_status_for_candidates(candidates: Vec<PathBuf>) -> LibmpvRuntimeStatus {
    let mut last_error = None;
    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }

        let loaded = DynamicLibrary::open(&candidate);
        let library = match loaded {
            Ok(library) => library,
            Err(error) => {
                last_error = Some(format!("{}: {error}", candidate.display()));
                continue;
            }
        };

        let symbols = REQUIRED_LIBMPV_SYMBOLS
            .iter()
            .map(|symbol| LibmpvSymbolStatus {
                name: (*symbol).to_string(),
                resolved: library.has_symbol(symbol),
            })
            .collect::<Vec<_>>();
        let missing_symbols = symbols
            .iter()
            .filter(|symbol| !symbol.resolved)
            .map(|symbol| symbol.name.clone())
            .collect::<Vec<_>>();

        return LibmpvRuntimeStatus {
            available: missing_symbols.is_empty(),
            path: Some(candidate.display().to_string()),
            load_error: None,
            missing_symbols,
            symbols,
        };
    }

    LibmpvRuntimeStatus {
        available: false,
        path: None,
        load_error: last_error.or_else(|| Some("libmpv dylib was not found".to_string())),
        missing_symbols: REQUIRED_LIBMPV_SYMBOLS
            .iter()
            .map(|symbol| (*symbol).to_string())
            .collect(),
        symbols: REQUIRED_LIBMPV_SYMBOLS
            .iter()
            .map(|symbol| LibmpvSymbolStatus {
                name: (*symbol).to_string(),
                resolved: false,
            })
            .collect(),
    }
}

fn smoke_libmpv_client_session_for_candidates(candidates: Vec<PathBuf>) -> LibmpvClientSmokeReport {
    let runtime = libmpv_runtime_status_for_candidates(candidates.clone());
    if !runtime.available {
        return LibmpvClientSmokeReport {
            available: false,
            path: runtime.path,
            steps: Vec::new(),
            error: runtime.load_error.or_else(|| {
                Some(format!(
                    "libmpv is unavailable; missing symbols: {}",
                    runtime.missing_symbols.join(", ")
                ))
            }),
        };
    }

    let Some(path) = runtime.path.as_deref().map(PathBuf::from) else {
        return LibmpvClientSmokeReport {
            available: false,
            path: None,
            steps: Vec::new(),
            error: Some("libmpv runtime status did not include a path".to_string()),
        };
    };

    let library = match DynamicLibrary::open(&path) {
        Ok(library) => library,
        Err(error) => {
            return LibmpvClientSmokeReport {
                available: false,
                path: Some(path.display().to_string()),
                steps: Vec::new(),
                error: Some(error),
            }
        }
    };
    let api = match unsafe { LibmpvApi::load(library) } {
        Ok(api) => api,
        Err(error) => {
            return LibmpvClientSmokeReport {
                available: false,
                path: Some(path.display().to_string()),
                steps: Vec::new(),
                error: Some(error),
            }
        }
    };

    let mut report = LibmpvClientSmokeReport {
        available: true,
        path: Some(path.display().to_string()),
        steps: Vec::new(),
        error: None,
    };

    let handle = unsafe { (api.mpv_create)() };
    if handle.is_null() {
        report.steps.push(smoke_step(
            "mpv_create",
            false,
            None,
            Some("returned null".into()),
        ));
        report.error = Some("mpv_create returned null".to_string());
        return report;
    }
    report
        .steps
        .push(smoke_step("mpv_create", true, None, None));

    for (name, value) in [
        ("config", "no"),
        ("idle", "yes"),
        ("terminal", "no"),
        ("osd-level", "0"),
        ("input-media-keys", "no"),
    ] {
        if !record_mpv_code_step(&api, handle, &mut report, "mpv_set_option_string", || {
            let name = CString::new(name).expect("static option name should not contain nul");
            let value = CString::new(value).expect("static option value should not contain nul");
            unsafe { (api.mpv_set_option_string)(handle, name.as_ptr(), value.as_ptr()) }
        }) {
            unsafe { (api.mpv_terminate_destroy)(handle) };
            return report;
        }
    }

    if !record_mpv_code_step(
        &api,
        handle,
        &mut report,
        "mpv_request_log_messages",
        || {
            let level = CString::new("warn").expect("static log level should not contain nul");
            unsafe { (api.mpv_request_log_messages)(handle, level.as_ptr()) }
        },
    ) {
        unsafe { (api.mpv_terminate_destroy)(handle) };
        return report;
    }

    for property in IINA_OBSERVED_PROPERTIES {
        if !record_mpv_code_step(&api, handle, &mut report, "mpv_observe_property", || {
            let name = CString::new(property.name).expect("static property should not contain nul");
            unsafe {
                (api.mpv_observe_property)(
                    handle,
                    0,
                    name.as_ptr(),
                    mpv_format_code(property.format),
                )
            }
        }) {
            unsafe { (api.mpv_terminate_destroy)(handle) };
            return report;
        }
    }

    if !record_mpv_code_step(&api, handle, &mut report, "mpv_initialize", || unsafe {
        (api.mpv_initialize)(handle)
    }) {
        unsafe { (api.mpv_terminate_destroy)(handle) };
        return report;
    }

    unsafe { (api.mpv_terminate_destroy)(handle) };
    report
        .steps
        .push(smoke_step("mpv_terminate_destroy", true, None, None));
    report
}

fn record_mpv_code_step(
    api: &LibmpvApi,
    _handle: *mut c_void,
    report: &mut LibmpvClientSmokeReport,
    name: &str,
    operation: impl FnOnce() -> c_int,
) -> bool {
    let code = operation();
    if code < 0 {
        let message = api.error_message(code);
        report
            .steps
            .push(smoke_step(name, false, Some(code), Some(message.clone())));
        report.error = Some(format!("{name} failed: {message}"));
        false
    } else {
        report.steps.push(smoke_step(name, true, Some(code), None));
        true
    }
}

fn smoke_step(
    name: impl Into<String>,
    ok: bool,
    code: Option<i32>,
    message: Option<String>,
) -> LibmpvClientSmokeStep {
    LibmpvClientSmokeStep {
        name: name.into(),
        ok,
        code,
        message,
    }
}

fn recent_operations(operations: &[MpvClientOperation]) -> Vec<MpvClientOperation> {
    let start = operations
        .len()
        .saturating_sub(MAX_MPV_EXECUTOR_RECENT_OPERATIONS);
    operations[start..].to_vec()
}

fn libmpv_unavailable_message(status: &LibmpvRuntimeStatus) -> String {
    if let Some(error) = status.load_error.as_deref() {
        format!("libmpv unavailable: {error}")
    } else if !status.missing_symbols.is_empty() {
        format!(
            "libmpv unavailable; missing symbols: {}",
            status.missing_symbols.join(", ")
        )
    } else {
        "libmpv unavailable".to_string()
    }
}

fn mpv_client_event_name(event_id: c_int) -> &'static str {
    match event_id {
        MPV_EVENT_NONE => "none",
        MPV_EVENT_SHUTDOWN => "shutdown",
        MPV_EVENT_LOG_MESSAGE => "log-message",
        MPV_EVENT_GET_PROPERTY_REPLY => "get-property-reply",
        MPV_EVENT_SET_PROPERTY_REPLY => "set-property-reply",
        MPV_EVENT_COMMAND_REPLY => "command-reply",
        MPV_EVENT_START_FILE => "start-file",
        MPV_EVENT_END_FILE => "end-file",
        MPV_EVENT_FILE_LOADED => "file-loaded",
        MPV_EVENT_IDLE => "idle",
        MPV_EVENT_TICK => "tick",
        MPV_EVENT_CLIENT_MESSAGE => "client-message",
        MPV_EVENT_VIDEO_RECONFIG => "video-reconfig",
        MPV_EVENT_AUDIO_RECONFIG => "audio-reconfig",
        MPV_EVENT_SEEK => "seek",
        MPV_EVENT_PLAYBACK_RESTART => "playback-restart",
        MPV_EVENT_PROPERTY_CHANGE => "property-change",
        MPV_EVENT_QUEUE_OVERFLOW => "queue-overflow",
        MPV_EVENT_HOOK => "hook",
        _ => "unknown",
    }
}

unsafe fn mpv_client_event_from_raw(event: &MpvEvent) -> MpvClientEvent {
    MpvClientEvent {
        event_id: event.event_id,
        name: mpv_client_event_name(event.event_id).to_string(),
        error: event.error,
        reply_userdata: event.reply_userdata,
        property: (event.event_id == MPV_EVENT_PROPERTY_CHANGE && !event.data.is_null())
            .then(|| unsafe {
                mpv_property_change_from_raw(&*(event.data as *const MpvEventProperty))
            })
            .flatten(),
        start_file: (event.event_id == MPV_EVENT_START_FILE && !event.data.is_null()).then(
            || unsafe { mpv_start_file_from_raw(&*(event.data as *const MpvEventStartFile)) },
        ),
        end_file: (event.event_id == MPV_EVENT_END_FILE && !event.data.is_null())
            .then(|| unsafe { mpv_end_file_from_raw(&*(event.data as *const MpvEventEndFile)) }),
        hook: (event.event_id == MPV_EVENT_HOOK && !event.data.is_null())
            .then(|| unsafe { mpv_hook_event_from_raw(&*(event.data as *const MpvEventHook)) }),
    }
}

unsafe fn mpv_hook_event_from_raw(event: &MpvEventHook) -> MpvHookEvent {
    MpvHookEvent {
        name: if event.name.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(event.name) }
                .to_string_lossy()
                .into_owned()
        },
        id: event.id,
    }
}

unsafe fn mpv_start_file_from_raw(event: &MpvEventStartFile) -> MpvStartFileEvent {
    MpvStartFileEvent {
        playlist_entry_id: event.playlist_entry_id,
    }
}

unsafe fn mpv_end_file_from_raw(event: &MpvEventEndFile) -> MpvEndFileEvent {
    MpvEndFileEvent {
        reason: mpv_end_file_reason_from_code(event.reason),
        reason_code: event.reason,
        error: event.error,
        error_message: None,
        playlist_entry_id: event.playlist_entry_id,
        playlist_insert_id: event.playlist_insert_id,
        playlist_insert_num_entries: event.playlist_insert_num_entries,
    }
}

fn mpv_end_file_reason_from_code(reason: c_int) -> MpvEndFileReason {
    match reason {
        0 => MpvEndFileReason::Eof,
        2 => MpvEndFileReason::Stop,
        3 => MpvEndFileReason::Quit,
        4 => MpvEndFileReason::Error,
        5 => MpvEndFileReason::Redirect,
        _ => MpvEndFileReason::Unknown,
    }
}

unsafe fn mpv_property_change_from_raw(property: &MpvEventProperty) -> Option<MpvPropertyChange> {
    if property.name.is_null() {
        return None;
    }
    let format = mpv_format_from_code(property.format)?;
    Some(MpvPropertyChange {
        name: unsafe { CStr::from_ptr(property.name) }
            .to_string_lossy()
            .into_owned(),
        format,
        value: unsafe { mpv_property_value_from_raw(format, property.data) },
    })
}

fn mpv_format_from_code(format: c_int) -> Option<MpvFormat> {
    match format {
        0 => Some(MpvFormat::None),
        1 => Some(MpvFormat::String),
        3 => Some(MpvFormat::Flag),
        4 => Some(MpvFormat::Int64),
        5 => Some(MpvFormat::Double),
        _ => None,
    }
}

unsafe fn mpv_property_value_from_raw(format: MpvFormat, data: *mut c_void) -> Option<String> {
    if data.is_null() {
        return None;
    }
    match format {
        MpvFormat::None => None,
        MpvFormat::Flag => {
            let value = unsafe { *(data as *const c_int) };
            Some(if value == 0 { "false" } else { "true" }.to_string())
        }
        MpvFormat::Int64 => Some(unsafe { *(data as *const i64) }.to_string()),
        MpvFormat::Double => Some(format_mpv_float(unsafe { *(data as *const f64) })),
        MpvFormat::String => {
            let value = unsafe { *(data as *const *const c_char) };
            if value.is_null() {
                None
            } else {
                Some(
                    unsafe { CStr::from_ptr(value) }
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
    }
}

fn format_mpv_float(value: f64) -> String {
    let mut formatted = format!("{value:.6}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

fn mpv_command_argv(command: &str, args: &[String]) -> Vec<String> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(command.to_string());
    argv.extend(args.iter().cloned());
    argv
}

fn parse_mpv_flag(value: &str) -> Result<bool, String> {
    match value {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        _ => Err(format!("invalid mpv flag value '{value}'")),
    }
}

fn parse_mpv_bool_value(value: &str) -> Option<bool> {
    match value {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    }
}

fn cstring(value: &str, label: &str) -> Result<CString, String> {
    CString::new(value).map_err(|_| format!("{label} contains an interior nul byte"))
}

unsafe extern "C" fn mpv_wakeup_callback(data: *mut c_void) {
    let Some(signal) = (data as *const MpvWakeupSignal).as_ref() else {
        return;
    };
    signal.notify();
}

fn mpv_format_code(format: MpvFormat) -> c_int {
    match format {
        MpvFormat::None => 0,
        MpvFormat::String => 1,
        MpvFormat::Flag => 3,
        MpvFormat::Int64 => 4,
        MpvFormat::Double => 5,
    }
}

fn libmpv_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    #[cfg(test)]
    if let Some(path) = std::env::var_os("IIMA_LIBMPV_MEDIA_RUNTIME_TEST") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(contents_dir) = current_exe
            .parent()
            .and_then(|macos_dir| macos_dir.parent())
        {
            candidates.push(contents_dir.join("Frameworks/libmpv.2.dylib"));
            candidates.push(contents_dir.join("Frameworks/libmpv.dylib"));
        }
    }

    for path in [
        "src-tauri/Frameworks/libmpv.2.dylib",
        "src-tauri/Frameworks/libmpv.dylib",
        "/opt/homebrew/lib/libmpv.2.dylib",
        "/opt/homebrew/lib/libmpv.dylib",
        "/usr/local/lib/libmpv.2.dylib",
        "/usr/local/lib/libmpv.dylib",
    ] {
        candidates.push(PathBuf::from(path));
    }
    candidates
}

struct DynamicLibrary {
    handle: *mut c_void,
}

struct LibmpvClient {
    api: LibmpvApi,
    handle: *mut c_void,
    initialized: bool,
    wakeup_handle: MpvWakeupHandle,
    wakeup_callback_registered: bool,
}

#[repr(C)]
struct MpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct MpvEventProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct MpvEventStartFile {
    playlist_entry_id: i64,
}

#[repr(C)]
struct MpvEventEndFile {
    reason: c_int,
    error: c_int,
    playlist_entry_id: i64,
    playlist_insert_id: i64,
    playlist_insert_num_entries: c_int,
}

#[repr(C)]
struct MpvEventHook {
    name: *const c_char,
    id: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
union MpvNodeValue {
    string: *mut c_char,
    flag: c_int,
    int64: i64,
    double_value: f64,
    list: *mut MpvNodeList,
    byte_array: *mut MpvByteArray,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MpvNode {
    value: MpvNodeValue,
    format: c_int,
}

#[repr(C)]
struct MpvNodeList {
    num: c_int,
    values: *mut MpvNode,
    keys: *mut *mut c_char,
}

#[repr(C)]
struct MpvByteArray {
    data: *mut c_void,
    size: usize,
}

fn empty_mpv_node() -> MpvNode {
    MpvNode {
        value: MpvNodeValue {
            list: std::ptr::null_mut(),
        },
        format: 0,
    }
}

const MAX_PLUGIN_MPV_NODE_DEPTH: usize = 32;
const MAX_PLUGIN_MPV_NODE_ITEMS: usize = 1_000_000;

#[derive(Default)]
struct MpvNodeArena {
    strings: Vec<CString>,
    value_arrays: Vec<Box<[MpvNode]>>,
    key_arrays: Vec<Box<[*mut c_char]>>,
    lists: Vec<Box<MpvNodeList>>,
    byte_buffers: Vec<Box<[u8]>>,
    byte_arrays: Vec<Box<MpvByteArray>>,
}

impl MpvNodeArena {
    fn build(&mut self, value: &MpvPluginValue, depth: usize) -> Result<MpvNode, String> {
        if depth > MAX_PLUGIN_MPV_NODE_DEPTH {
            return Err("Plugin mpv node exceeds the nesting limit".to_string());
        }
        let node = match value {
            MpvPluginValue::Null => empty_mpv_node(),
            MpvPluginValue::Flag(value) => MpvNode {
                value: MpvNodeValue {
                    flag: c_int::from(*value),
                },
                format: MPV_FORMAT_FLAG,
            },
            MpvPluginValue::Int64(value) => MpvNode {
                value: MpvNodeValue {
                    int64: value
                        .parse::<i64>()
                        .map_err(|_| "Plugin mpv int64 node is invalid".to_string())?,
                },
                format: MPV_FORMAT_INT64,
            },
            MpvPluginValue::Double(value) => MpvNode {
                value: MpvNodeValue {
                    double_value: parse_plugin_mpv_double(value)?,
                },
                format: MPV_FORMAT_DOUBLE,
            },
            MpvPluginValue::String(value) => {
                let value = cstring(value, "Plugin mpv node string")?;
                let pointer = value.as_ptr().cast_mut();
                self.strings.push(value);
                MpvNode {
                    value: MpvNodeValue { string: pointer },
                    format: MPV_FORMAT_STRING,
                }
            }
            MpvPluginValue::Array(values) => {
                if values.len() > MAX_PLUGIN_MPV_NODE_ITEMS {
                    return Err("Plugin mpv node array is too large".to_string());
                }
                let mut nodes = values
                    .iter()
                    .map(|value| self.build(value, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_boxed_slice();
                let values_pointer = if nodes.is_empty() {
                    std::ptr::null_mut()
                } else {
                    nodes.as_mut_ptr()
                };
                let count = c_int::try_from(nodes.len())
                    .map_err(|_| "Plugin mpv node array is too large".to_string())?;
                self.value_arrays.push(nodes);
                let mut list = Box::new(MpvNodeList {
                    num: count,
                    values: values_pointer,
                    keys: std::ptr::null_mut(),
                });
                let list_pointer = (&mut *list) as *mut MpvNodeList;
                self.lists.push(list);
                MpvNode {
                    value: MpvNodeValue { list: list_pointer },
                    format: MPV_FORMAT_NODE_ARRAY,
                }
            }
            MpvPluginValue::Map(values) => {
                if values.len() > MAX_PLUGIN_MPV_NODE_ITEMS {
                    return Err("Plugin mpv node map is too large".to_string());
                }
                let mut nodes = Vec::with_capacity(values.len());
                let mut keys = Vec::with_capacity(values.len());
                for (key, value) in values {
                    let key = cstring(key, "Plugin mpv node map key")?;
                    keys.push(key.as_ptr().cast_mut());
                    self.strings.push(key);
                    nodes.push(self.build(value, depth + 1)?);
                }
                let mut nodes = nodes.into_boxed_slice();
                let mut keys = keys.into_boxed_slice();
                let values_pointer = if nodes.is_empty() {
                    std::ptr::null_mut()
                } else {
                    nodes.as_mut_ptr()
                };
                let keys_pointer = if keys.is_empty() {
                    std::ptr::null_mut()
                } else {
                    keys.as_mut_ptr()
                };
                let count = c_int::try_from(nodes.len())
                    .map_err(|_| "Plugin mpv node map is too large".to_string())?;
                self.value_arrays.push(nodes);
                self.key_arrays.push(keys);
                let mut list = Box::new(MpvNodeList {
                    num: count,
                    values: values_pointer,
                    keys: keys_pointer,
                });
                let list_pointer = (&mut *list) as *mut MpvNodeList;
                self.lists.push(list);
                MpvNode {
                    value: MpvNodeValue { list: list_pointer },
                    format: MPV_FORMAT_NODE_MAP,
                }
            }
            MpvPluginValue::ByteArray(bytes) => {
                let mut bytes = bytes.clone().into_boxed_slice();
                let data = if bytes.is_empty() {
                    std::ptr::null_mut()
                } else {
                    bytes.as_mut_ptr().cast::<c_void>()
                };
                let size = bytes.len();
                self.byte_buffers.push(bytes);
                let mut byte_array = Box::new(MpvByteArray { data, size });
                let pointer = (&mut *byte_array) as *mut MpvByteArray;
                self.byte_arrays.push(byte_array);
                MpvNode {
                    value: MpvNodeValue {
                        byte_array: pointer,
                    },
                    format: MPV_FORMAT_BYTE_ARRAY,
                }
            }
        };
        Ok(node)
    }
}

fn parse_plugin_mpv_double(value: &str) -> Result<f64, String> {
    match value {
        "NaN" | "nan" => Ok(f64::NAN),
        "Infinity" | "+Infinity" | "inf" | "+inf" => Ok(f64::INFINITY),
        "-Infinity" | "-inf" => Ok(f64::NEG_INFINITY),
        value => value
            .parse::<f64>()
            .map_err(|_| "Plugin mpv double node is invalid".to_string()),
    }
}

fn plugin_mpv_double_string(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value == f64::INFINITY {
        "Infinity".to_string()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".to_string()
    } else {
        value.to_string()
    }
}

unsafe fn decode_plugin_mpv_node(
    node: &MpvNode,
    depth: usize,
    item_count: &mut usize,
) -> Result<MpvPluginValue, String> {
    if depth > MAX_PLUGIN_MPV_NODE_DEPTH {
        return Err("libmpv node exceeds the nesting limit".to_string());
    }
    *item_count = item_count.saturating_add(1);
    if *item_count > MAX_PLUGIN_MPV_NODE_ITEMS {
        return Err("libmpv node contains too many values".to_string());
    }
    match node.format {
        MPV_FORMAT_NONE => Ok(MpvPluginValue::Null),
        MPV_FORMAT_STRING => {
            let pointer = unsafe { node.value.string };
            if pointer.is_null() {
                Ok(MpvPluginValue::Null)
            } else {
                Ok(MpvPluginValue::String(
                    unsafe { CStr::from_ptr(pointer) }
                        .to_string_lossy()
                        .into_owned(),
                ))
            }
        }
        MPV_FORMAT_FLAG => Ok(MpvPluginValue::Flag(unsafe { node.value.flag } != 0)),
        MPV_FORMAT_INT64 => Ok(MpvPluginValue::Int64(
            unsafe { node.value.int64 }.to_string(),
        )),
        MPV_FORMAT_DOUBLE => Ok(MpvPluginValue::Double(plugin_mpv_double_string(unsafe {
            node.value.double_value
        }))),
        MPV_FORMAT_NODE_ARRAY | MPV_FORMAT_NODE_MAP => {
            let list = unsafe { node.value.list.as_ref() }
                .ok_or_else(|| "libmpv returned a null node list".to_string())?;
            let count = usize::try_from(list.num)
                .map_err(|_| "libmpv returned a negative node count".to_string())?;
            if count > MAX_PLUGIN_MPV_NODE_ITEMS
                || (count > 0 && list.values.is_null())
                || (node.format == MPV_FORMAT_NODE_MAP && count > 0 && list.keys.is_null())
            {
                return Err("libmpv returned an invalid node list".to_string());
            }
            if node.format == MPV_FORMAT_NODE_ARRAY {
                let mut values = Vec::with_capacity(count);
                for index in 0..count {
                    values.push(unsafe {
                        decode_plugin_mpv_node(&*list.values.add(index), depth + 1, item_count)?
                    });
                }
                Ok(MpvPluginValue::Array(values))
            } else if count == 0 {
                // MPVNode.parse in IINA 1.3.5 deliberately maps an empty node map to nil.
                Ok(MpvPluginValue::Null)
            } else {
                let mut values = BTreeMap::new();
                for index in 0..count {
                    let key = unsafe { *list.keys.add(index) };
                    if key.is_null() {
                        continue;
                    }
                    values.insert(
                        unsafe { CStr::from_ptr(key) }
                            .to_string_lossy()
                            .into_owned(),
                        unsafe {
                            decode_plugin_mpv_node(&*list.values.add(index), depth + 1, item_count)?
                        },
                    );
                }
                Ok(MpvPluginValue::Map(values))
            }
        }
        MPV_FORMAT_BYTE_ARRAY => {
            let bytes = unsafe { node.value.byte_array.as_ref() }
                .ok_or_else(|| "libmpv returned a null byte array".to_string())?;
            if bytes.size == 0 {
                return Ok(MpvPluginValue::ByteArray(Vec::new()));
            }
            if bytes.data.is_null() || bytes.size > MAX_PLUGIN_MPV_NODE_ITEMS {
                return Err("libmpv returned an invalid byte array".to_string());
            }
            Ok(MpvPluginValue::ByteArray(
                unsafe { std::slice::from_raw_parts(bytes.data.cast::<u8>(), bytes.size) }.to_vec(),
            ))
        }
        _ => Ok(MpvPluginValue::Null),
    }
}

struct LibmpvApi {
    _library: DynamicLibrary,
    mpv_create: unsafe extern "C" fn() -> *mut c_void,
    mpv_initialize: unsafe extern "C" fn(*mut c_void) -> c_int,
    mpv_destroy: unsafe extern "C" fn(*mut c_void),
    mpv_terminate_destroy: unsafe extern "C" fn(*mut c_void),
    mpv_command: unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int,
    mpv_command_string: unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int,
    mpv_command_async: unsafe extern "C" fn(*mut c_void, u64, *const *const c_char) -> c_int,
    mpv_set_option_string: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    mpv_set_property: unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int,
    mpv_set_property_string:
        unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    mpv_get_property: unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int,
    mpv_get_property_string: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char,
    mpv_observe_property: unsafe extern "C" fn(*mut c_void, u64, *const c_char, c_int) -> c_int,
    mpv_hook_add: unsafe extern "C" fn(*mut c_void, u64, *const c_char, c_int) -> c_int,
    mpv_hook_continue: unsafe extern "C" fn(*mut c_void, u64) -> c_int,
    mpv_request_log_messages: unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int,
    mpv_wait_event: unsafe extern "C" fn(*mut c_void, f64) -> *mut MpvEvent,
    mpv_set_wakeup_callback:
        unsafe extern "C" fn(*mut c_void, Option<unsafe extern "C" fn(*mut c_void)>, *mut c_void),
    mpv_free: unsafe extern "C" fn(*mut c_void),
    mpv_free_node_contents: unsafe extern "C" fn(*mut MpvNode),
    mpv_error_string: unsafe extern "C" fn(c_int) -> *const c_char,
}

const MPV_FORMAT_NONE: c_int = 0;
const MPV_FORMAT_STRING: c_int = 1;
const MPV_FORMAT_FLAG: c_int = 3;
const MPV_FORMAT_INT64: c_int = 4;
const MPV_FORMAT_DOUBLE: c_int = 5;
const MPV_FORMAT_NODE: c_int = 6;
const MPV_FORMAT_NODE_ARRAY: c_int = 7;
const MPV_FORMAT_NODE_MAP: c_int = 8;
const MPV_FORMAT_BYTE_ARRAY: c_int = 9;

const MPV_EVENT_NONE: c_int = 0;
const MPV_EVENT_SHUTDOWN: c_int = 1;
const MPV_EVENT_LOG_MESSAGE: c_int = 2;
const MPV_EVENT_GET_PROPERTY_REPLY: c_int = 3;
const MPV_EVENT_SET_PROPERTY_REPLY: c_int = 4;
const MPV_EVENT_COMMAND_REPLY: c_int = 5;
const MPV_EVENT_START_FILE: c_int = 6;
const MPV_EVENT_END_FILE: c_int = 7;
const MPV_EVENT_FILE_LOADED: c_int = 8;
const MPV_EVENT_IDLE: c_int = 11;
const MPV_EVENT_TICK: c_int = 14;
const MPV_EVENT_CLIENT_MESSAGE: c_int = 16;
const MPV_EVENT_VIDEO_RECONFIG: c_int = 17;
const MPV_EVENT_AUDIO_RECONFIG: c_int = 18;
const MPV_EVENT_SEEK: c_int = 20;
const MPV_EVENT_PLAYBACK_RESTART: c_int = 21;
const MPV_EVENT_PROPERTY_CHANGE: c_int = 22;
const MPV_EVENT_QUEUE_OVERFLOW: c_int = 24;
const MPV_EVENT_HOOK: c_int = 25;

unsafe impl Send for DynamicLibrary {}
unsafe impl Send for LibmpvApi {}
unsafe impl Send for LibmpvClient {}

impl LibmpvClient {
    fn create(api: LibmpvApi, wakeup_handle: MpvWakeupHandle) -> Result<Self, String> {
        let handle = unsafe { (api.mpv_create)() };
        if handle.is_null() {
            return Err("mpv_create returned null".to_string());
        }
        Ok(Self {
            api,
            handle,
            initialized: false,
            wakeup_handle,
            wakeup_callback_registered: false,
        })
    }

    fn execute_operation(&mut self, operation: &MpvClientOperation) -> Result<(), String> {
        match operation {
            MpvClientOperation::CreateClient => Ok(()),
            MpvClientOperation::SetOption { name, value } => {
                let name = cstring(name, "mpv option name")?;
                let value = cstring(value, "mpv option value")?;
                self.check_code("mpv_set_option_string", unsafe {
                    (self.api.mpv_set_option_string)(self.handle, name.as_ptr(), value.as_ptr())
                })
            }
            MpvClientOperation::RequestLogMessages { level } => {
                let level = cstring(level, "mpv log level")?;
                self.check_code("mpv_request_log_messages", unsafe {
                    (self.api.mpv_request_log_messages)(self.handle, level.as_ptr())
                })
            }
            MpvClientOperation::SetWakeupCallback => {
                unsafe {
                    (self.api.mpv_set_wakeup_callback)(
                        self.handle,
                        Some(mpv_wakeup_callback),
                        self.wakeup_handle.callback_context(),
                    )
                };
                self.wakeup_callback_registered = true;
                Ok(())
            }
            MpvClientOperation::ObserveProperty { name, format } => {
                let name = cstring(name, "mpv observed property")?;
                self.check_code("mpv_observe_property", unsafe {
                    (self.api.mpv_observe_property)(
                        self.handle,
                        0,
                        name.as_ptr(),
                        mpv_format_code(*format),
                    )
                })
            }
            MpvClientOperation::Initialize => {
                if self.initialized {
                    return Ok(());
                }
                self.check_code("mpv_initialize", unsafe {
                    (self.api.mpv_initialize)(self.handle)
                })?;
                self.initialized = true;
                Ok(())
            }
            MpvClientOperation::SetProperty {
                name,
                format,
                value,
            } => self.set_property(name, *format, value),
            MpvClientOperation::SetPropertyNode { name, value } => {
                self.set_property_node(name, value)
            }
            MpvClientOperation::Command { command, args } => self.command(command, args),
            MpvClientOperation::CommandString { action } => self.command_string(action),
            MpvClientOperation::RemoveFilterAt { name, index } => {
                self.remove_filter_at(name, *index)
            }
            MpvClientOperation::CreateRenderContext { .. }
            | MpvClientOperation::SetRenderUpdateCallback => Err(
                "mpv render-context operations require the native video view and are not executable by the client command executor yet"
                    .to_string(),
            ),
        }
    }

    fn command(&self, command: &str, args: &[String]) -> Result<(), String> {
        let argv = mpv_command_argv(command, args);
        let cargs = argv
            .iter()
            .map(|arg| cstring(arg, "mpv command argument"))
            .collect::<Result<Vec<_>, _>>()?;
        let mut pointers = cargs.iter().map(|arg| arg.as_ptr()).collect::<Vec<_>>();
        pointers.push(std::ptr::null());
        self.check_code("mpv_command", unsafe {
            (self.api.mpv_command)(self.handle, pointers.as_ptr())
        })
    }

    fn command_string(&self, action: &str) -> Result<(), String> {
        let action = cstring(action, "mpv command string")?;
        self.check_code("mpv_command_string", unsafe {
            (self.api.mpv_command_string)(self.handle, action.as_ptr())
        })
    }

    fn command_async(
        &self,
        reply_userdata: u64,
        command: &str,
        args: &[String],
    ) -> Result<(), String> {
        let argv = mpv_command_argv(command, args);
        let cargs = argv
            .iter()
            .map(|arg| cstring(arg, "mpv async command argument"))
            .collect::<Result<Vec<_>, _>>()?;
        let mut pointers = cargs.iter().map(|arg| arg.as_ptr()).collect::<Vec<_>>();
        pointers.push(std::ptr::null());
        self.check_code("mpv_command_async", unsafe {
            (self.api.mpv_command_async)(self.handle, reply_userdata, pointers.as_ptr())
        })
    }

    fn add_hook(&self, name: &str, priority: i32, reply_userdata: u64) -> Result<(), String> {
        if !self.initialized {
            return Err("mpv_hook_add requires an initialized client".to_string());
        }
        let name = cstring(name, "mpv hook name")?;
        self.check_code("mpv_hook_add", unsafe {
            (self.api.mpv_hook_add)(self.handle, reply_userdata, name.as_ptr(), priority)
        })
    }

    fn continue_hook(&self, hook_id: u64) -> Result<(), String> {
        if !self.initialized {
            return Err("mpv_hook_continue requires an initialized client".to_string());
        }
        self.check_code("mpv_hook_continue", unsafe {
            (self.api.mpv_hook_continue)(self.handle, hook_id)
        })
    }

    fn wait_for_command_reply(
        &mut self,
        reply_userdata: u64,
        timeout: Duration,
    ) -> (Vec<MpvClientEvent>, Result<(), String>) {
        let deadline = Instant::now() + timeout;
        let mut events = Vec::new();

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return (
                    events,
                    Err("Timed out waiting for mpv command reply".to_string()),
                );
            }
            let wait = remaining.min(Duration::from_millis(100));
            let event = unsafe { (self.api.mpv_wait_event)(self.handle, wait.as_secs_f64()) };
            if event.is_null() {
                return (
                    events,
                    Err("mpv_wait_event returned null while waiting for command reply".to_string()),
                );
            }
            let event = unsafe { &*event };
            if event.event_id == MPV_EVENT_NONE {
                continue;
            }
            let decoded = self.decode_event(event);
            let is_reply = decoded.event_id == MPV_EVENT_COMMAND_REPLY
                && decoded.reply_userdata == reply_userdata;
            let reply_error = decoded.error;
            events.push(decoded);
            if is_reply {
                return if reply_error < 0 {
                    (
                        events,
                        Err(format!(
                            "mpv screenshot command failed: {}",
                            self.api.error_message(reply_error)
                        )),
                    )
                } else {
                    (events, Ok(()))
                };
            }
        }
    }

    fn wait_for_event(
        &mut self,
        expected_event_id: c_int,
        timeout: Duration,
        context: &str,
    ) -> Result<Vec<MpvClientEvent>, String> {
        let deadline = Instant::now() + timeout;
        let mut events = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(format!(
                    "Timed out while {context}; expected mpv event {}",
                    mpv_client_event_name(expected_event_id)
                ));
            }
            let wait = remaining.min(Duration::from_millis(100));
            let event = unsafe { (self.api.mpv_wait_event)(self.handle, wait.as_secs_f64()) };
            if event.is_null() {
                return Err(format!("mpv_wait_event returned null while {context}"));
            }
            let event = unsafe { &*event };
            if event.event_id == MPV_EVENT_NONE {
                continue;
            }
            let decoded = self.decode_event(event);
            let found = decoded.event_id == expected_event_id;
            if decoded.event_id == MPV_EVENT_END_FILE && !found {
                let detail = decoded
                    .end_file
                    .as_ref()
                    .map(|end| {
                        end.error_message.clone().unwrap_or_else(|| {
                            format!(
                                "media ended before {} ({:?})",
                                mpv_client_event_name(expected_event_id),
                                end.reason
                            )
                        })
                    })
                    .unwrap_or_else(|| "media ended unexpectedly".to_string());
                events.push(decoded);
                return Err(format!("Unable to finish {context}: {detail}"));
            }
            if decoded.event_id == MPV_EVENT_SHUTDOWN && !found {
                events.push(decoded);
                return Err(format!("libmpv shut down while {context}"));
            }
            if decoded.error < 0 && !found {
                let error = self.api.error_message(decoded.error);
                events.push(decoded);
                return Err(format!("libmpv failed while {context}: {error}"));
            }
            events.push(decoded);
            if found {
                return Ok(events);
            }
        }
    }

    fn set_property(&self, name: &str, format: MpvFormat, value: &str) -> Result<(), String> {
        let name = cstring(name, "mpv property name")?;
        match format {
            MpvFormat::Flag => {
                let mut data = parse_mpv_flag(value)? as c_int;
                self.check_code("mpv_set_property", unsafe {
                    (self.api.mpv_set_property)(
                        self.handle,
                        name.as_ptr(),
                        mpv_format_code(format),
                        (&mut data as *mut c_int).cast::<c_void>(),
                    )
                })
            }
            MpvFormat::Int64 => {
                let mut data = value
                    .parse::<i64>()
                    .map_err(|error| format!("invalid mpv int64 value '{value}': {error}"))?;
                self.check_code("mpv_set_property", unsafe {
                    (self.api.mpv_set_property)(
                        self.handle,
                        name.as_ptr(),
                        mpv_format_code(format),
                        (&mut data as *mut i64).cast::<c_void>(),
                    )
                })
            }
            MpvFormat::Double => {
                let mut data = value
                    .parse::<f64>()
                    .map_err(|error| format!("invalid mpv double value '{value}': {error}"))?;
                self.check_code("mpv_set_property", unsafe {
                    (self.api.mpv_set_property)(
                        self.handle,
                        name.as_ptr(),
                        mpv_format_code(format),
                        (&mut data as *mut f64).cast::<c_void>(),
                    )
                })
            }
            MpvFormat::String | MpvFormat::None => {
                let value = cstring(value, "mpv property value")?;
                self.check_code("mpv_set_property_string", unsafe {
                    (self.api.mpv_set_property_string)(self.handle, name.as_ptr(), value.as_ptr())
                })
            }
        }
    }

    fn set_property_node(&self, name: &str, value: &MpvPluginValue) -> Result<(), String> {
        let name = cstring(name, "mpv property name")?;
        let mut arena = MpvNodeArena::default();
        let mut node = arena.build(value, 0)?;
        self.check_code("mpv_set_property", unsafe {
            (self.api.mpv_set_property)(
                self.handle,
                name.as_ptr(),
                MPV_FORMAT_NODE,
                (&mut node as *mut MpvNode).cast::<c_void>(),
            )
        })
    }

    fn plugin_property(
        &self,
        name: &str,
        kind: MpvPluginGetKind,
    ) -> Result<MpvPluginValue, String> {
        let name = cstring(name, "mpv property name")?;
        match kind {
            MpvPluginGetKind::Flag => {
                let mut data: c_int = 0;
                let code = unsafe {
                    (self.api.mpv_get_property)(
                        self.handle,
                        name.as_ptr(),
                        MPV_FORMAT_FLAG,
                        (&mut data as *mut c_int).cast::<c_void>(),
                    )
                };
                Ok(MpvPluginValue::Flag(code >= 0 && data != 0))
            }
            MpvPluginGetKind::Number => {
                let mut data = 0.0_f64;
                let code = unsafe {
                    (self.api.mpv_get_property)(
                        self.handle,
                        name.as_ptr(),
                        MPV_FORMAT_DOUBLE,
                        (&mut data as *mut f64).cast::<c_void>(),
                    )
                };
                Ok(MpvPluginValue::Double(if code < 0 {
                    "0".to_string()
                } else {
                    plugin_mpv_double_string(data)
                }))
            }
            MpvPluginGetKind::String => {
                let raw = unsafe { (self.api.mpv_get_property_string)(self.handle, name.as_ptr()) };
                if raw.is_null() {
                    return Ok(MpvPluginValue::Null);
                }
                let value = unsafe { CStr::from_ptr(raw) }
                    .to_string_lossy()
                    .into_owned();
                unsafe { (self.api.mpv_free)(raw.cast::<c_void>()) };
                Ok(MpvPluginValue::String(value))
            }
            MpvPluginGetKind::Native => {
                let mut node = empty_mpv_node();
                let code = unsafe {
                    (self.api.mpv_get_property)(
                        self.handle,
                        name.as_ptr(),
                        MPV_FORMAT_NODE,
                        (&mut node as *mut MpvNode).cast::<c_void>(),
                    )
                };
                if code < 0 {
                    return Ok(MpvPluginValue::Null);
                }
                let value = unsafe { decode_plugin_mpv_node(&node, 0, &mut 0) }
                    .unwrap_or(MpvPluginValue::Null);
                unsafe { (self.api.mpv_free_node_contents)(&mut node) };
                Ok(value)
            }
        }
    }

    fn remove_filter_at(&self, name: &str, index: usize) -> Result<(), String> {
        if !matches!(name, "vf" | "af") {
            return Err(format!("unsupported mpv filter property '{name}'"));
        }
        let name = cstring(name, "mpv filter property name")?;
        let mut old_node = empty_mpv_node();
        self.check_code("mpv_get_property", unsafe {
            (self.api.mpv_get_property)(
                self.handle,
                name.as_ptr(),
                MPV_FORMAT_NODE,
                (&mut old_node as *mut MpvNode).cast::<c_void>(),
            )
        })?;

        let result = (|| {
            if old_node.format != MPV_FORMAT_NODE_ARRAY {
                return Err("mpv filter property did not return a node array".to_string());
            }
            let old_list = unsafe { old_node.value.list.as_ref() }
                .ok_or_else(|| "mpv filter property returned a null node list".to_string())?;
            let count = usize::try_from(old_list.num)
                .map_err(|_| "mpv filter property returned a negative count".to_string())?;
            if index >= count {
                return Err(format!(
                    "mpv filter index {index} is outside the current {count}-item list"
                ));
            }
            if count > MAX_MPV_FILTERS || (count > 0 && old_list.values.is_null()) {
                return Err("mpv filter property returned an invalid node list".to_string());
            }

            let mut values = Vec::with_capacity(count.saturating_sub(1));
            for item_index in 0..count {
                if item_index != index {
                    values.push(unsafe { *old_list.values.add(item_index) });
                }
            }
            let mut new_list = MpvNodeList {
                num: values.len() as c_int,
                values: if values.is_empty() {
                    std::ptr::null_mut()
                } else {
                    values.as_mut_ptr()
                },
                keys: std::ptr::null_mut(),
            };
            let mut new_node = MpvNode {
                value: MpvNodeValue {
                    list: &mut new_list,
                },
                format: MPV_FORMAT_NODE_ARRAY,
            };
            self.check_code("mpv_set_property", unsafe {
                (self.api.mpv_set_property)(
                    self.handle,
                    name.as_ptr(),
                    MPV_FORMAT_NODE,
                    (&mut new_node as *mut MpvNode).cast::<c_void>(),
                )
            })
        })();

        unsafe { (self.api.mpv_free_node_contents)(&mut old_node) };
        result
    }

    fn poll_properties(&self, properties: &[MpvObservedProperty]) -> Vec<MpvPropertyChange> {
        properties
            .iter()
            .filter_map(|property| self.get_property_change(property).ok().flatten())
            .collect()
    }

    fn poll_track_list(&self) -> Vec<MpvTrackListItem> {
        let Some(count) = self.get_i64_property("track-list/count") else {
            return Vec::new();
        };
        let count = count.clamp(0, MAX_MPV_TRACK_LIST_ITEMS as i64) as usize;
        (0..count)
            .filter_map(|index| self.poll_track_list_item(index))
            .collect()
    }

    fn poll_playlist(&self) -> Vec<MpvPlaylistItem> {
        let Some(count) = self.get_i64_property("playlist-count") else {
            return Vec::new();
        };
        let count = count.clamp(0, MAX_MPV_PLAYLIST_ITEMS as i64) as usize;
        (0..count)
            .filter_map(|index| self.poll_playlist_item(index))
            .collect()
    }

    fn poll_audio_devices(&self) -> Vec<MpvAudioDevice> {
        let Ok(name) = cstring("audio-device-list", "mpv property name") else {
            return Vec::new();
        };
        let mut node = MpvNode {
            value: MpvNodeValue {
                list: std::ptr::null_mut(),
            },
            format: 0,
        };
        let code = unsafe {
            (self.api.mpv_get_property)(
                self.handle,
                name.as_ptr(),
                MPV_FORMAT_NODE,
                (&mut node as *mut MpvNode).cast::<c_void>(),
            )
        };
        if code < 0 {
            return Vec::new();
        }
        let devices = decode_audio_devices_node(&node);
        unsafe { (self.api.mpv_free_node_contents)(&mut node) };
        devices
    }

    fn poll_filters(&self, property: &str) -> Vec<MpvFilter> {
        let Ok(name) = cstring(property, "mpv filter property name") else {
            return Vec::new();
        };
        let mut node = empty_mpv_node();
        let code = unsafe {
            (self.api.mpv_get_property)(
                self.handle,
                name.as_ptr(),
                MPV_FORMAT_NODE,
                (&mut node as *mut MpvNode).cast::<c_void>(),
            )
        };
        if code < 0 {
            return Vec::new();
        }
        let filters = decode_filters_node(&node);
        unsafe { (self.api.mpv_free_node_contents)(&mut node) };
        filters
    }

    fn poll_playlist_item(&self, index: usize) -> Option<MpvPlaylistItem> {
        let prefix = format!("playlist/{index}");
        let filename = self.get_string_property(&format!("{prefix}/filename"))?;

        Some(MpvPlaylistItem {
            index,
            id: self.get_i64_property(&format!("{prefix}/id")),
            filename,
            current: self.get_bool_property(&format!("{prefix}/current")),
            playing: self.get_bool_property(&format!("{prefix}/playing")),
            title: self.get_string_property(&format!("{prefix}/title")),
        })
    }

    fn poll_track_list_item(&self, index: usize) -> Option<MpvTrackListItem> {
        let prefix = format!("track-list/{index}");
        let id = self.get_i64_property(&format!("{prefix}/id"))?;
        let track_type = self.get_string_property(&format!("{prefix}/type"))?;

        Some(MpvTrackListItem {
            index,
            id,
            track_type,
            src_id: self.get_i64_property(&format!("{prefix}/src-id")),
            title: self.get_string_property(&format!("{prefix}/title")),
            lang: self.get_string_property(&format!("{prefix}/lang")),
            image: self.get_bool_property(&format!("{prefix}/image")),
            albumart: self.get_bool_property(&format!("{prefix}/albumart")),
            default_track: self.get_bool_property(&format!("{prefix}/default")),
            forced: self.get_bool_property(&format!("{prefix}/forced")),
            codec: self.get_string_property(&format!("{prefix}/codec")),
            external: self.get_bool_property(&format!("{prefix}/external")),
            external_filename: self.get_string_property(&format!("{prefix}/external-filename")),
            selected: self.get_bool_property(&format!("{prefix}/selected")),
            main_selection: self.get_bool_property(&format!("{prefix}/main-selection")),
            ff_index: self.get_i64_property(&format!("{prefix}/ff-index")),
            decoder_desc: self.get_string_property(&format!("{prefix}/decoder-desc")),
            demux_w: self.get_i64_property(&format!("{prefix}/demux-w")),
            demux_h: self.get_i64_property(&format!("{prefix}/demux-h")),
            demux_channel_count: self.get_i64_property(&format!("{prefix}/demux-channel-count")),
            demux_channels: self.get_string_property(&format!("{prefix}/demux-channels")),
            demux_samplerate: self.get_i64_property(&format!("{prefix}/demux-samplerate")),
            demux_fps: self.get_f64_property(&format!("{prefix}/demux-fps")),
            demux_bitrate: self.get_i64_property(&format!("{prefix}/demux-bitrate")),
            demux_rotation: self.get_i64_property(&format!("{prefix}/demux-rotation")),
            demux_par: self.get_string_property(&format!("{prefix}/demux-par")),
            audio_channels: self.get_string_property(&format!("{prefix}/audio-channels")),
        })
    }

    fn get_property_change(
        &self,
        property: &MpvObservedProperty,
    ) -> Result<Option<MpvPropertyChange>, String> {
        self.get_property_change_by_name(property.name, property.format)
    }

    fn get_property_change_by_name(
        &self,
        name: &str,
        format: MpvFormat,
    ) -> Result<Option<MpvPropertyChange>, String> {
        let value = self.get_property_value(name, format)?;
        Ok(value.map(|value| MpvPropertyChange {
            name: name.to_string(),
            format,
            value: Some(value),
        }))
    }

    fn get_property_value(&self, name: &str, format: MpvFormat) -> Result<Option<String>, String> {
        let name = cstring(name, "mpv property name")?;
        let value = match format {
            MpvFormat::None => return Ok(None),
            MpvFormat::String => {
                let raw = unsafe { (self.api.mpv_get_property_string)(self.handle, name.as_ptr()) };
                if raw.is_null() {
                    return Ok(None);
                }
                let value = unsafe { CStr::from_ptr(raw) }
                    .to_string_lossy()
                    .into_owned();
                unsafe { (self.api.mpv_free)(raw.cast::<c_void>()) };
                Some(value)
            }
            MpvFormat::Flag => {
                let mut data: c_int = 0;
                let code = unsafe {
                    (self.api.mpv_get_property)(
                        self.handle,
                        name.as_ptr(),
                        mpv_format_code(format),
                        (&mut data as *mut c_int).cast::<c_void>(),
                    )
                };
                if code < 0 {
                    return Ok(None);
                }
                Some(if data == 0 { "false" } else { "true" }.to_string())
            }
            MpvFormat::Int64 => {
                let mut data: i64 = 0;
                let code = unsafe {
                    (self.api.mpv_get_property)(
                        self.handle,
                        name.as_ptr(),
                        mpv_format_code(format),
                        (&mut data as *mut i64).cast::<c_void>(),
                    )
                };
                if code < 0 {
                    return Ok(None);
                }
                Some(data.to_string())
            }
            MpvFormat::Double => {
                let mut data: f64 = 0.0;
                let code = unsafe {
                    (self.api.mpv_get_property)(
                        self.handle,
                        name.as_ptr(),
                        mpv_format_code(format),
                        (&mut data as *mut f64).cast::<c_void>(),
                    )
                };
                if code < 0 || !data.is_finite() {
                    return Ok(None);
                }
                Some(format_mpv_float(data))
            }
        };

        Ok(value)
    }

    fn get_string_property(&self, name: &str) -> Option<String> {
        self.get_property_value(name, MpvFormat::String)
            .ok()
            .flatten()
            .filter(|value| !value.is_empty())
    }

    fn get_bool_property(&self, name: &str) -> bool {
        self.get_property_value(name, MpvFormat::Flag)
            .ok()
            .flatten()
            .as_deref()
            .and_then(parse_mpv_bool_value)
            .unwrap_or(false)
    }

    fn get_i64_property(&self, name: &str) -> Option<i64> {
        self.get_property_value(name, MpvFormat::Int64)
            .ok()
            .flatten()
            .and_then(|value| value.parse::<i64>().ok())
    }

    fn get_f64_property(&self, name: &str) -> Option<f64> {
        self.get_property_value(name, MpvFormat::Double)
            .ok()
            .flatten()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite())
    }

    fn check_code(&self, operation: &str, code: c_int) -> Result<(), String> {
        if code < 0 {
            Err(format!(
                "{operation} failed: {}",
                self.api.error_message(code)
            ))
        } else {
            Ok(())
        }
    }

    fn decode_event(&self, event: &MpvEvent) -> MpvClientEvent {
        let mut decoded = unsafe { mpv_client_event_from_raw(event) };
        if let Some(end_file) = decoded.end_file.as_mut() {
            if end_file.error < 0 {
                end_file.error_message = Some(self.api.error_message(end_file.error));
            }
        }
        decoded
    }

    fn drain_events(&mut self, limit: usize) -> Vec<MpvClientEvent> {
        let mut events = Vec::new();
        for _ in 0..limit {
            let event = unsafe { (self.api.mpv_wait_event)(self.handle, 0.0) };
            if event.is_null() {
                break;
            }
            let event = unsafe { &*event };
            if event.event_id == MPV_EVENT_NONE {
                break;
            }
            events.push(self.decode_event(event));
        }
        events
    }
}

fn parsed_filter_params(name: &str, raw_params: Option<&str>) -> BTreeMap<String, String> {
    let Some(raw_params) = raw_params else {
        return BTreeMap::new();
    };
    if name == "lavfi" {
        let graph = raw_params
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .unwrap_or(raw_params);
        return BTreeMap::from([("graph".to_string(), graph.to_string())]);
    }
    if let Some(order) = filter_parameter_order(name) {
        return order
            .iter()
            .zip(raw_params.split(':'))
            .map(|(key, value)| ((*key).to_string(), unquote_filter_value(value)))
            .collect();
    }

    let mut params = BTreeMap::new();
    for pair in raw_params.split(':') {
        let Some((key, value)) = pair.split_once('=') else {
            return BTreeMap::new();
        };
        if key.is_empty() {
            return BTreeMap::new();
        }
        params.insert(key.to_string(), unquote_filter_value(value));
    }
    params
}

fn filter_parameter_order(name: &str) -> Option<&'static [&'static str]> {
    match name {
        "crop" => Some(&["w", "h", "x", "y"]),
        "expand" => Some(&["w", "h", "x", "y", "aspect", "round"]),
        _ => None,
    }
}

fn unquote_filter_value(value: &str) -> String {
    let Some(length_end) = value.strip_prefix('%').and_then(|value| value.find('%')) else {
        return value.to_string();
    };
    let length = &value[1..=length_end];
    if !length.bytes().all(|byte| byte.is_ascii_digit()) {
        return value.to_string();
    }
    value[length_end + 2..].to_string()
}

fn quote_filter_value(value: &str) -> String {
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        value.to_string()
    } else {
        format!("%{}%{value}", value.len())
    }
}

fn filter_string_format(
    name: &str,
    label: Option<&str>,
    params: &BTreeMap<String, String>,
) -> String {
    let mut filter = label
        .map(|label| format!("@{label}:{name}"))
        .unwrap_or_else(|| name.to_string());
    if params.is_empty() {
        return filter;
    }
    filter.push('=');
    if name == "lavfi" {
        if let Some(graph) = params.get("graph") {
            filter.push('[');
            filter.push_str(graph);
            filter.push(']');
        }
        return filter;
    }
    if let Some(order) = filter_parameter_order(name) {
        filter.push_str(
            &order
                .iter()
                .map(|key| params.get(*key).cloned().unwrap_or_default())
                .collect::<Vec<_>>()
                .join(":"),
        );
        return filter;
    }
    let positional = params
        .iter()
        .filter_map(|(key, value)| {
            key.strip_prefix('@')
                .and_then(|index| index.parse::<usize>().ok())
                .map(|index| (index, value))
        })
        .collect::<Vec<_>>();
    if positional.len() == params.len() {
        let mut positional = positional;
        positional.sort_by_key(|(index, _)| *index);
        filter.push_str(
            &positional
                .into_iter()
                .map(|(_, value)| quote_filter_value(value))
                .collect::<Vec<_>>()
                .join(":"),
        );
    } else {
        filter.push_str(
            &params
                .iter()
                .map(|(key, value)| format!("{key}={}", quote_filter_value(value)))
                .collect::<Vec<_>>()
                .join(":"),
        );
    }
    filter
}

fn decode_filters_node(node: &MpvNode) -> Vec<MpvFilter> {
    if node.format != MPV_FORMAT_NODE_ARRAY {
        return Vec::new();
    }
    let Some(list) = (unsafe { node.value.list.as_ref() }) else {
        return Vec::new();
    };
    let count = usize::try_from(list.num)
        .unwrap_or_default()
        .min(MAX_MPV_FILTERS);
    if count == 0 || list.values.is_null() {
        return Vec::new();
    }
    (0..count)
        .filter_map(|index| decode_filter_map(unsafe { &*list.values.add(index) }))
        .collect()
}

fn decode_filter_map(node: &MpvNode) -> Option<MpvFilter> {
    if node.format != MPV_FORMAT_NODE_MAP {
        return None;
    }
    let map = unsafe { node.value.list.as_ref() }?;
    let count = usize::try_from(map.num).ok()?.min(32);
    if count == 0 || map.values.is_null() || map.keys.is_null() {
        return None;
    }
    let mut name = None;
    let mut label = None;
    let mut params = BTreeMap::new();
    for index in 0..count {
        let key = unsafe { *map.keys.add(index) };
        if key.is_null() {
            continue;
        }
        let key = unsafe { CStr::from_ptr(key) }.to_string_lossy();
        let value = unsafe { &*map.values.add(index) };
        match key.as_ref() {
            "name" => name = mpv_node_string(value),
            "label" => label = mpv_node_string(value).filter(|value| !value.is_empty()),
            "params" => params = decode_string_map(value, MAX_MPV_FILTER_PARAMS),
            _ => {}
        }
    }
    let name = name.filter(|name| !name.is_empty())?;
    Some(MpvFilter {
        string_format: filter_string_format(&name, label.as_deref(), &params),
        name,
        label,
        params,
    })
}

fn decode_string_map(node: &MpvNode, limit: usize) -> BTreeMap<String, String> {
    if node.format != MPV_FORMAT_NODE_MAP {
        return BTreeMap::new();
    }
    let Some(map) = (unsafe { node.value.list.as_ref() }) else {
        return BTreeMap::new();
    };
    let count = usize::try_from(map.num).unwrap_or_default().min(limit);
    if count == 0 || map.values.is_null() || map.keys.is_null() {
        return BTreeMap::new();
    }
    let mut values = BTreeMap::new();
    for index in 0..count {
        let key = unsafe { *map.keys.add(index) };
        if key.is_null() {
            continue;
        }
        let key = unsafe { CStr::from_ptr(key) }
            .to_string_lossy()
            .into_owned();
        if let Some(value) = mpv_node_string(unsafe { &*map.values.add(index) }) {
            values.insert(key, value);
        }
    }
    values
}

fn decode_audio_devices_node(node: &MpvNode) -> Vec<MpvAudioDevice> {
    if node.format != MPV_FORMAT_NODE_ARRAY {
        return Vec::new();
    }
    let list = unsafe { node.value.list.as_ref() };
    let Some(list) = list else {
        return Vec::new();
    };
    let count = usize::try_from(list.num)
        .unwrap_or_default()
        .min(MAX_MPV_AUDIO_DEVICES);
    if count == 0 || list.values.is_null() {
        return Vec::new();
    }

    (0..count)
        .filter_map(|index| {
            let entry = unsafe { &*list.values.add(index) };
            decode_audio_device_map(entry)
        })
        .collect()
}

fn decode_audio_device_map(node: &MpvNode) -> Option<MpvAudioDevice> {
    if node.format != MPV_FORMAT_NODE_MAP {
        return None;
    }
    let map = unsafe { node.value.list.as_ref() }?;
    let count = usize::try_from(map.num).ok()?.min(32);
    if count == 0 || map.values.is_null() || map.keys.is_null() {
        return None;
    }

    let mut name = None;
    let mut description = None;
    for index in 0..count {
        let key = unsafe { *map.keys.add(index) };
        if key.is_null() {
            continue;
        }
        let key = unsafe { CStr::from_ptr(key) }.to_string_lossy();
        let value = unsafe { &*map.values.add(index) };
        match key.as_ref() {
            "name" => name = mpv_node_string(value),
            "description" => description = mpv_node_string(value),
            _ => {}
        }
    }

    let name = name.filter(|name| !name.is_empty())?;
    Some(MpvAudioDevice {
        description: description
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| name.clone()),
        name,
    })
}

fn mpv_node_string(node: &MpvNode) -> Option<String> {
    if node.format != MPV_FORMAT_STRING {
        return None;
    }
    let value = unsafe { node.value.string };
    (!value.is_null()).then(|| {
        unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned()
    })
}

impl Drop for LibmpvClient {
    fn drop(&mut self) {
        unsafe {
            if self.handle.is_null() {
                return;
            }
            if self.wakeup_callback_registered {
                (self.api.mpv_set_wakeup_callback)(self.handle, None, std::ptr::null_mut());
                self.wakeup_callback_registered = false;
            }
            if self.initialized {
                (self.api.mpv_terminate_destroy)(self.handle);
            } else {
                (self.api.mpv_destroy)(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}

impl LibmpvApi {
    unsafe fn load(library: DynamicLibrary) -> Result<Self, String> {
        Ok(Self {
            mpv_create: unsafe { library.symbol("mpv_create")? },
            mpv_initialize: unsafe { library.symbol("mpv_initialize")? },
            mpv_destroy: unsafe { library.symbol("mpv_destroy")? },
            mpv_terminate_destroy: unsafe { library.symbol("mpv_terminate_destroy")? },
            mpv_command: unsafe { library.symbol("mpv_command")? },
            mpv_command_string: unsafe { library.symbol("mpv_command_string")? },
            mpv_command_async: unsafe { library.symbol("mpv_command_async")? },
            mpv_set_option_string: unsafe { library.symbol("mpv_set_option_string")? },
            mpv_set_property: unsafe { library.symbol("mpv_set_property")? },
            mpv_set_property_string: unsafe { library.symbol("mpv_set_property_string")? },
            mpv_get_property: unsafe { library.symbol("mpv_get_property")? },
            mpv_get_property_string: unsafe { library.symbol("mpv_get_property_string")? },
            mpv_observe_property: unsafe { library.symbol("mpv_observe_property")? },
            mpv_hook_add: unsafe { library.symbol("mpv_hook_add")? },
            mpv_hook_continue: unsafe { library.symbol("mpv_hook_continue")? },
            mpv_request_log_messages: unsafe { library.symbol("mpv_request_log_messages")? },
            mpv_wait_event: unsafe { library.symbol("mpv_wait_event")? },
            mpv_set_wakeup_callback: unsafe { library.symbol("mpv_set_wakeup_callback")? },
            mpv_free: unsafe { library.symbol("mpv_free")? },
            mpv_free_node_contents: unsafe { library.symbol("mpv_free_node_contents")? },
            mpv_error_string: unsafe { library.symbol("mpv_error_string")? },
            _library: library,
        })
    }

    fn error_message(&self, code: c_int) -> String {
        let message = unsafe { (self.mpv_error_string)(code) };
        if message.is_null() {
            format!("mpv error {code}")
        } else {
            unsafe { CStr::from_ptr(message) }
                .to_string_lossy()
                .into_owned()
        }
    }
}

impl DynamicLibrary {
    fn open(path: &Path) -> Result<Self, String> {
        let path = CString::new(path.display().to_string())
            .map_err(|_| format!("path contains an interior nul byte: {}", path.display()))?;
        #[cfg(target_os = "macos")]
        unsafe {
            let handle = dlopen(path.as_ptr(), RTLD_NOW);
            if handle.is_null() {
                return Err(dlerror_message());
            }
            Ok(Self { handle })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = path;
            Err("dynamic libmpv loading is currently implemented for macOS only".to_string())
        }
    }

    fn has_symbol(&self, name: &str) -> bool {
        self.symbol_ptr(name).is_some()
    }

    fn symbol_ptr(&self, name: &str) -> Option<*mut c_void> {
        let Ok(name) = CString::new(name) else {
            return None;
        };
        #[cfg(target_os = "macos")]
        unsafe {
            let symbol = dlsym(self.handle, name.as_ptr());
            (!symbol.is_null()).then_some(symbol)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = name;
            None
        }
    }

    unsafe fn symbol<T: Copy>(&self, name: &str) -> Result<T, String> {
        let pointer = self
            .symbol_ptr(name)
            .ok_or_else(|| format!("missing libmpv symbol: {name}"))?;
        Ok(unsafe { std::mem::transmute_copy(&pointer) })
    }
}

impl Drop for DynamicLibrary {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        unsafe {
            if !self.handle.is_null() {
                let _ = dlclose(self.handle);
            }
        }
    }
}

#[cfg(target_os = "macos")]
const RTLD_NOW: c_int = 0x2;

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
    fn dlerror() -> *const c_char;
}

#[cfg(target_os = "macos")]
unsafe fn dlerror_message() -> String {
    let error = dlerror();
    if error.is_null() {
        "dlopen failed".to_string()
    } else {
        CStr::from_ptr(error).to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::AtomicUsize;

    fn environment_baseline(
        path: Option<&str>,
        proxy: Option<&str>,
    ) -> MpvProcessEnvironmentBaseline {
        MpvProcessEnvironmentBaseline {
            path: path.map(OsString::from),
            http_proxy: proxy.map(OsString::from),
            executable_directory: PathBuf::from("/Applications/IINA.app/Contents/MacOS"),
        }
    }

    #[test]
    fn process_environment_plan_handles_empty_environment_without_empty_path_entries() {
        let plan =
            build_mpv_process_environment_plan(&environment_baseline(None, None), "", "").unwrap();

        assert_eq!(
            plan.path,
            OsString::from("/Applications/IINA.app/Contents/MacOS")
        );
        assert_eq!(plan.http_proxy, None);
    }

    #[test]
    fn process_environment_path_order_is_reference_exact_deduplicated_and_stable() {
        let executable = "/Applications/IINA.app/Contents/MacOS";
        let baseline =
            environment_baseline(Some(&format!("/usr/bin:{executable}:/bin:/usr/bin")), None);
        let first = build_mpv_process_environment_plan(&baseline, "/opt/custom-ytdl", "").unwrap();
        assert_eq!(
            first.path,
            OsString::from(format!("/opt/custom-ytdl:{executable}:/usr/bin:/bin"))
        );

        let already_projected = MpvProcessEnvironmentBaseline {
            path: Some(first.path.clone()),
            ..baseline
        };
        let second =
            build_mpv_process_environment_plan(&already_projected, "/opt/custom-ytdl", "").unwrap();
        assert_eq!(second.path, first.path);
    }

    #[test]
    fn process_environment_proxy_matches_iina_prefix_and_restores_baseline_when_empty() {
        let baseline = environment_baseline(Some("/usr/bin"), Some("http://system:8080"));
        let configured =
            build_mpv_process_environment_plan(&baseline, "", "127.0.0.1:3128").unwrap();
        assert_eq!(
            configured.http_proxy,
            Some(OsString::from("http://127.0.0.1:3128"))
        );
        let already_prefixed =
            build_mpv_process_environment_plan(&baseline, "", "http://proxy:8080").unwrap();
        assert_eq!(
            already_prefixed.http_proxy,
            Some(OsString::from("http://http://proxy:8080"))
        );
        let empty = build_mpv_process_environment_plan(&baseline, "", "").unwrap();
        assert_eq!(empty.http_proxy, baseline.http_proxy);
    }

    #[test]
    fn process_environment_plan_rejects_nul_before_any_process_mutation() {
        let baseline = environment_baseline(Some("/usr/bin"), None);
        assert!(build_mpv_process_environment_plan(&baseline, "/opt/ytdl\0bad", "").is_err());
        assert!(build_mpv_process_environment_plan(&baseline, "", "proxy\0bad").is_err());
    }

    #[test]
    fn executor_applies_process_environment_before_create_client() {
        let source = include_str!("mpv.rs");
        let ensure_client = source
            .split("fn ensure_client(&mut self)")
            .nth(1)
            .and_then(|source| source.split("fn drain_client_events").next())
            .expect("ensure_client source");
        let apply = ensure_client
            .find("apply_mpv_process_environment_plan(environment)?")
            .expect("environment application");
        let create = ensure_client
            .find("LibmpvClient::create(api")
            .expect("libmpv create");
        assert!(apply < create);
    }

    #[test]
    fn wakeup_signal_notifies_waiters_and_coalesces_pending_callbacks() {
        let wakeup = MpvWakeupHandle::default();
        let callback_wakeup = wakeup.clone();
        let callback_thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
            unsafe { mpv_wakeup_callback(callback_wakeup.callback_context()) };
        });

        assert!(wakeup.wait_timeout(Duration::from_secs(1)));
        callback_thread.join().unwrap();
        assert_eq!(wakeup.callback_count(), 1);
        assert!(!wakeup.wait_timeout(Duration::ZERO));

        wakeup.notify();
        wakeup.notify();
        assert_eq!(wakeup.callback_count(), 3);
        assert!(wakeup.wait_timeout(Duration::ZERO));
        assert!(!wakeup.wait_timeout(Duration::ZERO));
    }

    #[test]
    fn wakeup_signal_does_not_lose_notification_during_wait_transition() {
        let wakeup = MpvWakeupHandle::default();
        let signal = Arc::clone(&wakeup.signal);
        let start_callback = Arc::new(std::sync::Barrier::new(2));
        let callback_barrier = Arc::clone(&start_callback);
        let callback_wakeup = wakeup.clone();
        let guard = signal.wait_lock.lock().unwrap();
        let first_check = AtomicBool::new(true);
        let callback_thread = std::thread::spawn(move || {
            callback_barrier.wait();
            unsafe { mpv_wakeup_callback(callback_wakeup.callback_context()) };
        });

        let (guard, timeout) = signal
            .wait_condvar
            .wait_timeout_while(guard, Duration::from_secs(1), |_| {
                if first_check.swap(false, Ordering::AcqRel) {
                    let should_wait = !signal.pending.load(Ordering::Acquire);
                    start_callback.wait();
                    while signal.callback_count.load(Ordering::Acquire) == 0 {
                        std::thread::yield_now();
                    }
                    should_wait
                } else {
                    !signal.pending.load(Ordering::Acquire)
                }
            })
            .unwrap();
        drop(guard);
        callback_thread.join().unwrap();

        assert!(!timeout.timed_out());
        assert!(signal.pending.swap(false, Ordering::AcqRel));
    }

    #[test]
    fn mirrors_iina_1_3_5_observed_property_contract() {
        assert_eq!(IINA_OBSERVED_PROPERTIES.len(), 34);
        assert_eq!(IINA_OBSERVED_PROPERTIES[0].name, "track-list");
        assert_eq!(IINA_OBSERVED_PROPERTIES[7].name, "pause");
        assert_eq!(IINA_OBSERVED_PROPERTIES[15].name, "volume");
        assert_eq!(IINA_OBSERVED_PROPERTIES[33].name, "idle-active");
        assert!(IINA_OBSERVED_PROPERTIES
            .iter()
            .any(|property| property.name == "video-params/primaries"
                && property.format == MpvFormat::String));
    }

    #[test]
    fn polled_properties_cover_authoritative_runtime_snapshot() {
        let names = IINA_POLLED_PROPERTIES
            .iter()
            .map(|property| property.name)
            .collect::<Vec<_>>();

        assert!(names.contains(&"path"));
        assert!(names.contains(&"media-title"));
        assert!(names.contains(&"duration"));
        assert!(names.contains(&"time-pos"));
        assert!(names.contains(&"percent-pos"));
        assert!(names.contains(&"pause"));
        assert!(names.contains(&"ab-loop-a"));
        assert!(names.contains(&"ab-loop-b"));
        assert!(names.contains(&"ab-loop-count"));
        assert!(names.contains(&"audio-device"));
        assert!(names.contains(&"playlist-count"));
        assert!(names.contains(&"playlist-pos"));
        assert!(names.contains(&"track-list/count"));
        assert!(names.contains(&"dwidth"));
        assert!(names.contains(&"dheight"));
        assert!(names.contains(&"sub-codepage"));
        assert!(names.contains(&"idle-active"));
        assert!(IINA_POLLED_PROPERTIES
            .iter()
            .any(|property| property.name == "duration" && property.format == MpvFormat::Double));
        assert!(IINA_POLLED_PROPERTIES
            .iter()
            .any(|property| property.name == "path" && property.format == MpvFormat::String));
        assert!(IINA_POLLED_PROPERTIES
            .iter()
            .any(|property| property.name == "dwidth" && property.format == MpvFormat::Int64));
        assert!(IINA_POLLED_PROPERTIES
            .iter()
            .any(|property| property.name == "dheight" && property.format == MpvFormat::Int64));
    }

    #[test]
    fn decodes_audio_device_node_array() {
        let name_key = CString::new("name").unwrap();
        let description_key = CString::new("description").unwrap();
        let name_value = CString::new("coreaudio/42").unwrap();
        let description_value = CString::new("Studio Display Speakers").unwrap();
        let mut map_values = [
            MpvNode {
                value: MpvNodeValue {
                    string: name_value.as_ptr().cast_mut(),
                },
                format: MPV_FORMAT_STRING,
            },
            MpvNode {
                value: MpvNodeValue {
                    string: description_value.as_ptr().cast_mut(),
                },
                format: MPV_FORMAT_STRING,
            },
        ];
        let mut map_keys = [
            name_key.as_ptr().cast_mut(),
            description_key.as_ptr().cast_mut(),
        ];
        let mut map = MpvNodeList {
            num: 2,
            values: map_values.as_mut_ptr(),
            keys: map_keys.as_mut_ptr(),
        };
        let mut array_values = [MpvNode {
            value: MpvNodeValue { list: &mut map },
            format: MPV_FORMAT_NODE_MAP,
        }];
        let mut array = MpvNodeList {
            num: 1,
            values: array_values.as_mut_ptr(),
            keys: std::ptr::null_mut(),
        };
        let root = MpvNode {
            value: MpvNodeValue { list: &mut array },
            format: MPV_FORMAT_NODE_ARRAY,
        };

        assert_eq!(
            decode_audio_devices_node(&root),
            vec![MpvAudioDevice {
                name: "coreaudio/42".to_string(),
                description: "Studio Display Speakers".to_string(),
            }]
        );
    }

    #[test]
    fn decodes_filter_nodes_and_matches_reordered_named_parameters() {
        let contrast_key = CString::new("contrast").unwrap();
        let gamma_key = CString::new("gamma").unwrap();
        let contrast_value = CString::new("1.2").unwrap();
        let gamma_value = CString::new("0.8").unwrap();
        let mut param_values = [
            MpvNode {
                value: MpvNodeValue {
                    string: gamma_value.as_ptr().cast_mut(),
                },
                format: MPV_FORMAT_STRING,
            },
            MpvNode {
                value: MpvNodeValue {
                    string: contrast_value.as_ptr().cast_mut(),
                },
                format: MPV_FORMAT_STRING,
            },
        ];
        let mut param_keys = [
            gamma_key.as_ptr().cast_mut(),
            contrast_key.as_ptr().cast_mut(),
        ];
        let mut params = MpvNodeList {
            num: 2,
            values: param_values.as_mut_ptr(),
            keys: param_keys.as_mut_ptr(),
        };

        let name_key = CString::new("name").unwrap();
        let label_key = CString::new("label").unwrap();
        let params_key = CString::new("params").unwrap();
        let name_value = CString::new("eq").unwrap();
        let label_value = CString::new("saved").unwrap();
        let mut filter_values = [
            MpvNode {
                value: MpvNodeValue {
                    string: name_value.as_ptr().cast_mut(),
                },
                format: MPV_FORMAT_STRING,
            },
            MpvNode {
                value: MpvNodeValue {
                    string: label_value.as_ptr().cast_mut(),
                },
                format: MPV_FORMAT_STRING,
            },
            MpvNode {
                value: MpvNodeValue { list: &mut params },
                format: MPV_FORMAT_NODE_MAP,
            },
        ];
        let mut filter_keys = [
            name_key.as_ptr().cast_mut(),
            label_key.as_ptr().cast_mut(),
            params_key.as_ptr().cast_mut(),
        ];
        let mut filter_map = MpvNodeList {
            num: 3,
            values: filter_values.as_mut_ptr(),
            keys: filter_keys.as_mut_ptr(),
        };
        let mut array_values = [MpvNode {
            value: MpvNodeValue {
                list: &mut filter_map,
            },
            format: MPV_FORMAT_NODE_MAP,
        }];
        let mut array = MpvNodeList {
            num: 1,
            values: array_values.as_mut_ptr(),
            keys: std::ptr::null_mut(),
        };
        let root = MpvNode {
            value: MpvNodeValue { list: &mut array },
            format: MPV_FORMAT_NODE_ARRAY,
        };

        let filters = decode_filters_node(&root);
        assert_eq!(filters.len(), 1);
        assert_eq!(
            filters[0].string_format,
            "@saved:eq=contrast=%3%1.2:gamma=%3%0.8"
        );
        assert!(filters[0].matches_raw("@saved:eq=gamma=0.8:contrast=1.2"));
        assert!(!filters[0].matches_raw("eq=gamma=0.8:contrast=1.2"));
    }

    #[test]
    fn parses_lavfi_saved_filter_identity() {
        let filter = MpvFilter::from_raw("@normalize:lavfi=[loudnorm=I=-16:LRA=11]")
            .expect("valid lavfi filter");
        assert_eq!(filter.name, "lavfi");
        assert_eq!(filter.label.as_deref(), Some("normalize"));
        assert_eq!(
            filter.params.get("graph").map(String::as_str),
            Some("loudnorm=I=-16:LRA=11")
        );
    }

    #[test]
    fn polls_real_audio_devices_when_requested() {
        let Some(path) = std::env::var_os("IIMA_LIBMPV_AUDIO_DEVICE_TEST") else {
            return;
        };
        let path = PathBuf::from(path);
        let runtime = libmpv_runtime_status_for_candidates(vec![path.clone()]);
        assert!(runtime.available, "runtime status: {runtime:?}");
        let library = DynamicLibrary::open(&path).expect("load requested libmpv");
        let api = unsafe { LibmpvApi::load(library) }.expect("resolve libmpv symbols");
        let mut client =
            LibmpvClient::create(api, MpvWakeupHandle::default()).expect("create libmpv client");
        for operation in iina_mpv_executor_client_startup_operations() {
            client
                .execute_operation(&operation)
                .unwrap_or_else(|error| panic!("execute {operation:?}: {error}"));
        }

        let devices = client.poll_audio_devices();
        assert!(!devices.is_empty(), "libmpv returned no audio devices");
        assert!(devices
            .iter()
            .all(|device| !device.name.is_empty() && !device.description.is_empty()));
    }

    #[test]
    fn receives_real_wakeup_callbacks_when_requested() {
        let Some(path) = std::env::var_os("IIMA_LIBMPV_WAKEUP_TEST") else {
            return;
        };
        let path = PathBuf::from(path);
        let library = DynamicLibrary::open(&path).expect("load requested libmpv");
        let api = unsafe { LibmpvApi::load(library) }.expect("resolve libmpv symbols");
        let wakeup = MpvWakeupHandle::default();
        let mut client = LibmpvClient::create(api, wakeup.clone()).expect("create libmpv client");
        for operation in iina_mpv_executor_client_startup_operations() {
            client
                .execute_operation(&operation)
                .unwrap_or_else(|error| panic!("execute {operation:?}: {error}"));
        }
        client.drain_events(MAX_MPV_EXECUTOR_DRAIN_EVENTS_PER_SYNC);
        while wakeup.wait_timeout(Duration::ZERO) {}
        let callback_count = wakeup.callback_count();
        let next_pause = !client.get_bool_property("pause");

        client
            .set_property("pause", MpvFormat::Flag, &next_pause.to_string())
            .expect("change observed pause property");

        assert!(wakeup.wait_timeout(Duration::from_secs(1)));
        assert!(wakeup.callback_count() > callback_count);
        assert!(client
            .drain_events(MAX_MPV_EXECUTOR_DRAIN_EVENTS_PER_SYNC)
            .iter()
            .any(|event| event
                .property
                .as_ref()
                .is_some_and(|property| property.name == "pause")));
    }

    #[test]
    fn receives_real_async_command_reply_when_requested() {
        let Some(path) = std::env::var_os("IIMA_LIBMPV_WAKEUP_TEST") else {
            return;
        };
        let path = PathBuf::from(path);
        let library = DynamicLibrary::open(&path).expect("load requested libmpv");
        let api = unsafe { LibmpvApi::load(library) }.expect("resolve libmpv symbols");
        let mut client =
            LibmpvClient::create(api, MpvWakeupHandle::default()).expect("create libmpv client");
        for operation in iina_mpv_executor_client_startup_operations() {
            client
                .execute_operation(&operation)
                .unwrap_or_else(|error| panic!("execute {operation:?}: {error}"));
        }
        client.drain_events(MAX_MPV_EXECUTOR_DRAIN_EVENTS_PER_SYNC);
        let reply_userdata = IINA_SCREENSHOT_REPLY_USERDATA + 1;

        client
            .command_async(
                reply_userdata,
                "set",
                &["pause".to_string(), "yes".to_string()],
            )
            .expect("queue async set command");
        let (events, result) =
            client.wait_for_command_reply(reply_userdata, Duration::from_secs(1));

        result.expect("receive successful async command reply");
        assert!(events.iter().any(|event| {
            event.event_id == MPV_EVENT_COMMAND_REPLY
                && event.reply_userdata == reply_userdata
                && event.error == 0
        }));
    }

    #[test]
    fn executes_real_command_string_when_requested() {
        let Some(path) = std::env::var_os("IIMA_LIBMPV_WAKEUP_TEST") else {
            return;
        };
        let path = PathBuf::from(path);
        let library = DynamicLibrary::open(&path).expect("load requested libmpv");
        let api = unsafe { LibmpvApi::load(library) }.expect("resolve libmpv symbols");
        let mut client =
            LibmpvClient::create(api, MpvWakeupHandle::default()).expect("create libmpv client");
        for operation in iina_mpv_executor_client_startup_operations() {
            client
                .execute_operation(&operation)
                .unwrap_or_else(|error| panic!("execute {operation:?}: {error}"));
        }

        client
            .execute_operation(&mpv_command_string("set pause yes"))
            .expect("execute raw mpv input command");

        assert!(client.get_bool_property("pause"));
    }

    #[test]
    fn captures_real_mpv_screenshot_when_requested() {
        let Some(library_path) = std::env::var_os("IIMA_LIBMPV_SCREENSHOT_TEST") else {
            return;
        };
        let Some(media_path) = std::env::var_os("IIMA_LIBMPV_SCREENSHOT_MEDIA") else {
            return;
        };
        let library_path = PathBuf::from(library_path);
        let media_path = PathBuf::from(media_path);
        let output_directory = unique_test_directory("real-mpv-screenshot");
        fs::create_dir_all(&output_directory).expect("create screenshot output directory");
        let library = DynamicLibrary::open(&library_path).expect("load requested libmpv");
        let api = unsafe { LibmpvApi::load(library) }.expect("resolve libmpv symbols");
        let mut client =
            LibmpvClient::create(api, MpvWakeupHandle::default()).expect("create libmpv client");
        for operation in [
            set_option("idle", "yes"),
            set_option("vo", "null"),
            set_option("ao", "null"),
            MpvClientOperation::Initialize,
        ] {
            client
                .execute_operation(&operation)
                .unwrap_or_else(|error| panic!("execute {operation:?}: {error}"));
        }
        client
            .command(
                "loadfile",
                &[media_path.display().to_string(), "replace".to_string()],
            )
            .expect("load screenshot fixture");
        wait_for_client_event(
            &mut client,
            MPV_EVENT_PLAYBACK_RESTART,
            Duration::from_secs(3),
        );
        client
            .set_property("pause", MpvFormat::Flag, "true")
            .expect("pause screenshot fixture after the first frame");
        client
            .set_property(
                "screenshot-directory",
                MpvFormat::String,
                output_directory.to_str().unwrap(),
            )
            .expect("set screenshot directory");
        client
            .set_property("screenshot-format", MpvFormat::String, "png")
            .expect("set screenshot format");
        client
            .set_property("screenshot-template", MpvFormat::String, "iima-real-%n")
            .expect("set screenshot template");
        let reply_userdata = IINA_SCREENSHOT_REPLY_USERDATA + 2;
        client
            .command_async(reply_userdata, "screenshot", &["video".to_string()])
            .expect("queue screenshot command");
        let (_, result) = client.wait_for_command_reply(reply_userdata, Duration::from_secs(3));
        result.expect("receive successful screenshot reply");

        let screenshots = fs::read_dir(&output_directory)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("png"))
            .collect::<Vec<_>>();
        assert_eq!(screenshots.len(), 1, "screenshots: {screenshots:?}");
        assert!(fs::metadata(&screenshots[0]).unwrap().len() > 0);
        fs::remove_dir_all(output_directory).unwrap();
    }

    #[test]
    fn writes_real_mpv_watch_later_progress_when_requested() {
        let Some(library_path) = std::env::var_os("IIMA_LIBMPV_WATCH_LATER_TEST") else {
            return;
        };
        let Some(media_path) = std::env::var_os("IIMA_LIBMPV_WATCH_LATER_MEDIA") else {
            return;
        };
        let library_path = PathBuf::from(library_path);
        let media_path = PathBuf::from(media_path);
        let watch_later_directory = unique_test_directory("real-mpv-watch-later");
        fs::create_dir_all(&watch_later_directory).expect("create watch later directory");
        let library = DynamicLibrary::open(&library_path).expect("load requested libmpv");
        let api = unsafe { LibmpvApi::load(library) }.expect("resolve libmpv symbols");
        let mut client =
            LibmpvClient::create(api, MpvWakeupHandle::default()).expect("create libmpv client");
        let configuration = MpvStartupConfiguration {
            watch_later_directory: Some(watch_later_directory.clone()),
            resume_last_position: true,
            input_config_path: None,
            preference_options: MpvStartupConfiguration::default().preference_options,
            process_environment: None,
        };
        for operation in
            iina_mpv_executor_client_startup_operations_with_configuration(&configuration)
        {
            if matches!(
                &operation,
                MpvClientOperation::SetProperty { name, .. } if name == "vo"
            ) {
                continue;
            }
            client
                .execute_operation(&operation)
                .unwrap_or_else(|error| panic!("execute {operation:?}: {error}"));
        }
        client
            .set_property("vo", MpvFormat::String, "null")
            .expect("disable video output for watch later fixture");
        client
            .set_property("ao", MpvFormat::String, "null")
            .expect("disable audio output for watch later fixture");
        client
            .command(
                "loadfile",
                &[media_path.display().to_string(), "replace".to_string()],
            )
            .expect("load watch later fixture");
        wait_for_client_event(
            &mut client,
            MPV_EVENT_PLAYBACK_RESTART,
            Duration::from_secs(3),
        );
        client
            .command("seek", &["0.5".to_string(), "absolute+exact".to_string()])
            .expect("seek watch later fixture");
        wait_for_client_event(
            &mut client,
            MPV_EVENT_PLAYBACK_RESTART,
            Duration::from_secs(3),
        );
        client
            .set_property("pause", MpvFormat::Flag, "true")
            .expect("pause watch later fixture");
        client
            .command("write-watch-later-config", &[])
            .expect("write watch later configuration");

        let resume_path = watch_later_directory.join(crate::history::mpv_watch_later_md5(
            &media_path.display().to_string(),
        ));
        let resume = fs::read_to_string(&resume_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", resume_path.display()));
        let progress = resume
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("start="))
            .and_then(|value| value.parse::<f64>().ok())
            .expect("watch later file begins with a numeric start option");
        assert!(progress >= 0.4, "watch later progress: {progress}");
        fs::remove_dir_all(watch_later_directory).unwrap();
    }

    #[test]
    fn toggles_real_filter_nodes_by_index_when_requested() {
        let Some(path) = std::env::var_os("IIMA_LIBMPV_FILTER_TEST") else {
            return;
        };
        let path = PathBuf::from(path);
        let library = DynamicLibrary::open(&path).expect("load requested libmpv");
        let api = unsafe { LibmpvApi::load(library) }.expect("resolve libmpv symbols");
        let mut client =
            LibmpvClient::create(api, MpvWakeupHandle::default()).expect("create libmpv client");
        for operation in iina_mpv_executor_client_startup_operations() {
            client
                .execute_operation(&operation)
                .unwrap_or_else(|error| panic!("execute {operation:?}: {error}"));
        }

        client
            .command("vf", &["add".to_string(), "hflip".to_string()])
            .expect("add hflip filter");
        let filters = client.poll_filters("vf");
        let index = filters
            .iter()
            .position(|filter| filter.matches_raw("hflip"))
            .expect("hflip should be returned in vf node array");
        client
            .remove_filter_at("vf", index)
            .expect("remove hflip by node index");
        assert!(!client
            .poll_filters("vf")
            .iter()
            .any(|filter| filter.matches_raw("hflip")));
    }

    #[test]
    fn startup_plan_matches_iina_mpv_controller_order() {
        let plan = iina_mpv_playback_session_plan();

        assert_eq!(
            plan.initialization.first(),
            Some(&MpvClientOperation::CreateClient)
        );
        assert!(matches!(
            plan.initialization
                .iter()
                .find(|step| matches!(step, MpvClientOperation::SetOption { name, .. } if name == "input-conf")),
            Some(MpvClientOperation::SetOption { .. })
        ));
        let wakeup_index = plan
            .initialization
            .iter()
            .position(|step| *step == MpvClientOperation::SetWakeupCallback)
            .unwrap();
        let observe_index = plan
            .initialization
            .iter()
            .position(|step| matches!(step, MpvClientOperation::ObserveProperty { name, .. } if name == "track-list"))
            .unwrap();
        let initialize_index = plan
            .initialization
            .iter()
            .position(|step| *step == MpvClientOperation::Initialize)
            .unwrap();
        let vo_index = plan
            .initialization
            .iter()
            .position(|step| {
                matches!(
                    step,
                    MpvClientOperation::SetProperty { name, value, .. }
                        if name == "vo" && value == "libmpv"
                )
            })
            .unwrap();

        assert!(wakeup_index < observe_index);
        assert!(observe_index < initialize_index);
        assert!(initialize_index < vo_index);
        assert_eq!(
            plan.initialization
                .iter()
                .filter(|step| matches!(step, MpvClientOperation::ObserveProperty { .. }))
                .count(),
            IINA_OBSERVED_PROPERTIES.len()
        );
        assert_eq!(
            plan.rendering,
            vec![
                MpvClientOperation::CreateRenderContext {
                    api: "opengl".to_string()
                },
                MpvClientOperation::SetRenderUpdateCallback
            ]
        );
    }

    #[test]
    fn required_libmpv_symbols_cover_iina_client_and_render_calls() {
        for symbol in [
            "mpv_create",
            "mpv_client_name",
            "mpv_initialize",
            "mpv_destroy",
            "mpv_terminate_destroy",
            "mpv_command",
            "mpv_command_async",
            "mpv_get_property",
            "mpv_get_property_string",
            "mpv_set_option_string",
            "mpv_set_property_string",
            "mpv_observe_property",
            "mpv_wait_event",
            "mpv_free",
            "mpv_request_log_messages",
            "mpv_render_context_create",
            "mpv_render_context_render",
            "mpv_render_context_report_swap",
        ] {
            assert!(REQUIRED_LIBMPV_SYMBOLS.contains(&symbol));
        }
    }

    #[test]
    fn reports_missing_libmpv_when_candidates_do_not_exist() {
        let status = libmpv_runtime_status_for_candidates(vec![PathBuf::from(
            "/tmp/iima-definitely-missing-libmpv.dylib",
        )]);

        assert!(!status.available);
        assert_eq!(status.path, None);
        assert_eq!(status.symbols.len(), REQUIRED_LIBMPV_SYMBOLS.len());
        assert_eq!(status.missing_symbols.len(), REQUIRED_LIBMPV_SYMBOLS.len());
    }

    #[test]
    fn about_runtime_version_probe_fails_empty_and_uses_only_the_playback_dylib() {
        assert_eq!(
            libmpv_runtime_versions_for_candidates(vec![PathBuf::from(
                "/tmp/iima-definitely-missing-libmpv-for-version.dylib",
            )]),
            (None, None)
        );
        let source = include_str!("mpv.rs");
        assert!(source.contains("client.get_string_property(\"mpv-version\")"));
        assert!(source.contains("client.get_string_property(\"ffmpeg-version\")"));
        for (name, value) in [
            ("config", "no"),
            ("terminal", "no"),
            ("input-default-bindings", "no"),
            ("vo", "null"),
            ("ao", "null"),
        ] {
            assert!(source.contains(&format!("(\"{name}\", \"{value}\")")));
        }
    }

    #[test]
    fn headless_media_session_uses_isolated_no_io_startup_contract() {
        assert_eq!(
            HEADLESS_MEDIA_OPTIONS,
            &[
                ("config", "no"),
                ("terminal", "no"),
                ("input-default-bindings", "no"),
                ("vo", "null"),
                ("ao", "null"),
                ("pause", "yes"),
            ]
        );
        let source = include_str!("mpv.rs");
        assert!(source.contains("Self::open_with_runtime_status(libmpv_runtime_status()"));
        assert!(source.contains("client.get_string_property(\"metadata/by-key/title\")"));
        assert!(source.contains("\"absolute+exact\".to_string()"));
        assert!(source.contains("\"screenshot-to-file\""));
    }

    #[test]
    fn headless_media_session_reports_an_explicit_runtime_path_failure() {
        let missing = PathBuf::from("/tmp/iima-definitely-missing-headless-libmpv.dylib");
        let result = MpvHeadlessMediaSession::open_with_runtime_path(
            "/tmp/iima-headless-media-fixture.mp4",
            &missing,
            Duration::from_millis(10),
        );
        let error = match result {
            Ok(_) => panic!("missing explicitly selected libmpv unexpectedly opened"),
            Err(error) => error,
        };
        assert!(error.contains("libmpv dylib was not found"), "{error}");
    }

    #[test]
    fn inspects_and_captures_real_media_with_explicit_libmpv_when_requested() {
        let Some(runtime_path) = std::env::var_os("IIMA_LIBMPV_MEDIA_HELPER_TEST") else {
            return;
        };
        let Some(media_path) = std::env::var_os("IIMA_LIBMPV_MEDIA_HELPER_MEDIA") else {
            return;
        };
        let runtime_path = PathBuf::from(runtime_path);
        let media_path = PathBuf::from(media_path);
        let media = media_path.to_string_lossy().into_owned();
        let output_directory = unique_test_directory("real-mpv-media-helper");
        fs::create_dir_all(&output_directory).expect("create media-helper output directory");
        let output_path = output_directory.join("thumb.jpg");

        let mut session = MpvHeadlessMediaSession::open_with_runtime_path(
            &media,
            &runtime_path,
            Duration::from_secs(30),
        )
        .expect("open media through explicitly selected libmpv");
        let inspection = session.inspection().clone();
        assert_eq!(inspection.runtime_path, runtime_path.display().to_string());
        assert!(inspection
            .duration_seconds
            .is_some_and(|duration| duration > 0.0));
        assert!(inspection
            .tracks
            .iter()
            .any(|track| track.track_type == "video"));
        assert!(inspection.file_format.is_some());
        let time_seconds = inspection
            .duration_seconds
            .map(|duration| (duration * 0.1).min(1.0))
            .unwrap_or_default();

        session
            .capture_video_frame(
                time_seconds,
                &output_path,
                Some(160),
                Duration::from_secs(30),
            )
            .expect("capture scaled JPEG through explicitly selected libmpv");
        let jpeg = fs::read(&output_path).expect("read libmpv JPEG");
        let (width, height) = jpeg_dimensions(&jpeg).expect("decode JPEG dimensions");
        assert_eq!(width, 160);
        assert!(height > 0);
        eprintln!(
            "headless libmpv media helper: duration={:?}, format={:?}, bitrate={:?}, tracks={}, chapters={}, jpeg={}x{}",
            inspection.duration_seconds,
            inspection.file_format,
            inspection.bit_rate,
            inspection.tracks.len(),
            inspection.chapters.len(),
            width,
            height
        );
        fs::remove_dir_all(output_directory).expect("remove media-helper output directory");
    }

    #[test]
    #[ignore = "requires an explicitly selected packaged libmpv and its framework directory"]
    fn live_about_runtime_version_probe_reads_mpv_and_ffmpeg_from_selected_dylib() {
        let path = std::env::var_os("IIMA_LIBMPV_VERSION_PROBE_PATH")
            .map(PathBuf::from)
            .expect("IIMA_LIBMPV_VERSION_PROBE_PATH");
        let (mpv, ffmpeg) = libmpv_runtime_versions_for_candidates(vec![path]);
        assert!(mpv.as_deref().is_some_and(|value| !value.trim().is_empty()));
        assert!(ffmpeg
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty()));
        eprintln!("About runtime probe: mpv={mpv:?}, ffmpeg={ffmpeg:?}");
    }

    #[test]
    fn smoke_report_is_unavailable_when_libmpv_is_missing() {
        let report = smoke_libmpv_client_session_for_candidates(vec![PathBuf::from(
            "/tmp/iima-definitely-missing-libmpv-for-smoke.dylib",
        )]);

        assert!(!report.available);
        assert_eq!(report.path, None);
        assert!(report.steps.is_empty());
        assert!(report
            .error
            .as_deref()
            .is_some_and(|error| error.contains("libmpv dylib was not found")));
    }

    #[test]
    fn executor_keeps_operations_pending_when_runtime_is_unavailable() {
        let mut executor = MpvExecutor::with_runtime_status(unavailable_runtime_status());

        let status = executor.submit_player_operation_log(
            0,
            1,
            &[mpv_command("loadfile", ["/tmp/current.mp4", "replace"])],
        );

        assert_eq!(status.lifecycle, MpvExecutorLifecycle::RuntimeUnavailable);
        assert!(!status.runtime_available);
        assert!(!status.client_running);
        assert_eq!(status.accepted_operation_count, 1);
        assert_eq!(status.pending_operation_count, 1);
        assert_eq!(status.executed_operation_count, 0);
        assert_eq!(status.seen_player_operation_sequence, 1);
        assert!(status
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("libmpv unavailable")));
        assert_eq!(
            status.last_operations,
            vec![mpv_command("loadfile", ["/tmp/current.mp4", "replace"])]
        );
    }

    #[test]
    fn executor_does_not_resubmit_seen_player_operations() {
        let mut executor = MpvExecutor::with_runtime_status(unavailable_runtime_status());
        let operations = vec![mpv_command("loadfile", ["/tmp/current.mp4", "replace"])];

        let first = executor.submit_player_operation_log(0, 1, &operations);
        let second = executor.submit_player_operation_log(0, 1, &operations);

        assert_eq!(first.pending_operation_count, 1);
        assert_eq!(second.pending_operation_count, 1);
        assert_eq!(second.accepted_operation_count, 1);

        let extended = vec![
            mpv_command("loadfile", ["/tmp/current.mp4", "replace"]),
            set_property("pause", MpvFormat::Flag, "true"),
        ];
        let third = executor.submit_player_operation_log(0, 2, &extended);

        assert_eq!(third.pending_operation_count, 2);
        assert_eq!(third.accepted_operation_count, 2);
        assert_eq!(
            third.last_operations,
            vec![
                mpv_command("loadfile", ["/tmp/current.mp4", "replace"]),
                set_property("pause", MpvFormat::Flag, "true"),
            ]
        );
    }

    #[test]
    fn executor_uses_sequence_cursor_when_player_log_is_trimmed() {
        let mut executor = MpvExecutor::with_runtime_status(unavailable_runtime_status());
        let initial = vec![
            mpv_command("seek", ["1", "absolute+exact"]),
            mpv_command("seek", ["2", "absolute+exact"]),
            mpv_command("seek", ["3", "absolute+exact"]),
        ];
        executor.submit_player_operation_log(10, 13, &initial);

        let trimmed = vec![
            mpv_command("seek", ["2", "absolute+exact"]),
            mpv_command("seek", ["3", "absolute+exact"]),
            mpv_command("seek", ["4", "absolute+exact"]),
        ];
        let status = executor.submit_player_operation_log(11, 14, &trimmed);

        assert_eq!(status.accepted_operation_count, 4);
        assert_eq!(status.pending_operation_count, 4);
        assert_eq!(status.seen_player_operation_sequence, 14);
        assert_eq!(
            status.last_operations.last(),
            Some(&mpv_command("seek", ["4", "absolute+exact"]))
        );
    }

    #[test]
    fn executor_reports_client_error_when_available_runtime_path_cannot_load() {
        let mut executor = MpvExecutor::with_runtime_status(available_runtime_status_at(
            "/tmp/iima-missing-runtime-libmpv.dylib",
        ));

        let status = executor.submit_player_operation_log(
            0,
            1,
            &[mpv_command("loadfile", ["/tmp/current.mp4", "replace"])],
        );

        assert_eq!(status.lifecycle, MpvExecutorLifecycle::ClientError);
        assert!(status.runtime_available);
        assert!(!status.client_running);
        assert_eq!(status.accepted_operation_count, 1);
        assert_eq!(status.pending_operation_count, 1);
        assert_eq!(status.executed_operation_count, 0);
        assert_eq!(status.startup_operation_count, 0);
        assert!(status
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("failed to load libmpv client")));
    }

    #[test]
    fn renderer_readiness_gate_keeps_the_whole_queue_and_retries_exactly_once() {
        let attach_attempts = Arc::new(AtomicUsize::new(0));
        let native_attached = Arc::new(AtomicBool::new(false));
        let bridge = MpvRendererBridge {
            attach: {
                let attach_attempts = attach_attempts.clone();
                let native_attached = native_attached.clone();
                Arc::new(move |_handle, _path, _session| {
                    if attach_attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                        return Err("native video surface is not ready".to_string());
                    }
                    native_attached.store(true, Ordering::SeqCst);
                    Ok(())
                })
            },
            is_attached: {
                let native_attached = native_attached.clone();
                Arc::new(move |_session| Ok(native_attached.load(Ordering::SeqCst)))
            },
        };
        let mut renderer_attachment = MpvRendererAttachment::required(bridge);
        let mpv_handle = std::ptr::NonNull::<u8>::dangling()
            .as_ptr()
            .cast::<c_void>();
        let operations = vec![
            mpv_command("loadfile", ["/tmp/current.mp4", "replace"]),
            mpv_command("loadfile", ["/tmp/next.mp4", "append"]),
            mpv_command("playlist-move", ["1", "0"]),
        ];
        let mut pending_operations = operations.clone();
        let mut executed_operation_count = 0;
        let mut observed_operations = Vec::new();

        let first_request = renderer_attachment
            .begin_attach(mpv_handle, "/tmp/libmpv.dylib", "main")
            .expect("the first renderer attach request should be prepared")
            .expect("the detached renderer should produce an attach request");
        let first_readiness = first_request.perform();
        let first_readiness = renderer_attachment
            .complete_attach(first_readiness)
            .map(|_| ());
        let first_error = drain_pending_operation_queue(
            &mut pending_operations,
            &mut executed_operation_count,
            first_readiness,
            |operation| {
                observed_operations.push(operation.clone());
                Ok(())
            },
        )
        .expect_err("the first native attach attempt must keep the queue blocked");

        assert!(first_error.contains("native video surface is not ready"));
        assert_eq!(pending_operations, operations);
        assert_eq!(executed_operation_count, 0);
        assert!(observed_operations.is_empty());
        assert_eq!(
            renderer_attachment.state,
            MpvRendererAttachmentState::Detached
        );
        assert_eq!(attach_attempts.load(Ordering::SeqCst), 1);

        let second_request = renderer_attachment
            .begin_attach(mpv_handle, "/tmp/libmpv.dylib", "main")
            .expect("the retry renderer attach request should be prepared")
            .expect("the detached renderer should retry attachment");
        let second_readiness = second_request.perform();
        let second_readiness = renderer_attachment
            .complete_attach(second_readiness)
            .map(|_| ());
        drain_pending_operation_queue(
            &mut pending_operations,
            &mut executed_operation_count,
            second_readiness,
            |operation| {
                observed_operations.push(operation.clone());
                Ok(())
            },
        )
        .expect("the second native attach attempt should release the queue");

        assert!(pending_operations.is_empty());
        assert_eq!(executed_operation_count, operations.len());
        assert_eq!(observed_operations, operations);
        assert_eq!(
            renderer_attachment.state,
            MpvRendererAttachmentState::Attached
        );
        assert_eq!(attach_attempts.load(Ordering::SeqCst), 2);

        let third_readiness = renderer_attachment
            .begin_attach(mpv_handle, "/tmp/libmpv.dylib", "main")
            .expect("the attached renderer should remain ready")
            .map_or(Ok(()), |request| {
                let result = request.perform();
                renderer_attachment.complete_attach(result)
            });
        drain_pending_operation_queue(
            &mut pending_operations,
            &mut executed_operation_count,
            third_readiness,
            |operation| {
                observed_operations.push(operation.clone());
                Ok(())
            },
        )
        .expect("an empty queue must stay empty after renderer attachment");

        assert_eq!(attach_attempts.load(Ordering::SeqCst), 2);
        assert_eq!(executed_operation_count, operations.len());
        assert_eq!(observed_operations, operations);
    }

    #[test]
    fn renderer_attachment_requires_native_attached_status_before_becoming_ready() {
        let attach_attempts = Arc::new(AtomicUsize::new(0));
        let native_attached = Arc::new(AtomicBool::new(false));
        let bridge = MpvRendererBridge {
            attach: {
                let attach_attempts = attach_attempts.clone();
                Arc::new(move |_handle, _path, _session| {
                    attach_attempts.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            },
            is_attached: {
                let native_attached = native_attached.clone();
                Arc::new(move |_session| Ok(native_attached.load(Ordering::SeqCst)))
            },
        };
        let mut renderer_attachment = MpvRendererAttachment::required(bridge);
        let mpv_handle = std::ptr::NonNull::<u8>::dangling()
            .as_ptr()
            .cast::<c_void>();

        let request = renderer_attachment
            .begin_attach(mpv_handle, "/tmp/libmpv.dylib", "main")
            .expect("the renderer attach request should be prepared")
            .expect("the detached renderer should produce an attach request");
        let error = renderer_attachment
            .complete_attach(request.perform())
            .expect_err("a missing native render context must keep the renderer detached");

        assert!(error.contains("did not report an attached render context"));
        assert_eq!(
            renderer_attachment.state,
            MpvRendererAttachmentState::Detached
        );
        assert_eq!(attach_attempts.load(Ordering::SeqCst), 1);

        native_attached.store(true, Ordering::SeqCst);
        let retry = renderer_attachment
            .begin_attach(mpv_handle, "/tmp/libmpv.dylib", "main")
            .expect("the retry renderer attach request should be prepared")
            .expect("the detached renderer should retry attachment");
        renderer_attachment
            .complete_attach(retry.perform())
            .expect("the native attached status should make the retry ready");

        assert_eq!(
            renderer_attachment.state,
            MpvRendererAttachmentState::Attached
        );
        assert_eq!(attach_attempts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn renderer_attachment_callback_runs_without_holding_executor_mutex() {
        let executor = Arc::new(Mutex::new(MpvExecutor::with_runtime_status(
            unavailable_runtime_status(),
        )));
        let executor_for_attach = Arc::downgrade(&executor);
        let bridge = MpvRendererBridge {
            attach: Arc::new(move |_handle, _path, _session| {
                let executor = executor_for_attach
                    .upgrade()
                    .expect("executor should remain alive during attachment");
                assert!(
                    executor.try_lock().is_ok(),
                    "native attachment must never run under the executor mutex"
                );
                Ok(())
            }),
            is_attached: Arc::new(|_session| Ok(true)),
        };
        let request = {
            let mut executor = executor
                .lock()
                .expect("lock executor to prepare attachment");
            executor.renderer_attachment = MpvRendererAttachment::required(bridge);
            executor
                .renderer_attachment
                .begin_attach(
                    std::ptr::NonNull::<u8>::dangling()
                        .as_ptr()
                        .cast::<c_void>(),
                    "/tmp/libmpv.dylib",
                    "main",
                )
                .expect("prepare renderer attachment")
                .expect("detached renderer should produce a request")
        };

        let mut errors = Vec::new();
        perform_renderer_attachment_without_executor_lock(&executor, request, &mut errors)
            .expect("perform renderer attachment outside the executor lock");
        assert!(errors.is_empty());
        assert_eq!(
            executor
                .lock()
                .expect("inspect completed renderer attachment")
                .renderer_attachment
                .state,
            MpvRendererAttachmentState::Attached
        );
    }

    #[test]
    fn interleaved_operation_syncs_preserve_each_calls_errors() {
        let mut executor = MpvExecutor::with_runtime_status(unavailable_runtime_status());
        let operations = vec![mpv_command("loadfile", ["/tmp/current.mp4", "replace"])];

        let first = executor.begin_player_operation_log_sync(1, 2, &operations);
        assert!(first.attachment.is_none());
        let second = executor.begin_player_operation_log_sync(1, 2, &operations);
        assert!(second.attachment.is_none());

        // Complete the later call first, matching the interleaving made possible by releasing the
        // executor mutex for AppKit attachment. Its status must not consume the first call's
        // operation-log diagnostic, and the first call must still return that diagnostic later.
        let second_status = executor.finish_player_operation_log_sync(second.errors);
        let first_status = executor.finish_player_operation_log_sync(first.errors);

        assert!(second_status
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("libmpv unavailable")));
        assert!(!second_status
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("trimmed before executor sync")));
        assert!(first_status
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("trimmed before executor sync")));
        assert!(first_status
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("libmpv unavailable")));
    }

    #[test]
    fn renderer_attachment_failure_appends_to_current_sync_errors() {
        let bridge = MpvRendererBridge {
            attach: Arc::new(|_handle, _path, _session| {
                Err("native video surface is not ready".to_string())
            }),
            is_attached: Arc::new(|_session| Ok(false)),
        };
        let mut executor = MpvExecutor::with_runtime_status(unavailable_runtime_status());
        executor.renderer_attachment = MpvRendererAttachment::required(bridge);
        let request = executor
            .renderer_attachment
            .begin_attach(
                std::ptr::NonNull::<u8>::dangling()
                    .as_ptr()
                    .cast::<c_void>(),
                "/tmp/libmpv.dylib",
                "main",
            )
            .expect("prepare renderer attachment")
            .expect("detached renderer should produce a request");
        let mut errors =
            vec!["mpv executor pending queue dropped 1 oldest operation(s)".to_string()];

        executor.complete_renderer_attachment(request.perform(), &mut errors);
        let status = executor.finish_player_operation_log_sync(errors);

        assert_eq!(
            executor.renderer_attachment.state,
            MpvRendererAttachmentState::Detached
        );
        let error = status
            .last_error
            .as_deref()
            .expect("both current-sync errors should be reported");
        assert!(error.contains("pending queue dropped 1 oldest operation"));
        assert!(error.contains("native video surface is not ready"));
    }

    #[test]
    fn executor_startup_operations_match_iina_client_contract_without_render_context() {
        let startup = iina_mpv_executor_client_startup_operations();

        assert_eq!(startup.first(), Some(&MpvClientOperation::CreateClient));
        assert!(matches!(
            startup
                .iter()
                .find(|operation| matches!(operation, MpvClientOperation::SetOption { name, value } if name == "idle" && value == "yes")),
            Some(MpvClientOperation::SetOption { .. })
        ));
        assert!(startup.contains(&MpvClientOperation::SetWakeupCallback));
        assert_eq!(
            startup
                .iter()
                .filter(|operation| matches!(operation, MpvClientOperation::ObserveProperty { .. }))
                .count(),
            IINA_OBSERVED_PROPERTIES.len()
        );
        assert!(startup.contains(&MpvClientOperation::Initialize));
        assert!(startup.iter().any(|operation| matches!(
            operation,
            MpvClientOperation::SetProperty { name, value, .. } if name == "vo" && value == "libmpv"
        )));
        assert!(!startup
            .iter()
            .any(|operation| matches!(operation, MpvClientOperation::CreateRenderContext { .. })));
    }

    #[test]
    fn executor_startup_uses_the_real_watch_later_directory_and_resume_preference() {
        let configuration = MpvStartupConfiguration {
            watch_later_directory: Some(PathBuf::from("/tmp/iima watch later")),
            resume_last_position: false,
            input_config_path: Some(PathBuf::from("/tmp/iima input.conf")),
            preference_options: vec![
                MpvStartupOption::new("volume-max", "250"),
                MpvStartupOption::best_effort("save-position-on-quit", "yes"),
                MpvStartupOption::best_effort("input-conf", "/tmp/user-option.conf"),
            ],
            process_environment: None,
        };
        let startup =
            iina_mpv_executor_client_startup_operations_with_configuration(&configuration);

        assert!(startup.contains(&set_option(
            "watch-later-directory",
            "/tmp/iima watch later"
        )));
        assert!(startup.contains(&set_option("save-position-on-quit", "no")));
        assert!(startup.contains(&set_option("resume-playback", "no")));
        assert!(startup.contains(&set_option("input-conf", "/tmp/iima input.conf")));
        let initialize_index = startup
            .iter()
            .position(|operation| operation == &MpvClientOperation::Initialize)
            .unwrap();
        for option in [
            "volume-max",
            "watch-later-directory",
            "save-position-on-quit",
            "resume-playback",
            "input-conf",
        ] {
            assert!(
                startup
                    .iter()
                    .position(|operation| matches!(
                        operation,
                        MpvClientOperation::SetOption { name, .. } if name == option
                    ))
                    .unwrap()
                    < initialize_index
            );
        }
        let matching_option_indices = |name: &str| {
            startup
                .iter()
                .enumerate()
                .filter_map(|(index, operation)| match operation {
                    MpvClientOperation::SetOption {
                        name: operation_name,
                        ..
                    } if operation_name == name => Some(index),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let resume_indices = matching_option_indices("save-position-on-quit");
        assert_eq!(resume_indices.len(), 2);
        assert!(resume_indices[0] < resume_indices[1]);
        assert_eq!(
            startup[resume_indices[1]],
            set_option("save-position-on-quit", "yes")
        );
        let input_indices = matching_option_indices("input-conf");
        assert_eq!(input_indices.len(), 2);
        assert!(input_indices[0] < input_indices[1]);
        assert_eq!(
            startup[input_indices[1]],
            set_option("input-conf", "/tmp/iima input.conf")
        );
        assert!(resume_indices[1] < initialize_index);
        assert!(input_indices[1] < initialize_index);
        assert!(is_best_effort_startup_operation(
            &startup[resume_indices[1]],
            &configuration
        ));
        assert!(!is_best_effort_startup_operation(
            &startup[resume_indices[0]],
            &configuration
        ));
    }

    #[test]
    fn command_argv_matches_mpv_command_call_shape() {
        assert_eq!(
            mpv_command_argv(
                "loadfile",
                &["/tmp/current.mp4".to_string(), "replace".to_string()],
            ),
            vec![
                "loadfile".to_string(),
                "/tmp/current.mp4".to_string(),
                "replace".to_string(),
            ]
        );
        assert_eq!(parse_mpv_flag("true"), Ok(true));
        assert_eq!(parse_mpv_flag("no"), Ok(false));
        assert!(parse_mpv_flag("maybe").is_err());
    }

    #[test]
    fn event_id_names_match_mpv_client_header_values() {
        assert_eq!(mpv_client_event_name(MPV_EVENT_NONE), "none");
        assert_eq!(
            mpv_client_event_name(MPV_EVENT_PROPERTY_CHANGE),
            "property-change"
        );
        assert_eq!(
            mpv_client_event_name(MPV_EVENT_VIDEO_RECONFIG),
            "video-reconfig"
        );
        assert_eq!(
            mpv_client_event_name(MPV_EVENT_QUEUE_OVERFLOW),
            "queue-overflow"
        );
        assert_eq!(mpv_client_event_name(999), "unknown");
    }

    #[test]
    fn executor_status_hands_every_repeated_event_to_the_player_once() {
        let mut executor = MpvExecutor::with_runtime_status(LibmpvRuntimeStatus {
            available: false,
            path: None,
            load_error: None,
            missing_symbols: Vec::new(),
            symbols: Vec::new(),
        });
        let event = MpvClientEvent {
            event_id: MPV_EVENT_SEEK,
            name: "seek".to_string(),
            error: 0,
            reply_userdata: 0,
            property: None,
            start_file: None,
            end_file: None,
            hook: None,
        };
        executor.record_client_events(vec![event.clone(), event]);

        let first = executor.status();
        assert_eq!(first.new_events.len(), 2);
        assert_eq!(first.new_events[0].name, "seek");
        assert_eq!(first.new_events[1].name, "seek");
        assert_eq!(first.drained_event_count, 2);

        let second = executor.status();
        assert!(second.new_events.is_empty());
        assert_eq!(second.drained_event_count, 2);
        assert_eq!(second.last_events.len(), 2);
    }

    #[test]
    fn plugin_property_observation_names_are_bounded_to_mpv_property_syntax() {
        assert!(validate_plugin_observed_property_name("playlist-pos").is_ok());
        assert!(validate_plugin_observed_property_name("video-params/primaries").is_ok());
        assert!(validate_plugin_observed_property_name("").is_err());
        assert!(validate_plugin_observed_property_name("pause.changed").is_err());
        assert!(validate_plugin_observed_property_name("bad\0name").is_err());
    }

    #[test]
    fn decodes_mpv_property_change_values() {
        let mut flag_value: c_int = 1;
        assert_eq!(
            unsafe {
                mpv_property_value_from_raw(
                    MpvFormat::Flag,
                    (&mut flag_value as *mut c_int).cast::<c_void>(),
                )
            },
            Some("true".to_string())
        );

        let mut int_value: i64 = 42;
        assert_eq!(
            unsafe {
                mpv_property_value_from_raw(
                    MpvFormat::Int64,
                    (&mut int_value as *mut i64).cast::<c_void>(),
                )
            },
            Some("42".to_string())
        );

        let mut double_value: f64 = 1.5;
        assert_eq!(
            unsafe {
                mpv_property_value_from_raw(
                    MpvFormat::Double,
                    (&mut double_value as *mut f64).cast::<c_void>(),
                )
            },
            Some("1.5".to_string())
        );

        let string_value = CString::new("Example Title").unwrap();
        let mut string_pointer = string_value.as_ptr();
        assert_eq!(
            unsafe {
                mpv_property_value_from_raw(
                    MpvFormat::String,
                    (&mut string_pointer as *mut *const c_char).cast::<c_void>(),
                )
            },
            Some("Example Title".to_string())
        );
    }

    #[test]
    fn decodes_mpv_property_change_event() {
        let name = CString::new("pause").unwrap();
        let mut flag_value: c_int = 0;
        let mut property = MpvEventProperty {
            name: name.as_ptr(),
            format: mpv_format_code(MpvFormat::Flag),
            data: (&mut flag_value as *mut c_int).cast::<c_void>(),
        };
        let event = MpvEvent {
            event_id: MPV_EVENT_PROPERTY_CHANGE,
            error: 0,
            reply_userdata: 0,
            data: (&mut property as *mut MpvEventProperty).cast::<c_void>(),
        };

        let decoded = unsafe { mpv_client_event_from_raw(&event) };

        assert_eq!(decoded.event_id, MPV_EVENT_PROPERTY_CHANGE);
        assert_eq!(decoded.name, "property-change");
        assert_eq!(
            decoded.property,
            Some(MpvPropertyChange {
                name: "pause".to_string(),
                format: MpvFormat::Flag,
                value: Some("false".to_string()),
            })
        );
        assert_eq!(decoded.start_file, None);
        assert_eq!(decoded.end_file, None);
    }

    #[test]
    fn decodes_mpv_command_reply_userdata_and_error() {
        let event = MpvEvent {
            event_id: MPV_EVENT_COMMAND_REPLY,
            error: -12,
            reply_userdata: 42,
            data: std::ptr::null_mut(),
        };

        let decoded = unsafe { mpv_client_event_from_raw(&event) };

        assert_eq!(decoded.event_id, MPV_EVENT_COMMAND_REPLY);
        assert_eq!(decoded.name, "command-reply");
        assert_eq!(decoded.error, -12);
        assert_eq!(decoded.reply_userdata, 42);
    }

    #[test]
    fn decodes_mpv_hook_registration_and_instance_ids() {
        let name = CString::new("on_load").unwrap();
        let mut hook = MpvEventHook {
            name: name.as_ptr(),
            id: 9_001,
        };
        let event = MpvEvent {
            event_id: MPV_EVENT_HOOK,
            error: 0,
            reply_userdata: 2_000_123,
            data: (&mut hook as *mut MpvEventHook).cast::<c_void>(),
        };

        let decoded = unsafe { mpv_client_event_from_raw(&event) };

        assert_eq!(decoded.name, "hook");
        assert_eq!(decoded.reply_userdata, 2_000_123);
        assert_eq!(
            decoded.hook,
            Some(MpvHookEvent {
                name: "on_load".to_string(),
                id: 9_001,
            })
        );
    }

    #[test]
    fn real_hook_registration_event_and_continuation_when_requested() {
        let Some(path) = std::env::var_os("IIMA_LIBMPV_HOOK_TEST") else {
            return;
        };
        let library = DynamicLibrary::open(Path::new(&path)).expect("load requested libmpv");
        let api = unsafe { LibmpvApi::load(library) }.expect("resolve libmpv hook symbols");
        let mut client =
            LibmpvClient::create(api, MpvWakeupHandle::default()).expect("create libmpv client");
        for operation in iina_mpv_executor_client_startup_operations() {
            client
                .execute_operation(&operation)
                .unwrap_or_else(|error| panic!("execute {operation:?}: {error}"));
        }

        let reply_userdata = 2_000_777;
        client
            .add_hook("on_load", 0, reply_userdata)
            .expect("register on_load hook");
        client
            .command(
                "loadfile",
                &[format!(
                    "/tmp/iima-hook-test-intentionally-missing-{}.mkv",
                    std::process::id()
                )],
            )
            .expect("queue a load that triggers on_load before opening the path");

        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for on_load hook"
            );
            let event = unsafe { (client.api.mpv_wait_event)(client.handle, 0.1) };
            assert!(!event.is_null(), "mpv_wait_event returned null");
            let event = unsafe { &*event };
            if event.event_id == MPV_EVENT_NONE {
                continue;
            }
            let decoded = client.decode_event(event);
            let Some(hook) = decoded.hook else {
                continue;
            };
            assert_eq!(decoded.reply_userdata, reply_userdata);
            assert_eq!(hook.name, "on_load");
            client
                .continue_hook(hook.id)
                .expect("continue the exact hook event on its owning client");
            break;
        }
    }

    #[test]
    fn decodes_mpv_start_and_end_file_events() {
        let mut start = MpvEventStartFile {
            playlist_entry_id: 42,
        };
        let start_event = MpvEvent {
            event_id: MPV_EVENT_START_FILE,
            error: 0,
            reply_userdata: 0,
            data: (&mut start as *mut MpvEventStartFile).cast::<c_void>(),
        };

        let decoded_start = unsafe { mpv_client_event_from_raw(&start_event) };

        assert_eq!(decoded_start.name, "start-file");
        assert_eq!(
            decoded_start.start_file,
            Some(MpvStartFileEvent {
                playlist_entry_id: 42
            })
        );
        assert_eq!(decoded_start.property, None);
        assert_eq!(decoded_start.end_file, None);

        let mut end = MpvEventEndFile {
            reason: 4,
            error: -3,
            playlist_entry_id: 42,
            playlist_insert_id: 100,
            playlist_insert_num_entries: 2,
        };
        let end_event = MpvEvent {
            event_id: MPV_EVENT_END_FILE,
            error: 0,
            reply_userdata: 0,
            data: (&mut end as *mut MpvEventEndFile).cast::<c_void>(),
        };

        let decoded_end = unsafe { mpv_client_event_from_raw(&end_event) };

        assert_eq!(decoded_end.name, "end-file");
        assert_eq!(
            decoded_end.end_file,
            Some(MpvEndFileEvent {
                reason: MpvEndFileReason::Error,
                reason_code: 4,
                error: -3,
                error_message: None,
                playlist_entry_id: 42,
                playlist_insert_id: 100,
                playlist_insert_num_entries: 2,
            })
        );
        assert_eq!(decoded_end.property, None);
        assert_eq!(decoded_end.start_file, None);
        assert_eq!(mpv_end_file_reason_from_code(0), MpvEndFileReason::Eof);
        assert_eq!(mpv_end_file_reason_from_code(2), MpvEndFileReason::Stop);
        assert_eq!(mpv_end_file_reason_from_code(3), MpvEndFileReason::Quit);
        assert_eq!(mpv_end_file_reason_from_code(5), MpvEndFileReason::Redirect);
        assert_eq!(mpv_end_file_reason_from_code(99), MpvEndFileReason::Unknown);
    }

    #[test]
    fn maps_iina_observed_formats_to_mpv_client_codes() {
        assert_eq!(mpv_format_code(MpvFormat::None), 0);
        assert_eq!(mpv_format_code(MpvFormat::String), 1);
        assert_eq!(mpv_format_code(MpvFormat::Flag), 3);
        assert_eq!(mpv_format_code(MpvFormat::Int64), 4);
        assert_eq!(mpv_format_code(MpvFormat::Double), 5);
        assert_eq!(mpv_format_from_code(0), Some(MpvFormat::None));
        assert_eq!(mpv_format_from_code(1), Some(MpvFormat::String));
        assert_eq!(mpv_format_from_code(3), Some(MpvFormat::Flag));
        assert_eq!(mpv_format_from_code(4), Some(MpvFormat::Int64));
        assert_eq!(mpv_format_from_code(5), Some(MpvFormat::Double));
        assert_eq!(mpv_format_from_code(99), None);
    }

    #[test]
    fn reports_load_error_for_non_dylib_candidate() {
        let fake = std::env::temp_dir().join("iima-fake-libmpv.dylib");
        fs::write(&fake, b"not a dylib").unwrap();

        let status = libmpv_runtime_status_for_candidates(vec![fake.clone()]);
        let _ = fs::remove_file(fake);

        assert!(!status.available);
        assert!(status.load_error.is_some());
        assert_eq!(status.missing_symbols.len(), REQUIRED_LIBMPV_SYMBOLS.len());
    }

    #[test]
    fn plugin_mpv_node_round_trips_recursive_native_values() {
        let value = MpvPluginValue::Map(BTreeMap::from([
            ("enabled".to_string(), MpvPluginValue::Flag(true)),
            (
                "values".to_string(),
                MpvPluginValue::Array(vec![
                    MpvPluginValue::Int64("9223372036854775807".to_string()),
                    MpvPluginValue::Double("Infinity".to_string()),
                    MpvPluginValue::String("fixture".to_string()),
                    MpvPluginValue::ByteArray(vec![0, 127, 255]),
                ]),
            ),
        ]));
        let mut arena = MpvNodeArena::default();
        let node = arena.build(&value, 0).unwrap();
        let decoded = unsafe { decode_plugin_mpv_node(&node, 0, &mut 0) }.unwrap();

        assert_eq!(decoded, value);
    }

    #[test]
    fn plugin_mpv_empty_node_map_matches_iina_nil_contract() {
        let value = MpvPluginValue::Map(BTreeMap::new());
        let mut arena = MpvNodeArena::default();
        let node = arena.build(&value, 0).unwrap();

        assert_eq!(
            unsafe { decode_plugin_mpv_node(&node, 0, &mut 0) }.unwrap(),
            MpvPluginValue::Null
        );
    }

    #[test]
    fn plugin_mpv_unavailable_runtime_uses_reference_get_defaults() {
        let mut executor = MpvExecutor::with_runtime_status(unavailable_runtime_status());

        assert_eq!(
            executor
                .plugin_property("pause", MpvPluginGetKind::Flag)
                .unwrap(),
            MpvPluginValue::Flag(false)
        );
        assert_eq!(
            executor
                .plugin_property("time-pos", MpvPluginGetKind::Number)
                .unwrap(),
            MpvPluginValue::Double("0".to_string())
        );
        assert_eq!(
            executor
                .plugin_property("path", MpvPluginGetKind::String)
                .unwrap(),
            MpvPluginValue::Null
        );
        assert!(executor
            .plugin_property("contains\0nul", MpvPluginGetKind::Native)
            .is_err());
    }

    #[test]
    fn plugin_mpv_typed_set_uses_native_formats_and_nodes() {
        assert_eq!(
            set_plugin_property("pause", MpvPluginValue::Flag(true)),
            set_property("pause", MpvFormat::Flag, "true")
        );
        assert_eq!(
            set_plugin_property(
                "script-opts",
                MpvPluginValue::Map(BTreeMap::from([(
                    "fixture".to_string(),
                    MpvPluginValue::String("value".to_string())
                )]))
            ),
            MpvClientOperation::SetPropertyNode {
                name: "script-opts".to_string(),
                value: MpvPluginValue::Map(BTreeMap::from([(
                    "fixture".to_string(),
                    MpvPluginValue::String("value".to_string())
                )]))
            }
        );
    }

    fn unavailable_runtime_status() -> LibmpvRuntimeStatus {
        LibmpvRuntimeStatus {
            available: false,
            path: None,
            load_error: Some("libmpv dylib was not found".to_string()),
            missing_symbols: REQUIRED_LIBMPV_SYMBOLS
                .iter()
                .map(|symbol| (*symbol).to_string())
                .collect(),
            symbols: REQUIRED_LIBMPV_SYMBOLS
                .iter()
                .map(|symbol| LibmpvSymbolStatus {
                    name: (*symbol).to_string(),
                    resolved: false,
                })
                .collect(),
        }
    }

    fn available_runtime_status_at(path: &str) -> LibmpvRuntimeStatus {
        LibmpvRuntimeStatus {
            available: true,
            path: Some(path.to_string()),
            load_error: None,
            missing_symbols: Vec::new(),
            symbols: REQUIRED_LIBMPV_SYMBOLS
                .iter()
                .map(|symbol| LibmpvSymbolStatus {
                    name: (*symbol).to_string(),
                    resolved: true,
                })
                .collect(),
        }
    }

    fn wait_for_client_event(
        client: &mut LibmpvClient,
        expected_event_id: c_int,
        timeout: Duration,
    ) -> Vec<MpvClientEvent> {
        let deadline = Instant::now() + timeout;
        let mut events = Vec::new();
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let event = unsafe {
                (client.api.mpv_wait_event)(
                    client.handle,
                    remaining.min(Duration::from_millis(100)).as_secs_f64(),
                )
            };
            assert!(!event.is_null(), "mpv_wait_event returned null");
            let event = unsafe { &*event };
            if event.event_id == MPV_EVENT_NONE {
                continue;
            }
            let decoded = client.decode_event(event);
            let found = decoded.event_id == expected_event_id;
            events.push(decoded);
            if found {
                return events;
            }
        }
        panic!("timed out waiting for mpv event {expected_event_id}; events: {events:?}");
    }

    fn jpeg_dimensions(bytes: &[u8]) -> Option<(u16, u16)> {
        if bytes.len() < 4 || bytes[..2] != [0xff, 0xd8] {
            return None;
        }
        let mut offset = 2;
        while offset + 3 < bytes.len() {
            if bytes[offset] != 0xff {
                offset += 1;
                continue;
            }
            while offset < bytes.len() && bytes[offset] == 0xff {
                offset += 1;
            }
            let marker = *bytes.get(offset)?;
            offset += 1;
            if marker == 0xd9 || marker == 0xda {
                break;
            }
            if marker == 0x01 || (0xd0..=0xd8).contains(&marker) {
                continue;
            }
            let length = u16::from_be_bytes([*bytes.get(offset)?, *bytes.get(offset + 1)?]);
            let length = usize::from(length);
            if length < 2 || offset.checked_add(length)? > bytes.len() {
                return None;
            }
            let is_start_of_frame = matches!(
                marker,
                0xc0 | 0xc1
                    | 0xc2
                    | 0xc3
                    | 0xc5
                    | 0xc6
                    | 0xc7
                    | 0xc9
                    | 0xca
                    | 0xcb
                    | 0xcd
                    | 0xce
                    | 0xcf
            );
            if is_start_of_frame && length >= 7 {
                let height = u16::from_be_bytes([*bytes.get(offset + 3)?, *bytes.get(offset + 4)?]);
                let width = u16::from_be_bytes([*bytes.get(offset + 5)?, *bytes.get(offset + 6)?]);
                return Some((width, height));
            }
            offset += length;
        }
        None
    }

    fn unique_test_directory(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("iima-{label}-{}-{nonce}", std::process::id()))
    }
}
