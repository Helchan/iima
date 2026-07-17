use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::media::{MediaInfo, MediaProbe};
use crate::mpv::{
    mpv_command, mpv_command_string, set_plugin_property, set_property, MpvAudioDevice,
    MpvClientEvent, MpvClientOperation, MpvEndFileReason, MpvFilter, MpvFormat, MpvPlaylistItem,
    MpvPluginValue, MpvPropertyChange, MpvTrackListItem,
};
use crate::playlist_cache::PlaylistCacheSnapshot;

const MAX_RECENT_DOCUMENTS: usize = 10;
const MAX_MPV_OPERATION_LOG: usize = 200;
const MAX_PLUGIN_MPV_EVENT_LOG: usize = 512;
const MIN_AB_LOOP_POINT_SECONDS: f64 = 0.000001;
const IINA_AUDIO_EQ_FREQUENCIES: [f64; 10] = [
    31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];
const IINA_SUBTITLE_FONT_SIZES: [f64; 9] = [30.0, 35.0, 40.0, 45.0, 50.0, 55.0, 60.0, 65.0, 70.0];
const IINA_SUBTITLE_BORDER_SIZES: [f64; 10] = [0.0, 0.25, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0];
pub const IINA_SUBTITLE_ENCODINGS: &[(&str, &str)] = &[
    ("Auto detect", "auto"),
    ("Universal (UTF-8)", "UTF-8"),
    ("Universal (UTF-16)", "UTF-16"),
    ("Universal (UTF-16BE)", "UTF-16BE"),
    ("Universal (UTF-16LE)", "UTF-16LE"),
    ("Arabic (ISO-8859-6)", "ISO-8859-6"),
    ("Arabic (WINDOWS-1256)", "WINDOWS-1256"),
    ("Baltic (LATIN7)", "LATIN7"),
    ("Baltic (WINDOWS-1257)", "WINDOWS-1257"),
    ("Celtic (LATIN8)", "LATIN8"),
    ("Central European (WINDOWS-1250)", "WINDOWS-1250"),
    ("Cyrillic (ISO-8859-5)", "ISO-8859-5"),
    ("Cyrillic (WINDOWS-1251)", "WINDOWS-1251"),
    ("Eastern European (ISO-8859-2)", "ISO-8859-2"),
    ("Western Languages (WINDOWS-1252)", "WINDOWS-1252"),
    ("Greek (ISO-8859-7)", "ISO-8859-7"),
    ("Greek (WINDOWS-1253)", "WINDOWS-1253"),
    ("Hebrew (ISO-8859-8)", "ISO-8859-8"),
    ("Hebrew (WINDOWS-1255)", "WINDOWS-1255"),
    ("Japanese (SHIFT-JIS)", "SHIFT-JIS"),
    ("Japanese (ISO-2022-JP-2)", "ISO-2022-JP-2"),
    ("Korean (EUC-KR)", "EUC-KR"),
    ("Korean (CP949)", "CP949"),
    ("Korean (ISO-2022-KR)", "ISO-2022-KR"),
    ("Nordic (LATIN6)", "LATIN6"),
    ("North European (LATIN4)", "LATIN4"),
    ("Russian (KOI8-R)", "KOI8-R"),
    ("Simplified Chinese (GBK)", "GBK"),
    ("Simplified Chinese (GB18030)", "GB18030"),
    ("Simplified Chinese (ISO-2022-CN-EXT)", "ISO-2022-CN-EXT"),
    ("South European (LATIN3)", "LATIN3"),
    ("South-Eastern European (LATIN10)", "LATIN10"),
    ("Thai (TIS-620)", "TIS-620"),
    ("Thai (WINDOWS-874)", "WINDOWS-874"),
    ("Traditional Chinese (EUC-TW)", "EUC-TW"),
    ("Traditional Chinese (BIG5)", "BIG5"),
    ("Traditional Chinese (BIG5-HKSCS)", "BIG5-HKSCS"),
    ("Turkish (LATIN5)", "LATIN5"),
    ("Turkish (WINDOWS-1254)", "WINDOWS-1254"),
    ("Ukrainian (KOI8-U)", "KOI8-U"),
    ("Vietnamese (WINDOWS-1258)", "WINDOWS-1258"),
    ("Vietnamese (VISCII)", "VISCII"),
    ("Western European (LATIN1)", "LATIN1"),
    ("Western European (LATIN-9)", "LATIN-9"),
];

