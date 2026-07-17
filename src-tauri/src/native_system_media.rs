//! macOS system-media controls, Now Playing metadata, and display-sleep prevention.
//!
//! IINA owns all three features at application scope while routing commands through
//! `PlayerCore.lastActive`.  This module keeps the AppKit/MediaPlayer/IOKit bridge isolated and
//! projects the shared Rust player model into that application-wide contract.

use crate::player::{LoopMode, PlayerCommand, PlayerState, RelativeSeekOption};
use crate::preferences::preference_file_path;
use crate::state::AppState;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub const POWER_WARNING_EVENT: &str = "iima-system-media-power-warning";

const REMOTE_PLAY: i32 = 1;
const REMOTE_PAUSE: i32 = 2;
const REMOTE_TOGGLE_PLAY_PAUSE: i32 = 3;
const REMOTE_STOP: i32 = 4;
const REMOTE_NEXT_TRACK: i32 = 5;
const REMOTE_PREVIOUS_TRACK: i32 = 6;
const REMOTE_CHANGE_REPEAT_MODE: i32 = 7;
const REMOTE_CHANGE_PLAYBACK_RATE: i32 = 8;
const REMOTE_SKIP_FORWARD: i32 = 9;
const REMOTE_SKIP_BACKWARD: i32 = 10;
const REMOTE_CHANGE_PLAYBACK_POSITION: i32 = 11;
const REMOTE_SKIP_INTERVAL_SECONDS: f64 = 15.0;
const SUPPORTED_PLAYBACK_RATES: [f64; 4] = [0.5, 1.0, 1.5, 2.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteCommand {
    Play,
    Pause,
    TogglePlayPause,
    Stop,
    NextTrack,
    PreviousTrack,
    ChangeRepeatMode,
    ChangePlaybackRate,
    SkipForward,
    SkipBackward,
    ChangePlaybackPosition,
}

impl TryFrom<i32> for RemoteCommand {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            REMOTE_PLAY => Ok(Self::Play),
            REMOTE_PAUSE => Ok(Self::Pause),
            REMOTE_TOGGLE_PLAY_PAUSE => Ok(Self::TogglePlayPause),
            REMOTE_STOP => Ok(Self::Stop),
            REMOTE_NEXT_TRACK => Ok(Self::NextTrack),
            REMOTE_PREVIOUS_TRACK => Ok(Self::PreviousTrack),
            REMOTE_CHANGE_REPEAT_MODE => Ok(Self::ChangeRepeatMode),
            REMOTE_CHANGE_PLAYBACK_RATE => Ok(Self::ChangePlaybackRate),
            REMOTE_SKIP_FORWARD => Ok(Self::SkipForward),
            REMOTE_SKIP_BACKWARD => Ok(Self::SkipBackward),
            REMOTE_CHANGE_PLAYBACK_POSITION => Ok(Self::ChangePlaybackPosition),
            _ => Err(format!("unsupported system media command: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
enum RemoteCommandStatus {
    Success = 0,
    NoSuchContent = 1,
    CommandFailed = 2,
}

fn mapped_player_command(
    command: RemoteCommand,
    value: f64,
    loop_mode: LoopMode,
) -> Result<PlayerCommand, String> {
    match command {
        RemoteCommand::Play => Ok(PlayerCommand::Resume),
        RemoteCommand::Pause => Ok(PlayerCommand::Pause),
        RemoteCommand::TogglePlayPause => Ok(PlayerCommand::TogglePause),
        RemoteCommand::Stop => Ok(PlayerCommand::Stop),
        RemoteCommand::NextTrack => Ok(PlayerCommand::PlaylistNext),
        RemoteCommand::PreviousTrack => Ok(PlayerCommand::PlaylistPrev),
        RemoteCommand::ChangeRepeatMode => Ok(match loop_mode {
            LoopMode::Off => PlayerCommand::ToggleFileLoop,
            LoopMode::File | LoopMode::Playlist => PlayerCommand::TogglePlaylistLoop,
        }),
        RemoteCommand::ChangePlaybackRate => {
            let supported = SUPPORTED_PLAYBACK_RATES
                .into_iter()
                .find(|candidate| (candidate - value).abs() < f64::EPSILON)
                .ok_or_else(|| format!("unsupported system playback rate: {value}"))?;
            Ok(PlayerCommand::SetSpeed { speed: supported })
        }
        RemoteCommand::SkipForward => Ok(PlayerCommand::SeekRelative {
            seconds: REMOTE_SKIP_INTERVAL_SECONDS,
            option: RelativeSeekOption::Exact,
        }),
        RemoteCommand::SkipBackward => Ok(PlayerCommand::SeekRelative {
            seconds: -REMOTE_SKIP_INTERVAL_SECONDS,
            option: RelativeSeekOption::Exact,
        }),
        RemoteCommand::ChangePlaybackPosition if value.is_finite() && value >= 0.0 => {
            Ok(PlayerCommand::Seek { seconds: value })
        }
        RemoteCommand::ChangePlaybackPosition => {
            Err("system playback position must be finite and non-negative".to_string())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum NowPlayingMediaType {
    Audio,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum NowPlayingPlaybackState {
    Unknown,
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct NowPlayingProjection {
    /// Participates in Rust-side deduplication so a `lastActive` change always refreshes the
    /// native center even when two sessions currently expose identical metadata.
    #[serde(skip_serializing)]
    session_label: String,
    media_type: Option<NowPlayingMediaType>,
    title: Option<String>,
    album: Option<String>,
    artist: Option<String>,
    duration: f64,
    elapsed: f64,
    rate: f64,
    default_rate: f64,
    playback_state: NowPlayingPlaybackState,
}

fn finite_non_negative(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn current_media_is_audio(player: &PlayerState) -> bool {
    let Some(current_url) = player.current_url.as_deref() else {
        return false;
    };
    if current_url.contains("://") {
        return false;
    }
    let has_track_status = !player.tracks.video.is_empty() || !player.tracks.audio.is_empty();
    has_track_status
        && (player.tracks.video.is_empty()
            || player
                .tracks
                .video
                .iter()
                .all(|track| track.metadata.albumart))
}

fn video_now_playing_title(player: &PlayerState) -> Option<String> {
    let title = non_empty(&player.media_title)?;
    let Some(path) = player.current_url.as_deref() else {
        return Some(title);
    };
    if path.contains("://") {
        return Some(title);
    }
    let file_name = std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str());
    if file_name != Some(title.as_str()) {
        return Some(title);
    }
    std::path::Path::new(&title)
        .file_stem()
        .and_then(|name| name.to_str())
        .and_then(non_empty)
        .or(Some(title))
}

#[derive(Debug, Clone)]
struct PlayerSystemMediaSnapshot {
    has_media: bool,
    paused: bool,
    media_type: Option<NowPlayingMediaType>,
    title: Option<String>,
    album: Option<String>,
    artist: Option<String>,
    duration: f64,
    elapsed: f64,
    rate: f64,
}

impl PlayerSystemMediaSnapshot {
    fn from_player(player: &PlayerState) -> Self {
        let has_media = player.current_url.is_some();
        let media_type = has_media.then(|| {
            if current_media_is_audio(player) {
                NowPlayingMediaType::Audio
            } else {
                NowPlayingMediaType::Video
            }
        });
        let (title, album, artist) = match media_type {
            Some(NowPlayingMediaType::Audio) => (
                non_empty(&player.music_title).or_else(|| non_empty(&player.media_title)),
                non_empty(&player.music_album),
                non_empty(&player.music_artist),
            ),
            Some(NowPlayingMediaType::Video) => (video_now_playing_title(player), None, None),
            None => (None, None, None),
        };
        let duration = finite_non_negative(player.duration_seconds);
        let elapsed = finite_non_negative(player.position_seconds).min(if duration > 0.0 {
            duration
        } else {
            f64::MAX
        });
        let rate = player.speed;
        Self {
            has_media,
            paused: player.paused,
            media_type,
            title,
            album,
            artist,
            duration,
            elapsed,
            rate: if rate.is_finite() && rate > 0.0 {
                rate
            } else {
                1.0
            },
        }
    }
}

fn now_playing_projection(
    session_label: &str,
    player: Option<&PlayerSystemMediaSnapshot>,
    had_media: bool,
) -> NowPlayingProjection {
    let playback_state = match player {
        Some(player) if player.has_media && player.paused => NowPlayingPlaybackState::Paused,
        Some(player) if player.has_media => NowPlayingPlaybackState::Playing,
        _ if had_media => NowPlayingPlaybackState::Stopped,
        _ => NowPlayingPlaybackState::Unknown,
    };
    NowPlayingProjection {
        session_label: session_label.to_string(),
        media_type: player.and_then(|player| player.media_type),
        title: player.and_then(|player| player.title.clone()),
        album: player.and_then(|player| player.album.clone()),
        artist: player.and_then(|player| player.artist.clone()),
        duration: player.map(|player| player.duration).unwrap_or_default(),
        elapsed: player.map(|player| player.elapsed).unwrap_or_default(),
        rate: player.map(|player| player.rate).unwrap_or(1.0),
        default_rate: 1.0,
        playback_state,
    }
}

fn materially_different_number(left: f64, right: f64) -> bool {
    (left - right).abs() > 0.000_001
}

fn should_write_now_playing(
    previous: Option<&NowPlayingProjection>,
    next: &NowPlayingProjection,
    elapsed_since_write: Duration,
) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    if previous.session_label != next.session_label
        || previous.media_type != next.media_type
        || previous.title != next.title
        || previous.album != next.album
        || previous.artist != next.artist
        || materially_different_number(previous.duration, next.duration)
        || materially_different_number(previous.rate, next.rate)
        || materially_different_number(previous.default_rate, next.default_rate)
        || previous.playback_state != next.playback_state
    {
        return true;
    }
    let expected_elapsed = if previous.playback_state == NowPlayingPlaybackState::Playing {
        previous.elapsed + elapsed_since_write.as_secs_f64() * previous.rate
    } else {
        previous.elapsed
    };
    let tolerance = if previous.playback_state == NowPlayingPlaybackState::Playing {
        1.0
    } else {
        0.25
    };
    (next.elapsed - expected_elapsed).abs() > tolerance
}

fn should_prevent_display_sleep<'a>(
    players: impl IntoIterator<Item = &'a PlayerSystemMediaSnapshot>,
) -> bool {
    players
        .into_iter()
        .any(|player| player.has_media && !player.paused)
}

#[derive(Debug, Clone)]
struct SessionSnapshot {
    label: String,
    player: PlayerSystemMediaSnapshot,
}

fn active_session_snapshots<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
) -> Result<Vec<SessionSnapshot>, String> {
    let labels = std::iter::once("main".to_string())
        .chain(state.player_session_labels()?)
        .collect::<Vec<_>>();
    labels
        .into_iter()
        .filter(|label| {
            app.get_webview_window(label).is_some()
                || app
                    .get_webview_window(&crate::commands::mini_player_label_for_session(label))
                    .is_some()
        })
        .map(|label| {
            let session = state.player_session_for_window(&label)?;
            let player = session
                .player()
                .lock()
                .map(|player| PlayerSystemMediaSnapshot::from_player(&player))
                .map_err(|error| error.to_string())?;
            Ok(SessionSnapshot { label, player })
        })
        .collect()
}

#[derive(Default)]
struct BridgeState {
    native_generation: u64,
    shutting_down: bool,
    remote_commands_enabled: Option<bool>,
    last_now_playing: Option<NowPlayingProjection>,
    last_now_playing_write: Option<Instant>,
    power_prevention_requested: Option<bool>,
    sessions_that_had_media: HashSet<String>,
    sleep_failure_alert_shown: bool,
}

impl BridgeState {
    fn next_native_generation(&mut self) -> u64 {
        self.native_generation = self.native_generation.saturating_add(1);
        self.native_generation
    }
}

fn bridge_state() -> &'static Mutex<BridgeState> {
    static STATE: OnceLock<Mutex<BridgeState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(BridgeState::default()))
}

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

pub fn initialize(app: &AppHandle, state: &AppState) -> Result<(), String> {
    if APP_HANDLE.get().is_none() {
        APP_HANDLE
            .set(app.clone())
            .map_err(|_| "system media app handle was initialized concurrently".to_string())?;
    }
    sync(app, state)
}

pub fn sync<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> Result<(), String> {
    let snapshots = active_session_snapshots(app, state)?;
    let last_active_label = state.last_active_player_session_label()?;
    let (use_media_keys, suppress_sleep_warning) = state
        .preferences
        .lock()
        .map(|preferences| {
            (
                preferences
                    .values
                    .get("useMediaKeys")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                preferences
                    .values
                    .get("suppressCannotPreventDisplaySleep")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            )
        })
        .map_err(|error| error.to_string())?;
    let prevent_display_sleep =
        should_prevent_display_sleep(snapshots.iter().map(|snapshot| &snapshot.player));
    let active_player = snapshots
        .iter()
        .find(|snapshot| snapshot.label == last_active_label);

    let now = Instant::now();
    let (remote_change, now_playing_write, power_change) = {
        let mut bridge = bridge_state().lock().map_err(|error| error.to_string())?;
        if bridge.shutting_down {
            return Ok(());
        }
        for snapshot in &snapshots {
            if snapshot.player.has_media {
                bridge
                    .sessions_that_had_media
                    .insert(snapshot.label.clone());
            }
        }
        let had_media = bridge.sessions_that_had_media.contains(&last_active_label);
        let projection = now_playing_projection(
            &last_active_label,
            active_player.map(|snapshot| &snapshot.player),
            had_media,
        );
        let remote_change = if bridge.remote_commands_enabled != Some(use_media_keys) {
            bridge.remote_commands_enabled = Some(use_media_keys);
            if !use_media_keys {
                bridge.last_now_playing = None;
                bridge.last_now_playing_write = None;
            }
            Some((use_media_keys, bridge.next_native_generation()))
        } else {
            None
        };
        let elapsed_since_write = bridge
            .last_now_playing_write
            .map(|written| now.saturating_duration_since(written))
            .unwrap_or_default();
        let write_projection = use_media_keys
            && should_write_now_playing(
                bridge.last_now_playing.as_ref(),
                &projection,
                elapsed_since_write,
            );
        let now_playing_write = if write_projection {
            Some((projection.clone(), bridge.next_native_generation()))
        } else {
            None
        };
        if write_projection {
            bridge.last_now_playing = Some(projection);
            bridge.last_now_playing_write = Some(now);
        }
        let power_change = if bridge.power_prevention_requested != Some(prevent_display_sleep) {
            bridge.power_prevention_requested = Some(prevent_display_sleep);
            Some((prevent_display_sleep, bridge.next_native_generation()))
        } else {
            None
        };
        (remote_change, now_playing_write, power_change)
    };

    if let Some((enabled, generation)) = remote_change {
        platform::set_remote_commands_enabled(enabled, generation)?;
    }
    if let Some((projection, generation)) = now_playing_write {
        platform::update_now_playing(&projection, generation)?;
    }
    if let Some((prevent, generation)) = power_change {
        if let Err(error) = platform::set_prevent_display_sleep(prevent, generation) {
            report_power_failure(
                app,
                state,
                &last_active_label,
                suppress_sleep_warning,
                error,
            );
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
struct PowerWarningPayload {
    operation: &'static str,
    status: u32,
    message: String,
    suppression_key: &'static str,
    alert_shown: bool,
    suppressed: bool,
}

#[derive(Debug)]
struct NativePowerError {
    operation: &'static str,
    status: u32,
    message: String,
}

fn report_power_failure<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    last_active_label: &str,
    suppressed: bool,
    error: NativePowerError,
) {
    eprintln!("iima: {}", error.message);
    let should_show = if error.operation == "prevent" && !suppressed {
        bridge_state()
            .lock()
            .map(|mut bridge| {
                if bridge.sleep_failure_alert_shown {
                    false
                } else {
                    bridge.sleep_failure_alert_shown = true;
                    true
                }
            })
            .unwrap_or(false)
    } else {
        false
    };
    let user_suppressed = should_show && platform::show_sleep_failure_alert(&error.message);
    if user_suppressed {
        let save_result = (|| {
            let preferences = {
                let mut preferences = state
                    .preferences
                    .lock()
                    .map_err(|error| error.to_string())?;
                preferences.values.insert(
                    "suppressCannotPreventDisplaySleep".to_string(),
                    serde_json::Value::Bool(true),
                );
                preferences.clone()
            };
            let path = preference_file_path(
                app.path()
                    .app_config_dir()
                    .map_err(|error| error.to_string())?,
            );
            preferences.save_to_file(&path)
        })();
        if let Err(save_error) = save_result {
            eprintln!("iima: unable to persist display-sleep warning suppression: {save_error}");
        }
    }
    let payload = PowerWarningPayload {
        operation: error.operation,
        status: error.status,
        message: error.message,
        suppression_key: "suppressCannotPreventDisplaySleep",
        alert_shown: should_show,
        suppressed: suppressed || user_suppressed,
    };
    let _ = app.emit_to(last_active_label, POWER_WARNING_EVENT, payload);
}

pub fn shutdown() {
    if let Ok(mut bridge) = bridge_state().lock() {
        bridge.shutting_down = true;
        bridge.remote_commands_enabled = Some(false);
        bridge.last_now_playing = None;
        bridge.last_now_playing_write = None;
        bridge.power_prevention_requested = Some(false);
    }
    if let Err(error) = platform::set_remote_commands_enabled(false, u64::MAX) {
        eprintln!("iima: unable to disable system media commands: {error}");
    }
    if let Err(error) = platform::set_prevent_display_sleep(false, u64::MAX) {
        eprintln!("iima: {}", error.message);
    }
}

unsafe extern "C" fn handle_remote_command(
    command: i32,
    value: f64,
    _context: *mut std::ffi::c_void,
) -> i32 {
    let Some(app) = APP_HANDLE.get() else {
        return RemoteCommandStatus::CommandFailed as i32;
    };
    match execute_remote_command(app, command, value) {
        Ok(status) => status as i32,
        Err(error) => {
            eprintln!("iima: system media command failed: {error}");
            RemoteCommandStatus::CommandFailed as i32
        }
    }
}

fn execute_remote_command(
    app: &AppHandle,
    raw_command: i32,
    value: f64,
) -> Result<RemoteCommandStatus, String> {
    let command = RemoteCommand::try_from(raw_command)?;
    let state = app.state::<AppState>();
    if bridge_state()
        .lock()
        .map(|bridge| bridge.shutting_down)
        .map_err(|error| error.to_string())?
    {
        return Ok(RemoteCommandStatus::CommandFailed);
    }
    let use_media_keys = state
        .preferences
        .lock()
        .map(|preferences| {
            preferences
                .values
                .get("useMediaKeys")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true)
        })
        .map_err(|error| error.to_string())?;
    if !use_media_keys {
        return Ok(RemoteCommandStatus::CommandFailed);
    }
    let session_label = state.last_active_player_session_label()?;
    let main_or_mini_exists = app.get_webview_window(&session_label).is_some()
        || app
            .get_webview_window(&crate::commands::mini_player_label_for_session(
                &session_label,
            ))
            .is_some();
    if !main_or_mini_exists {
        return Ok(RemoteCommandStatus::NoSuchContent);
    }
    let session = state.player_session_for_window(&session_label)?;
    let loop_mode = session
        .player()
        .lock()
        .map(|player| player.current_url.as_ref().map(|_| player.loop_mode))
        .map_err(|error| error.to_string())?;
    let Some(loop_mode) = loop_mode else {
        return Ok(RemoteCommandStatus::NoSuchContent);
    };
    let player_command = mapped_player_command(command, value, loop_mode)?;
    if matches!(command, RemoteCommand::Stop) {
        state.save_playback_position_for_window(&session_label)?;
    }
    session
        .player()
        .lock()
        .map(|mut player| player.apply(player_command))
        .map_err(|error| error.to_string())?;
    session.sync_mpv_executor_from_player()?;
    let snapshot = session
        .player()
        .lock()
        .map(|player| player.clone())
        .map_err(|error| error.to_string())?;
    crate::commands::emit_player_state_for_session(app, session.label(), &snapshot);
    Ok(RemoteCommandStatus::Success)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{handle_remote_command, NativePowerError, NowPlayingProjection};
    use std::ffi::{c_char, c_int, c_void, CStr, CString};
    use std::ptr;

    unsafe extern "C" {
        fn iima_system_media_set_remote_commands_enabled(
            enabled: c_int,
            generation: u64,
            callback: Option<unsafe extern "C" fn(c_int, f64, *mut c_void) -> c_int>,
            context: *mut c_void,
        );
        fn iima_system_media_update_now_playing_json(
            json: *const c_char,
            generation: u64,
            error_out: *mut *mut c_char,
        ) -> c_int;
        fn iima_system_media_set_prevent_display_sleep(
            prevent: c_int,
            generation: u64,
            error_out: *mut *mut c_char,
        ) -> c_int;
        fn iima_system_media_show_sleep_failure_alert(message: *const c_char) -> c_int;
        fn iima_system_media_free_string(value: *mut c_char);
    }

    pub fn set_remote_commands_enabled(enabled: bool, generation: u64) -> Result<(), String> {
        unsafe {
            iima_system_media_set_remote_commands_enabled(
                i32::from(enabled),
                generation,
                enabled.then_some(handle_remote_command),
                ptr::null_mut(),
            )
        };
        Ok(())
    }

    pub fn update_now_playing(
        projection: &NowPlayingProjection,
        generation: u64,
    ) -> Result<(), String> {
        let json = serde_json::to_string(projection)
            .map_err(|error| format!("unable to encode Now Playing metadata: {error}"))?;
        let json = CString::new(json)
            .map_err(|_| "Now Playing metadata contains a NUL byte".to_string())?;
        let mut error = ptr::null_mut();
        let status = unsafe {
            iima_system_media_update_now_playing_json(json.as_ptr(), generation, &mut error)
        };
        if status == 0 {
            Ok(())
        } else {
            Err(take_string(error)
                .unwrap_or_else(|| "unable to update MPNowPlayingInfoCenter".to_string()))
        }
    }

    pub fn set_prevent_display_sleep(
        prevent: bool,
        generation: u64,
    ) -> Result<(), NativePowerError> {
        let mut error = ptr::null_mut();
        let status = unsafe {
            iima_system_media_set_prevent_display_sleep(i32::from(prevent), generation, &mut error)
        };
        if status == 0 {
            Ok(())
        } else {
            let code = status as u32;
            Err(NativePowerError {
                operation: if prevent { "prevent" } else { "release" },
                status: code,
                message: take_string(error).unwrap_or_else(|| {
                    format!(
                        "{} display-sleep assertion failed with IOReturn 0x{code:08X}",
                        if prevent { "creating" } else { "releasing" }
                    )
                }),
            })
        }
    }

    pub fn show_sleep_failure_alert(message: &str) -> bool {
        let Ok(message) = CString::new(message) else {
            return false;
        };
        unsafe { iima_system_media_show_sleep_failure_alert(message.as_ptr()) != 0 }
    }

    fn take_string(value: *mut c_char) -> Option<String> {
        if value.is_null() {
            return None;
        }
        let result = unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned();
        unsafe { iima_system_media_free_string(value) };
        Some(result)
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::{NativePowerError, NowPlayingProjection};

    pub fn set_remote_commands_enabled(_enabled: bool, _generation: u64) -> Result<(), String> {
        Ok(())
    }

    pub fn update_now_playing(
        _projection: &NowPlayingProjection,
        _generation: u64,
    ) -> Result<(), String> {
        Ok(())
    }

    pub fn set_prevent_display_sleep(
        _prevent: bool,
        _generation: u64,
    ) -> Result<(), NativePowerError> {
        Ok(())
    }

    pub fn show_sleep_failure_alert(_message: &str) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{LoopMode, Track, TrackMetadata};

    fn assert_command(command: PlayerCommand, expected: &str) {
        let actual = match command {
            PlayerCommand::Resume => "resume",
            PlayerCommand::Pause => "pause",
            PlayerCommand::TogglePause => "toggle",
            PlayerCommand::Stop => "stop",
            PlayerCommand::PlaylistNext => "next",
            PlayerCommand::PlaylistPrev => "previous",
            PlayerCommand::ToggleFileLoop => "file-loop",
            PlayerCommand::TogglePlaylistLoop => "playlist-loop",
            PlayerCommand::SetSpeed { speed } if speed == 1.5 => "speed-1.5",
            PlayerCommand::SeekRelative {
                seconds: 15.0,
                option: RelativeSeekOption::Exact,
            } => "skip-forward-15",
            PlayerCommand::SeekRelative {
                seconds: -15.0,
                option: RelativeSeekOption::Exact,
            } => "skip-backward-15",
            PlayerCommand::Seek { seconds } if seconds == 42.5 => "position-42.5",
            _ => "unexpected",
        };
        assert_eq!(actual, expected);
    }

    fn projected(
        session_label: &str,
        player: Option<&PlayerState>,
        had_media: bool,
    ) -> NowPlayingProjection {
        let player = player.map(PlayerSystemMediaSnapshot::from_player);
        now_playing_projection(session_label, player.as_ref(), had_media)
    }

    fn prevents_display_sleep(players: &[&PlayerState]) -> bool {
        let players = players
            .iter()
            .map(|player| PlayerSystemMediaSnapshot::from_player(player))
            .collect::<Vec<_>>();
        should_prevent_display_sleep(players.iter())
    }

    #[test]
    fn maps_the_complete_reference_remote_command_surface() {
        for (raw, value, loop_mode, expected) in [
            (REMOTE_PLAY, 0.0, LoopMode::Off, "resume"),
            (REMOTE_PAUSE, 0.0, LoopMode::Off, "pause"),
            (REMOTE_TOGGLE_PLAY_PAUSE, 0.0, LoopMode::Off, "toggle"),
            (REMOTE_STOP, 0.0, LoopMode::Off, "stop"),
            (REMOTE_NEXT_TRACK, 0.0, LoopMode::Off, "next"),
            (REMOTE_PREVIOUS_TRACK, 0.0, LoopMode::Off, "previous"),
            (REMOTE_CHANGE_REPEAT_MODE, 0.0, LoopMode::Off, "file-loop"),
            (
                REMOTE_CHANGE_REPEAT_MODE,
                0.0,
                LoopMode::File,
                "playlist-loop",
            ),
            (
                REMOTE_CHANGE_REPEAT_MODE,
                0.0,
                LoopMode::Playlist,
                "playlist-loop",
            ),
            (REMOTE_CHANGE_PLAYBACK_RATE, 1.5, LoopMode::Off, "speed-1.5"),
            (REMOTE_SKIP_FORWARD, 99.0, LoopMode::Off, "skip-forward-15"),
            (
                REMOTE_SKIP_BACKWARD,
                99.0,
                LoopMode::Off,
                "skip-backward-15",
            ),
            (
                REMOTE_CHANGE_PLAYBACK_POSITION,
                42.5,
                LoopMode::Off,
                "position-42.5",
            ),
        ] {
            let command =
                mapped_player_command(RemoteCommand::try_from(raw).unwrap(), value, loop_mode)
                    .unwrap();
            assert_command(command, expected);
        }
        for rate in SUPPORTED_PLAYBACK_RATES {
            assert!(
                mapped_player_command(RemoteCommand::ChangePlaybackRate, rate, LoopMode::Off)
                    .is_ok()
            );
        }
        assert!(
            mapped_player_command(RemoteCommand::ChangePlaybackRate, 1.25, LoopMode::Off).is_err()
        );
        assert!(mapped_player_command(
            RemoteCommand::ChangePlaybackPosition,
            f64::NAN,
            LoopMode::Off
        )
        .is_err());
    }

    #[test]
    fn projects_video_audio_and_all_four_playback_states() {
        let unknown = projected("main", None, false);
        assert_eq!(unknown.playback_state, NowPlayingPlaybackState::Unknown);
        assert_eq!(unknown.media_type, None);

        let mut player = PlayerState::default();
        player.current_url = Some("/tmp/Movie.mp4".to_string());
        player.media_title = "Movie.mp4".to_string();
        player.paused = false;
        player.duration_seconds = 120.0;
        player.position_seconds = 12.0;
        player.speed = 1.5;
        player.tracks.video.clear();
        player.tracks.audio.clear();
        player.tracks.video.push(Track {
            id: 1,
            title: "Video".to_string(),
            selected: true,
            metadata: TrackMetadata::default(),
        });
        let playing = projected("main", Some(&player), true);
        assert_eq!(playing.media_type, Some(NowPlayingMediaType::Video));
        assert_eq!(playing.title.as_deref(), Some("Movie"));
        assert_eq!(playing.playback_state, NowPlayingPlaybackState::Playing);
        assert_eq!(playing.duration, 120.0);
        assert_eq!(playing.elapsed, 12.0);
        assert_eq!(playing.rate, 1.5);

        player.paused = true;
        let paused = projected("main", Some(&player), true);
        assert_eq!(paused.playback_state, NowPlayingPlaybackState::Paused);

        player.current_url = None;
        let stopped = projected("main", Some(&player), true);
        assert_eq!(stopped.playback_state, NowPlayingPlaybackState::Stopped);

        player.current_url = Some("/tmp/Album.flac".to_string());
        player.music_title = "Song".to_string();
        player.music_album = "Album".to_string();
        player.music_artist = "Artist".to_string();
        player.tracks.audio.push(Track {
            id: 1,
            title: "Audio".to_string(),
            selected: true,
            metadata: TrackMetadata::default(),
        });
        player.tracks.video[0].metadata.albumart = true;
        let audio = projected("main", Some(&player), true);
        assert_eq!(audio.media_type, Some(NowPlayingMediaType::Audio));
        assert_eq!(audio.title.as_deref(), Some("Song"));
        assert_eq!(audio.album.as_deref(), Some("Album"));
        assert_eq!(audio.artist.as_deref(), Some("Artist"));

        player.current_url = Some("https://example.test/radio".to_string());
        let network = projected("main", Some(&player), true);
        assert_eq!(network.media_type, Some(NowPlayingMediaType::Video));
    }

    #[test]
    fn deduplicates_clock_progress_but_refreshes_seek_focus_and_state_changes() {
        let mut player = PlayerState::default();
        player.current_url = Some("/tmp/movie.mp4".to_string());
        player.media_title = "Movie".to_string();
        player.paused = false;
        player.position_seconds = 10.0;
        let previous = projected("main", Some(&player), true);

        player.position_seconds = 10.25;
        let ticking = projected("main", Some(&player), true);
        assert!(!should_write_now_playing(
            Some(&previous),
            &ticking,
            Duration::from_millis(250)
        ));

        player.position_seconds = 40.0;
        let seeked = projected("main", Some(&player), true);
        assert!(should_write_now_playing(
            Some(&previous),
            &seeked,
            Duration::from_millis(250)
        ));

        let focused_other = projected("player-1", Some(&player), true);
        assert!(should_write_now_playing(
            Some(&seeked),
            &focused_other,
            Duration::ZERO
        ));

        player.paused = true;
        let paused = projected("main", Some(&player), true);
        assert!(should_write_now_playing(
            Some(&seeked),
            &paused,
            Duration::ZERO
        ));
    }

    #[test]
    fn display_sleep_plan_is_idempotent_over_all_player_sessions() {
        let mut first = PlayerState::default();
        let mut second = PlayerState::default();
        assert!(!prevents_display_sleep(&[&first, &second]));

        first.current_url = Some("/tmp/first.mp4".to_string());
        first.paused = true;
        second.current_url = Some("/tmp/second.mp4".to_string());
        second.paused = false;
        assert!(prevents_display_sleep(&[&first, &second]));

        second.paused = true;
        assert!(!prevents_display_sleep(&[&first, &second]));
        second.current_url = None;
        assert!(!prevents_display_sleep(&[&first, &second]));
    }

    #[test]
    fn objective_c_bridge_keeps_the_reference_media_and_power_contract() {
        let source = include_str!("native_system_media.m");
        for contract in [
            "MPRemoteCommandCenter",
            "playCommand",
            "pauseCommand",
            "togglePlayPauseCommand",
            "stopCommand",
            "nextTrackCommand",
            "previousTrackCommand",
            "changeRepeatModeCommand",
            "changePlaybackRateCommand",
            "skipForwardCommand",
            "skipBackwardCommand",
            "changePlaybackPositionCommand",
            "MPNowPlayingInfoCenter",
            "MPMediaItemPropertyMediaType",
            "MPNowPlayingInfoPropertyElapsedPlaybackTime",
            "MPNowPlayingInfoPropertyPlaybackRate",
            "MPNowPlayingInfoPropertyDefaultPlaybackRate",
            "IOPMAssertionCreateWithName",
            "kIOPMAssertionTypeNoDisplaySleep",
            "IOPMAssertionRelease",
            "showsSuppressionButton",
            "generation <= iima_system_media_remote_generation",
            "generation <= iima_system_media_now_playing_generation",
            "generation <= iima_system_media_power_generation",
        ] {
            assert!(
                source.contains(contract),
                "missing native contract: {contract}"
            );
        }
        let build = include_str!("../build.rs");
        assert!(build.contains("src/native_system_media.m"));
        assert!(build.contains("framework=MediaPlayer"));

        let preferences = crate::preferences::PreferenceStore::default();
        assert_eq!(
            preferences
                .values
                .get("useMediaKeys")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            preferences
                .values
                .get("suppressCannotPreventDisplaySleep")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );

        let library = include_str!("lib.rs");
        assert!(library.contains("native_system_media::initialize(app.handle(), state.inner())"));
        assert!(library.contains("native_system_media::sync(&app, state.inner())"));
        assert!(library.contains("native_system_media::shutdown()"));
        let commands = include_str!("commands.rs");
        assert!(commands.contains("crate::native_system_media::sync(&app, state.inner())"));
        assert!(commands.contains("crate::native_system_media::sync(app, state.inner())"));

        let mut bridge = BridgeState::default();
        assert_eq!(bridge.next_native_generation(), 1);
        assert_eq!(bridge.next_native_generation(), 2);
    }
}