#[derive(Debug, Clone, Serialize)]
pub struct PlayerState {
    pub mode: PlayerMode,
    pub current_url: Option<String>,
    pub file_loading: bool,
    pub playback_error: Option<PlaybackError>,
    pub media_title: String,
    pub music_title: String,
    pub music_album: String,
    pub music_artist: String,
    pub media_info: Option<MediaInfo>,
    pub duration_seconds: f64,
    pub position_seconds: f64,
    pub volume: f64,
    pub speed: f64,
    pub muted: bool,
    pub paused: bool,
    pub loop_mode: LoopMode,
    pub ab_loop: AbLoopState,
    pub audio_devices: Vec<AudioDevice>,
    pub audio_device: String,
    pub video_filters: Vec<MpvFilter>,
    pub audio_filters: Vec<MpvFilter>,
    pub playlist: Vec<PlaylistItem>,
    pub playlist_cache: PlaylistCacheSnapshot,
    pub recent_documents: Vec<RecentDocument>,
    pub last_playback: Option<LastPlayback>,
    pub chapters: Vec<Chapter>,
    pub tracks: TrackGroups,
    pub second_subtitle_id: i64,
    pub sidebar: SidebarState,
    pub quick_settings: QuickSettingsState,
    pub osc_visible: bool,
    pub pip_active: bool,
    pub osd_message: Option<String>,
    pub osd_message_id: u64,
    pub mpv_properties: MpvPropertySnapshot,
    /// Ordered native mpv events retained for the per-player plugin Event API.
    ///
    /// This is deliberately independent from `mpv_operation_log`: commands and events have
    /// different owners and cursors, and repeated native events must remain observable even when
    /// they do not change the reduced `PlayerState`.
    pub mpv_event_cursor: u64,
    pub mpv_events: Vec<PlayerMpvEvent>,
    pub mpv_operation_log: Vec<MpvClientOperation>,
    #[serde(skip)]
    runtime_loop_file_active: bool,
    #[serde(skip)]
    runtime_loop_playlist_active: bool,
    #[serde(skip)]
    command_line_shuffle_pending: bool,
    #[serde(skip)]
    pending_open_error: Option<PlaybackError>,
    #[serde(skip)]
    pending_idle_reset: bool,
    #[serde(skip)]
    mini_player_entered_manually: bool,
    #[serde(skip)]
    mini_player_left_manually: bool,
    #[serde(skip)]
    mpv_operation_log_first_sequence: u64,
    #[serde(skip)]
    mpv_operation_log_next_sequence: u64,
    #[serde(skip)]
    window_resize_file_generation: u64,
    #[serde(skip)]
    window_resize_manually_opened_generation: Option<u64>,
    #[serde(skip)]
    window_resize_expects_start_file: bool,
    #[serde(skip)]
    window_resize_video_reconfiguration_generation: u64,
    #[serde(skip)]
    window_resize_geometry_ready: bool,
    #[serde(skip)]
    tried_using_exact_seek_for_current_file: bool,
    #[serde(skip)]
    use_exact_seek_for_current_file: bool,
    #[serde(skip)]
    auto_seek_probe_pending: bool,
    #[serde(skip)]
    auto_seek_probe_started_at: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlayerMpvEvent {
    pub cursor: u64,
    pub event: MpvClientEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerMpvEventBatch {
    pub cursor: u64,
    pub dropped_event_count: u64,
    pub current_url: Option<String>,
    pub events: Vec<PlayerMpvEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlaybackError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlayerMode {
    Initial,
    Player,
    MiniPlayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutomaticMusicModeTransition {
    Enter,
    Leave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoopMode {
    Off,
    File,
    Playlist,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AbLoopState {
    pub a_seconds: f64,
    pub b_seconds: f64,
    pub count: String,
    pub status: AbLoopStatus,
}

impl AbLoopState {
    pub fn is_active(&self) -> bool {
        self.status == AbLoopStatus::BSet && mpv_loop_value_is_active(&self.count, false)
    }
}

impl Default for AbLoopState {
    fn default() -> Self {
        Self {
            a_seconds: 0.0,
            b_seconds: 0.0,
            count: "inf".to_string(),
            status: AbLoopStatus::Cleared,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AbLoopStatus {
    Cleared,
    ASet,
    BSet,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AbLoopPoint {
    A,
    B,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistItem {
    pub id: usize,
    pub mpv_id: Option<i64>,
    pub path: String,
    pub title: String,
    pub duration_seconds: Option<f64>,
    pub current: bool,
    pub playing: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct RecentDocument {
    pub id: usize,
    pub path: String,
    pub title: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct LastPlayback {
    pub path: String,
    pub title: String,
    pub position_seconds: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Chapter {
    pub index: usize,
    pub title: String,
    pub time_seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioDevice {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FilterKind {
    Video,
    Audio,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrackGroups {
    pub video: Vec<Track>,
    pub audio: Vec<Track>,
    pub subtitles: Vec<Track>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Track {
    pub id: i64,
    pub title: String,
    pub selected: bool,
    pub metadata: TrackMetadata,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TrackMetadata {
    pub source_id: Option<i64>,
    pub source_title: Option<String>,
    pub language: Option<String>,
    pub image: bool,
    pub albumart: bool,
    pub default_track: bool,
    pub forced: bool,
    pub codec: Option<String>,
    pub external: bool,
    pub external_filename: Option<String>,
    pub main_selection: bool,
    pub ff_index: Option<i64>,
    pub decoder_description: Option<String>,
    pub demux_width: Option<i64>,
    pub demux_height: Option<i64>,
    pub demux_channel_count: Option<i64>,
    pub demux_channels: Option<String>,
    pub demux_samplerate: Option<i64>,
    pub demux_fps: Option<f64>,
    pub demux_bitrate: Option<i64>,
    pub demux_rotation: Option<i64>,
    pub demux_par: Option<String>,
    pub audio_channels: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SidebarState {
    pub visible: bool,
    pub tab: SidebarTab,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuickSettingsState {
    pub deinterlace: bool,
    pub hardware_decoding: bool,
    pub hdr_available: bool,
    pub hdr_enabled: bool,
    pub audio_eq: [f64; 10],
    pub audio_eq_active: bool,
    pub sub_text_color: String,
    pub sub_text_size: f64,
    pub sub_border_color: String,
    pub sub_border_size: f64,
    pub sub_background_color: String,
    pub sub_font: String,
    pub sub_encoding: String,
    pub video_aspect: String,
    pub video_crop: String,
    pub custom_crop: Option<CustomCrop>,
    pub video_rotate: i64,
    pub video_flipped: bool,
    pub video_mirrored: bool,
    pub brightness: i64,
    pub contrast: i64,
    pub saturation: i64,
    pub gamma: i64,
    pub hue: i64,
    pub audio_delay: f64,
    pub sub_delay: f64,
    pub sub_scale: f64,
    pub sub_pos: i64,
}

impl Default for QuickSettingsState {
    fn default() -> Self {
        Self {
            deinterlace: false,
            hardware_decoding: true,
            hdr_available: false,
            hdr_enabled: true,
            audio_eq: [0.0; 10],
            audio_eq_active: false,
            sub_text_color: "1/1/1/1".to_string(),
            sub_text_size: 55.0,
            sub_border_color: "0/0/0/1".to_string(),
            sub_border_size: 3.0,
            sub_background_color: "1/1/1/0".to_string(),
            sub_font: "sans-serif".to_string(),
            sub_encoding: "auto".to_string(),
            video_aspect: "Default".to_string(),
            video_crop: "None".to_string(),
            custom_crop: None,
            video_rotate: 0,
            video_flipped: false,
            video_mirrored: false,
            brightness: 0,
            contrast: 0,
            saturation: 0,
            gamma: 0,
            hue: 0,
            audio_delay: 0.0,
            sub_delay: 0.0,
            sub_scale: 1.0,
            sub_pos: 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CustomCrop {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

impl QuickSettingsState {
    fn set_video_equalizer(
        &mut self,
        option: VideoEqualizer,
        value: i64,
    ) -> (&'static str, &'static str) {
        match option {
            VideoEqualizer::Brightness => {
                self.brightness = value;
                ("brightness", "Brightness")
            }
            VideoEqualizer::Contrast => {
                self.contrast = value;
                ("contrast", "Contrast")
            }
            VideoEqualizer::Saturation => {
                self.saturation = value;
                ("saturation", "Saturation")
            }
            VideoEqualizer::Gamma => {
                self.gamma = value;
                ("gamma", "Gamma")
            }
            VideoEqualizer::Hue => {
                self.hue = value;
                ("hue", "Hue")
            }
        }
    }

    fn set_runtime_equalizer(&mut self, option: VideoEqualizer, value: Option<i64>) -> bool {
        let Some(value) = value else {
            return false;
        };
        self.set_video_equalizer(option, value.clamp(-100, 100));
        true
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MpvPropertySnapshot {
    pub path: Option<String>,
    #[serde(rename = "media-title")]
    pub media_title: String,
    pub duration: f64,
    #[serde(rename = "time-pos")]
    pub time_pos: f64,
    #[serde(rename = "percent-pos")]
    pub percent_pos: f64,
    pub pause: bool,
    pub volume: f64,
    pub speed: f64,
    pub mute: bool,
    pub chapter: i64,
    pub chapters: usize,
    #[serde(rename = "playlist-count")]
    pub playlist_count: usize,
    #[serde(rename = "playlist-pos")]
    pub playlist_pos: i64,
    #[serde(rename = "track-list/count")]
    pub track_list_count: usize,
    pub vid: i64,
    pub aid: i64,
    pub sid: i64,
    #[serde(rename = "secondary-sid")]
    pub secondary_sid: i64,
    #[serde(rename = "idle-active")]
    pub idle_active: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SidebarTab {
    Playlist,
    Chapters,
    Video,
    Audio,
    Subtitles,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TrackSelectionKind {
    Video,
    Audio,
    Subtitles,
    SecondSubtitles,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExternalTrackKind {
    Video,
    Audio,
    Subtitles,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RelativeSeekOption {
    Relative,
    Exact,
    Auto,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoEqualizer {
    Brightness,
    Contrast,
    Saturation,
    Gamma,
    Hue,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubtitleStyleColorTarget {
    Text,
    Border,
    Background,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PlayerCommand {
    TogglePause,
    Pause,
    Resume,
    Stop,
    Seek {
        seconds: f64,
    },
    SeekAbsolute {
        seconds: f64,
    },
    /// IINA's OSC/Mini Player sliders seek in percent and optionally force the
    /// exact mpv mode independently of every relative-seek path.
    SeekPercent {
        percent: f64,
        exact: bool,
    },
    SeekRelative {
        seconds: f64,
        option: RelativeSeekOption,
    },
    /// IINA's Jump To path forwards the entered timestamp to mpv unchanged.
    /// Unlike timeline seeks, this must not clamp or optimistically rewrite the
    /// modeled playback position before mpv publishes its authoritative value.
    SeekAbsoluteExact {
        seconds: f64,
    },
    SetVolume {
        volume: f64,
    },
    SetSpeed {
        speed: f64,
    },
    MultiplySpeed {
        factor: f64,
    },
    ToggleMute,
    CycleAbLoop,
    SetAbLoopPoint {
        point: AbLoopPoint,
        seconds: f64,
    },
    SelectAudioDevice {
        name: String,
    },
    AddFilter {
        kind: FilterKind,
        filter: String,
    },
    RemoveFilter {
        kind: FilterKind,
        index: usize,
    },
    ToggleSavedFilter {
        kind: FilterKind,
        name: String,
        filter: String,
    },
    ToggleFileLoop,
    TogglePlaylistLoop,
    FrameStep {
        backwards: bool,
    },
    PlaylistNext,
    PlaylistPrev,
    SelectChapter {
        index: usize,
    },
    SelectPlaylistItem {
        index: usize,
    },
    MovePlaylistItems {
        indexes: Vec<usize>,
        destination: usize,
    },
    InsertPlaylistItems {
        paths: Vec<String>,
        destination: usize,
    },
    PlayPlaylistItemsNext {
        indexes: Vec<usize>,
    },
    RemovePlaylistItem {
        index: usize,
    },
    RemovePlaylistItems {
        indexes: Vec<usize>,
    },
    ClearPlaylist,
    CycleTrack {
        kind: TrackSelectionKind,
    },
    SelectTrack {
        kind: TrackSelectionKind,
        id: i64,
    },
    SwapSubtitleTracks,
    LoadExternalTrack {
        kind: ExternalTrackKind,
        path: String,
    },
    SetDeinterlace {
        enabled: bool,
    },
    SetHardwareDecoding {
        enabled: bool,
        decoder: i64,
    },
    SetHdrEnabled {
        enabled: bool,
    },
    SetVideoAspect {
        aspect: String,
    },
    SetVideoCrop {
        crop: String,
    },
    SetCustomVideoCrop {
        x: i64,
        y: i64,
        width: i64,
        height: i64,
    },
    SetDelogoRegion {
        x: i64,
        y: i64,
        width: i64,
        height: i64,
    },
    RemoveDelogo,
    SetVideoRotate {
        degrees: i64,
    },
    SetVideoFlip {
        enabled: bool,
    },
    SetVideoMirror {
        enabled: bool,
    },
    SetVideoEqualizer {
        option: VideoEqualizer,
        value: i64,
    },
    SetAudioDelay {
        seconds: f64,
    },
    SetAudioEqualizer {
        gains: Vec<f64>,
    },
    ResetAudioEqualizer,
    SetSubtitleStyleColor {
        target: SubtitleStyleColorTarget,
        color: String,
    },
    SetSubtitleTextSize {
        size: f64,
    },
    SetSubtitleBorderSize {
        size: f64,
    },
    SetSubtitleFont {
        font: String,
    },
    SetSubEncoding {
        encoding: String,
    },
    SetSubDelay {
        seconds: f64,
    },
    SetSubScale {
        scale: f64,
    },
    SetSubPosition {
        position: i64,
    },
    KeyBindingMpvCommand {
        action: String,
    },
    PluginMpvCommand {
        command: String,
        args: Vec<String>,
    },
    PluginMpvSet {
        property: String,
        value: String,
    },
    PluginMpvSetNative {
        property: String,
        value: MpvPluginValue,
    },
    ShowSidebar {
        tab: SidebarTab,
    },
    HideSidebar,
    ToggleOsc,
    EnterMiniPlayer,
    LeaveMiniPlayer,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            mode: PlayerMode::Initial,
            current_url: None,
            file_loading: false,
            playback_error: None,
            media_title: "IINA".to_string(),
            music_title: "IINA".to_string(),
            music_album: String::new(),
            music_artist: String::new(),
            media_info: None,
            duration_seconds: 0.0,
            position_seconds: 0.0,
            volume: 100.0,
            speed: 1.0,
            muted: false,
            paused: true,
            loop_mode: LoopMode::Off,
            ab_loop: AbLoopState::default(),
            audio_devices: vec![AudioDevice {
                name: "auto".to_string(),
                description: "Autoselect device".to_string(),
            }],
            audio_device: "auto".to_string(),
            video_filters: Vec::new(),
            audio_filters: Vec::new(),
            playlist: Vec::new(),
            playlist_cache: PlaylistCacheSnapshot::default(),
            recent_documents: Vec::new(),
            last_playback: None,
            chapters: Vec::new(),
            tracks: TrackGroups::default(),
            second_subtitle_id: 0,
            sidebar: SidebarState {
                visible: false,
                tab: SidebarTab::Playlist,
            },
            quick_settings: QuickSettingsState::default(),
            osc_visible: true,
            pip_active: false,
            osd_message: None,
            osd_message_id: 0,
            mpv_properties: MpvPropertySnapshot::default(),
            mpv_event_cursor: 0,
            mpv_events: Vec::new(),
            mpv_operation_log: Vec::new(),
            runtime_loop_file_active: false,
            runtime_loop_playlist_active: false,
            command_line_shuffle_pending: false,
            pending_open_error: None,
            pending_idle_reset: false,
            mini_player_entered_manually: false,
            mini_player_left_manually: false,
            mpv_operation_log_first_sequence: 0,
            mpv_operation_log_next_sequence: 0,
            window_resize_file_generation: 0,
            window_resize_manually_opened_generation: None,
            window_resize_expects_start_file: false,
            window_resize_video_reconfiguration_generation: 0,
            window_resize_geometry_ready: false,
            tried_using_exact_seek_for_current_file: false,
            use_exact_seek_for_current_file: true,
            auto_seek_probe_pending: false,
            auto_seek_probe_started_at: None,
        }
    }
}

impl PlayerState {
    /// Arms IINA's command-line-only `--mpv-shuffle=yes` behavior for the next media batch.
    ///
    /// IINA installs a one-shot `on_before_start_file` hook before opening the batch, then issues
    /// `playlist-shuffle` followed by `playlist-play-index 0` after the full playlist has been
    /// appended. Recording those commands with the batch before its executor sync preserves the
    /// same ordering without turning `shuffle` into a persistent mpv property.
    pub(crate) fn arm_command_line_shuffle_once(&mut self) {
        self.command_line_shuffle_pending = true;
    }

    pub fn send_osd(&mut self, message: impl Into<String>) {
        self.osd_message = Some(message.into());
        self.osd_message_id = self.osd_message_id.wrapping_add(1);
    }

    fn clear_osd(&mut self) {
        self.osd_message = None;
    }

    pub fn mpv_operation_log_first_sequence(&self) -> u64 {
        self.mpv_operation_log_first_sequence
    }

    pub fn mpv_operation_log_next_sequence(&self) -> u64 {
        self.mpv_operation_log_next_sequence
    }

    pub fn prepare_playback_position_save(&mut self) -> Option<LastPlayback> {
        let path = self.current_url.clone()?;
        let last_playback = LastPlayback {
            path,
            title: self.media_title.clone(),
            position_seconds: self.position_seconds.max(0.0),
        };
        self.last_playback = Some(last_playback.clone());
        self.record_mpv_command("write-watch-later-config", std::iter::empty::<&str>());
        Some(last_playback)
    }

    #[cfg(test)]
    pub fn open_media(&mut self, path: String, probe: Result<MediaProbe, String>) {
        self.open_media_batch(vec![path], probe);
    }

    #[cfg(test)]
    pub fn open_media_with_pause(
        &mut self,
        path: String,
        probe: Result<MediaProbe, String>,
        pause_when_open: bool,
    ) {
        self.open_media_batch_with_pause(vec![path], probe, pause_when_open);
    }

    #[cfg(test)]
    pub fn open_media_batch(&mut self, paths: Vec<String>, probe: Result<MediaProbe, String>) {
        self.open_media_batch_internal(paths, probe, false, false);
    }

    pub fn open_media_batch_with_pause(
        &mut self,
        paths: Vec<String>,
        probe: Result<MediaProbe, String>,
        pause_when_open: bool,
    ) {
        self.open_media_batch_internal(paths, probe, pause_when_open, true);
    }

    fn open_media_batch_internal(
        &mut self,
        paths: Vec<String>,
        probe: Result<MediaProbe, String>,
        pause_when_open: bool,
        synchronize_pause: bool,
    ) {
        let Some(path) = paths.first().cloned() else {
            return;
        };
        let title = path
            .rsplit(['/', '\\'])
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or(&path)
            .to_string();
        let probe = match probe {
            Ok(probe) => Some(probe),
            Err(error) => {
                self.media_info = Some(MediaInfo::unavailable(error));
                None
            }
        };
        let probed_title = probe
            .as_ref()
            .and_then(|probe| probe.title.clone())
            .unwrap_or(title);
        let duration_seconds = probe
            .as_ref()
            .and_then(|probe| probe.duration_seconds)
            .unwrap_or(0.0);

        self.mode = PlayerMode::Player;
        self.current_url = Some(path.clone());
        self.begin_manually_opened_file_for_window_resize();
        self.begin_file_loading();
        self.media_title = probed_title.clone();
        self.reset_music_metadata(&probed_title);
        self.media_info = probe
            .as_ref()
            .map(MediaInfo::from_probe)
            .or(self.media_info.take());
        self.duration_seconds = duration_seconds;
        self.position_seconds = 0.0;
        self.speed = 1.0;
        self.paused = pause_when_open;
        self.osc_visible = true;
        self.second_subtitle_id = 0;
        self.ab_loop = AbLoopState::default();
        self.send_osd(format!("Opening {probed_title}"));
        self.note_recent_document(path.clone(), probed_title.clone());
        self.last_playback = Some(LastPlayback {
            path: path.clone(),
            title: probed_title.clone(),
            position_seconds: 0.0,
        });
        self.playlist = paths
            .into_iter()
            .enumerate()
            .map(|(index, item_path)| {
                let current = index == 0;
                PlaylistItem {
                    id: index + 1,
                    mpv_id: None,
                    title: if current {
                        probed_title.clone()
                    } else {
                        title_from_path(&item_path)
                    },
                    path: item_path,
                    duration_seconds: (current && duration_seconds > 0.0)
                        .then_some(duration_seconds),
                    current,
                    playing: current,
                }
            })
            .collect();
        if let Some(probe) = probe {
            self.chapters = probe
                .chapters
                .iter()
                .map(|chapter| Chapter {
                    index: chapter.index,
                    title: chapter.title.clone(),
                    time_seconds: chapter.start_time_seconds,
                })
                .collect();
            self.tracks = TrackGroups::from_probe(&probe);
        } else {
            self.chapters.clear();
            self.tracks = TrackGroups::default();
        }
        self.record_mpv_command("loadfile", [path.as_str(), "replace"]);
        if synchronize_pause {
            self.record_mpv_flag("pause", pause_when_open);
        }
        let queued_paths = self
            .playlist
            .iter()
            .skip(1)
            .map(|item| item.path.clone())
            .collect::<Vec<_>>();
        for queued_path in queued_paths {
            self.record_mpv_command("loadfile", [queued_path.as_str(), "append"]);
        }
        if self.command_line_shuffle_pending {
            self.command_line_shuffle_pending = false;
            self.record_mpv_command("playlist-shuffle", std::iter::empty::<&str>());
            self.record_mpv_command("playlist-play-index", ["0"]);
        }
        self.refresh_mpv_properties();
    }

    pub fn enqueue_media(&mut self, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }

        let added_count = paths.len();
        let mut next_id = self.playlist.len() + 1;
        for path in paths {
            self.record_mpv_command("loadfile", [path.as_str(), "append"]);
            self.playlist.push(PlaylistItem {
                id: next_id,
                mpv_id: None,
                title: title_from_path(&path),
                path,
                duration_seconds: None,
                current: false,
                playing: false,
            });
            next_id += 1;
        }
        self.send_osd(format!("Added {added_count} Files to Playlist"));
        self.refresh_mpv_properties();
    }

    fn note_recent_document(&mut self, path: String, title: String) {
        self.recent_documents.retain(|item| item.path != path);
        self.recent_documents
            .insert(0, RecentDocument { id: 1, path, title });
        self.recent_documents.truncate(MAX_RECENT_DOCUMENTS);
        for (index, item) in self.recent_documents.iter_mut().enumerate() {
            item.id = index + 1;
        }
    }

    fn selected_video_dimensions(&self) -> Option<(i64, i64)> {
        self.tracks
            .video
            .iter()
            .find(|track| track.selected)
            .or_else(|| self.tracks.video.first())
            .and_then(|track| Some((track.metadata.demux_width?, track.metadata.demux_height?)))
            .filter(|(width, height)| *width > 0 && *height > 0)
    }

    pub fn video_size_for_display(&self) -> Option<(f64, f64)> {
        let track = self
            .tracks
            .video
            .iter()
            .find(|track| track.selected)
            .or_else(|| self.tracks.video.first())?;
        let mut width = track.metadata.demux_width? as f64;
        let mut height = track.metadata.demux_height? as f64;
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        let source_rotation = track.metadata.demux_rotation.unwrap_or_default();
        let display_rotation = (source_rotation - self.quick_settings.video_rotate).rem_euclid(360);
        if matches!(display_rotation, 90 | 270) {
            std::mem::swap(&mut width, &mut height);
        }
        Some((width, height))
    }

    pub(crate) fn window_resize_observation(&self) -> (u64, u64, bool, bool) {
        (
            self.window_resize_file_generation,
            self.window_resize_video_reconfiguration_generation,
            self.window_resize_manually_opened_generation
                == Some(self.window_resize_file_generation),
            self.window_resize_geometry_ready,
        )
    }

    fn selected_audio_channel_count(&self) -> Option<i64> {
        self.tracks
            .audio
            .iter()
            .find(|track| track.selected)
            .or_else(|| self.tracks.audio.first())
            .and_then(|track| track.metadata.demux_channel_count)
            .filter(|channel_count| *channel_count > 0)
    }

    fn filters(&self, kind: FilterKind) -> &[MpvFilter] {
        match kind {
            FilterKind::Video => &self.video_filters,
            FilterKind::Audio => &self.audio_filters,
        }
    }

    fn filters_mut(&mut self, kind: FilterKind) -> &mut Vec<MpvFilter> {
        match kind {
            FilterKind::Video => &mut self.video_filters,
            FilterKind::Audio => &mut self.audio_filters,
        }
    }

    fn filter_property(kind: FilterKind) -> &'static str {
        match kind {
            FilterKind::Video => "vf",
            FilterKind::Audio => "af",
        }
    }

    pub fn has_filter(&self, kind: FilterKind, raw: &str) -> bool {
        self.filters(kind)
            .iter()
            .any(|filter| filter.matches_raw(raw))
    }

    fn add_filter(&mut self, kind: FilterKind, raw: &str, display_name: &str) {
        let Some(filter) = MpvFilter::from_raw(raw) else {
            self.send_osd("Invalid Filter");
            return;
        };
        self.record_mpv_command(Self::filter_property(kind), ["add", raw]);
        self.filters_mut(kind).push(filter);
        self.send_osd(format!("Added Filter: {display_name}"));
    }

    fn remove_filter(&mut self, kind: FilterKind, index: usize) {
        if index >= self.filters(kind).len() {
            return;
        }
        self.record_mpv_operation(MpvClientOperation::RemoveFilterAt {
            name: Self::filter_property(kind).to_string(),
            index,
        });
        self.filters_mut(kind).remove(index);
        self.send_osd("Removed Filter");
    }

    fn set_loop_mode(&mut self, mode: LoopMode) {
        match mode {
            LoopMode::Playlist => {
                self.record_mpv_string("loop-playlist", "inf");
                self.record_mpv_string("loop-file", "no");
                self.runtime_loop_playlist_active = true;
                self.runtime_loop_file_active = false;
                self.send_osd("Playlist Loop");
            }
            LoopMode::File => {
                self.record_mpv_string("loop-file", "inf");
                self.runtime_loop_file_active = true;
                self.send_osd("File Loop");
            }
            LoopMode::Off => {
                self.record_mpv_string("loop-playlist", "no");
                self.record_mpv_string("loop-file", "no");
                self.runtime_loop_playlist_active = false;
                self.runtime_loop_file_active = false;
                self.send_osd("Loop Off");
            }
        }
        self.loop_mode = mode;
    }

    fn sync_ab_loop_status(&mut self) {
        self.ab_loop.status = if self.ab_loop.a_seconds == 0.0 {
            if self.ab_loop.b_seconds == 0.0 {
                AbLoopStatus::Cleared
            } else {
                AbLoopStatus::ASet
            }
        } else if self.ab_loop.b_seconds == 0.0 {
            AbLoopStatus::ASet
        } else {
            AbLoopStatus::BSet
        };
    }

    pub fn apply(&mut self, command: PlayerCommand) {
        match command {
            PlayerCommand::TogglePause => {
                self.paused = !self.paused;
                self.record_mpv_flag("pause", self.paused);
                self.send_osd(if self.paused { "Paused" } else { "Playing" });
            }
            PlayerCommand::Pause => {
                self.paused = true;
                self.record_mpv_flag("pause", true);
                self.send_osd("Paused");
            }
            PlayerCommand::Resume => {
                self.paused = false;
                self.record_mpv_flag("pause", false);
                self.send_osd("Playing");
            }
            PlayerCommand::Stop => {
                if self.current_url.is_none() || self.pending_idle_reset {
                    return;
                }
                self.record_mpv_command("stop", std::iter::empty::<&str>());
                self.pending_idle_reset = true;
            }
            PlayerCommand::Seek { seconds } => {
                let upper_bound = if self.duration_seconds > 0.0 {
                    self.duration_seconds
                } else {
                    f64::MAX
                };
                self.position_seconds = seconds.clamp(0.0, upper_bound);
                if let (Some(current_url), Some(last_playback)) =
                    (self.current_url.as_deref(), self.last_playback.as_mut())
                {
                    if last_playback.path == current_url {
                        last_playback.position_seconds = self.position_seconds;
                    }
                }
                self.send_osd(format!("Seek {:.0}s", self.position_seconds));
                self.record_mpv_command(
                    "seek",
                    [
                        format_mpv_number(self.position_seconds),
                        "absolute+exact".to_string(),
                    ],
                );
            }
            PlayerCommand::SeekAbsolute { seconds } if seconds.is_finite() => {
                let upper_bound = if self.duration_seconds > 0.0 {
                    self.duration_seconds
                } else {
                    f64::MAX
                };
                self.position_seconds = seconds.clamp(0.0, upper_bound);
                if let (Some(current_url), Some(last_playback)) =
                    (self.current_url.as_deref(), self.last_playback.as_mut())
                {
                    if last_playback.path == current_url {
                        last_playback.position_seconds = self.position_seconds;
                    }
                }
                self.send_osd(format!("Seek {:.0}s", self.position_seconds));
                self.record_mpv_command(
                    "seek",
                    [
                        format_mpv_number(self.position_seconds),
                        "absolute".to_string(),
                    ],
                );
            }
            PlayerCommand::SeekAbsolute { .. } => {}
            PlayerCommand::SeekPercent { percent, exact } if percent.is_finite() => {
                let maximum = if self.duration_seconds > 0.0 {
                    f64::from_bits(100.0_f64.to_bits() - 1)
                } else {
                    100.0
                };
                let percent = percent.clamp(0.0, maximum);
                if self.duration_seconds > 0.0 {
                    self.position_seconds = self.duration_seconds * percent / 100.0;
                    if let (Some(current_url), Some(last_playback)) =
                        (self.current_url.as_deref(), self.last_playback.as_mut())
                    {
                        if last_playback.path == current_url {
                            last_playback.position_seconds = self.position_seconds;
                        }
                    }
                }
                self.send_osd(format!("Seek {:.0}s", self.position_seconds));
                self.record_mpv_command(
                    "seek",
                    [
                        // Keep `100.nextDown` distinguishable from `100`.
                        // The general six-decimal formatter would round the
                        // half-open clamp back to EOF and make mpv advance to
                        // the next playlist item.
                        percent.to_string(),
                        if exact {
                            "absolute-percent+exact".to_string()
                        } else {
                            "absolute-percent".to_string()
                        },
                    ],
                );
            }
            PlayerCommand::SeekPercent { .. } => {}
            PlayerCommand::SeekRelative { seconds, option } => {
                let upper_bound = if self.duration_seconds > 0.0 {
                    self.duration_seconds
                } else {
                    f64::MAX
                };
                self.position_seconds = (self.position_seconds + seconds).clamp(0.0, upper_bound);
                if let (Some(current_url), Some(last_playback)) =
                    (self.current_url.as_deref(), self.last_playback.as_mut())
                {
                    if last_playback.path == current_url {
                        last_playback.position_seconds = self.position_seconds;
                    }
                }
                self.send_osd(format!("Seek {:.0}s", self.position_seconds));
                let seek_mode = match option {
                    RelativeSeekOption::Relative => "relative",
                    RelativeSeekOption::Exact => "relative+exact",
                    RelativeSeekOption::Auto => {
                        if !self.tried_using_exact_seek_for_current_file {
                            self.tried_using_exact_seek_for_current_file = true;
                            self.auto_seek_probe_pending = true;
                            self.auto_seek_probe_started_at = None;
                        }
                        if self.use_exact_seek_for_current_file {
                            "relative+exact"
                        } else {
                            "relative"
                        }
                    }
                };
                self.record_mpv_command(
                    "seek",
                    [format_mpv_number(seconds), seek_mode.to_string()],
                );
            }
            PlayerCommand::SeekAbsoluteExact { seconds } => {
                self.record_mpv_command(
                    "seek",
                    [format_mpv_number(seconds), "absolute+exact".to_string()],
                );
            }
            PlayerCommand::SelectChapter { index } => {
                let Some(chapter) = self.chapters.get(index).cloned() else {
                    return;
                };
                self.set_runtime_position(chapter.time_seconds);
                self.paused = false;
                self.record_mpv_command(
                    "seek",
                    [
                        format_mpv_number(chapter.time_seconds),
                        "absolute".to_string(),
                    ],
                );
                self.record_mpv_flag("pause", false);
                self.send_osd(format!("Chapter: {}", chapter.title));
            }
            PlayerCommand::SetVolume { volume } => {
                self.volume = volume.clamp(0.0, 200.0);
                self.record_mpv_double("volume", self.volume);
                self.send_osd(format!("Volume {:.0}%", self.volume));
            }
            PlayerCommand::SetSpeed { speed } => {
                self.speed = speed.clamp(0.01, 100.0);
                self.record_mpv_double("speed", self.speed);
                self.send_osd(format!("Speed {:.2}x", self.speed));
            }
            PlayerCommand::MultiplySpeed { factor } => {
                self.speed = (self.speed * factor).clamp(0.01, 100.0);
                self.record_mpv_double("speed", self.speed);
                self.send_osd(format!("Speed {:.2}x", self.speed));
            }
            PlayerCommand::ToggleMute => {
                self.muted = !self.muted;
                self.record_mpv_flag("mute", self.muted);
                self.send_osd(if self.muted { "Muted" } else { "Unmuted" });
            }
            PlayerCommand::CycleAbLoop => {
                self.record_mpv_command("ab-loop", std::iter::empty::<&str>());
                match self.ab_loop.status {
                    AbLoopStatus::Cleared => {
                        self.ab_loop.a_seconds =
                            self.position_seconds.max(MIN_AB_LOOP_POINT_SECONDS);
                        self.ab_loop.b_seconds = 0.0;
                        self.ab_loop.status = AbLoopStatus::ASet;
                        self.send_osd("A-B Loop: A");
                    }
                    AbLoopStatus::ASet => {
                        self.ab_loop.b_seconds =
                            self.position_seconds.max(MIN_AB_LOOP_POINT_SECONDS);
                        self.ab_loop.status = AbLoopStatus::BSet;
                        self.send_osd("A-B Loop: B");
                    }
                    AbLoopStatus::BSet => {
                        self.ab_loop.a_seconds = 0.0;
                        self.ab_loop.b_seconds = 0.0;
                        self.ab_loop.status = AbLoopStatus::Cleared;
                        self.send_osd("A-B Loop: Cleared");
                    }
                }
            }
            PlayerCommand::SetAbLoopPoint { point, seconds } => {
                let seconds = seconds.max(MIN_AB_LOOP_POINT_SECONDS);
                match point {
                    AbLoopPoint::A
                        if matches!(
                            self.ab_loop.status,
                            AbLoopStatus::ASet | AbLoopStatus::BSet
                        ) =>
                    {
                        self.ab_loop.a_seconds = seconds;
                        self.record_mpv_double("ab-loop-a", seconds);
                        self.send_osd("A-B Loop: A");
                    }
                    AbLoopPoint::B if self.ab_loop.status == AbLoopStatus::BSet => {
                        self.ab_loop.b_seconds = seconds;
                        self.record_mpv_double("ab-loop-b", seconds);
                        self.send_osd("A-B Loop: B");
                    }
                    _ => return,
                }
            }
            PlayerCommand::SelectAudioDevice { name } => {
                let Some(device) = self
                    .audio_devices
                    .iter()
                    .find(|device| device.name == name)
                    .cloned()
                else {
                    return;
                };
                self.audio_device = device.name.clone();
                self.record_mpv_string("audio-device", &device.name);
                self.send_osd(format!("Audio Device: {}", device.description));
            }
            PlayerCommand::AddFilter { kind, filter } => {
                let display_name = filter.clone();
                self.add_filter(kind, &filter, &display_name);
            }
            PlayerCommand::RemoveFilter { kind, index } => {
                self.remove_filter(kind, index);
            }
            PlayerCommand::ToggleSavedFilter { kind, name, filter } => {
                if let Some(index) = self
                    .filters(kind)
                    .iter()
                    .position(|active| active.matches_raw(&filter))
                {
                    self.remove_filter(kind, index);
                } else {
                    self.add_filter(kind, &filter, &name);
                }
            }
            PlayerCommand::ToggleFileLoop => {
                let mode = if self.loop_mode == LoopMode::File {
                    LoopMode::Off
                } else {
                    LoopMode::File
                };
                self.set_loop_mode(mode);
            }
            PlayerCommand::TogglePlaylistLoop => {
                let mode = if self.loop_mode == LoopMode::Playlist {
                    LoopMode::Off
                } else {
                    LoopMode::Playlist
                };
                self.set_loop_mode(mode);
            }
            PlayerCommand::FrameStep { backwards } => {
                let delta = if backwards { -1.0 / 30.0 } else { 1.0 / 30.0 };
                let upper_bound = if self.duration_seconds > 0.0 {
                    self.duration_seconds
                } else {
                    f64::MAX
                };
                self.position_seconds = (self.position_seconds + delta).clamp(0.0, upper_bound);
                self.paused = true;
                self.send_osd(if backwards {
                    "Frame Back Step"
                } else {
                    "Frame Step"
                });
                self.record_mpv_command(
                    if backwards {
                        "frame-back-step"
                    } else {
                        "frame-step"
                    },
                    std::iter::empty::<&str>(),
                );
            }
            PlayerCommand::PlaylistNext => {
                self.select_relative_playlist_item(1);
                self.record_mpv_command("playlist-next", std::iter::empty::<&str>());
            }
            PlayerCommand::PlaylistPrev => {
                self.select_relative_playlist_item(-1);
                self.record_mpv_command("playlist-prev", std::iter::empty::<&str>());
            }
            PlayerCommand::SelectPlaylistItem { index } => {
                if self.select_playlist_item(index) {
                    self.record_mpv_int("playlist-pos", index as i64);
                }
            }
            PlayerCommand::MovePlaylistItems {
                indexes,
                destination,
            } => {
                for (from, to) in self.move_playlist_items(indexes, destination) {
                    self.record_mpv_command("playlist-move", [from.to_string(), to.to_string()]);
                }
            }
            PlayerCommand::InsertPlaylistItems { paths, destination } => {
                self.insert_playlist_items(paths, destination);
            }
            PlayerCommand::PlayPlaylistItemsNext { indexes } => {
                for (from, to) in self.play_playlist_items_next(&indexes) {
                    self.record_mpv_command("playlist-move", [from.to_string(), to.to_string()]);
                }
            }
            PlayerCommand::RemovePlaylistItem { index } => {
                self.remove_playlist_items(&[index]);
            }
            PlayerCommand::RemovePlaylistItems { indexes } => {
                self.remove_playlist_items(&indexes);
            }
            PlayerCommand::ClearPlaylist => {
                self.clear_playlist_except_current();
                self.record_mpv_command("playlist-clear", std::iter::empty::<&str>());
                self.send_osd("Cleared Playlist");
            }
            PlayerCommand::CycleTrack { kind } => {
                if kind == TrackSelectionKind::SecondSubtitles {
                    let track_ids = self
                        .tracks
                        .subtitles
                        .iter()
                        .map(|track| track.id)
                        .collect::<Vec<_>>();
                    if track_ids.len() > 1 {
                        let current = track_ids
                            .iter()
                            .position(|id| *id == self.second_subtitle_id)
                            .unwrap_or_default();
                        self.second_subtitle_id = track_ids[(current + 1) % track_ids.len()];
                        self.record_mpv_int("secondary-sid", self.second_subtitle_id);
                    }
                } else {
                    let (property, track_kind) = track_selection_target(kind);
                    self.tracks.cycle(kind);
                    if let Some(track_id) = self.tracks.selected_id(track_kind) {
                        self.record_mpv_int(property, track_id);
                    }
                }
                self.send_osd("Track Switched");
            }
            PlayerCommand::SelectTrack { kind, id } => {
                if kind == TrackSelectionKind::SecondSubtitles {
                    let valid = id == 0 || self.tracks.subtitles.iter().any(|track| track.id == id);
                    if valid && self.second_subtitle_id != id {
                        self.second_subtitle_id = id;
                        self.record_mpv_int("secondary-sid", id);
                        self.send_osd("Track Switched");
                    }
                } else {
                    let (property, track_kind) = track_selection_target(kind);
                    if self.tracks.select_id(track_kind, id) {
                        self.record_mpv_int(property, id);
                        self.send_osd("Track Switched");
                    }
                }
            }
            PlayerCommand::SwapSubtitleTracks => {
                let primary_id = self.tracks.selected_id(TrackKind::Subtitles).unwrap_or(0);
                let secondary_id = self.second_subtitle_id;
                if primary_id != secondary_id
                    && self.tracks.select_id(TrackKind::Subtitles, secondary_id)
                {
                    self.second_subtitle_id = primary_id;
                    self.record_mpv_int("sid", secondary_id);
                    self.record_mpv_int("secondary-sid", primary_id);
                    self.send_osd("Track Switched");
                }
            }
            PlayerCommand::LoadExternalTrack { kind, path } => {
                let path = path.trim();
                if path.is_empty() {
                    return;
                }
                match kind {
                    ExternalTrackKind::Video => {
                        self.record_mpv_command("video-add", [path]);
                        self.send_osd("Loading External Video");
                    }
                    ExternalTrackKind::Audio => {
                        self.record_mpv_command("audio-add", [path]);
                        self.send_osd("Loading External Audio");
                    }
                    ExternalTrackKind::Subtitles => {
                        if let Some(track) =
                            self.tracks.subtitles.iter().find(|track| {
                                track.metadata.external_filename.as_deref() == Some(path)
                            })
                        {
                            self.record_mpv_command("sub-reload", [track.id.to_string()]);
                            self.send_osd("Reloading External Subtitle");
                        } else {
                            self.record_mpv_command("sub-add", [path]);
                            self.send_osd("Loading External Subtitle");
                        }
                    }
                }
            }
            PlayerCommand::SetDeinterlace { enabled } => {
                self.quick_settings.deinterlace = enabled;
                self.record_mpv_flag("deinterlace", enabled);
                self.send_osd(if enabled {
                    "Deinterlace On"
                } else {
                    "Deinterlace Off"
                });
            }
            PlayerCommand::SetHardwareDecoding { enabled, decoder } => {
                let hwdec = if enabled {
                    iina_hardware_decoder_value(decoder)
                } else {
                    "no"
                };
                self.quick_settings.hardware_decoding = hwdec != "no";
                self.record_mpv_string("hwdec", hwdec);
                self.send_osd(if self.quick_settings.hardware_decoding {
                    "Hardware Decoding On"
                } else {
                    "Hardware Decoding Off"
                });
            }
            PlayerCommand::SetHdrEnabled { enabled } => {
                self.quick_settings.hdr_enabled = enabled;
            }
            PlayerCommand::SetVideoAspect { aspect } => {
                if let Some(aspect) = parse_iina_aspect(&aspect) {
                    self.quick_settings.video_aspect = aspect;
                    let aspect = self.quick_settings.video_aspect.clone();
                    self.record_mpv_string("video-aspect", &aspect);
                } else {
                    self.quick_settings.video_aspect = "Default".to_string();
                    self.record_mpv_string("video-aspect", "-1");
                }
                let aspect = self.quick_settings.video_aspect.clone();
                self.send_osd(format!("Aspect Ratio: {aspect}"));
            }
            PlayerCommand::SetVideoCrop { crop } => {
                if let Some(crop) = parse_iina_aspect(&crop) {
                    if let Some((video_width, video_height)) = self.selected_video_dimensions() {
                        if let Some((crop_width, crop_height)) = iina_crop_dimensions(
                            video_width,
                            video_height,
                            aspect_ratio(&crop).expect("validated IINA aspect must have a ratio"),
                        ) {
                            let filter = format!("@iina_crop:crop={crop_width}:{crop_height}::");
                            self.record_mpv_command("vf", ["add", filter.as_str()]);
                            self.quick_settings.video_crop = crop;
                            self.quick_settings.custom_crop = None;
                            let crop = self.quick_settings.video_crop.clone();
                            self.send_osd(format!("Crop: {crop}"));
                        } else {
                            self.send_osd("Crop unavailable for current video");
                        }
                    } else {
                        self.send_osd("Crop unavailable for current video");
                    }
                } else {
                    self.quick_settings.video_crop = "None".to_string();
                    self.quick_settings.custom_crop = None;
                    self.record_mpv_command("vf", ["remove", "@iina_crop"]);
                    self.send_osd("Crop: None");
                }
            }
            PlayerCommand::SetCustomVideoCrop {
                x,
                y,
                width,
                height,
            } => {
                let Some((video_width, video_height)) = self.selected_video_dimensions() else {
                    self.send_osd("Crop unavailable for current video");
                    self.refresh_mpv_properties();
                    return;
                };
                if !iina_custom_crop_is_valid(x, y, width, height, video_width, video_height) {
                    self.send_osd("Crop unavailable for current video");
                } else if x == 0 && y == 0 && width == video_width && height == video_height {
                    self.quick_settings.video_crop = "None".to_string();
                    self.quick_settings.custom_crop = None;
                    self.record_mpv_command("vf", ["remove", "@iina_crop"]);
                    self.send_osd("Crop: None");
                } else {
                    let filter = format!("@iina_crop:crop={width}:{height}:{x}:{y}");
                    self.record_mpv_command("vf", ["add", filter.as_str()]);
                    self.quick_settings.video_crop.clear();
                    self.quick_settings.custom_crop = Some(CustomCrop {
                        x,
                        y,
                        width,
                        height,
                    });
                    self.send_osd(format!("Crop: ({x}, {y}) ({width}x{height})"));
                }
            }
            PlayerCommand::SetDelogoRegion {
                x,
                y,
                width,
                height,
            } => {
                let Some((video_width, video_height)) = self.selected_video_dimensions() else {
                    self.send_osd("Delogo unavailable for current video");
                    self.refresh_mpv_properties();
                    return;
                };
                if !iina_custom_crop_is_valid(x, y, width, height, video_width, video_height) {
                    self.send_osd("Delogo unavailable for current video");
                } else {
                    if let Some(index) = self
                        .video_filters
                        .iter()
                        .position(|filter| filter.label.as_deref() == Some("iina_delogo"))
                    {
                        self.remove_filter(FilterKind::Video, index);
                    }
                    self.add_filter(
                        FilterKind::Video,
                        &format!("@iina_delogo:lavfi=[delogo=x={x}:y={y}:w={width}:h={height}]"),
                        "Delogo",
                    );
                }
            }
            PlayerCommand::RemoveDelogo => {
                if let Some(index) = self
                    .video_filters
                    .iter()
                    .position(|filter| filter.label.as_deref() == Some("iina_delogo"))
                {
                    self.remove_filter(FilterKind::Video, index);
                }
            }
            PlayerCommand::SetVideoRotate { degrees } => {
                if [0, 90, 180, 270].contains(&degrees) {
                    self.quick_settings.video_rotate = degrees;
                    self.record_mpv_int("video-rotate", degrees);
                    self.send_osd(format!("Rotate {degrees}°"));
                }
            }
            PlayerCommand::SetVideoFlip { enabled } => {
                if self.quick_settings.video_flipped != enabled {
                    self.quick_settings.video_flipped = enabled;
                    if enabled {
                        self.record_mpv_command("vf", ["add", "@iina_flip:vflip"]);
                    } else {
                        self.record_mpv_command("vf", ["remove", "@iina_flip"]);
                    }
                    self.send_osd(if enabled {
                        "Vertical Flip"
                    } else {
                        "Vertical Flip Off"
                    });
                }
            }
            PlayerCommand::SetVideoMirror { enabled } => {
                if self.quick_settings.video_mirrored != enabled {
                    self.quick_settings.video_mirrored = enabled;
                    if enabled {
                        self.record_mpv_command("vf", ["add", "@iina_mirror:hflip"]);
                    } else {
                        self.record_mpv_command("vf", ["remove", "@iina_mirror"]);
                    }
                    self.send_osd(if enabled {
                        "Horizontal Mirror"
                    } else {
                        "Horizontal Mirror Off"
                    });
                }
            }
            PlayerCommand::SetVideoEqualizer { option, value } => {
                let value = value.clamp(-100, 100);
                let (property, label) = self.quick_settings.set_video_equalizer(option, value);
                self.record_mpv_int(property, value);
                self.send_osd(format!("{label}: {value:+}"));
            }
            PlayerCommand::SetAudioDelay { seconds } if seconds.is_finite() => {
                self.quick_settings.audio_delay = seconds;
                self.record_mpv_double("audio-delay", seconds);
                self.send_osd(format!("Audio Delay: {seconds:+.2}s"));
            }
            PlayerCommand::SetAudioEqualizer { gains } => {
                let gains = normalize_audio_eq_gains(&gains);
                let Some(channel_count) = self.selected_audio_channel_count() else {
                    self.send_osd("Audio equalizer unavailable for current audio");
                    self.refresh_mpv_properties();
                    return;
                };
                for filter in iina_audio_eq_filters(channel_count, &gains) {
                    self.record_mpv_command("af", ["add", filter.as_str()]);
                }
                self.quick_settings.audio_eq = gains;
                self.quick_settings.audio_eq_active = true;
            }
            PlayerCommand::ResetAudioEqualizer => {
                if self.quick_settings.audio_eq_active {
                    for index in 0..IINA_AUDIO_EQ_FREQUENCIES.len() {
                        let label = format!("@iina_aeq{index}");
                        self.record_mpv_command("af", ["remove", label.as_str()]);
                    }
                }
                self.quick_settings.audio_eq = [0.0; 10];
                self.quick_settings.audio_eq_active = false;
            }
            PlayerCommand::SetSubtitleStyleColor { target, color } => {
                let Some(color) = normalize_iina_subtitle_color(&color) else {
                    return;
                };
                match target {
                    SubtitleStyleColorTarget::Text => {
                        self.quick_settings.sub_text_color = color.clone();
                        self.record_mpv_string("options/sub-color", &color);
                    }
                    SubtitleStyleColorTarget::Border => {
                        self.quick_settings.sub_border_color = color.clone();
                        self.record_mpv_string("options/sub-border-color", &color);
                    }
                    SubtitleStyleColorTarget::Background => {
                        self.quick_settings.sub_background_color = color.clone();
                        self.record_mpv_string("options/sub-back-color", &color);
                    }
                }
            }
            PlayerCommand::SetSubtitleTextSize { size }
                if IINA_SUBTITLE_FONT_SIZES.contains(&size) =>
            {
                self.quick_settings.sub_text_size = size;
                self.record_mpv_double("options/sub-font-size", size);
            }
            PlayerCommand::SetSubtitleBorderSize { size }
                if IINA_SUBTITLE_BORDER_SIZES.contains(&size) =>
            {
                self.quick_settings.sub_border_size = size;
                self.record_mpv_double("options/sub-border-size", size);
            }
            PlayerCommand::SetSubtitleFont { font } if !font.contains('\0') => {
                self.quick_settings.sub_font = font.clone();
                self.record_mpv_string("options/sub-font", &font);
            }
            PlayerCommand::SetSubEncoding { encoding }
                if IINA_SUBTITLE_ENCODINGS
                    .iter()
                    .any(|(_, candidate)| *candidate == encoding) =>
            {
                self.quick_settings.sub_encoding = encoding.clone();
                self.record_mpv_string("sub-codepage", &encoding);
            }
            PlayerCommand::SetSubDelay { seconds } if seconds.is_finite() => {
                self.quick_settings.sub_delay = seconds;
                self.record_mpv_double("sub-delay", seconds);
                self.send_osd(format!("Subtitle Delay: {seconds:+.2}s"));
            }
            PlayerCommand::SetSubScale { scale } if scale.is_finite() => {
                let scale = scale.clamp(0.1, 10.0);
                self.quick_settings.sub_scale = scale;
                self.record_mpv_double("sub-scale", scale);
                self.send_osd(format!("Subtitle Scale: {scale:.2}"));
            }
            PlayerCommand::SetSubPosition { position } => {
                let position = position.clamp(0, 100);
                self.quick_settings.sub_pos = position;
                self.record_mpv_int("sub-pos", position);
                self.send_osd(format!("Subtitle Position: {position}"));
            }
            PlayerCommand::KeyBindingMpvCommand { action }
                if is_safe_key_binding_mpv_action(&action) =>
            {
                self.record_mpv_operation(mpv_command_string(action));
            }
            PlayerCommand::PluginMpvCommand { command, args }
                if is_safe_plugin_mpv_name(&command)
                    && args
                        .iter()
                        .all(|argument| is_safe_plugin_mpv_text(argument)) =>
            {
                self.record_mpv_command(&command, &args);
            }
            PlayerCommand::PluginMpvSet { property, value }
                if is_safe_plugin_mpv_name(&property) && is_safe_plugin_mpv_text(&value) =>
            {
                self.record_mpv_string(&property, &value);
            }
            PlayerCommand::PluginMpvSetNative { property, value }
                if is_safe_plugin_mpv_name(&property) && is_safe_plugin_mpv_value(&value, 0) =>
            {
                self.record_mpv_operation(set_plugin_property(property, value));
            }
            PlayerCommand::SetAudioDelay { .. }
            | PlayerCommand::SetSubtitleTextSize { .. }
            | PlayerCommand::SetSubtitleBorderSize { .. }
            | PlayerCommand::SetSubtitleFont { .. }
            | PlayerCommand::SetSubEncoding { .. }
            | PlayerCommand::SetSubDelay { .. }
            | PlayerCommand::SetSubScale { .. }
            | PlayerCommand::KeyBindingMpvCommand { .. }
            | PlayerCommand::PluginMpvCommand { .. }
            | PlayerCommand::PluginMpvSet { .. }
            | PlayerCommand::PluginMpvSetNative { .. } => {}
            PlayerCommand::ShowSidebar { tab } => {
                self.sidebar.visible = true;
                self.sidebar.tab = tab;
            }
            PlayerCommand::HideSidebar => {
                self.sidebar.visible = false;
            }
            PlayerCommand::ToggleOsc => {
                self.osc_visible = !self.osc_visible;
                self.clear_osd();
            }
            PlayerCommand::EnterMiniPlayer => {
                self.enter_mini_player(false);
            }
            PlayerCommand::LeaveMiniPlayer => {
                self.leave_mini_player(false);
            }
        }
        self.refresh_mpv_properties();
    }

    pub fn apply_mpv_events(&mut self, events: &[MpvClientEvent]) {
        self.apply_mpv_lifecycle_events(events);
        let properties = events
            .iter()
            .filter_map(|event| event.property.clone())
            .collect::<Vec<_>>();
        self.apply_mpv_property_changes(&properties);
        self.record_plugin_mpv_events(events);
    }

    fn record_plugin_mpv_events(&mut self, events: &[MpvClientEvent]) {
        for event in events {
            self.mpv_event_cursor = self.mpv_event_cursor.saturating_add(1);
            self.mpv_events.push(PlayerMpvEvent {
                cursor: self.mpv_event_cursor,
                event: event.clone(),
            });
        }
        if self.mpv_events.len() > MAX_PLUGIN_MPV_EVENT_LOG {
            let overflow = self.mpv_events.len() - MAX_PLUGIN_MPV_EVENT_LOG;
            self.mpv_events.drain(0..overflow);
        }
    }

    pub fn plugin_mpv_events_after(&self, cursor: u64) -> PlayerMpvEventBatch {
        let first_available_cursor = self
            .mpv_events
            .first()
            .map(|event| event.cursor)
            .unwrap_or_else(|| self.mpv_event_cursor.saturating_add(1));
        let dropped_event_count = first_available_cursor.saturating_sub(cursor.saturating_add(1));
        PlayerMpvEventBatch {
            cursor: self.mpv_event_cursor,
            dropped_event_count,
            current_url: self.current_url.clone(),
            events: self
                .mpv_events
                .iter()
                .filter(|event| event.cursor > cursor)
                .cloned()
                .collect(),
        }
    }

    pub fn set_hdr_status(&mut self, available: bool, enabled: bool) {
        self.quick_settings.hdr_available = available;
        self.quick_settings.hdr_enabled = enabled;
    }

    pub fn set_pip_active(&mut self, active: bool) {
        self.pip_active = active;
    }

    pub fn enter_mini_player(&mut self, automatically: bool) {
        if !automatically {
            self.mini_player_entered_manually = true;
        }
        self.mini_player_left_manually = false;
        self.mode = PlayerMode::MiniPlayer;
        self.send_osd("Music Mode");
        self.refresh_mpv_properties();
    }

    pub fn leave_mini_player(&mut self, automatically: bool) {
        if !automatically {
            self.mini_player_left_manually = true;
        }
        self.mini_player_entered_manually = true;
        self.mode = if self.current_url.is_some() {
            PlayerMode::Player
        } else {
            PlayerMode::Initial
        };
        self.clear_osd();
        self.refresh_mpv_properties();
    }

    pub fn reset_music_mode_switch_history(&mut self) {
        self.mini_player_entered_manually = false;
        self.mini_player_left_manually = false;
    }

    pub(crate) fn automatic_music_mode_transition(&self) -> Option<AutomaticMusicModeTransition> {
        let current_url = self.current_url.as_deref()?;
        let track_status_available = !self.tracks.video.is_empty() || !self.tracks.audio.is_empty();
        if !track_status_available {
            return None;
        }
        let is_audio = !current_url.contains("://")
            && (self.tracks.video.is_empty()
                || self
                    .tracks
                    .video
                    .iter()
                    .all(|track| track.metadata.albumart));

        match (&self.mode, is_audio) {
            (PlayerMode::Player, true) if !self.mini_player_left_manually => {
                Some(AutomaticMusicModeTransition::Enter)
            }
            (PlayerMode::MiniPlayer, false) if !self.mini_player_entered_manually => {
                Some(AutomaticMusicModeTransition::Leave)
            }
            _ => None,
        }
    }

    fn apply_mpv_lifecycle_events(&mut self, events: &[MpvClientEvent]) {
        let mut refresh_needed = false;
        let mut idle_from_mpv = false;

        for event in events {
            match event.name.as_str() {
                "start-file" => {
                    self.observe_start_file_for_window_resize();
                    self.pending_idle_reset = false;
                    self.mode = PlayerMode::Player;
                    self.paused = false;
                    self.begin_file_loading();
                    refresh_needed = true;
                }
                "file-loaded" => {
                    self.pending_idle_reset = false;
                    if self.current_url.is_some() {
                        self.mode = PlayerMode::Player;
                    }
                    self.paused = false;
                    self.file_loading = false;
                    self.pending_open_error = None;
                    self.tried_using_exact_seek_for_current_file = false;
                    self.auto_seek_probe_pending = false;
                    self.auto_seek_probe_started_at = None;
                    self.clear_osd();
                    refresh_needed = true;
                }
                "video-reconfig" => {
                    if self.window_resize_geometry_ready {
                        self.window_resize_video_reconfiguration_generation = self
                            .window_resize_video_reconfiguration_generation
                            .wrapping_add(1);
                    } else {
                        self.window_resize_geometry_ready = true;
                    }
                }
                "playback-restart" => {
                    if self.auto_seek_probe_pending {
                        if let Some(started_at) = self.auto_seek_probe_started_at.take() {
                            self.use_exact_seek_for_current_file =
                                started_at.elapsed().as_secs_f64() < 0.05;
                            self.auto_seek_probe_pending = false;
                        }
                    }
                    self.window_resize_geometry_ready = true;
                    if self.current_url.is_some() {
                        self.mode = PlayerMode::Player;
                    }
                    refresh_needed = true;
                }
                "seek" => {
                    self.mpv_properties.idle_active = false;
                    if self.auto_seek_probe_pending && self.auto_seek_probe_started_at.is_none() {
                        self.auto_seek_probe_started_at = Some(Instant::now());
                    }
                }
                "end-file" => {
                    let Some(end_file) = event.end_file.as_ref() else {
                        continue;
                    };
                    if end_file.reason == MpvEndFileReason::Redirect {
                        continue;
                    }
                    self.pending_idle_reset = true;
                    if self.file_loading {
                        if end_file.reason == MpvEndFileReason::Stop {
                            self.file_loading = false;
                            self.pending_open_error = None;
                            self.paused = true;
                            self.send_osd("Stopped");
                            refresh_needed = true;
                        } else {
                            self.pending_open_error = Some(PlaybackError {
                                code: end_file.error,
                                message: end_file
                                    .error_message
                                    .clone()
                                    .unwrap_or_else(|| "Cannot open file or stream!".to_string()),
                            });
                        }
                        continue;
                    }
                    self.paused = true;
                    match end_file.reason {
                        MpvEndFileReason::Error => self.send_osd("Playback Error"),
                        MpvEndFileReason::Stop | MpvEndFileReason::Quit => self.send_osd("Stopped"),
                        _ => self.clear_osd(),
                    }
                    refresh_needed = true;
                }
                "idle" => {
                    self.paused = true;
                    idle_from_mpv = true;
                    self.finalize_idle_transition();
                    refresh_needed = true;
                }
                _ => {}
            }
        }

        if refresh_needed {
            self.refresh_mpv_properties();
        }
        if idle_from_mpv {
            self.mpv_properties.idle_active = true;
            self.mpv_properties.pause = true;
        }
    }

    pub fn apply_mpv_property_changes(&mut self, properties: &[MpvPropertyChange]) {
        let mut refresh_needed = false;
        let mut loop_state_changed = false;
        let mut ab_loop_state_changed = false;
        let mut chapter_from_mpv = None;
        let mut chapters_from_mpv = None;
        let mut idle_active_from_mpv = None;
        let mut percent_pos_from_mpv = None;
        let mut playlist_count_from_mpv = None;
        let mut playlist_pos_from_mpv = None;
        let mut track_list_count_from_mpv = None;
        let music_snapshot_is_authoritative =
            properties.iter().any(|property| property.name == "chapter")
                && properties
                    .iter()
                    .any(|property| property.name == "chapters");
        let mut metadata_artist_from_mpv = None;
        let mut chapter_title_from_mpv = None;
        let mut chapter_performer_from_mpv = None;

        if music_snapshot_is_authoritative {
            self.music_album.clear();
            self.music_artist.clear();
        }

        for property in properties {
            match property.name.as_str() {
                "path" => {
                    if let Some(path) = property.value.as_deref().filter(|value| !value.is_empty())
                    {
                        self.apply_runtime_path(path);
                        refresh_needed = true;
                    }
                }
                "pause" => {
                    if let Some(value) = property.value.as_deref().and_then(parse_mpv_bool) {
                        self.paused = value;
                        refresh_needed = true;
                    }
                }
                "volume" => {
                    if let Some(value) = property.value.as_deref().and_then(parse_mpv_f64) {
                        self.volume = value.clamp(0.0, 200.0);
                        refresh_needed = true;
                    }
                }
                "speed" => {
                    if let Some(value) = property.value.as_deref().and_then(parse_mpv_f64) {
                        self.speed = value.clamp(0.01, 100.0);
                        refresh_needed = true;
                    }
                }
                "mute" => {
                    if let Some(value) = property.value.as_deref().and_then(parse_mpv_bool) {
                        self.muted = value;
                        refresh_needed = true;
                    }
                }
                "loop-file" => {
                    if let Some(value) = property.value.as_deref() {
                        let active = mpv_loop_value_is_active(value, false);
                        if self.runtime_loop_file_active != active {
                            self.runtime_loop_file_active = active;
                            loop_state_changed = true;
                        }
                    }
                }
                "loop-playlist" => {
                    if let Some(value) = property.value.as_deref() {
                        let active = mpv_loop_value_is_active(value, true);
                        if self.runtime_loop_playlist_active != active {
                            self.runtime_loop_playlist_active = active;
                            loop_state_changed = true;
                        }
                    }
                }
                "ab-loop-a" => {
                    if let Some(value) = property.value.as_deref().and_then(parse_mpv_f64) {
                        let value = value.max(0.0);
                        if self.ab_loop.a_seconds != value {
                            self.ab_loop.a_seconds = value;
                            ab_loop_state_changed = true;
                        }
                    }
                }
                "ab-loop-b" => {
                    if let Some(value) = property.value.as_deref().and_then(parse_mpv_f64) {
                        let value = value.max(0.0);
                        if self.ab_loop.b_seconds != value {
                            self.ab_loop.b_seconds = value;
                            ab_loop_state_changed = true;
                        }
                    }
                }
                "ab-loop-count" => {
                    if let Some(value) = property.value.as_deref() {
                        if self.ab_loop.count != value {
                            self.ab_loop.count = value.to_string();
                            ab_loop_state_changed = true;
                        }
                    }
                }
                "duration" => {
                    if let Some(value) = property.value.as_deref().and_then(parse_mpv_f64) {
                        self.duration_seconds = value.max(0.0);
                        self.update_current_playlist_duration();
                        refresh_needed = true;
                    }
                }
                "time-pos" => {
                    if let Some(value) = property.value.as_deref().and_then(parse_mpv_f64) {
                        self.set_runtime_position(value);
                        refresh_needed = true;
                    }
                }
                "percent-pos" => {
                    percent_pos_from_mpv = property.value.as_deref().and_then(parse_mpv_f64);
                }
                "media-title" => {
                    if let Some(title) = property.value.as_deref().filter(|value| !value.is_empty())
                    {
                        self.apply_runtime_title(title);
                        refresh_needed = true;
                    }
                }
                "metadata/by-key/album" => {
                    self.music_album = property.value.clone().unwrap_or_default();
                }
                "metadata/by-key/artist" => {
                    let artist = property.value.clone().unwrap_or_default();
                    metadata_artist_from_mpv = Some(artist.clone());
                    if !music_snapshot_is_authoritative {
                        self.music_artist = artist;
                    }
                }
                "chapter-metadata/by-key/title" => {
                    chapter_title_from_mpv = property.value.clone();
                }
                "chapter-metadata/by-key/performer" => {
                    let artist = property.value.clone().unwrap_or_default();
                    chapter_performer_from_mpv = Some(artist.clone());
                    if !music_snapshot_is_authoritative && !artist.is_empty() {
                        self.music_artist = artist;
                    }
                }
                "vid" => {
                    if let Some(id) = property.value.as_deref().and_then(parse_mpv_i64) {
                        if self.tracks.select_id(TrackKind::Video, id) {
                            refresh_needed = true;
                        }
                    }
                }
                "aid" => {
                    if let Some(id) = property.value.as_deref().and_then(parse_mpv_i64) {
                        if self.tracks.select_id(TrackKind::Audio, id) {
                            refresh_needed = true;
                        }
                    }
                }
                "sid" => {
                    if let Some(id) = property.value.as_deref().and_then(parse_mpv_i64) {
                        if self.tracks.select_id(TrackKind::Subtitles, id) {
                            refresh_needed = true;
                        }
                    }
                }
                "secondary-sid" => {
                    if let Some(id) = property.value.as_deref().and_then(parse_mpv_i64) {
                        let id = id.max(0);
                        if self.second_subtitle_id != id {
                            self.second_subtitle_id = id;
                            refresh_needed = true;
                        }
                    }
                }
                "deinterlace" => {
                    if let Some(value) = property.value.as_deref().and_then(parse_mpv_bool) {
                        self.quick_settings.deinterlace = value;
                        refresh_needed = true;
                    }
                }
                "hwdec" => {
                    if let Some(value) = property.value.as_deref() {
                        self.quick_settings.hardware_decoding = value != "no";
                        refresh_needed = true;
                    }
                }
                "video-rotate" => {
                    if let Some(value) = property.value.as_deref().and_then(parse_mpv_i64) {
                        self.quick_settings.video_rotate = value;
                        refresh_needed = true;
                    }
                }
                "brightness" => {
                    refresh_needed |= self.quick_settings.set_runtime_equalizer(
                        VideoEqualizer::Brightness,
                        property.value.as_deref().and_then(parse_mpv_i64),
                    )
                }
                "contrast" => {
                    refresh_needed |= self.quick_settings.set_runtime_equalizer(
                        VideoEqualizer::Contrast,
                        property.value.as_deref().and_then(parse_mpv_i64),
                    )
                }
                "saturation" => {
                    refresh_needed |= self.quick_settings.set_runtime_equalizer(
                        VideoEqualizer::Saturation,
                        property.value.as_deref().and_then(parse_mpv_i64),
                    )
                }
                "gamma" => {
                    refresh_needed |= self.quick_settings.set_runtime_equalizer(
                        VideoEqualizer::Gamma,
                        property.value.as_deref().and_then(parse_mpv_i64),
                    )
                }
                "hue" => {
                    refresh_needed |= self.quick_settings.set_runtime_equalizer(
                        VideoEqualizer::Hue,
                        property.value.as_deref().and_then(parse_mpv_i64),
                    )
                }
                "audio-delay" => {
                    if let Some(value) = property.value.as_deref().and_then(parse_mpv_f64) {
                        self.quick_settings.audio_delay = value;
                        refresh_needed = true;
                    }
                }
                "audio-device" => {
                    if let Some(value) = property.value.as_deref().filter(|value| !value.is_empty())
                    {
                        if self.audio_device != value {
                            self.audio_device = value.to_string();
                            refresh_needed = true;
                        }
                    }
                }
                "sub-delay" => {
                    if let Some(value) = property.value.as_deref().and_then(parse_mpv_f64) {
                        self.quick_settings.sub_delay = value;
                        refresh_needed = true;
                    }
                }
                "sub-codepage" => {
                    if let Some(value) = property.value.as_deref().filter(|value| {
                        IINA_SUBTITLE_ENCODINGS
                            .iter()
                            .any(|(_, candidate)| candidate == value)
                    }) {
                        self.quick_settings.sub_encoding = value.to_string();
                        refresh_needed = true;
                    }
                }
                "sub-scale" => {
                    if let Some(value) = property.value.as_deref().and_then(parse_mpv_f64) {
                        self.quick_settings.sub_scale = value.clamp(0.1, 10.0);
                        refresh_needed = true;
                    }
                }
                "sub-pos" => {
                    if let Some(value) = property.value.as_deref().and_then(parse_mpv_i64) {
                        self.quick_settings.sub_pos = value.clamp(0, 100);
                        refresh_needed = true;
                    }
                }
                "chapter" => {
                    chapter_from_mpv = property.value.as_deref().and_then(parse_mpv_i64);
                }
                "chapters" => {
                    chapters_from_mpv = property.value.as_deref().and_then(parse_mpv_i64);
                }
                "playlist-count" => {
                    playlist_count_from_mpv = property.value.as_deref().and_then(parse_mpv_i64);
                }
                "playlist-pos" => {
                    playlist_pos_from_mpv = property.value.as_deref().and_then(parse_mpv_i64);
                }
                "track-list/count" => {
                    track_list_count_from_mpv = property.value.as_deref().and_then(parse_mpv_i64);
                }
                "idle-active" => {
                    idle_active_from_mpv = property.value.as_deref().and_then(parse_mpv_bool);
                }
                _ => {}
            }
        }

        if loop_state_changed {
            let loop_mode = if self.runtime_loop_file_active {
                LoopMode::File
            } else if self.runtime_loop_playlist_active {
                LoopMode::Playlist
            } else {
                LoopMode::Off
            };
            if self.loop_mode != loop_mode {
                self.loop_mode = loop_mode;
                refresh_needed = true;
            }
        }
        if ab_loop_state_changed {
            self.sync_ab_loop_status();
            refresh_needed = true;
        }

        if music_snapshot_is_authoritative {
            let chapter_count = chapters_from_mpv
                .unwrap_or(self.mpv_properties.chapters as i64)
                .max(0) as usize;
            let chapter_index = chapter_from_mpv
                .unwrap_or(self.mpv_properties.chapter)
                .max(0) as usize;
            let chapter_title = self
                .chapters
                .iter()
                .find(|chapter| chapter.index == chapter_index)
                .or_else(|| self.chapters.get(chapter_index))
                .map(|chapter| chapter.title.as_str())
                .filter(|title| !title.is_empty())
                .or_else(|| {
                    chapter_title_from_mpv
                        .as_deref()
                        .filter(|title| !title.is_empty())
                });

            self.music_title = if chapter_count > 0 {
                chapter_title.unwrap_or(&self.media_title).to_string()
            } else {
                self.media_title.clone()
            };
            self.music_artist = if chapter_count > 0 {
                chapter_performer_from_mpv
                    .as_deref()
                    .filter(|artist| !artist.is_empty())
                    .or_else(|| {
                        metadata_artist_from_mpv
                            .as_deref()
                            .filter(|artist| !artist.is_empty())
                    })
                    .unwrap_or_default()
                    .to_string()
            } else {
                metadata_artist_from_mpv.unwrap_or_default()
            };
        } else if let Some(title) = chapter_title_from_mpv.filter(|title| !title.is_empty()) {
            self.music_title = title;
        }

        if refresh_needed {
            self.refresh_mpv_properties();
        }
        if let Some(chapter) = chapter_from_mpv {
            self.mpv_properties.chapter = chapter.max(0);
        }
        if let Some(chapters) = chapters_from_mpv {
            self.mpv_properties.chapters = chapters.max(0) as usize;
        }
        if let Some(percent_pos) = percent_pos_from_mpv {
            self.mpv_properties.percent_pos = percent_pos.clamp(0.0, 100.0);
        }
        if let Some(playlist_count) = playlist_count_from_mpv {
            self.mpv_properties.playlist_count = playlist_count.max(0) as usize;
        }
        if let Some(playlist_pos) = playlist_pos_from_mpv {
            self.mpv_properties.playlist_pos = playlist_pos.max(-1);
        }
        if let Some(track_list_count) = track_list_count_from_mpv {
            self.mpv_properties.track_list_count = track_list_count.max(0) as usize;
        }
        if let Some(idle_active) = idle_active_from_mpv {
            self.mpv_properties.idle_active = idle_active;
            if idle_active {
                self.paused = true;
                self.mpv_properties.pause = true;
                if self.finalize_pending_open_error() {
                    self.refresh_mpv_properties();
                } else if self.finalize_idle_transition() {
                    self.refresh_mpv_properties();
                }
            }
        }
    }

    pub fn apply_mpv_track_list(&mut self, track_list: &[MpvTrackListItem]) {
        if track_list.is_empty() {
            return;
        }
        self.tracks = TrackGroups::from_mpv_track_list(track_list);
        if self.second_subtitle_id != 0
            && !self
                .tracks
                .subtitles
                .iter()
                .any(|track| track.id == self.second_subtitle_id)
        {
            self.second_subtitle_id = 0;
        }
        self.refresh_mpv_properties();
    }

    pub fn apply_mpv_audio_devices(&mut self, devices: &[MpvAudioDevice]) {
        if devices.is_empty() {
            return;
        }
        self.audio_devices = devices
            .iter()
            .map(|device| AudioDevice {
                name: device.name.clone(),
                description: device.description.clone(),
            })
            .collect();
    }

    pub fn apply_mpv_filters(&mut self, video: &[MpvFilter], audio: &[MpvFilter]) {
        self.video_filters = video.to_vec();
        self.audio_filters = audio.to_vec();
    }

    pub fn apply_mpv_playlist(&mut self, playlist: &[MpvPlaylistItem]) {
        if playlist.is_empty() {
            return;
        }

        let previous_playlist = self.playlist.clone();
        self.playlist = playlist
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let current = item.current || item.playing;
                let previous_duration = previous_playlist
                    .iter()
                    .find(|previous| previous.path == item.filename)
                    .and_then(|previous| previous.duration_seconds);
                PlaylistItem {
                    id: index + 1,
                    mpv_id: item.id,
                    path: item.filename.clone(),
                    title: item
                        .title
                        .as_ref()
                        .filter(|title| !title.is_empty())
                        .cloned()
                        .unwrap_or_else(|| title_from_path(&item.filename)),
                    duration_seconds: if current && self.duration_seconds > 0.0 {
                        Some(self.duration_seconds)
                    } else {
                        previous_duration
                    },
                    current,
                    playing: item.playing,
                }
            })
            .collect();

        if !self.playlist.iter().any(|item| item.current) && self.mpv_properties.playlist_pos >= 0 {
            if let Some(item) = self
                .playlist
                .get_mut(self.mpv_properties.playlist_pos.max(0) as usize)
            {
                item.current = true;
            }
        }

        if let Some(current_item) = self.playlist.iter().find(|item| item.current).cloned() {
            let changed_media = self.current_url.as_deref() != Some(current_item.path.as_str());
            self.mode = PlayerMode::Player;
            self.current_url = Some(current_item.path.clone());
            self.media_title = current_item.title.clone();
            if changed_media {
                self.reset_music_metadata(&current_item.title);
            }
            if self.last_playback.as_ref().map(|item| item.path.as_str())
                != Some(current_item.path.as_str())
            {
                self.last_playback = Some(LastPlayback {
                    path: current_item.path.clone(),
                    title: current_item.title.clone(),
                    position_seconds: self.position_seconds,
                });
            }
        }

        self.refresh_mpv_properties();
    }

    fn refresh_mpv_properties(&mut self) {
        self.mpv_properties = MpvPropertySnapshot::from_player(self);
    }

    fn apply_runtime_path(&mut self, path: &str) {
        let path_changed = self.current_url.as_deref() != Some(path);
        if path_changed {
            self.current_url = Some(path.to_string());
            self.mode = PlayerMode::Player;
            self.position_seconds = 0.0;
            self.media_title = self
                .playlist
                .iter()
                .find(|item| item.path == path)
                .map(|item| item.title.clone())
                .unwrap_or_else(|| title_from_path(path));
            let title = self.media_title.clone();
            self.reset_music_metadata(&title);
        }

        if self.playlist.is_empty() {
            self.playlist.push(PlaylistItem {
                id: 1,
                mpv_id: None,
                path: path.to_string(),
                title: self.media_title.clone(),
                duration_seconds: (self.duration_seconds > 0.0).then_some(self.duration_seconds),
                current: true,
                playing: true,
            });
        } else {
            let mut matched = false;
            for item in &mut self.playlist {
                let current = item.path == path;
                matched |= current;
                item.current = current;
                item.playing = current;
            }
            if !matched {
                let id = self.playlist.len() + 1;
                self.playlist.push(PlaylistItem {
                    id,
                    mpv_id: None,
                    path: path.to_string(),
                    title: title_from_path(path),
                    duration_seconds: (self.duration_seconds > 0.0)
                        .then_some(self.duration_seconds),
                    current: true,
                    playing: true,
                });
                if let Some(item) = self.playlist.iter_mut().find(|item| item.id != id) {
                    item.current = false;
                    item.playing = false;
                }
            }
        }

        if self.last_playback.as_ref().map(|item| item.path.as_str()) != Some(path) {
            self.last_playback = Some(LastPlayback {
                path: path.to_string(),
                title: self.media_title.clone(),
                position_seconds: self.position_seconds,
            });
        }
    }

    fn apply_runtime_title(&mut self, title: &str) {
        self.media_title = title.to_string();
        if self.mpv_properties.chapters == 0 {
            self.music_title = title.to_string();
        }
        if let Some(last_playback) = self.last_playback.as_mut() {
            last_playback.title = self.media_title.clone();
        }
        if let Some(current_item) = self.playlist.iter_mut().find(|item| item.current) {
            current_item.title = self.media_title.clone();
        }
    }

    fn reset_music_metadata(&mut self, title: &str) {
        self.music_title = title.to_string();
        self.music_album.clear();
        self.music_artist.clear();
    }

    fn set_runtime_position(&mut self, seconds: f64) {
        let upper_bound = if self.duration_seconds > 0.0 {
            self.duration_seconds
        } else {
            f64::MAX
        };
        self.position_seconds = seconds.max(0.0).min(upper_bound);
        if let (Some(current_url), Some(last_playback)) =
            (self.current_url.as_deref(), self.last_playback.as_mut())
        {
            if last_playback.path == current_url {
                last_playback.position_seconds = self.position_seconds;
            }
        }
    }

    fn update_current_playlist_duration(&mut self) {
        if let Some(current_item) = self.playlist.iter_mut().find(|item| item.current) {
            current_item.duration_seconds =
                (self.duration_seconds > 0.0).then_some(self.duration_seconds);
        }
    }

    fn record_mpv_command(
        &mut self,
        command: &str,
        args: impl IntoIterator<Item = impl AsRef<str>>,
    ) {
        self.record_mpv_operation(mpv_command(
            command,
            args.into_iter()
                .map(|arg| arg.as_ref().to_string())
                .collect::<Vec<_>>(),
        ));
    }

    fn record_mpv_flag(&mut self, name: &str, value: bool) {
        self.record_mpv_operation(set_property(
            name,
            MpvFormat::Flag,
            if value { "true" } else { "false" },
        ));
    }

    fn record_mpv_int(&mut self, name: &str, value: i64) {
        self.record_mpv_operation(set_property(name, MpvFormat::Int64, value.to_string()));
    }

    fn record_mpv_double(&mut self, name: &str, value: f64) {
        self.record_mpv_operation(set_property(
            name,
            MpvFormat::Double,
            format_mpv_number(value),
        ));
    }

    fn record_mpv_string(&mut self, name: &str, value: &str) {
        self.record_mpv_operation(set_property(name, MpvFormat::String, value));
    }

    fn record_mpv_operation(&mut self, operation: MpvClientOperation) {
        if self.mpv_operation_log.is_empty() {
            self.mpv_operation_log_first_sequence = self.mpv_operation_log_next_sequence;
        }
        self.mpv_operation_log.push(operation);
        self.mpv_operation_log_next_sequence += 1;
        if self.mpv_operation_log.len() > MAX_MPV_OPERATION_LOG {
            let overflow = self.mpv_operation_log.len() - MAX_MPV_OPERATION_LOG;
            self.mpv_operation_log.drain(0..overflow);
            self.mpv_operation_log_first_sequence += overflow as u64;
        }
    }

    pub(crate) fn record_preference_operations(
        &mut self,
        operations: impl IntoIterator<Item = MpvClientOperation>,
    ) {
        for operation in operations {
            self.record_mpv_operation(operation);
        }
    }

    fn select_relative_playlist_item(&mut self, delta: isize) {
        let len = self.playlist.len();
        if len <= 1 {
            return;
        }

        let current = self
            .playlist
            .iter()
            .position(|item| item.current)
            .unwrap_or_default();
        let next = (current as isize + delta).rem_euclid(len as isize) as usize;
        self.select_playlist_item(next);
    }

    fn select_playlist_item(&mut self, selected_index: usize) -> bool {
        if selected_index >= self.playlist.len() {
            return false;
        }

        for (index, item) in self.playlist.iter_mut().enumerate() {
            item.current = index == selected_index;
            item.playing = index == selected_index;
        }

        self.window_resize_expects_start_file = false;
        self.begin_file_loading();
        if let Some(item) = self.playlist.get(selected_index).cloned() {
            self.current_url = Some(item.path.clone());
            self.media_title = item.title.clone();
            self.reset_music_metadata(&item.title);
            self.duration_seconds = item.duration_seconds.unwrap_or_default();
            self.position_seconds = 0.0;
            self.paused = false;
            self.send_osd(format!("Opening {}", item.title));
            self.last_playback = Some(LastPlayback {
                path: item.path.clone(),
                title: item.title.clone(),
                position_seconds: 0.0,
            });
        }
        true
    }

    fn remove_playlist_item(&mut self, index: usize) -> bool {
        if index >= self.playlist.len() {
            return false;
        }
        let removed_current = self.playlist[index].current || self.playlist[index].playing;
        self.playlist.remove(index);
        for (index, item) in self.playlist.iter_mut().enumerate() {
            item.id = index + 1;
        }

        if self.playlist.is_empty() {
            self.current_url = None;
            self.file_loading = false;
            self.pending_open_error = None;
            self.media_title = "IINA".to_string();
            self.reset_music_metadata("IINA");
            self.duration_seconds = 0.0;
            self.position_seconds = 0.0;
            self.paused = true;
            self.mode = PlayerMode::Initial;
            self.refresh_mpv_properties();
            return true;
        }

        if removed_current
            || !self
                .playlist
                .iter()
                .any(|item| item.current || item.playing)
        {
            let next_index = index.min(self.playlist.len() - 1);
            self.select_playlist_item(next_index);
        } else {
            self.refresh_mpv_properties();
        }
        true
    }

    fn begin_file_loading(&mut self) {
        self.file_loading = true;
        self.playback_error = None;
        self.pending_open_error = None;
        self.pending_idle_reset = false;
    }

    fn begin_manually_opened_file_for_window_resize(&mut self) {
        self.window_resize_file_generation = self.window_resize_file_generation.wrapping_add(1);
        self.window_resize_manually_opened_generation = Some(self.window_resize_file_generation);
        self.window_resize_expects_start_file = true;
        self.window_resize_geometry_ready = false;
    }

    fn observe_start_file_for_window_resize(&mut self) {
        if self.window_resize_expects_start_file {
            self.window_resize_expects_start_file = false;
            self.window_resize_geometry_ready = false;
            return;
        }
        self.window_resize_file_generation = self.window_resize_file_generation.wrapping_add(1);
        self.window_resize_manually_opened_generation = None;
        self.window_resize_geometry_ready = false;
    }

    fn finalize_idle_transition(&mut self) -> bool {
        if !self.pending_idle_reset || self.pending_open_error.is_some() {
            return false;
        }
        self.pending_idle_reset = false;
        self.mode = PlayerMode::Initial;
        self.current_url = None;
        self.file_loading = false;
        self.playback_error = None;
        self.media_title = "IINA".to_string();
        self.reset_music_metadata("IINA");
        self.media_info = None;
        self.duration_seconds = 0.0;
        self.position_seconds = 0.0;
        self.speed = 1.0;
        self.paused = true;
        self.osc_visible = true;
        self.playlist.clear();
        self.chapters.clear();
        self.tracks = TrackGroups::default();
        self.second_subtitle_id = 0;
        self.ab_loop = AbLoopState::default();
        self.send_osd("Stopped");
        true
    }

    fn finalize_pending_open_error(&mut self) -> bool {
        let Some(error) = self.pending_open_error.take() else {
            return false;
        };
        if !self.file_loading {
            return false;
        }
        self.file_loading = false;
        self.pending_idle_reset = false;
        self.playback_error = Some(error);
        self.mode = PlayerMode::Initial;
        self.current_url = None;
        self.media_title = "IINA".to_string();
        self.reset_music_metadata("IINA");
        self.media_info = None;
        self.duration_seconds = 0.0;
        self.position_seconds = 0.0;
        self.paused = true;
        self.playlist.clear();
        self.chapters.clear();
        self.tracks = TrackGroups::default();
        self.second_subtitle_id = 0;
        self.send_osd("Playback Error");
        true
    }

    fn move_playlist_items(
        &mut self,
        mut indexes: Vec<usize>,
        destination: usize,
    ) -> Vec<(usize, usize)> {
        let playlist_len = self.playlist.len();
        indexes.sort_unstable();
        indexes.dedup();
        indexes.retain(|index| *index < playlist_len);
        if indexes.is_empty() {
            return Vec::new();
        }

        let destination = destination.min(playlist_len);
        let selected = indexes
            .iter()
            .map(|index| self.playlist[*index].clone())
            .collect::<Vec<_>>();
        let selected_indexes = indexes
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        let mut reordered = self
            .playlist
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                (!selected_indexes.contains(&index)).then_some(item.clone())
            })
            .collect::<Vec<_>>();
        let adjusted_destination = destination
            .saturating_sub(indexes.iter().filter(|index| **index < destination).count())
            .min(reordered.len());
        reordered.splice(adjusted_destination..adjusted_destination, selected);
        if reordered.iter().zip(&self.playlist).all(|(left, right)| {
            left.id == right.id && left.path == right.path && left.mpv_id == right.mpv_id
        }) {
            return Vec::new();
        }

        let mut operations = Vec::with_capacity(indexes.len());
        let mut old_index_offset = 0isize;
        let mut new_index_offset = 0usize;
        for old_index in indexes {
            if old_index < destination {
                operations.push((
                    (old_index as isize + old_index_offset) as usize,
                    destination,
                ));
                old_index_offset -= 1;
            } else {
                operations.push((old_index, destination + new_index_offset));
                new_index_offset += 1;
            }
        }

        self.playlist = reordered;
        for (index, item) in self.playlist.iter_mut().enumerate() {
            item.id = index + 1;
        }
        self.refresh_mpv_properties();
        operations
    }

    fn play_playlist_items_next(&mut self, indexes: &[usize]) -> Vec<(usize, usize)> {
        let Some(current_index) = self
            .playlist
            .iter()
            .position(|item| item.current || item.playing)
        else {
            return Vec::new();
        };
        let moves =
            crate::playlist_actions::play_next_moves(indexes, current_index, self.playlist.len());
        for operation in &moves {
            let item = self.playlist.remove(operation.from);
            let destination = if operation.from < operation.to {
                operation.to.saturating_sub(1)
            } else {
                operation.to
            }
            .min(self.playlist.len());
            self.playlist.insert(destination, item);
        }
        if !moves.is_empty() {
            for (index, item) in self.playlist.iter_mut().enumerate() {
                item.id = index + 1;
            }
            self.refresh_mpv_properties();
        }
        moves
            .into_iter()
            .map(|operation| (operation.from, operation.to))
            .collect()
    }

    fn insert_playlist_items(&mut self, paths: Vec<String>, destination: usize) {
        if paths.is_empty() {
            return;
        }
        if self.playlist.is_empty() {
            self.open_media_batch_internal(
                paths,
                Err("Playlist insertion did not probe the first item".to_string()),
                false,
                false,
            );
            return;
        }

        let previous_count = self.playlist.len();
        let destination = destination.min(previous_count);
        let moves =
            crate::playlist_actions::insertion_moves(previous_count, paths.len(), destination);
        for path in &paths {
            self.record_mpv_command("loadfile", [path.as_str(), "append"]);
        }
        for operation in &moves {
            self.record_mpv_command(
                "playlist-move",
                [operation.from.to_string(), operation.to.to_string()],
            );
        }

        let added_count = paths.len();
        let items = paths.into_iter().map(|path| PlaylistItem {
            id: 0,
            mpv_id: None,
            title: title_from_path(&path),
            path,
            duration_seconds: None,
            current: false,
            playing: false,
        });
        self.playlist.splice(destination..destination, items);
        for (index, item) in self.playlist.iter_mut().enumerate() {
            item.id = index + 1;
        }
        self.send_osd(format!("Added {added_count} Files to Playlist"));
        self.refresh_mpv_properties();
    }

    fn remove_playlist_items(&mut self, indexes: &[usize]) {
        for command_index in
            crate::playlist_actions::removal_command_indexes(indexes, self.playlist.len())
        {
            if self.remove_playlist_item(command_index) {
                self.record_mpv_command("playlist-remove", [command_index.to_string()]);
            }
        }
    }

    fn clear_playlist_except_current(&mut self) {
        let Some(current_index) = self
            .playlist
            .iter()
            .position(|item| item.current || item.playing)
        else {
            self.playlist.clear();
            self.refresh_mpv_properties();
            return;
        };

        let mut current = self.playlist[current_index].clone();
        current.id = 1;
        current.current = true;
        current.playing = true;
        self.playlist = vec![current];
        self.refresh_mpv_properties();
    }
}

fn is_safe_plugin_mpv_text(value: &str) -> bool {
    !value.contains('\0') && value.len() <= 8192
}

fn is_safe_key_binding_mpv_action(value: &str) -> bool {
    !value.trim().is_empty() && is_safe_plugin_mpv_text(value)
}

fn is_safe_plugin_mpv_name(value: &str) -> bool {
    !value.trim().is_empty() && is_safe_plugin_mpv_text(value)
}

fn is_safe_plugin_mpv_value(value: &MpvPluginValue, depth: usize) -> bool {
    if depth > 32 {
        return false;
    }
    match value {
        MpvPluginValue::Null | MpvPluginValue::Flag(_) => true,
        MpvPluginValue::Int64(value) => value.parse::<i64>().is_ok(),
        MpvPluginValue::Double(value) => {
            matches!(
                value.as_str(),
                "NaN" | "nan" | "Infinity" | "+Infinity" | "-Infinity" | "inf" | "+inf" | "-inf"
            ) || value.parse::<f64>().is_ok()
        }
        MpvPluginValue::String(value) => is_safe_plugin_mpv_text(value),
        MpvPluginValue::Array(values) => {
            values.len() <= 1_000_000
                && values
                    .iter()
                    .all(|value| is_safe_plugin_mpv_value(value, depth + 1))
        }
        MpvPluginValue::Map(values) => {
            values.len() <= 1_000_000
                && values.iter().all(|(key, value)| {
                    is_safe_plugin_mpv_text(key) && is_safe_plugin_mpv_value(value, depth + 1)
                })
        }
        MpvPluginValue::ByteArray(values) => values.len() <= 1_000_000,
    }
}

fn title_from_path(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn format_mpv_number(value: f64) -> String {
    let mut formatted = format!("{value:.6}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

fn parse_mpv_bool(value: &str) -> Option<bool> {
    match value {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    }
}

fn parse_mpv_i64(value: &str) -> Option<i64> {
    value.parse::<i64>().ok()
}

fn parse_mpv_f64(value: &str) -> Option<f64> {
    value.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn mpv_loop_value_is_active(value: &str, playlist: bool) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    normalized == "inf"
        || (playlist && normalized == "force")
        || normalized
            .parse::<i64>()
            .is_ok_and(|iterations| iterations != 0)
}

fn iina_hardware_decoder_value(decoder: i64) -> &'static str {
    match decoder {
        0 => "no",
        2 => "auto-copy",
        _ => "auto",
    }
}

fn normalize_audio_eq_gains(gains: &[f64]) -> [f64; 10] {
    let mut normalized = [0.0; 10];
    for (index, gain) in gains.iter().take(normalized.len()).enumerate() {
        if gain.is_finite() {
            normalized[index] = gain.clamp(-12.0, 12.0);
        }
    }
    normalized
}

fn iina_audio_eq_filters(channel_count: i64, gains: &[f64; 10]) -> Vec<String> {
    let channel_count = usize::try_from(channel_count).unwrap_or_default();
    IINA_AUDIO_EQ_FREQUENCIES
        .iter()
        .enumerate()
        .map(|(index, frequency)| {
            let channels = (0..channel_count)
                .map(|channel| {
                    format!(
                        "c{channel} f={} w={} g={}",
                        format_iina_audio_eq_number(*frequency),
                        format_iina_audio_eq_number(*frequency / 1.224_744_871),
                        format_iina_audio_eq_number(gains[index]),
                    )
                })
                .collect::<Vec<_>>()
                .join("|");
            format!("@iina_aeq{index}:lavfi=[anequalizer={channels}]")
        })
        .collect()
}

fn format_iina_audio_eq_number(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

fn normalize_iina_subtitle_color(value: &str) -> Option<String> {
    let mut components = value
        .split('/')
        .map(str::trim)
        .map(|component| component.parse::<f64>().ok())
        .collect::<Option<Vec<_>>>()?;
    if !(components.len() == 3 || components.len() == 4)
        || components
            .iter()
            .any(|component| !component.is_finite() || !(0.0..=1.0).contains(component))
    {
        return None;
    }
    if components.len() == 3 {
        components.push(1.0);
    }
    Some(
        components
            .into_iter()
            .map(format_mpv_number)
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn parse_iina_aspect(value: &str) -> Option<String> {
    let value = value.trim();
    let (width, height) = value.split_once(':')?;
    if value.matches(':').count() != 1
        || !is_iina_aspect_component(width)
        || !is_iina_aspect_component(height)
    {
        return None;
    }
    let ratio = aspect_ratio(value)?;
    (ratio.is_finite() && ratio > 0.0).then(|| value.to_string())
}

fn is_iina_aspect_component(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_digit() {
        return false;
    }
    let mut seen_decimal = false;
    let mut decimal_digits = 0usize;
    for character in chars {
        if character.is_ascii_digit() {
            if seen_decimal {
                decimal_digits += 1;
            }
        } else if character == '.' && !seen_decimal {
            seen_decimal = true;
        } else {
            return false;
        }
    }
    !seen_decimal || decimal_digits > 0
}

fn aspect_ratio(value: &str) -> Option<f64> {
    let (width, height) = value.split_once(':')?;
    let width = width.parse::<f64>().ok()?;
    let height = height.parse::<f64>().ok()?;
    (width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0)
        .then_some(width / height)
}

fn iina_crop_dimensions(width: i64, height: i64, aspect: f64) -> Option<(i64, i64)> {
    if width <= 0 || height <= 0 || !aspect.is_finite() || aspect <= 0.0 {
        return None;
    }
    let (crop_width, crop_height) = if width as f64 / height as f64 > aspect {
        (height as f64 * aspect, height as f64)
    } else {
        (width as f64, width as f64 / aspect)
    };
    let crop_width = crop_width as i64;
    let crop_height = crop_height as i64;
    (crop_width > 0 && crop_height > 0).then_some((crop_width, crop_height))
}

fn iina_custom_crop_is_valid(
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    video_width: i64,
    video_height: i64,
) -> bool {
    x >= 0
        && y >= 0
        && width > 0
        && height > 0
        && x.checked_add(width)
            .is_some_and(|right| right <= video_width)
        && y.checked_add(height)
            .is_some_and(|bottom| bottom <= video_height)
}

impl Default for TrackGroups {
    fn default() -> Self {
        Self {
            video: vec![Track {
                id: 1,
                title: "Default Video Track".to_string(),
                selected: true,
                metadata: TrackMetadata::default(),
            }],
            audio: vec![Track {
                id: 1,
                title: "Default Audio Track".to_string(),
                selected: true,
                metadata: TrackMetadata::default(),
            }],
            subtitles: vec![Track {
                id: 0,
                title: "None".to_string(),
                selected: true,
                metadata: TrackMetadata::default(),
            }],
        }
    }
}

impl Default for MpvPropertySnapshot {
    fn default() -> Self {
        Self {
            path: None,
            media_title: "IINA".to_string(),
            duration: 0.0,
            time_pos: 0.0,
            percent_pos: 0.0,
            pause: true,
            volume: 100.0,
            speed: 1.0,
            mute: false,
            chapter: 0,
            chapters: 0,
            playlist_count: 0,
            playlist_pos: -1,
            track_list_count: TrackGroups::default().track_count(),
            vid: 1,
            aid: 1,
            sid: 0,
            secondary_sid: 0,
            idle_active: true,
        }
    }
}

impl MpvPropertySnapshot {
    fn from_player(player: &PlayerState) -> Self {
        let duration = player.duration_seconds.max(0.0);
        let time_pos = player
            .position_seconds
            .clamp(0.0, duration.max(player.position_seconds));
        let percent_pos = if duration > 0.0 {
            (time_pos / duration * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        };
        Self {
            path: player.current_url.clone(),
            media_title: player.media_title.clone(),
            duration,
            time_pos,
            percent_pos,
            pause: player.paused,
            volume: player.volume,
            speed: player.speed,
            mute: player.muted,
            chapter: current_chapter_index(player),
            chapters: player.chapters.len(),
            playlist_count: player.playlist.len(),
            playlist_pos: player
                .playlist
                .iter()
                .position(|item| item.current || item.playing)
                .map(|index| index as i64)
                .unwrap_or(-1),
            track_list_count: player.tracks.track_count(),
            vid: player.tracks.selected_id(TrackKind::Video).unwrap_or(0),
            aid: player.tracks.selected_id(TrackKind::Audio).unwrap_or(0),
            sid: player.tracks.selected_id(TrackKind::Subtitles).unwrap_or(0),
            secondary_sid: player.second_subtitle_id,
            idle_active: matches!(player.mode, PlayerMode::Initial),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TrackKind {
    Video,
    Audio,
    Subtitles,
}

fn track_selection_target(kind: TrackSelectionKind) -> (&'static str, TrackKind) {
    match kind {
        TrackSelectionKind::Video => ("vid", TrackKind::Video),
        TrackSelectionKind::Audio => ("aid", TrackKind::Audio),
        TrackSelectionKind::Subtitles => ("sid", TrackKind::Subtitles),
        TrackSelectionKind::SecondSubtitles => ("secondary-sid", TrackKind::Subtitles),
    }
}

impl TrackGroups {
    fn from_mpv_track_list(track_list: &[MpvTrackListItem]) -> Self {
        let mut video = Vec::new();
        let mut audio = Vec::new();
        let mut subtitles = vec![Track {
            id: 0,
            title: "None".to_string(),
            selected: !track_list
                .iter()
                .any(|track| track.track_type == "sub" && track.selected),
            metadata: TrackMetadata::default(),
        }];

        for item in track_list {
            let track = Track::from_mpv_track(item);
            match item.track_type.as_str() {
                "video" => video.push(track),
                "audio" => audio.push(track),
                "sub" => subtitles.push(track),
                _ => {}
            }
        }

        if !video.iter().any(|track| track.selected) {
            if let Some(first) = video.first_mut() {
                first.selected = true;
            }
        }
        if !audio.iter().any(|track| track.selected) {
            if let Some(first) = audio.first_mut() {
                first.selected = true;
            }
        }

        Self {
            video: if video.is_empty() {
                TrackGroups::default().video
            } else {
                video
            },
            audio: if audio.is_empty() {
                TrackGroups::default().audio
            } else {
                audio
            },
            subtitles,
        }
    }

    fn from_probe(probe: &MediaProbe) -> Self {
        let mut video = Vec::new();
        let mut audio = Vec::new();
        let mut subtitles = vec![Track {
            id: 0,
            title: "None".to_string(),
            selected: true,
            metadata: TrackMetadata::default(),
        }];

        for stream in &probe.streams {
            let track = Track {
                id: stream.index,
                title: stream.display_title(),
                selected: false,
                metadata: TrackMetadata {
                    source_title: stream.title.clone(),
                    language: stream.language.clone(),
                    codec: stream.codec_name.clone(),
                    decoder_description: stream.codec_long_name.clone(),
                    demux_width: stream.width.and_then(|value| i64::try_from(value).ok()),
                    demux_height: stream.height.and_then(|value| i64::try_from(value).ok()),
                    demux_channel_count: stream
                        .channels
                        .and_then(|value| i64::try_from(value).ok()),
                    demux_samplerate: stream
                        .sample_rate
                        .and_then(|value| i64::try_from(value).ok()),
                    ..TrackMetadata::default()
                },
            };

            match stream.codec_type.as_str() {
                "video" => video.push(track),
                "audio" => audio.push(track),
                "subtitle" => subtitles.push(track),
                _ => {}
            }
        }

        if let Some(first) = video.first_mut() {
            first.selected = true;
        }
        if let Some(first) = audio.first_mut() {
            first.selected = true;
        }

        Self {
            video: if video.is_empty() {
                TrackGroups::default().video
            } else {
                video
            },
            audio: if audio.is_empty() {
                TrackGroups::default().audio
            } else {
                audio
            },
            subtitles,
        }
    }

    fn track_count(&self) -> usize {
        self.video.len() + self.audio.len() + self.subtitles.len()
    }

    fn selected_id(&self, kind: TrackKind) -> Option<i64> {
        let tracks = match kind {
            TrackKind::Video => &self.video,
            TrackKind::Audio => &self.audio,
            TrackKind::Subtitles => &self.subtitles,
        };
        tracks
            .iter()
            .find(|track| track.selected)
            .map(|track| track.id)
    }

    fn cycle(&mut self, kind: TrackSelectionKind) {
        let tracks = match kind {
            TrackSelectionKind::Video => &mut self.video,
            TrackSelectionKind::Audio => &mut self.audio,
            TrackSelectionKind::Subtitles => &mut self.subtitles,
            TrackSelectionKind::SecondSubtitles => return,
        };

        if tracks.len() <= 1 {
            return;
        }

        let current = tracks
            .iter()
            .position(|track| track.selected)
            .unwrap_or_default();
        let next = (current + 1) % tracks.len();
        for (index, track) in tracks.iter_mut().enumerate() {
            track.selected = index == next;
        }
    }

    fn select_id(&mut self, kind: TrackKind, id: i64) -> bool {
        let tracks = match kind {
            TrackKind::Video => &mut self.video,
            TrackKind::Audio => &mut self.audio,
            TrackKind::Subtitles => &mut self.subtitles,
        };
        if id == 0 && !tracks.iter().any(|track| track.id == 0) {
            let changed = tracks.iter().any(|track| track.selected);
            for track in tracks {
                track.selected = false;
            }
            return changed;
        }
        if !tracks.iter().any(|track| track.id == id) {
            return false;
        }
        let mut changed = false;
        for track in tracks {
            let selected = track.id == id;
            changed |= track.selected != selected;
            track.selected = selected;
        }
        changed
    }
}

impl Track {
    fn from_mpv_track(item: &MpvTrackListItem) -> Self {
        let metadata = TrackMetadata {
            source_id: item.src_id,
            source_title: item.title.clone(),
            language: item.lang.clone(),
            image: item.image,
            albumart: item.albumart,
            default_track: item.default_track,
            forced: item.forced,
            codec: item.codec.clone(),
            external: item.external,
            external_filename: item.external_filename.clone(),
            main_selection: item.main_selection,
            ff_index: item.ff_index,
            decoder_description: item.decoder_desc.clone(),
            demux_width: item.demux_w,
            demux_height: item.demux_h,
            demux_channel_count: item.demux_channel_count,
            demux_channels: item.demux_channels.clone(),
            demux_samplerate: item.demux_samplerate,
            demux_fps: item.demux_fps,
            demux_bitrate: item.demux_bitrate,
            demux_rotation: item.demux_rotation,
            demux_par: item.demux_par.clone(),
            audio_channels: item.audio_channels.clone(),
        };
        Self {
            id: item.id,
            title: runtime_track_title(item, &metadata),
            selected: item.selected,
            metadata,
        }
    }
}

fn runtime_track_title(item: &MpvTrackListItem, metadata: &TrackMetadata) -> String {
    let mut parts = Vec::new();
    if let Some(language) = metadata
        .language
        .as_deref()
        .filter(|language| !language.is_empty() && *language != "und")
    {
        parts.push(format!("[{language}]"));
    }
    if let Some(title) = item.title.as_deref().filter(|title| !title.is_empty()) {
        parts.push(title.to_string());
    }

    let decoder = metadata
        .decoder_description
        .as_deref()
        .and_then(|value| value.split('(').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(metadata.codec.as_deref());
    if let Some(decoder) = decoder {
        parts.push(decoder.replace(' ', ""));
    }

    match item.track_type.as_str() {
        "video" => {
            if let (Some(width), Some(height)) = (metadata.demux_width, metadata.demux_height) {
                parts.push(format!("{width}x{height}"));
            }
            if let Some(fps) = metadata.demux_fps {
                parts.push(format!("{}fps", format_track_number(fps)));
            }
        }
        "audio" => {
            if let Some(channel_count) = metadata.demux_channel_count {
                parts.push(format!("{channel_count}ch"));
            }
            if let Some(sample_rate) = metadata.demux_samplerate {
                parts.push(format!(
                    "{}kHz",
                    format_track_number(sample_rate as f64 / 1000.0)
                ));
            }
        }
        _ => {}
    }

    if metadata.default_track {
        parts.push("(Default)".to_string());
    }
    if metadata.forced {
        parts.push("(Forced)".to_string());
    }
    if metadata.external {
        parts.push("(External)".to_string());
    }

    let detail = parts.join(" ");
    if detail.is_empty() {
        format!("#{}", item.id)
    } else {
        format!("#{} {detail}", item.id)
    }
}

fn format_track_number(value: f64) -> String {
    let mut formatted = format!("{value:.3}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

fn current_chapter_index(player: &PlayerState) -> i64 {
    player
        .chapters
        .iter()
        .rev()
        .find(|chapter| chapter.time_seconds <= player.position_seconds)
        .map(|chapter| chapter.index as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{MediaChapterProbe, MediaStreamProbe};
    use crate::mpv::{MpvEndFileEvent, MpvPlaylistItem, MpvStartFileEvent, MpvTrackListItem};

    fn probed_media() -> MediaProbe {
        MediaProbe {
            path: "/tmp/current.mp4".to_string(),
            title: Some("Probed Title".to_string()),
            album: None,
            artist: None,
            duration_seconds: Some(120.0),
            format_name: Some("mov,mp4,m4a,3gp,3g2,mj2".to_string()),
            format_long_name: Some("QuickTime / MOV".to_string()),
            bit_rate: Some(800_000),
            streams: vec![
                MediaStreamProbe {
                    index: 10,
                    codec_type: "video".to_string(),
                    codec_name: Some("h264".to_string()),
                    codec_long_name: None,
                    language: None,
                    title: None,
                    width: Some(1920),
                    height: Some(1080),
                    channels: None,
                    sample_rate: None,
                },
                MediaStreamProbe {
                    index: 11,
                    codec_type: "audio".to_string(),
                    codec_name: Some("aac".to_string()),
                    codec_long_name: None,
                    language: Some("eng".to_string()),
                    title: Some("Stereo".to_string()),
                    width: None,
                    height: None,
                    channels: Some(2),
                    sample_rate: Some(48_000),
                },
                MediaStreamProbe {
                    index: 12,
                    codec_type: "subtitle".to_string(),
                    codec_name: Some("ass".to_string()),
                    codec_long_name: None,
                    language: Some("eng".to_string()),
                    title: Some("English".to_string()),
                    width: None,
                    height: None,
                    channels: None,
                    sample_rate: None,
                },
            ],
            chapters: vec![
                MediaChapterProbe {
                    index: 0,
                    title: "Opening".to_string(),
                    start_time_seconds: 0.0,
                },
                MediaChapterProbe {
                    index: 1,
                    title: "Middle".to_string(),
                    start_time_seconds: 30.0,
                },
            ],
        }
    }

    #[test]
    fn repeated_osd_messages_have_distinct_event_ids() {
        let mut player = PlayerState::default();

        player.apply(PlayerCommand::Pause);
        let first_id = player.osd_message_id;
        player.apply(PlayerCommand::Pause);

        assert_eq!(player.osd_message.as_deref(), Some("Paused"));
        assert_eq!(first_id, 1);
        assert_eq!(player.osd_message_id, 2);
    }

    fn mpv_property_event(name: &str, format: MpvFormat, value: Option<&str>) -> MpvClientEvent {
        MpvClientEvent {
            event_id: 22,
            name: "property-change".to_string(),
            error: 0,
            reply_userdata: 0,
            property: Some(MpvPropertyChange {
                name: name.to_string(),
                format,
                value: value.map(ToString::to_string),
            }),
            start_file: None,
            end_file: None,
            hook: None,
        }
    }

    fn mpv_lifecycle_event(event_id: i32, name: &str) -> MpvClientEvent {
        MpvClientEvent {
            event_id,
            name: name.to_string(),
            error: 0,
            reply_userdata: 0,
            property: None,
            start_file: None,
            end_file: None,
            hook: None,
        }
    }

    fn mpv_start_file_event(playlist_entry_id: i64) -> MpvClientEvent {
        MpvClientEvent {
            start_file: Some(MpvStartFileEvent { playlist_entry_id }),
            ..mpv_lifecycle_event(6, "start-file")
        }
    }

    fn mpv_end_file_event(reason: MpvEndFileReason) -> MpvClientEvent {
        MpvClientEvent {
            end_file: Some(MpvEndFileEvent {
                reason,
                reason_code: match reason {
                    MpvEndFileReason::Eof => 0,
                    MpvEndFileReason::Stop => 2,
                    MpvEndFileReason::Quit => 3,
                    MpvEndFileReason::Error => 4,
                    MpvEndFileReason::Redirect => 5,
                    MpvEndFileReason::Unknown => 99,
                },
                error: if reason == MpvEndFileReason::Error {
                    -13
                } else {
                    0
                },
                error_message: (reason == MpvEndFileReason::Error)
                    .then(|| "error loading file".to_string()),
                playlist_entry_id: 1,
                playlist_insert_id: 0,
                playlist_insert_num_entries: 0,
            }),
            ..mpv_lifecycle_event(7, "end-file")
        }
    }

    #[test]
    fn plugin_mpv_event_log_preserves_repeated_native_events_with_an_independent_cursor() {
        let mut player = PlayerState::default();
        let seek = mpv_lifecycle_event(20, "seek");

        player.apply_mpv_events(&[seek.clone(), seek]);

        assert_eq!(player.mpv_event_cursor, 2);
        assert_eq!(player.mpv_events.len(), 2);
        assert_eq!(player.mpv_events[0].cursor, 1);
        assert_eq!(player.mpv_events[1].cursor, 2);
        assert_eq!(player.mpv_events[0].event.name, "seek");
        assert_eq!(player.mpv_events[1].event.name, "seek");
        assert_eq!(player.mpv_operation_log_next_sequence(), 0);

        let batch = player.plugin_mpv_events_after(1);
        assert_eq!(batch.cursor, 2);
        assert_eq!(batch.dropped_event_count, 0);
        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0].cursor, 2);
    }

    #[test]
    fn plugin_mpv_event_log_is_bounded_and_reports_cursor_gaps() {
        let mut player = PlayerState::default();
        let events = (0..(MAX_PLUGIN_MPV_EVENT_LOG + 3))
            .map(|_| mpv_lifecycle_event(21, "playback-restart"))
            .collect::<Vec<_>>();

        player.apply_mpv_events(&events);

        assert_eq!(player.mpv_events.len(), MAX_PLUGIN_MPV_EVENT_LOG);
        let batch = player.plugin_mpv_events_after(0);
        assert_eq!(batch.dropped_event_count, 3);
        assert_eq!(batch.events.first().map(|event| event.cursor), Some(4));
        assert_eq!(batch.events.last().map(|event| event.cursor), Some(515));
    }

    fn mpv_property_change(
        name: &str,
        format: MpvFormat,
        value: Option<&str>,
    ) -> MpvPropertyChange {
        MpvPropertyChange {
            name: name.to_string(),
            format,
            value: value.map(ToString::to_string),
        }
    }

    fn mpv_track(index: usize, id: i64, track_type: &str, selected: bool) -> MpvTrackListItem {
        MpvTrackListItem {
            index,
            id,
            track_type: track_type.to_string(),
            src_id: Some(id + 1000),
            title: None,
            lang: None,
            image: false,
            albumart: false,
            default_track: false,
            forced: false,
            codec: None,
            external: false,
            external_filename: None,
            selected,
            main_selection: selected,
            ff_index: Some(index as i64),
            decoder_desc: None,
            demux_w: None,
            demux_h: None,
            demux_channel_count: None,
            demux_channels: None,
            demux_samplerate: None,
            demux_fps: None,
            demux_bitrate: None,
            demux_rotation: None,
            demux_par: None,
            audio_channels: None,
        }
    }

    fn mpv_playlist_item(
        index: usize,
        id: i64,
        filename: &str,
        current: bool,
        playing: bool,
        title: Option<&str>,
    ) -> MpvPlaylistItem {
        MpvPlaylistItem {
            index,
            id: Some(id),
            filename: filename.to_string(),
            current,
            playing,
            title: title.map(ToString::to_string),
        }
    }

    #[test]
    fn open_media_batch_sets_first_item_current_and_keeps_playlist() {
        let mut player = PlayerState::default();

        player.open_media_batch(
            vec![
                "/tmp/current.mp4".to_string(),
                "/tmp/queued.mkv".to_string(),
            ],
            Err("probe failed".to_string()),
        );

        assert!(matches!(player.mode, PlayerMode::Player));
        assert_eq!(player.current_url.as_deref(), Some("/tmp/current.mp4"));
        assert_eq!(player.playlist.len(), 2);
        assert!(player.playlist[0].current);
        assert_eq!(player.playlist[0].title, "current.mp4");
        assert!(!player.playlist[1].current);
        assert_eq!(player.playlist[1].title, "queued.mkv");
        assert_eq!(player.recent_documents.len(), 1);
        assert_eq!(player.recent_documents[0].path, "/tmp/current.mp4");
        assert_eq!(
            player.last_playback.as_ref().map(|item| item.path.as_str()),
            Some("/tmp/current.mp4")
        );
    }

    #[test]
    fn open_media_populates_mpv_property_snapshot() {
        let mut player = PlayerState::default();

        player.open_media_batch(
            vec![
                "/tmp/current.mp4".to_string(),
                "/tmp/queued.mkv".to_string(),
            ],
            Ok(probed_media()),
        );

        assert_eq!(
            player.mpv_properties.path.as_deref(),
            Some("/tmp/current.mp4")
        );
        assert_eq!(player.mpv_properties.media_title, "Probed Title");
        assert_eq!(player.mpv_properties.duration, 120.0);
        assert_eq!(player.mpv_properties.time_pos, 0.0);
        assert_eq!(player.mpv_properties.percent_pos, 0.0);
        assert!(!player.mpv_properties.pause);
        assert_eq!(player.mpv_properties.playlist_count, 2);
        assert_eq!(player.mpv_properties.playlist_pos, 0);
        assert_eq!(player.mpv_properties.track_list_count, 4);
        assert_eq!(player.mpv_properties.vid, 10);
        assert_eq!(player.mpv_properties.aid, 11);
        assert_eq!(player.mpv_properties.sid, 0);
        assert_eq!(player.mpv_properties.chapters, 2);
        assert!(!player.mpv_properties.idle_active);
    }

    #[test]
    fn open_media_batch_records_iina_loadfile_operations() {
        let mut player = PlayerState::default();

        player.open_media_batch(
            vec![
                "/tmp/current.mp4".to_string(),
                "/tmp/queued.mkv".to_string(),
            ],
            Ok(probed_media()),
        );

        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("loadfile", ["/tmp/current.mp4", "replace"]),
                mpv_command("loadfile", ["/tmp/queued.mkv", "append"]),
            ]
        );

        player.enqueue_media(vec!["/tmp/third.mov".to_string()]);

        assert_eq!(
            player.mpv_operation_log.last(),
            Some(&mpv_command("loadfile", ["/tmp/third.mov", "append"]))
        );
    }

    #[test]
    fn command_line_shuffle_runs_once_after_the_complete_playlist_is_appended() {
        let mut player = PlayerState::default();
        player.arm_command_line_shuffle_once();

        player.open_media_batch(
            vec![
                "/tmp/current.mp4".to_string(),
                "/tmp/queued.mkv".to_string(),
            ],
            Ok(probed_media()),
        );

        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("loadfile", ["/tmp/current.mp4", "replace"]),
                mpv_command("loadfile", ["/tmp/queued.mkv", "append"]),
                mpv_command("playlist-shuffle", std::iter::empty::<&str>()),
                mpv_command("playlist-play-index", ["0"]),
            ]
        );

        player.open_media_batch(vec!["/tmp/later.mp4".to_string()], Ok(probed_media()));
        assert_eq!(
            player.mpv_operation_log.last(),
            Some(&mpv_command("loadfile", ["/tmp/later.mp4", "replace"]))
        );
        assert_eq!(
            player
                .mpv_operation_log
                .iter()
                .filter(|operation| {
                    matches!(operation, MpvClientOperation::Command { command, .. } if command == "playlist-shuffle")
                })
                .count(),
            1
        );
    }

    #[test]
    fn configured_media_open_synchronizes_persistent_mpv_pause_state() {
        let mut player = PlayerState::default();

        player.open_media_with_pause("/tmp/paused.mp4".to_string(), Ok(probed_media()), true);

        assert!(player.paused);
        assert!(player.mpv_properties.pause);
        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("loadfile", ["/tmp/paused.mp4", "replace"]),
                set_property("pause", MpvFormat::Flag, "true"),
            ]
        );

        player.mpv_operation_log.clear();
        player.open_media_with_pause("/tmp/playing.mp4".to_string(), Ok(probed_media()), false);

        assert!(!player.paused);
        assert!(!player.mpv_properties.pause);
        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("loadfile", ["/tmp/playing.mp4", "replace"]),
                set_property("pause", MpvFormat::Flag, "false"),
            ]
        );
    }

    #[test]
    fn enqueue_media_appends_without_changing_current_item() {
        let mut player = PlayerState::default();
        player.open_media(
            "/tmp/current.mp4".to_string(),
            Err("probe failed".to_string()),
        );

        player.enqueue_media(vec!["/tmp/queued.mkv".to_string()]);

        assert_eq!(player.current_url.as_deref(), Some("/tmp/current.mp4"));
        assert_eq!(player.playlist.len(), 2);
        assert!(player.playlist[0].current);
        assert!(!player.playlist[1].current);
        assert_eq!(player.playlist[1].title, "queued.mkv");
        assert_eq!(
            player.osd_message.as_deref(),
            Some("Added 1 Files to Playlist")
        );
        assert_eq!(player.recent_documents.len(), 1);
        assert_eq!(player.recent_documents[0].path, "/tmp/current.mp4");
    }

    #[test]
    fn recent_documents_deduplicate_reorder_and_cap_items() {
        let mut player = PlayerState::default();

        for index in 0..12 {
            player.open_media(
                format!("/tmp/media-{index}.mp4"),
                Err("probe failed".to_string()),
            );
        }
        player.open_media(
            "/tmp/media-3.mp4".to_string(),
            Err("probe failed".to_string()),
        );

        assert_eq!(player.recent_documents.len(), MAX_RECENT_DOCUMENTS);
        assert_eq!(player.recent_documents[0].path, "/tmp/media-3.mp4");
        assert_eq!(player.recent_documents[0].id, 1);
        assert_eq!(player.recent_documents[9].id, 10);
        assert_eq!(
            player
                .recent_documents
                .iter()
                .filter(|item| item.path == "/tmp/media-3.mp4")
                .count(),
            1
        );
    }

    #[test]
    fn stop_waits_for_mpv_idle_before_returning_to_initial_mode() {
        let mut player = PlayerState::default();
        player.open_media(
            "/tmp/current.mp4".to_string(),
            Err("probe failed".to_string()),
        );

        player.apply(PlayerCommand::Stop);

        assert!(matches!(player.mode, PlayerMode::Player));
        assert_eq!(player.playlist.len(), 1);
        assert_eq!(
            player.mpv_operation_log.last(),
            Some(&mpv_command("stop", std::iter::empty::<&str>()))
        );

        player.apply_mpv_property_changes(&[mpv_property_change(
            "idle-active",
            MpvFormat::Flag,
            Some("true"),
        )]);

        assert!(matches!(player.mode, PlayerMode::Initial));
        assert!(player.playlist.is_empty());
        assert_eq!(player.recent_documents.len(), 1);
        assert_eq!(player.recent_documents[0].path, "/tmp/current.mp4");
        assert_eq!(
            player.last_playback.as_ref().map(|item| item.path.as_str()),
            Some("/tmp/current.mp4")
        );
    }

    #[test]
    fn seek_updates_last_playback_position_for_resume() {
        let mut player = PlayerState::default();
        player.open_media(
            "/tmp/current.mp4".to_string(),
            Err("probe failed".to_string()),
        );

        player.apply(PlayerCommand::Seek { seconds: 42.0 });

        assert_eq!(
            player
                .last_playback
                .as_ref()
                .map(|item| item.position_seconds),
            Some(42.0)
        );
    }

    #[test]
    fn relative_seek_preserves_iina_seek_modes_and_updates_position() {
        let mut player = PlayerState::default();
        player.open_media(
            "/tmp/current.mp4".to_string(),
            Err("probe failed".to_string()),
        );
        player.position_seconds = 20.0;
        player.mpv_operation_log.clear();
        player.mpv_operation_log_first_sequence = player.mpv_operation_log_next_sequence;

        player.apply(PlayerCommand::SeekRelative {
            seconds: -2.0,
            option: RelativeSeekOption::Relative,
        });
        player.apply(PlayerCommand::SeekRelative {
            seconds: 0.25,
            option: RelativeSeekOption::Exact,
        });
        player.apply(PlayerCommand::SeekRelative {
            seconds: 0.5,
            option: RelativeSeekOption::Auto,
        });

        assert_eq!(player.position_seconds, 18.75);
        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("seek", ["-2", "relative"]),
                mpv_command("seek", ["0.25", "relative+exact"]),
                mpv_command("seek", ["0.5", "relative+exact"]),
            ]
        );
    }

    #[test]
    fn timeline_percent_seek_preserves_forced_exact_and_half_open_eof_semantics() {
        let mut player = PlayerState::default();
        player.open_media(
            "/tmp/current.mp4".to_string(),
            Err("probe failed".to_string()),
        );
        player.duration_seconds = 120.0;
        player.mpv_operation_log.clear();
        player.mpv_operation_log_first_sequence = player.mpv_operation_log_next_sequence;

        player.apply(PlayerCommand::SeekPercent {
            percent: 25.0,
            exact: false,
        });
        player.apply(PlayerCommand::SeekPercent {
            percent: 100.0,
            exact: true,
        });

        assert!(player.position_seconds < player.duration_seconds);
        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("seek", ["25", "absolute-percent"]),
                mpv_command("seek", ["99.99999999999999", "absolute-percent+exact"]),
            ]
        );
    }

    #[test]
    fn automatic_relative_seek_probes_first_seek_latency_per_file() {
        let mut player = PlayerState::default();
        player.open_media(
            "/tmp/current.mp4".to_string(),
            Err("probe failed".to_string()),
        );
        player.mpv_operation_log.clear();
        player.mpv_operation_log_first_sequence = player.mpv_operation_log_next_sequence;

        player.apply(PlayerCommand::SeekRelative {
            seconds: 1.0,
            option: RelativeSeekOption::Auto,
        });
        player.apply_mpv_events(&[mpv_lifecycle_event(20, "seek")]);
        player.auto_seek_probe_started_at =
            Some(Instant::now() - std::time::Duration::from_millis(60));
        player.apply_mpv_events(&[mpv_lifecycle_event(21, "playback-restart")]);
        player.apply(PlayerCommand::SeekRelative {
            seconds: 2.0,
            option: RelativeSeekOption::Auto,
        });

        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("seek", ["1", "relative+exact"]),
                mpv_command("seek", ["2", "relative"]),
            ]
        );

        player.apply_mpv_events(&[mpv_lifecycle_event(8, "file-loaded")]);
        player.apply(PlayerCommand::SeekRelative {
            seconds: 3.0,
            option: RelativeSeekOption::Auto,
        });
        player.apply_mpv_events(&[mpv_lifecycle_event(20, "seek")]);
        player.auto_seek_probe_started_at =
            Some(Instant::now() - std::time::Duration::from_millis(10));
        player.apply_mpv_events(&[mpv_lifecycle_event(21, "playback-restart")]);
        player.apply(PlayerCommand::SeekRelative {
            seconds: 4.0,
            option: RelativeSeekOption::Auto,
        });

        assert_eq!(
            &player.mpv_operation_log[2..],
            [
                mpv_command("seek", ["3", "relative"]),
                mpv_command("seek", ["4", "relative+exact"]),
            ]
        );
    }

    #[test]
    fn preparing_playback_position_save_queues_mpv_watch_later_before_stop() {
        let mut player = PlayerState::default();
        player.open_media(
            "/tmp/current.mp4".to_string(),
            Err("probe failed".to_string()),
        );
        player.position_seconds = 42.5;
        player.media_title = "Current title".to_string();
        player.mpv_operation_log.clear();
        player.mpv_operation_log_first_sequence = player.mpv_operation_log_next_sequence;

        let saved = player.prepare_playback_position_save().unwrap();

        assert_eq!(
            saved,
            LastPlayback {
                path: "/tmp/current.mp4".to_string(),
                title: "Current title".to_string(),
                position_seconds: 42.5,
            }
        );
        assert_eq!(
            player.mpv_operation_log,
            vec![mpv_command(
                "write-watch-later-config",
                std::iter::empty::<&str>()
            )]
        );
    }

    #[test]
    fn seek_clamps_to_media_duration_and_zero() {
        let mut player = PlayerState::default();
        player.open_media(
            "/tmp/current.mp4".to_string(),
            Err("probe failed".to_string()),
        );
        player.duration_seconds = 60.0;

        player.apply(PlayerCommand::Seek { seconds: 90.0 });
        assert_eq!(player.position_seconds, 60.0);
        assert_eq!(
            player
                .last_playback
                .as_ref()
                .map(|item| item.position_seconds),
            Some(60.0)
        );

        player.apply(PlayerCommand::Seek { seconds: -10.0 });
        assert_eq!(player.position_seconds, 0.0);
        assert_eq!(
            player
                .last_playback
                .as_ref()
                .map(|item| item.position_seconds),
            Some(0.0)
        );
    }

    #[test]
    fn jump_to_seek_forwards_unclamped_absolute_exact_time_to_mpv() {
        let mut player = PlayerState::default();
        player.open_media(
            "/tmp/current.mp4".to_string(),
            Err("probe failed".to_string()),
        );
        player.duration_seconds = 60.0;
        player.position_seconds = 12.0;
        player.mpv_operation_log.clear();
        player.mpv_operation_log_first_sequence = player.mpv_operation_log_next_sequence;

        player.apply(PlayerCommand::SeekAbsoluteExact { seconds: 90.5 });

        assert_eq!(player.position_seconds, 12.0);
        assert_eq!(
            player.mpv_operation_log,
            vec![mpv_command("seek", ["90.5", "absolute+exact"])]
        );
    }

    #[test]
    fn commands_keep_mpv_property_snapshot_in_sync() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));

        player.apply(PlayerCommand::Seek { seconds: 45.0 });
        assert_eq!(player.mpv_properties.time_pos, 45.0);
        assert_eq!(player.mpv_properties.percent_pos, 37.5);
        assert_eq!(player.mpv_properties.chapter, 1);

        player.apply(PlayerCommand::Pause);
        assert!(player.mpv_properties.pause);

        player.apply(PlayerCommand::SetVolume { volume: 140.0 });
        assert_eq!(player.mpv_properties.volume, 140.0);

        player.apply(PlayerCommand::SetSpeed { speed: 1.5 });
        assert_eq!(player.mpv_properties.speed, 1.5);

        player.apply(PlayerCommand::ToggleMute);
        assert!(player.mpv_properties.mute);

        player.apply(PlayerCommand::Stop);
        assert!(!player.mpv_properties.idle_active);
        assert_eq!(
            player.mpv_properties.path.as_deref(),
            Some("/tmp/current.mp4")
        );

        player.apply_mpv_property_changes(&[mpv_property_change(
            "idle-active",
            MpvFormat::Flag,
            Some("true"),
        )]);
        assert!(player.mpv_properties.idle_active);
        assert_eq!(player.mpv_properties.path, None);
        assert_eq!(player.mpv_properties.playlist_count, 0);
        assert_eq!(player.mpv_properties.playlist_pos, -1);
    }

    #[test]
    fn window_resize_generations_pair_manual_open_with_its_start_file_event() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));
        assert_eq!(player.window_resize_observation(), (1, 0, true, false));

        player.apply_mpv_events(&[mpv_start_file_event(10)]);
        assert_eq!(player.window_resize_observation(), (1, 0, true, false));

        player.apply_mpv_events(&[mpv_start_file_event(11)]);
        assert_eq!(player.window_resize_observation(), (2, 0, false, false));
    }

    #[test]
    fn playlist_navigation_supersedes_a_pending_manual_start_marker() {
        let mut player = PlayerState::default();
        player.open_media_batch(
            vec!["/tmp/current.mp4".to_string(), "/tmp/next.mp4".to_string()],
            Ok(probed_media()),
        );
        player.apply(PlayerCommand::PlaylistNext);
        player.apply_mpv_events(&[mpv_start_file_event(11)]);
        assert_eq!(player.window_resize_observation(), (2, 0, false, false));
    }

    #[test]
    fn window_resize_generation_tracks_each_video_reconfiguration() {
        let mut player = PlayerState::default();
        player.apply_mpv_events(&[mpv_lifecycle_event(17, "video-reconfig")]);
        assert_eq!(player.window_resize_observation(), (0, 0, false, true));
        player.apply_mpv_events(&[mpv_lifecycle_event(17, "video-reconfig")]);
        assert_eq!(player.window_resize_observation(), (0, 1, false, true));
    }

    #[test]
    fn mpv_property_events_update_authoritative_player_state() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));

        player.apply_mpv_events(&[
            mpv_property_event("pause", MpvFormat::Flag, Some("true")),
            mpv_property_event("volume", MpvFormat::Double, Some("55.5")),
            mpv_property_event("speed", MpvFormat::Double, Some("1.25")),
            mpv_property_event("mute", MpvFormat::Flag, Some("yes")),
            mpv_property_event("media-title", MpvFormat::String, Some("Runtime Title")),
            mpv_property_event("sid", MpvFormat::Int64, Some("12")),
            mpv_property_event("secondary-sid", MpvFormat::Int64, Some("12")),
            mpv_property_event("chapter", MpvFormat::Int64, Some("1")),
            mpv_property_event("idle-active", MpvFormat::Flag, Some("true")),
        ]);

        assert!(player.paused);
        assert_eq!(player.volume, 55.5);
        assert_eq!(player.speed, 1.25);
        assert!(player.muted);
        assert_eq!(player.media_title, "Runtime Title");
        assert_eq!(player.playlist[0].title, "Runtime Title");
        assert_eq!(
            player
                .last_playback
                .as_ref()
                .map(|item| item.title.as_str()),
            Some("Runtime Title")
        );
        assert_eq!(player.mpv_properties.sid, 12);
        assert_eq!(player.mpv_properties.secondary_sid, 12);
        assert_eq!(player.mpv_properties.chapter, 1);
        assert!(player.mpv_properties.idle_active);
        assert!(player.mpv_properties.pause);
    }

    #[test]
    fn mpv_lifecycle_events_update_playback_idle_snapshot() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));
        player.paused = true;
        player.refresh_mpv_properties();

        player.apply_mpv_events(&[
            mpv_start_file_event(1),
            mpv_lifecycle_event(8, "file-loaded"),
            mpv_lifecycle_event(21, "playback-restart"),
        ]);

        assert!(matches!(player.mode, PlayerMode::Player));
        assert!(!player.paused);
        assert!(!player.mpv_properties.pause);
        assert!(!player.mpv_properties.idle_active);
        assert_eq!(player.osd_message, None);

        player.apply_mpv_events(&[mpv_lifecycle_event(11, "idle")]);

        assert!(player.paused);
        assert!(player.mpv_properties.pause);
        assert!(player.mpv_properties.idle_active);
        assert_eq!(player.current_url.as_deref(), Some("/tmp/current.mp4"));
    }

    #[test]
    fn mpv_end_file_events_pause_without_destroying_playlist() {
        let mut player = PlayerState::default();
        player.open_media_batch(
            vec![
                "/tmp/current.mp4".to_string(),
                "/tmp/queued.mkv".to_string(),
            ],
            Ok(probed_media()),
        );
        player.send_osd("Playing");

        player.apply_mpv_events(&[mpv_end_file_event(MpvEndFileReason::Stop)]);

        assert!(player.paused);
        assert!(player.mpv_properties.pause);
        assert!(!player.file_loading);
        assert_eq!(player.playback_error, None);
        assert_eq!(player.osd_message.as_deref(), Some("Stopped"));
        assert_eq!(player.current_url.as_deref(), Some("/tmp/current.mp4"));
        assert_eq!(player.playlist.len(), 2);

        player.paused = false;
        player.refresh_mpv_properties();
        player.apply_mpv_events(&[mpv_end_file_event(MpvEndFileReason::Redirect)]);

        assert!(!player.paused);
        assert!(!player.mpv_properties.pause);
        assert_eq!(player.playlist.len(), 2);
    }

    #[test]
    fn loading_failure_waits_for_idle_before_closing_player_state() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/broken.mp4".to_string(), Ok(probed_media()));
        assert!(player.file_loading);

        player.apply_mpv_events(&[mpv_end_file_event(MpvEndFileReason::Error)]);

        assert!(player.file_loading);
        assert_eq!(player.current_url.as_deref(), Some("/tmp/broken.mp4"));
        assert_eq!(player.playback_error, None);

        player.apply_mpv_property_changes(&[mpv_property_change(
            "idle-active",
            MpvFormat::Flag,
            Some("true"),
        )]);

        assert!(!player.file_loading);
        assert!(matches!(player.mode, PlayerMode::Initial));
        assert_eq!(player.current_url, None);
        assert!(player.playlist.is_empty());
        assert_eq!(
            player.playback_error,
            Some(PlaybackError {
                code: -13,
                message: "error loading file".to_string(),
            })
        );
        assert_eq!(player.osd_message.as_deref(), Some("Playback Error"));
    }

    #[test]
    fn mpv_polled_properties_update_time_duration_and_resume_position() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));

        player.apply_mpv_property_changes(&[
            mpv_property_change("duration", MpvFormat::Double, Some("240")),
            mpv_property_change("time-pos", MpvFormat::Double, Some("42.5")),
            mpv_property_change("percent-pos", MpvFormat::Double, Some("17.7")),
            mpv_property_change("pause", MpvFormat::Flag, Some("false")),
        ]);

        assert_eq!(player.duration_seconds, 240.0);
        assert_eq!(player.position_seconds, 42.5);
        assert_eq!(
            player
                .last_playback
                .as_ref()
                .map(|item| item.position_seconds),
            Some(42.5)
        );
        assert_eq!(player.playlist[0].duration_seconds, Some(240.0));
        assert_eq!(player.mpv_properties.duration, 240.0);
        assert_eq!(player.mpv_properties.time_pos, 42.5);
        assert_eq!(player.mpv_properties.percent_pos, 17.7);
        assert!(!player.mpv_properties.pause);
    }

    #[test]
    fn music_metadata_matches_iina_chapter_precedence_and_resets_cleanly() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));

        player.apply_mpv_property_changes(&[
            mpv_property_change("media-title", MpvFormat::String, Some("Media Title")),
            mpv_property_change("metadata/by-key/album", MpvFormat::String, Some("Album")),
            mpv_property_change(
                "metadata/by-key/artist",
                MpvFormat::String,
                Some("Track Artist"),
            ),
            mpv_property_change(
                "chapter-metadata/by-key/title",
                MpvFormat::String,
                Some("Fallback Chapter Title"),
            ),
            mpv_property_change(
                "chapter-metadata/by-key/performer",
                MpvFormat::String,
                Some("Chapter Performer"),
            ),
            mpv_property_change("chapter", MpvFormat::Int64, Some("1")),
            mpv_property_change("chapters", MpvFormat::Int64, Some("2")),
        ]);

        assert_eq!(player.music_title, "Middle");
        assert_eq!(player.music_album, "Album");
        assert_eq!(player.music_artist, "Chapter Performer");

        player.apply_mpv_property_changes(&[
            mpv_property_change("media-title", MpvFormat::String, Some("No Tags")),
            mpv_property_change("chapter", MpvFormat::Int64, Some("-1")),
            mpv_property_change("chapters", MpvFormat::Int64, Some("0")),
        ]);

        assert_eq!(player.music_title, "No Tags");
        assert!(player.music_album.is_empty());
        assert!(player.music_artist.is_empty());

        player.music_album = "Stale Album".to_string();
        player.music_artist = "Stale Artist".to_string();
        player.apply(PlayerCommand::Stop);
        assert_eq!(player.music_album, "Stale Album");
        player.apply_mpv_events(&[mpv_end_file_event(MpvEndFileReason::Stop)]);
        player.apply_mpv_property_changes(&[mpv_property_change(
            "idle-active",
            MpvFormat::Flag,
            Some("true"),
        )]);
        assert_eq!(player.music_title, "IINA");
        assert!(player.music_album.is_empty());
        assert!(player.music_artist.is_empty());
    }

    #[test]
    fn automatic_music_mode_respects_track_kind_networks_and_manual_overrides() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/audio.flac".to_string(), Ok(probed_media()));
        let video_track = player.tracks.video[0].clone();
        player.tracks.video.clear();

        assert_eq!(
            player.automatic_music_mode_transition(),
            Some(AutomaticMusicModeTransition::Enter)
        );

        player.enter_mini_player(true);
        player.tracks.video = vec![video_track.clone()];
        assert_eq!(
            player.automatic_music_mode_transition(),
            Some(AutomaticMusicModeTransition::Leave)
        );

        player.leave_mini_player(true);
        player.tracks.video[0].metadata.albumart = true;
        assert_eq!(
            player.automatic_music_mode_transition(),
            Some(AutomaticMusicModeTransition::Enter)
        );

        player.enter_mini_player(false);
        player.tracks.video[0].metadata.albumart = false;
        assert_eq!(player.automatic_music_mode_transition(), None);

        player.leave_mini_player(false);
        player.tracks.video.clear();
        assert_eq!(player.automatic_music_mode_transition(), None);
        player.reset_music_mode_switch_history();
        assert_eq!(
            player.automatic_music_mode_transition(),
            Some(AutomaticMusicModeTransition::Enter)
        );

        player.current_url = Some("https://example.com/radio".to_string());
        assert_eq!(player.automatic_music_mode_transition(), None);
    }

    #[test]
    fn mpv_polled_path_switches_current_playlist_item() {
        let mut player = PlayerState::default();
        player.open_media_batch(
            vec![
                "/tmp/current.mp4".to_string(),
                "/tmp/queued.mkv".to_string(),
            ],
            Ok(probed_media()),
        );

        player.apply_mpv_property_changes(&[
            mpv_property_change("path", MpvFormat::String, Some("/tmp/queued.mkv")),
            mpv_property_change(
                "media-title",
                MpvFormat::String,
                Some("Queued Runtime Title"),
            ),
            mpv_property_change("duration", MpvFormat::Double, Some("30")),
            mpv_property_change("time-pos", MpvFormat::Double, Some("5")),
        ]);

        assert_eq!(player.current_url.as_deref(), Some("/tmp/queued.mkv"));
        assert!(!player.playlist[0].current);
        assert!(player.playlist[1].current);
        assert_eq!(player.playlist[1].title, "Queued Runtime Title");
        assert_eq!(player.playlist[1].duration_seconds, Some(30.0));
        assert_eq!(player.media_title, "Queued Runtime Title");
        assert_eq!(
            player.last_playback.as_ref().map(|item| item.path.as_str()),
            Some("/tmp/queued.mkv")
        );
        assert_eq!(
            player
                .last_playback
                .as_ref()
                .map(|item| item.position_seconds),
            Some(5.0)
        );
    }

    #[test]
    fn mpv_playlist_snapshot_updates_playlist_items() {
        let mut player = PlayerState::default();
        player.open_media_batch(
            vec![
                "/tmp/current.mp4".to_string(),
                "/tmp/queued.mkv".to_string(),
            ],
            Ok(probed_media()),
        );
        player.playlist[0].duration_seconds = Some(120.0);
        player.duration_seconds = 240.0;
        player.position_seconds = 12.0;
        player.apply_mpv_property_changes(&[mpv_property_change(
            "playlist-pos",
            MpvFormat::Int64,
            Some("1"),
        )]);

        player.apply_mpv_playlist(&[
            mpv_playlist_item(0, 50, "/tmp/current.mp4", false, false, None),
            mpv_playlist_item(
                1,
                51,
                "/tmp/queued.mkv",
                true,
                true,
                Some("Runtime Queue Title"),
            ),
        ]);

        assert_eq!(player.playlist.len(), 2);
        assert_eq!(player.playlist[0].id, 1);
        assert_eq!(player.playlist[0].mpv_id, Some(50));
        assert_eq!(player.playlist[0].title, "current.mp4");
        assert_eq!(player.playlist[0].duration_seconds, Some(120.0));
        assert!(!player.playlist[0].current);
        assert_eq!(player.playlist[1].mpv_id, Some(51));
        assert_eq!(player.playlist[1].title, "Runtime Queue Title");
        assert_eq!(player.playlist[1].duration_seconds, Some(240.0));
        assert!(player.playlist[1].current);
        assert!(player.playlist[1].playing);
        assert_eq!(player.current_url.as_deref(), Some("/tmp/queued.mkv"));
        assert_eq!(player.media_title, "Runtime Queue Title");
        assert_eq!(
            player.last_playback.as_ref().map(|item| item.path.as_str()),
            Some("/tmp/queued.mkv")
        );
        assert_eq!(player.mpv_properties.playlist_count, 2);
        assert_eq!(player.mpv_properties.playlist_pos, 1);
    }

    #[test]
    fn mpv_track_list_snapshot_updates_track_groups() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));

        let mut video = mpv_track(0, 10, "video", true);
        video.title = Some("Main Video".to_string());
        video.codec = Some("h264".to_string());
        video.demux_w = Some(1920);
        video.demux_h = Some(1080);
        video.demux_fps = Some(23.976);

        let mut audio = mpv_track(1, 21, "audio", true);
        audio.lang = Some("eng".to_string());
        audio.title = Some("Stereo".to_string());
        audio.codec = Some("aac".to_string());
        audio.default_track = true;
        audio.demux_channel_count = Some(2);
        audio.demux_samplerate = Some(48_000);

        let mut subtitle = mpv_track(2, 31, "sub", true);
        subtitle.lang = Some("jpn".to_string());
        subtitle.codec = Some("ass".to_string());
        subtitle.external = true;
        subtitle.forced = true;
        subtitle.external_filename = Some("/tmp/sub.ass".to_string());

        player.apply_mpv_track_list(&[video, audio, subtitle]);

        assert_eq!(player.tracks.video.len(), 1);
        assert_eq!(player.tracks.audio.len(), 1);
        assert_eq!(player.tracks.subtitles.len(), 2);
        assert_eq!(player.mpv_properties.track_list_count, 4);
        assert_eq!(player.mpv_properties.vid, 10);
        assert_eq!(player.mpv_properties.aid, 21);
        assert_eq!(player.mpv_properties.sid, 31);
        assert!(player.tracks.video[0].title.contains("1920x1080"));
        assert!(player.tracks.video[0].title.contains("23.976fps"));
        assert_eq!(
            player.tracks.video[0].metadata.source_title.as_deref(),
            Some("Main Video")
        );
        assert!(player.tracks.audio[0].title.contains("[eng]"));
        assert!(player.tracks.audio[0].title.contains("2ch"));
        assert!(player.tracks.audio[0].title.contains("48kHz"));
        assert!(player.tracks.audio[0].metadata.default_track);
        assert!(player.tracks.subtitles[1].metadata.external);
        assert!(player.tracks.subtitles[1].metadata.forced);
        assert_eq!(
            player.tracks.subtitles[1]
                .metadata
                .external_filename
                .as_deref(),
            Some("/tmp/sub.ass")
        );
    }

    #[test]
    fn mpv_property_events_ignore_invalid_values() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));

        player.apply_mpv_events(&[
            mpv_property_event("pause", MpvFormat::Flag, Some("maybe")),
            mpv_property_event("volume", MpvFormat::Double, Some("nan")),
            mpv_property_event("speed", MpvFormat::Double, Some("inf")),
            mpv_property_event("media-title", MpvFormat::String, Some("")),
            mpv_property_event("aid", MpvFormat::Int64, Some("999")),
            mpv_property_event("duration", MpvFormat::Double, Some("nan")),
            mpv_property_event("time-pos", MpvFormat::Double, Some("-inf")),
        ]);

        assert!(!player.paused);
        assert_eq!(player.volume, 100.0);
        assert_eq!(player.speed, 1.0);
        assert_eq!(player.media_title, "Probed Title");
        assert_eq!(player.duration_seconds, 120.0);
        assert_eq!(player.position_seconds, 0.0);
        assert_eq!(player.mpv_properties.aid, 11);
    }

    #[test]
    fn commands_record_iina_mpv_operations() {
        let mut player = PlayerState::default();
        player.open_media_batch(
            vec![
                "/tmp/current.mp4".to_string(),
                "/tmp/queued.mkv".to_string(),
            ],
            Ok(probed_media()),
        );
        player.tracks.audio.push(Track {
            id: 99,
            title: "Commentary".to_string(),
            selected: false,
            metadata: TrackMetadata::default(),
        });
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::Seek { seconds: 45.0 });
        player.apply(PlayerCommand::Pause);
        player.apply(PlayerCommand::SetVolume { volume: 140.0 });
        player.apply(PlayerCommand::SetSpeed { speed: 1.5 });
        player.apply(PlayerCommand::ToggleMute);
        player.apply(PlayerCommand::FrameStep { backwards: false });
        player.apply(PlayerCommand::PlaylistNext);
        player.apply(PlayerCommand::CycleTrack {
            kind: TrackSelectionKind::Audio,
        });

        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("seek", ["45", "absolute+exact"]),
                set_property("pause", MpvFormat::Flag, "true"),
                set_property("volume", MpvFormat::Double, "140"),
                set_property("speed", MpvFormat::Double, "1.5"),
                set_property("mute", MpvFormat::Flag, "true"),
                mpv_command("frame-step", std::iter::empty::<&str>()),
                mpv_command("playlist-next", std::iter::empty::<&str>()),
                set_property("aid", MpvFormat::Int64, "99"),
            ]
        );
    }

    #[test]
    fn quick_settings_commands_record_iina_mpv_properties() {
        let mut player = PlayerState::default();
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::SetDeinterlace { enabled: true });
        player.apply(PlayerCommand::SetVideoRotate { degrees: 90 });
        player.apply(PlayerCommand::SetVideoEqualizer {
            option: VideoEqualizer::Brightness,
            value: 120,
        });
        player.apply(PlayerCommand::SetAudioDelay { seconds: -0.25 });
        player.apply(PlayerCommand::SetSubEncoding {
            encoding: "GB18030".to_string(),
        });
        player.apply(PlayerCommand::SetSubDelay { seconds: 0.5 });
        player.apply(PlayerCommand::SetSubScale { scale: 1.25 });
        player.apply(PlayerCommand::SetSubPosition { position: 80 });

        assert!(player.quick_settings.deinterlace);
        assert_eq!(player.quick_settings.video_rotate, 90);
        assert_eq!(player.quick_settings.brightness, 100);
        assert_eq!(player.quick_settings.audio_delay, -0.25);
        assert_eq!(player.quick_settings.sub_encoding, "GB18030");
        assert_eq!(player.quick_settings.sub_delay, 0.5);
        assert_eq!(player.quick_settings.sub_scale, 1.25);
        assert_eq!(player.quick_settings.sub_pos, 80);
        assert_eq!(
            player.mpv_operation_log,
            vec![
                set_property("deinterlace", MpvFormat::Flag, "true"),
                set_property("video-rotate", MpvFormat::Int64, "90"),
                set_property("brightness", MpvFormat::Int64, "100"),
                set_property("audio-delay", MpvFormat::Double, "-0.25"),
                set_property("sub-codepage", MpvFormat::String, "GB18030"),
                set_property("sub-delay", MpvFormat::Double, "0.5"),
                set_property("sub-scale", MpvFormat::Double, "1.25"),
                set_property("sub-pos", MpvFormat::Int64, "80"),
            ]
        );
    }

    #[test]
    fn subtitle_encoding_list_and_runtime_property_match_iina_135() {
        assert_eq!(IINA_SUBTITLE_ENCODINGS.len(), 44);
        assert_eq!(IINA_SUBTITLE_ENCODINGS[0], ("Auto detect", "auto"));
        assert_eq!(
            IINA_SUBTITLE_ENCODINGS[43],
            ("Western European (LATIN-9)", "LATIN-9")
        );

        let mut player = PlayerState::default();
        player.apply_mpv_property_changes(&[mpv_property_change(
            "sub-codepage",
            MpvFormat::String,
            Some("SHIFT-JIS"),
        )]);
        assert_eq!(player.quick_settings.sub_encoding, "SHIFT-JIS");

        player.apply_mpv_property_changes(&[mpv_property_change(
            "sub-codepage",
            MpvFormat::String,
            Some("not-an-iina-encoding"),
        )]);
        assert_eq!(player.quick_settings.sub_encoding, "SHIFT-JIS");
    }

    #[test]
    fn loop_commands_follow_iina_property_sequence() {
        let mut player = PlayerState::default();

        player.apply(PlayerCommand::TogglePlaylistLoop);
        assert_eq!(player.loop_mode, LoopMode::Playlist);
        player.apply(PlayerCommand::ToggleFileLoop);
        assert_eq!(player.loop_mode, LoopMode::File);
        player.apply(PlayerCommand::ToggleFileLoop);
        assert_eq!(player.loop_mode, LoopMode::Off);

        assert_eq!(
            player.mpv_operation_log,
            vec![
                set_property("loop-playlist", MpvFormat::String, "inf"),
                set_property("loop-file", MpvFormat::String, "no"),
                set_property("loop-file", MpvFormat::String, "inf"),
                set_property("loop-playlist", MpvFormat::String, "no"),
                set_property("loop-file", MpvFormat::String, "no"),
            ]
        );
    }

    #[test]
    fn ab_loop_cycles_and_updates_points_like_iina() {
        let mut player = PlayerState::default();
        player.position_seconds = 12.5;

        player.apply(PlayerCommand::CycleAbLoop);
        assert_eq!(player.ab_loop.status, AbLoopStatus::ASet);
        assert_eq!(player.ab_loop.a_seconds, 12.5);
        assert!(!player.ab_loop.is_active());

        player.position_seconds = 24.0;
        player.apply(PlayerCommand::CycleAbLoop);
        assert_eq!(player.ab_loop.status, AbLoopStatus::BSet);
        assert_eq!(player.ab_loop.b_seconds, 24.0);
        assert!(player.ab_loop.is_active());

        player.apply(PlayerCommand::SetAbLoopPoint {
            point: AbLoopPoint::A,
            seconds: 0.0,
        });
        assert_eq!(player.ab_loop.a_seconds, MIN_AB_LOOP_POINT_SECONDS);

        player.apply(PlayerCommand::CycleAbLoop);
        assert_eq!(player.ab_loop, AbLoopState::default());
        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("ab-loop", std::iter::empty::<&str>()),
                mpv_command("ab-loop", std::iter::empty::<&str>()),
                set_property(
                    "ab-loop-a",
                    MpvFormat::Double,
                    format_mpv_number(MIN_AB_LOOP_POINT_SECONDS),
                ),
                mpv_command("ab-loop", std::iter::empty::<&str>()),
            ]
        );
    }

    #[test]
    fn runtime_ab_loop_properties_drive_status_and_active_state() {
        let mut player = PlayerState::default();

        player.apply_mpv_property_changes(&[
            mpv_property_change("ab-loop-a", MpvFormat::Double, Some("3.5")),
            mpv_property_change("ab-loop-b", MpvFormat::Double, Some("8.25")),
            mpv_property_change("ab-loop-count", MpvFormat::String, Some("0")),
        ]);
        assert_eq!(player.ab_loop.status, AbLoopStatus::BSet);
        assert!(!player.ab_loop.is_active());

        player.apply_mpv_property_changes(&[mpv_property_change(
            "ab-loop-count",
            MpvFormat::String,
            Some("inf"),
        )]);
        assert!(player.ab_loop.is_active());

        player.apply_mpv_property_changes(&[mpv_property_change(
            "ab-loop-b",
            MpvFormat::Double,
            Some("0"),
        )]);
        assert_eq!(player.ab_loop.status, AbLoopStatus::ASet);
        assert!(!player.ab_loop.is_active());
    }

    #[test]
    fn audio_device_snapshot_and_selection_follow_mpv_names() {
        let mut player = PlayerState::default();
        player.apply_mpv_audio_devices(&[
            MpvAudioDevice {
                name: "auto".to_string(),
                description: "Autoselect device".to_string(),
            },
            MpvAudioDevice {
                name: "coreaudio/42".to_string(),
                description: "Studio Display Speakers".to_string(),
            },
        ]);

        player.apply(PlayerCommand::SelectAudioDevice {
            name: "coreaudio/42".to_string(),
        });
        assert_eq!(player.audio_device, "coreaudio/42");
        assert_eq!(
            player.mpv_operation_log,
            vec![set_property(
                "audio-device",
                MpvFormat::String,
                "coreaudio/42"
            )]
        );

        player.apply_mpv_property_changes(&[mpv_property_change(
            "audio-device",
            MpvFormat::String,
            Some("auto"),
        )]);
        assert_eq!(player.audio_device, "auto");
    }

    #[test]
    fn runtime_loop_properties_keep_file_priority() {
        let mut player = PlayerState::default();

        player.apply_mpv_property_changes(&[mpv_property_change(
            "loop-playlist",
            MpvFormat::String,
            Some("force"),
        )]);
        assert_eq!(player.loop_mode, LoopMode::Playlist);

        player.apply_mpv_property_changes(&[mpv_property_change(
            "loop-file",
            MpvFormat::String,
            Some("3"),
        )]);
        assert_eq!(player.loop_mode, LoopMode::File);

        player.apply_mpv_property_changes(&[mpv_property_change(
            "loop-file",
            MpvFormat::String,
            Some("no"),
        )]);
        assert_eq!(player.loop_mode, LoopMode::Playlist);

        player.apply_mpv_property_changes(&[mpv_property_change(
            "loop-playlist",
            MpvFormat::String,
            Some("0"),
        )]);
        assert_eq!(player.loop_mode, LoopMode::Off);
    }

    #[test]
    fn flip_and_mirror_use_iina_named_video_filters() {
        let mut player = PlayerState::default();

        player.apply(PlayerCommand::SetVideoMirror { enabled: true });
        player.apply(PlayerCommand::SetVideoFlip { enabled: true });
        player.apply(PlayerCommand::SetVideoMirror { enabled: false });
        player.apply(PlayerCommand::SetVideoFlip { enabled: false });

        assert!(!player.quick_settings.video_mirrored);
        assert!(!player.quick_settings.video_flipped);
        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("vf", ["add", "@iina_mirror:hflip"]),
                mpv_command("vf", ["add", "@iina_flip:vflip"]),
                mpv_command("vf", ["remove", "@iina_mirror"]),
                mpv_command("vf", ["remove", "@iina_flip"]),
            ]
        );
    }

    #[test]
    fn video_aspect_and_crop_presets_follow_iina_mpv_operations() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::SetVideoAspect {
            aspect: "21:9".to_string(),
        });
        player.apply(PlayerCommand::SetVideoCrop {
            crop: "4:3".to_string(),
        });
        player.apply(PlayerCommand::SetVideoCrop {
            crop: "None".to_string(),
        });
        player.apply(PlayerCommand::SetVideoAspect {
            aspect: "invalid".to_string(),
        });

        assert_eq!(player.quick_settings.video_aspect, "Default");
        assert_eq!(player.quick_settings.video_crop, "None");
        assert_eq!(
            player.mpv_operation_log,
            vec![
                set_property("video-aspect", MpvFormat::String, "21:9"),
                mpv_command("vf", ["add", "@iina_crop:crop=1440:1080::"]),
                mpv_command("vf", ["remove", "@iina_crop"]),
                set_property("video-aspect", MpvFormat::String, "-1"),
            ]
        );
    }

    #[test]
    fn custom_crop_uses_iina_source_coordinates_and_removes_full_selection() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::SetCustomVideoCrop {
            x: 120,
            y: 60,
            width: 1440,
            height: 900,
        });
        player.apply(PlayerCommand::SetCustomVideoCrop {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        });
        player.apply(PlayerCommand::SetCustomVideoCrop {
            x: 1920,
            y: 0,
            width: 1,
            height: 1,
        });

        assert_eq!(player.quick_settings.video_crop, "None");
        assert_eq!(player.quick_settings.custom_crop, None);
        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("vf", ["add", "@iina_crop:crop=1440:900:120:60"]),
                mpv_command("vf", ["remove", "@iina_crop"]),
            ]
        );
        assert_eq!(
            player.osd_message.as_deref(),
            Some("Crop unavailable for current video")
        );
    }

    #[test]
    fn delogo_replaces_and_removes_the_single_iina_labeled_filter() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::SetDelogoRegion {
            x: 100,
            y: 75,
            width: 320,
            height: 180,
        });
        assert_eq!(player.video_filters.len(), 1);
        assert_eq!(
            player.video_filters[0].string_format,
            "@iina_delogo:lavfi=[delogo=x=100:y=75:w=320:h=180]"
        );

        player.apply(PlayerCommand::SetDelogoRegion {
            x: 200,
            y: 125,
            width: 240,
            height: 120,
        });
        assert_eq!(player.video_filters.len(), 1);
        assert_eq!(
            player.video_filters[0].string_format,
            "@iina_delogo:lavfi=[delogo=x=200:y=125:w=240:h=120]"
        );

        player.apply(PlayerCommand::RemoveDelogo);
        player.apply(PlayerCommand::SetDelogoRegion {
            x: 1919,
            y: 1079,
            width: 2,
            height: 2,
        });

        assert!(player.video_filters.is_empty());
        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command(
                    "vf",
                    ["add", "@iina_delogo:lavfi=[delogo=x=100:y=75:w=320:h=180]",],
                ),
                MpvClientOperation::RemoveFilterAt {
                    name: "vf".to_string(),
                    index: 0,
                },
                mpv_command(
                    "vf",
                    ["add", "@iina_delogo:lavfi=[delogo=x=200:y=125:w=240:h=120]",],
                ),
                MpvClientOperation::RemoveFilterAt {
                    name: "vf".to_string(),
                    index: 0,
                },
            ]
        );
        assert_eq!(
            player.osd_message.as_deref(),
            Some("Delogo unavailable for current video")
        );
    }

    #[test]
    fn hardware_decoder_toggle_uses_iina_preference_modes() {
        let mut player = PlayerState::default();
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::SetHardwareDecoding {
            enabled: true,
            decoder: 2,
        });
        player.apply(PlayerCommand::SetHardwareDecoding {
            enabled: false,
            decoder: 1,
        });

        assert!(!player.quick_settings.hardware_decoding);
        assert_eq!(
            player.mpv_operation_log,
            vec![
                set_property("hwdec", MpvFormat::String, "auto-copy"),
                set_property("hwdec", MpvFormat::String, "no"),
            ]
        );

        player.apply_mpv_property_changes(&[mpv_property_change(
            "hwdec",
            MpvFormat::String,
            Some("auto"),
        )]);
        assert!(player.quick_settings.hardware_decoding);
    }

    #[test]
    fn hdr_toggle_is_runtime_state_without_an_mpv_property_write() {
        let mut player = PlayerState::default();
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::SetHdrEnabled { enabled: false });
        player.set_hdr_status(true, false);

        assert!(player.quick_settings.hdr_available);
        assert!(!player.quick_settings.hdr_enabled);
        assert!(player.mpv_operation_log.is_empty());
    }

    #[test]
    fn audio_equalizer_uses_iina_labeled_anequalizer_filters() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::SetAudioEqualizer {
            gains: vec![0.0, 20.0, -3.5],
        });

        assert!(player.quick_settings.audio_eq_active);
        assert_eq!(player.quick_settings.audio_eq[0], 0.0);
        assert_eq!(player.quick_settings.audio_eq[1], 12.0);
        assert_eq!(player.quick_settings.audio_eq[2], -3.5);
        assert_eq!(player.mpv_operation_log.len(), 10);
        match &player.mpv_operation_log[0] {
            MpvClientOperation::Command { command, args } => {
                assert_eq!(command, "af");
                assert_eq!(args.first().map(String::as_str), Some("add"));
                let filter = args.get(1).expect("audio equalizer filter argument");
                assert!(filter.starts_with("@iina_aeq0:lavfi=[anequalizer="));
                assert!(filter.contains("c0 f=31.25"));
                assert!(filter.contains("c1 f=31.25"));
                assert!(filter.ends_with("g=0.0]"));
            }
            operation => panic!("expected mpv af command, got {operation:?}"),
        }

        player.mpv_operation_log.clear();
        player.apply(PlayerCommand::ResetAudioEqualizer);

        assert!(!player.quick_settings.audio_eq_active);
        assert_eq!(player.quick_settings.audio_eq, [0.0; 10]);
        assert_eq!(player.mpv_operation_log.len(), 10);
        assert_eq!(
            player.mpv_operation_log[0],
            mpv_command("af", ["remove", "@iina_aeq0"])
        );
        assert_eq!(
            player.mpv_operation_log[9],
            mpv_command("af", ["remove", "@iina_aeq9"])
        );
    }

    #[test]
    fn subtitle_style_commands_use_iina_options_properties() {
        let mut player = PlayerState::default();
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::SetSubtitleStyleColor {
            target: SubtitleStyleColorTarget::Text,
            color: "0.5/0.25/1/0.75".to_string(),
        });
        player.apply(PlayerCommand::SetSubtitleTextSize { size: 60.0 });
        player.apply(PlayerCommand::SetSubtitleStyleColor {
            target: SubtitleStyleColorTarget::Border,
            color: "0/0/0".to_string(),
        });
        player.apply(PlayerCommand::SetSubtitleBorderSize { size: 1.5 });
        player.apply(PlayerCommand::SetSubtitleStyleColor {
            target: SubtitleStyleColorTarget::Background,
            color: "1/1/1/0".to_string(),
        });
        player.apply(PlayerCommand::SetSubtitleFont {
            font: "Helvetica Neue".to_string(),
        });

        assert_eq!(player.quick_settings.sub_text_color, "0.5/0.25/1/0.75");
        assert_eq!(player.quick_settings.sub_text_size, 60.0);
        assert_eq!(player.quick_settings.sub_border_color, "0/0/0/1");
        assert_eq!(player.quick_settings.sub_border_size, 1.5);
        assert_eq!(player.quick_settings.sub_background_color, "1/1/1/0");
        assert_eq!(player.quick_settings.sub_font, "Helvetica Neue");
        assert_eq!(
            player.mpv_operation_log,
            vec![
                set_property("options/sub-color", MpvFormat::String, "0.5/0.25/1/0.75"),
                set_property("options/sub-font-size", MpvFormat::Double, "60"),
                set_property("options/sub-border-color", MpvFormat::String, "0/0/0/1"),
                set_property("options/sub-border-size", MpvFormat::Double, "1.5"),
                set_property("options/sub-back-color", MpvFormat::String, "1/1/1/0"),
                set_property("options/sub-font", MpvFormat::String, "Helvetica Neue"),
            ]
        );

        player.mpv_operation_log.clear();
        player.apply(PlayerCommand::SetSubtitleStyleColor {
            target: SubtitleStyleColorTarget::Text,
            color: "2/0/0/1".to_string(),
        });
        player.apply(PlayerCommand::SetSubtitleTextSize { size: 57.0 });
        assert!(player.mpv_operation_log.is_empty());
    }

    #[test]
    fn subtitle_font_selection_allows_iina_empty_default_value() {
        let mut player = PlayerState::default();
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::SetSubtitleFont {
            font: String::new(),
        });

        assert_eq!(player.quick_settings.sub_font, "");
        assert_eq!(
            player.mpv_operation_log,
            vec![set_property("options/sub-font", MpvFormat::String, "")]
        );
    }

    #[test]
    fn iina_aspect_parser_matches_the_reference_ratio_shape() {
        assert_eq!(parse_iina_aspect(" 2.35:1 ").as_deref(), Some("2.35:1"));
        assert!(parse_iina_aspect("16:0").is_none());
        assert!(parse_iina_aspect("16:9:1").is_none());
        assert!(parse_iina_aspect("16.:9").is_none());
        assert!(parse_iina_aspect("1e3:1").is_none());
        assert_eq!(
            iina_crop_dimensions(1920, 1080, 4.0 / 3.0),
            Some((1440, 1080))
        );
        assert_eq!(
            iina_crop_dimensions(1080, 1920, 16.0 / 9.0),
            Some((1080, 607))
        );
        assert_eq!(iina_hardware_decoder_value(0), "no");
        assert_eq!(iina_hardware_decoder_value(1), "auto");
        assert_eq!(iina_hardware_decoder_value(2), "auto-copy");
    }

    #[test]
    fn external_track_loading_uses_iina_audio_add_sub_add_and_sub_reload() {
        let mut player = PlayerState::default();
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::LoadExternalTrack {
            kind: ExternalTrackKind::Audio,
            path: "/tmp/commentary.flac".to_string(),
        });
        player.apply(PlayerCommand::LoadExternalTrack {
            kind: ExternalTrackKind::Subtitles,
            path: "/tmp/commentary.srt".to_string(),
        });

        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("audio-add", ["/tmp/commentary.flac"]),
                mpv_command("sub-add", ["/tmp/commentary.srt"]),
            ]
        );

        let mut existing_subtitle = player.tracks.subtitles[0].clone();
        existing_subtitle.id = 47;
        existing_subtitle.metadata.external = true;
        existing_subtitle.metadata.external_filename = Some("/tmp/commentary.srt".to_string());
        player.tracks.subtitles.push(existing_subtitle);
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::LoadExternalTrack {
            kind: ExternalTrackKind::Subtitles,
            path: "/tmp/commentary.srt".to_string(),
        });

        assert_eq!(
            player.mpv_operation_log,
            vec![mpv_command("sub-reload", ["47"])]
        );
        assert_eq!(
            player.osd_message.as_deref(),
            Some("Reloading External Subtitle")
        );
    }

    #[test]
    fn runtime_quick_setting_properties_refresh_player_snapshot() {
        let mut player = PlayerState::default();

        player.apply_mpv_property_changes(&[
            mpv_property_change("deinterlace", MpvFormat::Flag, Some("yes")),
            mpv_property_change("video-rotate", MpvFormat::Int64, Some("270")),
            mpv_property_change("contrast", MpvFormat::Int64, Some("-25")),
            mpv_property_change("audio-delay", MpvFormat::Double, Some("0.125")),
            mpv_property_change("sub-delay", MpvFormat::Double, Some("-0.375")),
            mpv_property_change("sub-scale", MpvFormat::Double, Some("1.5")),
            mpv_property_change("sub-pos", MpvFormat::Int64, Some("70")),
        ]);

        assert!(player.quick_settings.deinterlace);
        assert_eq!(player.quick_settings.video_rotate, 270);
        assert_eq!(player.quick_settings.contrast, -25);
        assert_eq!(player.quick_settings.audio_delay, 0.125);
        assert_eq!(player.quick_settings.sub_delay, -0.375);
        assert_eq!(player.quick_settings.sub_scale, 1.5);
        assert_eq!(player.quick_settings.sub_pos, 70);
    }

    #[test]
    fn mpv_operation_log_is_bounded() {
        let mut player = PlayerState::default();

        for _ in 0..(MAX_MPV_OPERATION_LOG + 12) {
            player.apply(PlayerCommand::ToggleMute);
        }

        assert_eq!(player.mpv_operation_log.len(), MAX_MPV_OPERATION_LOG);
    }

    #[test]
    fn mpv_operation_log_sequence_tracks_bounded_window() {
        let mut player = PlayerState::default();

        for _ in 0..(MAX_MPV_OPERATION_LOG + 12) {
            player.apply(PlayerCommand::ToggleMute);
        }

        assert_eq!(
            player.mpv_operation_log_next_sequence() - player.mpv_operation_log_first_sequence(),
            player.mpv_operation_log.len() as u64
        );
        assert_eq!(player.mpv_operation_log_first_sequence(), 12);
        assert_eq!(
            player.mpv_operation_log_next_sequence(),
            (MAX_MPV_OPERATION_LOG + 12) as u64
        );
    }

    #[test]
    fn speed_commands_clamp_and_update_osd() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));

        player.apply(PlayerCommand::MultiplySpeed { factor: 2.0 });
        assert_eq!(player.speed, 2.0);
        assert_eq!(player.osd_message.as_deref(), Some("Speed 2.00x"));

        player.apply(PlayerCommand::SetSpeed { speed: 0.0 });
        assert_eq!(player.speed, 0.01);
        assert_eq!(player.mpv_properties.speed, 0.01);
    }

    #[test]
    fn playlist_navigation_wraps_and_switches_current_item() {
        let mut player = PlayerState::default();
        player.open_media_batch(
            vec![
                "/tmp/current.mp4".to_string(),
                "/tmp/queued.mkv".to_string(),
            ],
            Ok(probed_media()),
        );

        player.apply(PlayerCommand::PlaylistNext);
        assert_eq!(player.current_url.as_deref(), Some("/tmp/queued.mkv"));
        assert_eq!(player.media_title, "queued.mkv");
        assert_eq!(player.mpv_properties.playlist_pos, 1);

        player.apply(PlayerCommand::PlaylistNext);
        assert_eq!(player.current_url.as_deref(), Some("/tmp/current.mp4"));
        assert_eq!(player.mpv_properties.playlist_pos, 0);

        player.apply(PlayerCommand::PlaylistPrev);
        assert_eq!(player.current_url.as_deref(), Some("/tmp/queued.mkv"));
        assert_eq!(player.mpv_properties.playlist_pos, 1);
    }

    #[test]
    fn select_playlist_item_sets_playlist_pos_property() {
        let mut player = PlayerState::default();
        player.open_media_batch(
            vec![
                "/tmp/current.mp4".to_string(),
                "/tmp/queued.mkv".to_string(),
            ],
            Ok(probed_media()),
        );
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::SelectPlaylistItem { index: 1 });
        assert_eq!(player.current_url.as_deref(), Some("/tmp/queued.mkv"));
        assert!(!player.playlist[0].current);
        assert!(player.playlist[1].current);
        assert!(player.playlist[1].playing);
        assert_eq!(player.mpv_properties.playlist_pos, 1);
        assert_eq!(
            player.mpv_operation_log,
            vec![set_property("playlist-pos", MpvFormat::Int64, "1")]
        );

        player.mpv_operation_log.clear();
        player.apply(PlayerCommand::SelectPlaylistItem { index: 99 });
        assert_eq!(player.current_url.as_deref(), Some("/tmp/queued.mkv"));
        assert!(player.mpv_operation_log.is_empty());
    }

    #[test]
    fn remove_playlist_item_records_playlist_remove_command() {
        let mut player = PlayerState::default();
        player.open_media_batch(
            vec![
                "/tmp/current.mp4".to_string(),
                "/tmp/queued.mkv".to_string(),
            ],
            Ok(probed_media()),
        );
        player.mpv_operation_log.clear();
        let previous_osd = player.osd_message.clone();

        player.apply(PlayerCommand::RemovePlaylistItem { index: 1 });
        assert_eq!(player.playlist.len(), 1);
        assert_eq!(player.playlist[0].id, 1);
        assert_eq!(player.current_url.as_deref(), Some("/tmp/current.mp4"));
        assert!(player.playlist[0].current);
        assert_eq!(player.mpv_properties.playlist_count, 1);
        assert_eq!(player.mpv_properties.playlist_pos, 0);
        assert_eq!(
            player.mpv_operation_log,
            vec![mpv_command("playlist-remove", ["1"])]
        );
        assert_eq!(player.osd_message, previous_osd);

        player.mpv_operation_log.clear();
        player.apply(PlayerCommand::RemovePlaylistItem { index: 99 });
        assert_eq!(player.playlist.len(), 1);
        assert!(player.mpv_operation_log.is_empty());
    }

    #[test]
    fn remove_playlist_items_uses_one_batch_with_iina_index_offsets() {
        let mut player = PlayerState::default();
        player.open_media_batch(
            vec![
                "/tmp/zero.mp4".to_string(),
                "/tmp/one.mp4".to_string(),
                "/tmp/two.mp4".to_string(),
                "/tmp/three.mp4".to_string(),
                "/tmp/four.mp4".to_string(),
            ],
            Err("probe failed".to_string()),
        );
        player.mpv_operation_log.clear();
        let previous_osd = player.osd_message.clone();

        player.apply(PlayerCommand::RemovePlaylistItems {
            indexes: vec![1, 3],
        });

        assert_eq!(
            player
                .playlist
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>(),
            vec!["zero.mp4", "two.mp4", "four.mp4"]
        );
        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("playlist-remove", ["1"]),
                mpv_command("playlist-remove", ["2"]),
            ]
        );
        assert_eq!(player.osd_message, previous_osd);
    }

    #[test]
    fn play_playlist_items_next_matches_iina_when_selection_contains_current() {
        let mut player = PlayerState::default();
        player.open_media_batch(
            vec![
                "/tmp/a.mp4".to_string(),
                "/tmp/b.mp4".to_string(),
                "/tmp/c.mp4".to_string(),
                "/tmp/d.mp4".to_string(),
                "/tmp/e.mp4".to_string(),
            ],
            Err("probe failed".to_string()),
        );
        player.apply(PlayerCommand::SelectPlaylistItem { index: 1 });
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::PlayPlaylistItemsNext {
            indexes: vec![0, 1, 3],
        });

        assert_eq!(
            player
                .playlist
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>(),
            vec!["b.mp4", "a.mp4", "d.mp4", "c.mp4", "e.mp4"]
        );
        assert!(player.playlist[0].current);
        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("playlist-move", ["0", "2"]),
                mpv_command("playlist-move", ["3", "2"]),
            ]
        );
    }

    #[test]
    fn insert_playlist_items_appends_then_moves_into_the_requested_row() {
        let mut player = PlayerState::default();
        player.open_media_batch(
            vec![
                "/tmp/current.mp4".to_string(),
                "/tmp/two.mp4".to_string(),
                "/tmp/three.mp4".to_string(),
            ],
            Err("probe failed".to_string()),
        );
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::InsertPlaylistItems {
            paths: vec!["/tmp/a.mp3".into(), "/tmp/b.mkv".into()],
            destination: 1,
        });

        assert_eq!(
            player
                .playlist
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>(),
            vec!["current.mp4", "a.mp3", "b.mkv", "two.mp4", "three.mp4"]
        );
        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("loadfile", ["/tmp/a.mp3", "append"]),
                mpv_command("loadfile", ["/tmp/b.mkv", "append"]),
                mpv_command("playlist-move", ["3", "1"]),
                mpv_command("playlist-move", ["4", "2"]),
            ]
        );
        assert_eq!(
            player.osd_message.as_deref(),
            Some("Added 2 Files to Playlist")
        );
    }

    #[test]
    fn plugin_mpv_commands_are_recorded_without_bypassing_executor_sync() {
        let mut player = PlayerState::default();
        player.apply(PlayerCommand::PluginMpvCommand {
            command: "script-message".to_string(),
            args: vec!["fixture".to_string(), "payload".to_string()],
        });
        player.apply(PlayerCommand::PluginMpvSet {
            property: "options/osd-level".to_string(),
            value: "2".to_string(),
        });

        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("script-message", ["fixture", "payload"]),
                set_property("options/osd-level", MpvFormat::String, "2"),
            ]
        );
    }

    #[test]
    fn plugin_core_uses_reference_absolute_seek_and_external_video_commands() {
        let mut player = PlayerState {
            current_url: Some("/tmp/current.mkv".to_string()),
            duration_seconds: 120.0,
            ..PlayerState::default()
        };

        player.apply(PlayerCommand::SeekAbsolute { seconds: 42.5 });
        player.apply(PlayerCommand::LoadExternalTrack {
            kind: ExternalTrackKind::Video,
            path: "/tmp/external.mp4".to_string(),
        });

        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("seek", ["42.5", "absolute"]),
                mpv_command("video-add", ["/tmp/external.mp4"]),
            ]
        );
    }

    #[test]
    fn key_binding_mpv_command_preserves_the_raw_input_action() {
        let mut player = PlayerState::default();
        let action = r#"script-message fixture \"two words\""#;

        player.apply(PlayerCommand::KeyBindingMpvCommand {
            action: action.to_string(),
        });
        player.apply(PlayerCommand::KeyBindingMpvCommand {
            action: "   ".to_string(),
        });
        player.apply(PlayerCommand::KeyBindingMpvCommand {
            action: "quit\0now".to_string(),
        });

        assert_eq!(player.mpv_operation_log, vec![mpv_command_string(action)]);
    }

    #[test]
    fn move_playlist_items_matches_iina_drag_offsets_and_preserves_current_item() {
        let mut player = PlayerState::default();
        player.open_media_batch(
            vec![
                "/tmp/one.mp4".to_string(),
                "/tmp/two.mkv".to_string(),
                "/tmp/three.mov".to_string(),
                "/tmp/four.webm".to_string(),
            ],
            Err("probe failed".to_string()),
        );
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::MovePlaylistItems {
            indexes: vec![0, 2],
            destination: 4,
        });

        assert_eq!(
            player
                .playlist
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>(),
            vec!["two.mkv", "four.webm", "one.mp4", "three.mov"]
        );
        assert_eq!(player.current_url.as_deref(), Some("/tmp/one.mp4"));
        assert!(player.playlist[2].current);
        assert_eq!(player.mpv_properties.playlist_pos, 2);
        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("playlist-move", ["0", "4"]),
                mpv_command("playlist-move", ["1", "4"]),
            ]
        );

        player.mpv_operation_log.clear();
        player.apply(PlayerCommand::MovePlaylistItems {
            indexes: vec![2, 3],
            destination: 2,
        });
        assert!(player.mpv_operation_log.is_empty());
    }

    #[test]
    fn clear_playlist_keeps_current_item_and_records_playlist_clear() {
        let mut player = PlayerState::default();
        player.open_media_batch(
            vec![
                "/tmp/current.mp4".to_string(),
                "/tmp/queued.mkv".to_string(),
            ],
            Ok(probed_media()),
        );
        player.apply(PlayerCommand::SelectPlaylistItem { index: 1 });
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::ClearPlaylist);
        assert_eq!(player.playlist.len(), 1);
        assert_eq!(player.playlist[0].id, 1);
        assert_eq!(player.playlist[0].path, "/tmp/queued.mkv");
        assert!(player.playlist[0].current);
        assert!(player.playlist[0].playing);
        assert_eq!(player.current_url.as_deref(), Some("/tmp/queued.mkv"));
        assert_eq!(player.mpv_properties.playlist_count, 1);
        assert_eq!(player.mpv_properties.playlist_pos, 0);
        assert_eq!(
            player.mpv_operation_log,
            vec![mpv_command("playlist-clear", std::iter::empty::<&str>())]
        );
        assert_eq!(player.osd_message.as_deref(), Some("Cleared Playlist"));
    }

    #[test]
    fn cycle_track_advances_selected_track() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));
        player.tracks.audio.push(Track {
            id: 99,
            title: "Commentary".to_string(),
            selected: false,
            metadata: TrackMetadata::default(),
        });
        player.tracks.subtitles.push(Track {
            id: 100,
            title: "Signs".to_string(),
            selected: false,
            metadata: TrackMetadata::default(),
        });

        player.apply(PlayerCommand::CycleTrack {
            kind: TrackSelectionKind::Audio,
        });
        assert_eq!(player.mpv_properties.aid, 99);

        player.apply(PlayerCommand::CycleTrack {
            kind: TrackSelectionKind::Subtitles,
        });
        assert_eq!(player.mpv_properties.sid, 12);
    }

    #[test]
    fn select_track_sets_exact_track_property() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));
        player.tracks.audio.push(Track {
            id: 99,
            title: "Commentary".to_string(),
            selected: false,
            metadata: TrackMetadata::default(),
        });
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::SelectTrack {
            kind: TrackSelectionKind::Audio,
            id: 99,
        });
        assert_eq!(player.mpv_properties.aid, 99);
        assert!(player.tracks.audio[1].selected);
        assert_eq!(
            player.mpv_operation_log,
            vec![set_property("aid", MpvFormat::Int64, "99")]
        );

        player.mpv_operation_log.clear();
        player.apply(PlayerCommand::SelectTrack {
            kind: TrackSelectionKind::Audio,
            id: 0,
        });
        assert_eq!(player.mpv_properties.aid, 0);
        assert!(player.tracks.audio.iter().all(|track| !track.selected));
        assert_eq!(
            player.mpv_operation_log,
            vec![set_property("aid", MpvFormat::Int64, "0")]
        );

        player.mpv_operation_log.clear();
        player.apply(PlayerCommand::SelectTrack {
            kind: TrackSelectionKind::Audio,
            id: 404,
        });
        assert_eq!(player.mpv_properties.aid, 0);
        assert!(player.mpv_operation_log.is_empty());
    }

    #[test]
    fn second_subtitle_selection_is_independent_from_primary_subtitle() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::SelectTrack {
            kind: TrackSelectionKind::SecondSubtitles,
            id: 12,
        });

        assert_eq!(player.second_subtitle_id, 12);
        assert_eq!(player.mpv_properties.secondary_sid, 12);
        assert_eq!(player.mpv_properties.sid, 0);
        assert_eq!(
            player.mpv_operation_log,
            vec![set_property("secondary-sid", MpvFormat::Int64, "12")]
        );

        player.mpv_operation_log.clear();
        player.apply(PlayerCommand::SelectTrack {
            kind: TrackSelectionKind::Subtitles,
            id: 12,
        });
        assert_eq!(player.mpv_properties.sid, 12);
        assert_eq!(player.mpv_properties.secondary_sid, 12);
    }

    #[test]
    fn primary_and_secondary_subtitle_tracks_swap_as_one_player_command() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));
        player.apply(PlayerCommand::SelectTrack {
            kind: TrackSelectionKind::Subtitles,
            id: 12,
        });
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::SwapSubtitleTracks);

        assert_eq!(player.mpv_properties.sid, 0);
        assert_eq!(player.mpv_properties.secondary_sid, 12);
        assert_eq!(
            player.mpv_operation_log,
            vec![
                set_property("sid", MpvFormat::Int64, "0"),
                set_property("secondary-sid", MpvFormat::Int64, "12"),
            ]
        );

        player.mpv_operation_log.clear();
        player.apply(PlayerCommand::SwapSubtitleTracks);
        assert_eq!(player.mpv_properties.sid, 12);
        assert_eq!(player.mpv_properties.secondary_sid, 0);
        assert_eq!(
            player.mpv_operation_log,
            vec![
                set_property("sid", MpvFormat::Int64, "12"),
                set_property("secondary-sid", MpvFormat::Int64, "0"),
            ]
        );
    }

    #[test]
    fn selecting_chapter_seeks_absolutely_and_resumes() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));
        player.paused = true;
        player.mpv_operation_log.clear();

        player.apply(PlayerCommand::SelectChapter { index: 1 });

        assert_eq!(player.position_seconds, 30.0);
        assert!(!player.paused);
        assert_eq!(player.osd_message.as_deref(), Some("Chapter: Middle"));
        assert_eq!(
            player.mpv_operation_log,
            vec![
                mpv_command("seek", ["30", "absolute"]),
                set_property("pause", MpvFormat::Flag, "false"),
            ]
        );
    }

    #[test]
    fn frame_step_pauses_and_moves_by_one_frame() {
        let mut player = PlayerState::default();
        player.open_media("/tmp/current.mp4".to_string(), Ok(probed_media()));

        player.apply(PlayerCommand::FrameStep { backwards: false });
        assert!(player.paused);
        assert!((player.position_seconds - (1.0 / 30.0)).abs() < f64::EPSILON);

        player.apply(PlayerCommand::FrameStep { backwards: true });
        assert_eq!(player.position_seconds, 0.0);
    }

    #[test]
    fn toggle_osc_updates_visibility_without_stale_osd() {
        let mut player = PlayerState::default();
        player.open_media(
            "/tmp/current.mp4".to_string(),
            Err("probe failed".to_string()),
        );

        player.apply(PlayerCommand::ToggleOsc);
        assert!(!player.osc_visible);
        assert_eq!(player.osd_message, None);

        player.apply(PlayerCommand::ToggleOsc);
        assert!(player.osc_visible);
        assert_eq!(player.osd_message, None);
    }

    #[test]
    fn saved_filters_add_and_remove_through_exact_mpv_operations() {
        let mut player = PlayerState::default();

        player.apply(PlayerCommand::AddFilter {
            kind: FilterKind::Video,
            filter: "hflip".to_string(),
        });
        assert_eq!(player.video_filters.len(), 1);
        assert_eq!(player.osd_message.as_deref(), Some("Added Filter: hflip"));
        assert_eq!(
            player.mpv_operation_log,
            vec![mpv_command("vf", ["add", "hflip"])]
        );

        player.mpv_operation_log.clear();
        player.apply(PlayerCommand::ToggleSavedFilter {
            kind: FilterKind::Video,
            name: "Mirror".to_string(),
            filter: "hflip".to_string(),
        });
        assert!(player.video_filters.is_empty());
        assert_eq!(player.osd_message.as_deref(), Some("Removed Filter"));
        assert_eq!(
            player.mpv_operation_log,
            vec![MpvClientOperation::RemoveFilterAt {
                name: "vf".to_string(),
                index: 0,
            }]
        );
    }

    #[test]
    fn saved_filter_matching_ignores_named_parameter_order() {
        let mut player = PlayerState::default();
        player.apply_mpv_filters(
            &[MpvFilter {
                name: "eq".to_string(),
                label: None,
                params: std::collections::BTreeMap::from([
                    ("contrast".to_string(), "1.2".to_string()),
                    ("gamma".to_string(), "0.8".to_string()),
                ]),
                string_format: "eq=contrast=1.2:gamma=0.8".to_string(),
            }],
            &[],
        );

        assert!(player.has_filter(FilterKind::Video, "eq=gamma=0.8:contrast=1.2"));
        player.apply(PlayerCommand::ToggleSavedFilter {
            kind: FilterKind::Video,
            name: "Cinema".to_string(),
            filter: "eq=gamma=0.8:contrast=1.2".to_string(),
        });
        assert!(player.video_filters.is_empty());
        assert_eq!(
            player.mpv_operation_log,
            vec![MpvClientOperation::RemoveFilterAt {
                name: "vf".to_string(),
                index: 0,
            }]
        );
    }
}
