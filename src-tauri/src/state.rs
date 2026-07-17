use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::history::{PlaybackHistoryItem, PlaybackHistoryStore};
use crate::key_bindings::KeyBindingRepository;
use crate::mpv::{
    build_mpv_process_environment_plan, MpvExecutor, MpvExecutorStatus,
    MpvProcessEnvironmentBaseline, MpvStartupConfiguration, MpvWakeupHandle,
};
use crate::native_video;
use crate::online_subtitles::{
    OnlineSubtitleStore, OpenSubtitlesRateLimiter, OpenSubtitlesSession,
};
use crate::player::{LastPlayback, PlayerState, RecentDocument};
use crate::playlist_cache::PlaylistInfoCache;
use crate::plugins::PluginMenuDefinition;
use crate::preference_effects;
use crate::preferences::{preference_file_path, PreferenceStore};
use crate::window_lifecycle::PlayerWindowLifecycle;

#[derive(Debug, Clone)]
struct PlaybackPersistencePaths {
    preferences_file: PathBuf,
    history_file: PathBuf,
    watch_later_directory: PathBuf,
}

/// State owned by exactly one IINA-equivalent `PlayerCore`.
///
/// This deliberately excludes preferences, installed plugins, and menu definitions. Those are
/// application-wide in IINA, while playback, mpv event cursors, and subtitle searches are not.
pub struct PlayerSession {
    label: String,
    pub player: Mutex<PlayerState>,
    pub mpv_executor: Mutex<MpvExecutor>,
    pub online_subtitles: Mutex<OnlineSubtitleStore>,
    pub playlist_cache: Arc<Mutex<PlaylistInfoCache>>,
    mpv_applied_event_count: Mutex<usize>,
}

impl PlayerSession {
    pub fn new(
        label: &str,
        wakeup_handle: MpvWakeupHandle,
        startup_configuration: MpvStartupConfiguration,
    ) -> Self {
        let mut mpv_executor = MpvExecutor::with_runtime_status_and_wakeup_for_session(
            crate::mpv::libmpv_runtime_status(),
            label,
            wakeup_handle,
        );
        mpv_executor.configure_startup(startup_configuration);
        Self {
            label: label.to_string(),
            player: Mutex::new(PlayerState::default()),
            mpv_executor: Mutex::new(mpv_executor),
            online_subtitles: Mutex::new(OnlineSubtitleStore::default()),
            playlist_cache: Arc::new(Mutex::new(PlaylistInfoCache::default())),
            mpv_applied_event_count: Mutex::new(0),
        }
    }
}

pub enum PlayerSessionRef<'a> {
    Main(&'a AppState),
    Secondary {
        state: &'a AppState,
        session: Arc<PlayerSession>,
    },
}

pub struct AppState {
    /// Transitional primary-session aliases retained while command routing migrates to
    /// `PlayerSessionRegistry`. They represent the main player only.
    pub player: Mutex<PlayerState>,
    pub preferences: Mutex<PreferenceStore>,
    pub mpv_executor: Mutex<MpvExecutor>,
    pub online_subtitles: Mutex<OnlineSubtitleStore>,
    pub playlist_cache: Arc<Mutex<PlaylistInfoCache>>,
    pub opensubtitles_session: Mutex<Option<OpenSubtitlesSession>>,
    pub opensubtitles_rate_limiter: Arc<Mutex<OpenSubtitlesRateLimiter>>,
    pub plugin_menus: Mutex<Vec<PluginMenuDefinition>>,
    mpv_wakeup_handle: MpvWakeupHandle,
    recent_documents: Mutex<Vec<RecentDocument>>,
    mpv_applied_event_count: Mutex<usize>,
    player_sessions: Mutex<BTreeMap<String, Arc<PlayerSession>>>,
    next_player_session_id: Mutex<u64>,
    next_thumbnail_generation_id: AtomicU64,
    thumbnail_generations: Mutex<BTreeMap<String, u64>>,
    playback_persistence_paths: Mutex<Option<PlaybackPersistencePaths>>,
    playback_history: Mutex<PlaybackHistoryStore>,
    playback_history_revision: AtomicU64,
    saved_last_playback: Mutex<Option<LastPlayback>>,
    mpv_process_environment_baseline: Result<MpvProcessEnvironmentBaseline, String>,
    mpv_startup_configuration: Mutex<MpvStartupConfiguration>,
    external_open_received: AtomicBool,
    initial_launch_action_completed: AtomicBool,
    last_active_player_session_label: Mutex<String>,
    playlist_auto_add_at_startup: AtomicBool,
    pub player_window_lifecycle: Mutex<BTreeMap<String, PlayerWindowLifecycle>>,
}

impl Default for AppState {
    fn default() -> Self {
        let mpv_wakeup_handle = MpvWakeupHandle::default();
        Self {
            player: Mutex::new(PlayerState::default()),
            preferences: Mutex::new(PreferenceStore::default()),
            mpv_executor: Mutex::new(MpvExecutor::with_runtime_status_and_wakeup_for_session(
                crate::mpv::libmpv_runtime_status(),
                "main",
                mpv_wakeup_handle.clone(),
            )),
            online_subtitles: Mutex::new(OnlineSubtitleStore::default()),
            playlist_cache: Arc::new(Mutex::new(PlaylistInfoCache::default())),
            opensubtitles_session: Mutex::new(None),
            opensubtitles_rate_limiter: Arc::new(Mutex::new(OpenSubtitlesRateLimiter::default())),
            plugin_menus: Mutex::new(Vec::new()),
            mpv_wakeup_handle,
            recent_documents: Mutex::new(Vec::new()),
            mpv_applied_event_count: Mutex::new(0),
            player_sessions: Mutex::new(BTreeMap::new()),
            next_player_session_id: Mutex::new(0),
            next_thumbnail_generation_id: AtomicU64::new(1),
            thumbnail_generations: Mutex::new(BTreeMap::new()),
            playback_persistence_paths: Mutex::new(None),
            playback_history: Mutex::new(PlaybackHistoryStore::default()),
            playback_history_revision: AtomicU64::new(0),
            saved_last_playback: Mutex::new(None),
            mpv_process_environment_baseline: MpvProcessEnvironmentBaseline::capture(),
            mpv_startup_configuration: Mutex::new(MpvStartupConfiguration::default()),
            external_open_received: AtomicBool::new(false),
            initial_launch_action_completed: AtomicBool::new(false),
            last_active_player_session_label: Mutex::new("main".to_string()),
            playlist_auto_add_at_startup: AtomicBool::new(true),
            player_window_lifecycle: Mutex::new(BTreeMap::new()),
        }
    }
}

impl AppState {
    /// Captures policies whose reference UI explicitly requires an application restart.
    /// Preference writes after setup intentionally do not mutate this snapshot.
    pub fn capture_general_startup_policy(&self) -> Result<(), String> {
        let playlist_auto_add = self
            .preferences
            .lock()
            .map(|preferences| bool_preference(&preferences, "playlistAutoAdd", true))
            .map_err(|error| error.to_string())?;
        self.playlist_auto_add_at_startup
            .store(playlist_auto_add, Ordering::Release);
        Ok(())
    }

    pub fn playlist_auto_add_at_startup(&self) -> bool {
        self.playlist_auto_add_at_startup.load(Ordering::Acquire)
    }

    pub fn note_external_open_request(&self) {
        self.external_open_received.store(true, Ordering::Release);
    }

    pub fn claim_initial_launch_action(&self) -> bool {
        if self.external_open_received.load(Ordering::Acquire) {
            return false;
        }
        self.initial_launch_action_completed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn has_playing_media(&self) -> Result<bool, String> {
        if self
            .player
            .lock()
            .map(|player| player.current_url.is_some())
            .map_err(|error| error.to_string())?
        {
            return Ok(true);
        }
        let sessions = self
            .player_sessions
            .lock()
            .map(|sessions| sessions.values().cloned().collect::<Vec<_>>())
            .map_err(|error| error.to_string())?;
        for session in sessions {
            if session
                .player
                .lock()
                .map(|player| player.current_url.is_some())
                .map_err(|error| error.to_string())?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn idle_player_session_label(&self) -> Result<Option<String>, String> {
        if self
            .player
            .lock()
            .map(|player| player.current_url.is_none())
            .map_err(|error| error.to_string())?
        {
            return Ok(Some("main".to_string()));
        }
        let sessions = self
            .player_sessions
            .lock()
            .map(|sessions| {
                sessions
                    .iter()
                    .map(|(label, session)| (label.clone(), session.clone()))
                    .collect::<Vec<_>>()
            })
            .map_err(|error| error.to_string())?;
        for (label, session) in sessions {
            if session
                .player
                .lock()
                .map(|player| player.current_url.is_none())
                .map_err(|error| error.to_string())?
            {
                return Ok(Some(label));
            }
        }
        Ok(None)
    }

    pub fn note_player_session_active(&self, window_label: &str) -> Result<(), String> {
        let session_label = player_session_label_for_window(window_label);
        self.player_session_for_window(session_label)?;
        *self
            .last_active_player_session_label
            .lock()
            .map_err(|error| error.to_string())? = session_label.to_string();
        Ok(())
    }

    pub fn last_active_player_session_label(&self) -> Result<String, String> {
        let label = self
            .last_active_player_session_label
            .lock()
            .map(|label| label.clone())
            .map_err(|error| error.to_string())?;
        if label == "main" || self.player_session(&label)?.is_some() {
            Ok(label)
        } else {
            Ok("main".to_string())
        }
    }

    /// Allocates a self-contained player session for a future player window.
    ///
    /// The returned label is monotonically generated instead of trusting a window label supplied
    /// by untrusted URLs or plugins. Command routing will use this registry as it moves away from
    /// the legacy main-session fields above.
    pub fn create_player_session(&self) -> Result<(String, Arc<PlayerSession>), String> {
        let mut next_id = self
            .next_player_session_id
            .lock()
            .map_err(|error| error.to_string())?;
        let label = format!("player-{}", *next_id);
        *next_id += 1;

        let startup_configuration = self
            .mpv_startup_configuration
            .lock()
            .map(|configuration| configuration.clone())
            .map_err(|error| error.to_string())?;
        let session = Arc::new(PlayerSession::new(
            &label,
            self.mpv_wakeup_handle.clone(),
            startup_configuration,
        ));
        let recent_documents = self
            .recent_documents
            .lock()
            .map(|recent_documents| recent_documents.clone())
            .map_err(|error| error.to_string())?;
        let last_playback = self
            .saved_last_playback
            .lock()
            .map(|last_playback| last_playback.clone())
            .map_err(|error| error.to_string())?;
        session
            .player
            .lock()
            .map(|mut player| {
                player.recent_documents = recent_documents;
                player.last_playback = last_playback;
            })
            .map_err(|error| error.to_string())?;
        self.player_sessions
            .lock()
            .map_err(|error| error.to_string())?
            .insert(label.clone(), Arc::clone(&session));
        Ok((label, session))
    }

    pub fn configure_playback_persistence(
        &self,
        config_directory: &Path,
        data_directory: &Path,
    ) -> Result<(), String> {
        let watch_later_directory = data_directory.join("watch_later");
        fs::create_dir_all(&watch_later_directory).map_err(|error| {
            format!(
                "Unable to create watch later directory at {}: {error}",
                watch_later_directory.display()
            )
        })?;
        let paths = PlaybackPersistencePaths {
            preferences_file: preference_file_path(config_directory),
            history_file: data_directory.join("history.plist"),
            watch_later_directory,
        };
        *self
            .playback_history
            .lock()
            .map_err(|error| error.to_string())? =
            PlaybackHistoryStore::load_or_recover(&paths.history_file);
        *self
            .playback_persistence_paths
            .lock()
            .map_err(|error| error.to_string())? = Some(paths);
        self.refresh_mpv_startup_configuration()?;

        let last_playback = self
            .preferences
            .lock()
            .map(|preferences| restorable_last_playback(&preferences))
            .map_err(|error| error.to_string())?;
        self.restore_last_playback(last_playback)
    }

    pub fn refresh_mpv_startup_configuration(&self) -> Result<(), String> {
        let paths = self
            .playback_persistence_paths
            .lock()
            .map(|paths| paths.clone())
            .map_err(|error| error.to_string())?;
        let preferences = self
            .preferences
            .lock()
            .map(|preferences| preferences.clone())
            .map_err(|error| error.to_string())?;
        let resume_last_position = bool_preference(&preferences, "resumeLastPosition", true);
        let current_input_config_name = preferences
            .values
            .get("currentInputConfigName")
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("IINA Default")
            .to_string();
        let input_config_path = if let Some(paths) = &paths {
            let config_directory = paths
                .preferences_file
                .parent()
                .ok_or_else(|| "Preferences file has no app config directory".to_string())?;
            let key_bindings = KeyBindingRepository::new(config_directory);
            Some(match key_bindings.runtime_path(&current_input_config_name) {
                Ok(path) => path,
                Err(selection_error) => key_bindings.runtime_path("IINA Default").map_err(
                    |fallback_error| {
                        format!(
                            "Unable to resolve selected key binding profile {current_input_config_name:?}: {selection_error}; default fallback also failed: {fallback_error}"
                        )
                    },
                )?,
            })
        } else {
            None
        };
        let environment_baseline = self
            .mpv_process_environment_baseline
            .as_ref()
            .map_err(Clone::clone)?;
        let ytdl_search_path = preferences
            .values
            .get("ytdlSearchPath")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let http_proxy = preferences
            .values
            .get("httpProxy")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let configuration = MpvStartupConfiguration {
            watch_later_directory: paths.map(|paths| paths.watch_later_directory),
            resume_last_position,
            input_config_path,
            preference_options: preference_effects::startup_options(&preferences),
            process_environment: Some(build_mpv_process_environment_plan(
                environment_baseline,
                ytdl_search_path,
                http_proxy,
            )?),
        };
        *self
            .mpv_startup_configuration
            .lock()
            .map_err(|error| error.to_string())? = configuration.clone();
        self.mpv_executor
            .lock()
            .map(|mut executor| {
                executor.configure_startup(configuration.clone());
            })
            .map_err(|error| error.to_string())?;
        let sessions = self
            .player_sessions
            .lock()
            .map(|sessions| sessions.values().cloned().collect::<Vec<_>>())
            .map_err(|error| error.to_string())?;
        for session in sessions {
            session
                .mpv_executor
                .lock()
                .map(|mut executor| {
                    executor.configure_startup(configuration.clone());
                })
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn playback_history(&self) -> Result<Vec<PlaybackHistoryItem>, String> {
        let Some(paths) = self
            .playback_persistence_paths
            .lock()
            .map(|paths| paths.clone())
            .map_err(|error| error.to_string())?
        else {
            return Ok(Vec::new());
        };
        self.playback_history
            .lock()
            .map(|history| history.items(&paths.watch_later_directory))
            .map_err(|error| error.to_string())
    }

    pub fn remove_playback_history_entries(
        &self,
        ids: &[String],
    ) -> Result<Vec<PlaybackHistoryItem>, String> {
        let Some(paths) = self
            .playback_persistence_paths
            .lock()
            .map(|paths| paths.clone())
            .map_err(|error| error.to_string())?
        else {
            return Ok(Vec::new());
        };
        let mut history = self
            .playback_history
            .lock()
            .map_err(|error| error.to_string())?;
        if history.remove(ids) > 0 {
            history.save(&paths.history_file)?;
            self.playback_history_revision
                .fetch_add(1, Ordering::Relaxed);
        }
        Ok(history.items(&paths.watch_later_directory))
    }

    pub fn save_playback_position_for_window(&self, label: &str) -> Result<bool, String> {
        let resume_last_position = self
            .preferences
            .lock()
            .map(|preferences| bool_preference(&preferences, "resumeLastPosition", true))
            .map_err(|error| error.to_string())?;
        if !resume_last_position {
            return Ok(false);
        }
        let session = self.player_session_for_window(label)?;
        let last_playback = session
            .player()
            .lock()
            .map(|mut player| player.prepare_playback_position_save())
            .map_err(|error| error.to_string())?;
        let Some(last_playback) = last_playback else {
            return Ok(false);
        };
        session.sync_mpv_executor_from_player()?;

        let preferences = {
            let mut preferences = self.preferences.lock().map_err(|error| error.to_string())?;
            preferences.values.insert(
                "iinaLastPlayedFilePath".to_string(),
                serde_json::Value::String(last_playback.path.clone()),
            );
            preferences.values.insert(
                "iinaLastPlayedFilePosition".to_string(),
                serde_json::json!(last_playback.position_seconds),
            );
            preferences.clone()
        };
        self.save_preferences_if_configured(&preferences)?;
        *self
            .saved_last_playback
            .lock()
            .map_err(|error| error.to_string())? = Some(last_playback.clone());
        self.apply_last_playback_to_initial_players(Some(last_playback))?;
        Ok(true)
    }

    pub fn save_all_playback_positions(&self) -> Result<(), String> {
        let mut labels = vec!["main".to_string()];
        labels.extend(self.player_session_labels()?);
        let mut errors = Vec::new();
        for label in labels {
            if let Err(error) = self.save_playback_position_for_window(&label) {
                errors.push(format!("{label}: {error}"));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn restore_last_playback(&self, last_playback: Option<LastPlayback>) -> Result<(), String> {
        *self
            .saved_last_playback
            .lock()
            .map_err(|error| error.to_string())? = last_playback.clone();
        self.apply_last_playback_to_initial_players(last_playback)
    }

    fn apply_last_playback_to_initial_players(
        &self,
        last_playback: Option<LastPlayback>,
    ) -> Result<(), String> {
        self.player
            .lock()
            .map(|mut player| {
                if player.current_url.is_none() {
                    player.last_playback = last_playback.clone();
                }
            })
            .map_err(|error| error.to_string())?;
        let sessions = self
            .player_sessions
            .lock()
            .map(|sessions| sessions.values().cloned().collect::<Vec<_>>())
            .map_err(|error| error.to_string())?;
        for session in sessions {
            session
                .player
                .lock()
                .map(|mut player| {
                    if player.current_url.is_none() {
                        player.last_playback = last_playback.clone();
                    }
                })
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn record_loaded_media(&self, player: &Mutex<PlayerState>) -> Result<(), String> {
        let should_record = self
            .preferences
            .lock()
            .map(|preferences| bool_preference(&preferences, "recordPlaybackHistory", true))
            .map_err(|error| error.to_string())?;
        if !should_record {
            return Ok(());
        }
        let (path, name, duration_seconds) = player
            .lock()
            .map(|player| {
                (
                    player.current_url.clone(),
                    player.media_title.clone(),
                    player.duration_seconds,
                )
            })
            .map_err(|error| error.to_string())?;
        let Some(path) = path.filter(|path| !path.trim().is_empty()) else {
            return Ok(());
        };
        let Some(paths) = self
            .playback_persistence_paths
            .lock()
            .map(|paths| paths.clone())
            .map_err(|error| error.to_string())?
        else {
            return Ok(());
        };
        let mut history = self
            .playback_history
            .lock()
            .map_err(|error| error.to_string())?;
        history.record(path, name, duration_seconds);
        history.save(&paths.history_file)?;
        self.playback_history_revision
            .fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn playback_history_revision(&self) -> u64 {
        self.playback_history_revision.load(Ordering::Relaxed)
    }

    fn save_preferences_if_configured(&self, preferences: &PreferenceStore) -> Result<(), String> {
        let Some(paths) = self
            .playback_persistence_paths
            .lock()
            .map(|paths| paths.clone())
            .map_err(|error| error.to_string())?
        else {
            return Ok(());
        };
        preferences.save_to_file(&paths.preferences_file)
    }

    pub fn player_session(&self, label: &str) -> Result<Option<Arc<PlayerSession>>, String> {
        self.player_sessions
            .lock()
            .map(|sessions| sessions.get(label).cloned())
            .map_err(|error| error.to_string())
    }

    pub fn remove_player_session(&self, label: &str) -> Result<Option<Arc<PlayerSession>>, String> {
        let removed = self
            .player_sessions
            .lock()
            .map(|mut sessions| sessions.remove(label))
            .map_err(|error| error.to_string())?;
        let mut last_active = self
            .last_active_player_session_label
            .lock()
            .map_err(|error| error.to_string())?;
        if last_active.as_str() == label {
            *last_active = "main".to_string();
        }
        Ok(removed)
    }

    pub fn player_session_labels(&self) -> Result<Vec<String>, String> {
        self.player_sessions
            .lock()
            .map(|sessions| sessions.keys().cloned().collect())
            .map_err(|error| error.to_string())
    }

    pub fn begin_thumbnail_generation(&self, session_label: &str) -> Result<u64, String> {
        let generation_id = self
            .next_thumbnail_generation_id
            .fetch_add(1, Ordering::Relaxed);
        self.thumbnail_generations
            .lock()
            .map_err(|error| error.to_string())?
            .insert(session_label.to_string(), generation_id);
        Ok(generation_id)
    }

    pub fn cancel_thumbnail_generation(&self, session_label: &str) -> Result<(), String> {
        self.thumbnail_generations
            .lock()
            .map_err(|error| error.to_string())?
            .remove(session_label);
        Ok(())
    }

    pub fn cancel_all_thumbnail_generations(&self) -> Result<(), String> {
        self.thumbnail_generations
            .lock()
            .map_err(|error| error.to_string())?
            .clear();
        Ok(())
    }

    pub fn thumbnail_generation_is_current(
        &self,
        session_label: &str,
        generation_id: u64,
    ) -> Result<bool, String> {
        self.thumbnail_generations
            .lock()
            .map(|generations| generations.get(session_label) == Some(&generation_id))
            .map_err(|error| error.to_string())
    }

    pub fn restore_recent_documents(
        &self,
        recent_documents: Vec<RecentDocument>,
    ) -> Result<(), String> {
        let recent_documents = normalize_recent_documents(recent_documents);
        *self
            .recent_documents
            .lock()
            .map_err(|error| error.to_string())? = recent_documents.clone();
        self.apply_recent_documents_to_players(recent_documents)
    }

    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn record_recent_document(
        &self,
        path: String,
        title: String,
    ) -> Result<Vec<RecentDocument>, String> {
        let recent_documents = {
            let mut recent_documents = self
                .recent_documents
                .lock()
                .map_err(|error| error.to_string())?;
            recent_documents.retain(|item| item.path != path);
            recent_documents.insert(0, RecentDocument { id: 1, path, title });
            *recent_documents = normalize_recent_documents(recent_documents.clone());
            recent_documents.clone()
        };
        self.apply_recent_documents_to_players(recent_documents.clone())?;
        Ok(recent_documents)
    }

    pub fn recent_documents(&self) -> Result<Vec<RecentDocument>, String> {
        self.recent_documents
            .lock()
            .map(|recent_documents| recent_documents.clone())
            .map_err(|error| error.to_string())
    }

    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn clear_recent_documents(&self) -> Result<(), String> {
        *self
            .recent_documents
            .lock()
            .map_err(|error| error.to_string())? = Vec::new();
        self.apply_recent_documents_to_players(Vec::new())
    }

    pub fn clear_playback_history(&self) -> Result<(), String> {
        *self
            .recent_documents
            .lock()
            .map_err(|error| error.to_string())? = Vec::new();
        self.playback_history
            .lock()
            .map(|mut history| history.clear())
            .map_err(|error| error.to_string())?;
        self.playback_history_revision
            .fetch_add(1, Ordering::Relaxed);
        *self
            .saved_last_playback
            .lock()
            .map_err(|error| error.to_string())? = None;
        self.player
            .lock()
            .map(|mut player| {
                player.recent_documents.clear();
                player.last_playback = None;
            })
            .map_err(|error| error.to_string())?;
        let sessions = self
            .player_sessions
            .lock()
            .map(|sessions| sessions.values().cloned().collect::<Vec<_>>())
            .map_err(|error| error.to_string())?;
        for session in sessions {
            session
                .player
                .lock()
                .map(|mut player| {
                    player.recent_documents.clear();
                    player.last_playback = None;
                })
                .map_err(|error| error.to_string())?;
        }
        let preferences = {
            let mut preferences = self.preferences.lock().map_err(|error| error.to_string())?;
            preferences.values.insert(
                "iinaLastPlayedFilePath".to_string(),
                serde_json::Value::Null,
            );
            preferences.values.insert(
                "iinaLastPlayedFilePosition".to_string(),
                serde_json::json!(0.0),
            );
            preferences.clone()
        };
        self.save_preferences_if_configured(&preferences)?;
        Ok(())
    }

    fn apply_recent_documents_to_players(
        &self,
        recent_documents: Vec<RecentDocument>,
    ) -> Result<(), String> {
        self.player
            .lock()
            .map(|mut player| player.recent_documents = recent_documents.clone())
            .map_err(|error| error.to_string())?;
        let sessions = self
            .player_sessions
            .lock()
            .map(|sessions| sessions.values().cloned().collect::<Vec<_>>())
            .map_err(|error| error.to_string())?;
        for session in sessions {
            session
                .player
                .lock()
                .map(|mut player| player.recent_documents = recent_documents.clone())
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn player_session_for_window(&self, label: &str) -> Result<PlayerSessionRef<'_>, String> {
        let session_label = player_session_label_for_window(label);
        if session_label == "main" {
            return Ok(PlayerSessionRef::Main(self));
        }
        self.player_session(session_label)?
            .map(|session| PlayerSessionRef::Secondary {
                state: self,
                session,
            })
            .ok_or_else(|| format!("player session is not available for window {label}"))
    }

    /// Resolves keyboard/menu input with IINA's per-window ownership rules.
    ///
    /// Player and Mini Player windows are strict: a missing `player-*` owner is an error and
    /// must never leak into another player. Utility windows have no player of their own, so they
    /// follow IINA's `PlayerCore.lastActive` behavior and target the last active player session.
    pub fn shortcut_player_session_label(&self, window_label: &str) -> Result<String, String> {
        if is_player_input_window_label(window_label) {
            return self
                .player_session_for_window(window_label)
                .map(|session| session.label().to_string());
        }
        self.last_active_player_session_label()
    }

    pub fn player_session_for_shortcut_window(
        &self,
        window_label: &str,
    ) -> Result<PlayerSessionRef<'_>, String> {
        let label = self.shortcut_player_session_label(window_label)?;
        self.player_session_for_window(&label)
    }

    pub fn mpv_executor_status(&self) -> Result<MpvExecutorStatus, String> {
        let status = self
            .mpv_executor
            .lock()
            .map(|mut executor| executor.poll_status())
            .map_err(|error| error.to_string())?;
        self.apply_new_mpv_events(&status)?;
        Ok(status)
    }

    pub fn sync_mpv_executor_from_player(&self) -> Result<MpvExecutorStatus, String> {
        let (first_sequence, next_sequence, operations) = {
            let player = self.player.lock().map_err(|error| error.to_string())?;
            (
                player.mpv_operation_log_first_sequence(),
                player.mpv_operation_log_next_sequence(),
                player.mpv_operation_log.clone(),
            )
        };
        let status = self
            .mpv_executor
            .lock()
            .map(|mut executor| {
                executor.submit_player_operation_log(first_sequence, next_sequence, &operations)
            })
            .map_err(|error| error.to_string())?;
        self.apply_new_mpv_events(&status)?;
        Ok(status)
    }

    pub fn wait_for_mpv_wakeup(&self, timeout: Duration) -> bool {
        self.mpv_wakeup_handle.wait_timeout(timeout)
    }

    pub fn sync_all_mpv_executors_from_players(&self) -> Result<(), String> {
        self.sync_mpv_executor_from_player()?;
        let sessions = self
            .player_sessions
            .lock()
            .map(|sessions| sessions.values().cloned().collect::<Vec<_>>())
            .map_err(|error| error.to_string())?;
        for session in sessions {
            PlayerSessionRef::Secondary {
                state: self,
                session,
            }
            .sync_mpv_executor_from_player()?;
        }
        Ok(())
    }

    /// Queues one preference projection on every IINA-equivalent PlayerCore and immediately
    /// synchronizes initialized clients. Startup-only preferences deliberately produce no live
    /// operations; their refreshed startup configuration is inherited by future clients instead.
    pub fn apply_live_preference_effects(
        &self,
        key: &str,
        preferences: &PreferenceStore,
    ) -> Result<usize, String> {
        let shared_operations = preference_effects::live_operations(key, preferences);
        if shared_operations.is_empty() {
            return Ok(0);
        }
        let operations_for_player = |player: &PlayerState| {
            if key == "defaultEncoding" {
                preference_effects::subtitle_encoding_live_operations(
                    preferences,
                    player
                        .tracks
                        .subtitles
                        .iter()
                        .filter(|track| track.id != 0)
                        .map(|track| track.id),
                )
            } else {
                shared_operations.clone()
            }
        };
        self.player
            .lock()
            .map(|mut player| {
                let operations = operations_for_player(&player);
                player.record_preference_operations(operations);
            })
            .map_err(|error| error.to_string())?;
        let sessions = self
            .player_sessions
            .lock()
            .map(|sessions| sessions.values().cloned().collect::<Vec<_>>())
            .map_err(|error| error.to_string())?;
        for session in &sessions {
            session
                .player
                .lock()
                .map(|mut player| {
                    let operations = operations_for_player(&player);
                    player.record_preference_operations(operations);
                })
                .map_err(|error| error.to_string())?;
        }
        self.sync_all_mpv_executors_from_players()?;
        Ok(sessions.len() + 1)
    }

    fn apply_new_mpv_events(&self, status: &MpvExecutorStatus) -> Result<(), String> {
        let events = status.new_events.clone();
        *self
            .mpv_applied_event_count
            .lock()
            .map_err(|error| error.to_string())? = status.drained_event_count;
        let should_refresh_native_color = events.iter().any(|event| {
            matches!(event.name.as_str(), "file-loaded" | "video-reconfig")
                || event.property.as_ref().is_some_and(|property| {
                    matches!(
                        property.name.as_str(),
                        "video-params/primaries" | "video-params/gamma"
                    )
                })
        });
        let file_loaded = events.iter().any(|event| event.name == "file-loaded");

        if !events.is_empty()
            || !status.polled_properties.is_empty()
            || !status.track_list.is_empty()
            || !status.playlist.is_empty()
            || !status.audio_devices.is_empty()
            || status.client_running
        {
            let mut player = self.player.lock().map_err(|error| error.to_string())?;
            if !events.is_empty() {
                player.apply_mpv_events(&events);
            }
            if !status.polled_properties.is_empty() {
                player.apply_mpv_property_changes(&status.polled_properties);
            }
            if !status.track_list.is_empty() {
                player.apply_mpv_track_list(&status.track_list);
            }
            if !status.playlist.is_empty() {
                player.apply_mpv_playlist(&status.playlist);
            }
            if !status.audio_devices.is_empty() {
                player.apply_mpv_audio_devices(&status.audio_devices);
            }
            if status.client_running {
                player.apply_mpv_filters(&status.video_filters, &status.audio_filters);
            }
        }
        if should_refresh_native_color {
            native_video::request_color_refresh("main");
        }
        if file_loaded {
            self.record_loaded_media(&self.player)?;
        }
        Ok(())
    }
}

/// Resolves a player or its dedicated Mini Player window to the session that owns playback.
/// Main-player compatibility keeps the original `mini-player` label, while independent players
/// use `mini-player-player-N` labels so their native video surface can move independently.
pub fn player_session_label_for_window(label: &str) -> &str {
    match label {
        "main" | "mini-player" => "main",
        _ => label.strip_prefix("mini-player-").unwrap_or(label),
    }
}

pub fn is_player_input_window_label(label: &str) -> bool {
    label == "main"
        || label == "mini-player"
        || label.starts_with("player-")
        || label.starts_with("mini-player-player-")
}

fn normalize_recent_documents(mut recent_documents: Vec<RecentDocument>) -> Vec<RecentDocument> {
    recent_documents.retain(|item| !item.path.trim().is_empty());
    let mut seen = HashSet::new();
    recent_documents.retain(|document| seen.insert(document.path.clone()));
    recent_documents.truncate(10);
    for (index, document) in recent_documents.iter_mut().enumerate() {
        document.id = index + 1;
    }
    recent_documents
}

fn bool_preference(preferences: &PreferenceStore, key: &str, fallback: bool) -> bool {
    preferences
        .values
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(fallback)
}

fn restorable_last_playback(preferences: &PreferenceStore) -> Option<LastPlayback> {
    if !bool_preference(preferences, "recordRecentFiles", true)
        || !bool_preference(preferences, "resumeLastPosition", true)
    {
        return None;
    }
    let path = preferences
        .values
        .get("iinaLastPlayedFilePath")?
        .as_str()?
        .to_string();
    if path.is_empty() || !Path::new(&path).exists() {
        return None;
    }
    let position_seconds = preferences
        .values
        .get("iinaLastPlayedFilePosition")
        .and_then(serde_json::Value::as_f64)
        .filter(|position| position.is_finite())
        .unwrap_or(0.0)
        .max(0.0);
    let title = Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(&path)
        .to_string();
    Some(LastPlayback {
        path,
        title,
        position_seconds,
    })
}

impl PlayerSessionRef<'_> {
    pub fn label(&self) -> &str {
        match self {
            Self::Main(_) => "main",
            Self::Secondary { session, .. } => &session.label,
        }
    }

    pub fn player(&self) -> &Mutex<PlayerState> {
        match self {
            Self::Main(state) => &state.player,
            Self::Secondary { session, .. } => &session.player,
        }
    }

    pub fn mpv_executor(&self) -> &Mutex<MpvExecutor> {
        match self {
            Self::Main(state) => &state.mpv_executor,
            Self::Secondary { session, .. } => &session.mpv_executor,
        }
    }

    pub fn online_subtitles(&self) -> &Mutex<OnlineSubtitleStore> {
        match self {
            Self::Main(state) => &state.online_subtitles,
            Self::Secondary { session, .. } => &session.online_subtitles,
        }
    }

    pub fn playlist_cache(&self) -> &Arc<Mutex<PlaylistInfoCache>> {
        match self {
            Self::Main(state) => &state.playlist_cache,
            Self::Secondary { session, .. } => &session.playlist_cache,
        }
    }

    pub fn playlist_watch_later_directory(&self) -> Result<Option<PathBuf>, String> {
        let state = match self {
            Self::Main(state) | Self::Secondary { state, .. } => state,
        };
        state
            .playback_persistence_paths
            .lock()
            .map(|paths| {
                paths
                    .as_ref()
                    .map(|paths| paths.watch_later_directory.clone())
            })
            .map_err(|error| error.to_string())
    }

    pub fn prefetch_playlist_video_duration(&self) -> Result<bool, String> {
        let state = match self {
            Self::Main(state) | Self::Secondary { state, .. } => state,
        };
        state
            .preferences
            .lock()
            .map(|preferences| {
                preferences
                    .values
                    .get("prefetchPlaylistVideoDuration")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true)
            })
            .map_err(|error| error.to_string())
    }

    pub fn mpv_executor_status(&self) -> Result<MpvExecutorStatus, String> {
        let status = match self {
            Self::Main(state) => return state.mpv_executor_status(),
            Self::Secondary { session, .. } => session
                .mpv_executor
                .lock()
                .map(|mut executor| executor.poll_status())
                .map_err(|error| error.to_string())?,
        };
        self.apply_new_mpv_events(&status)?;
        Ok(status)
    }

    pub fn sync_mpv_executor_from_player(&self) -> Result<MpvExecutorStatus, String> {
        if let Self::Main(state) = self {
            return state.sync_mpv_executor_from_player();
        }
        let (first_sequence, next_sequence, operations) = {
            let player = self.player().lock().map_err(|error| error.to_string())?;
            (
                player.mpv_operation_log_first_sequence(),
                player.mpv_operation_log_next_sequence(),
                player.mpv_operation_log.clone(),
            )
        };
        let Self::Secondary { session, .. } = self else {
            unreachable!("main session returned above")
        };
        let status = session
            .mpv_executor
            .lock()
            .map(|mut executor| {
                executor.submit_player_operation_log(first_sequence, next_sequence, &operations)
            })
            .map_err(|error| error.to_string())?;
        self.apply_new_mpv_events(&status)?;
        Ok(status)
    }

    fn apply_new_mpv_events(&self, status: &MpvExecutorStatus) -> Result<(), String> {
        let Self::Secondary { state, session } = self else {
            return Ok(());
        };
        let events = status.new_events.clone();
        *session
            .mpv_applied_event_count
            .lock()
            .map_err(|error| error.to_string())? = status.drained_event_count;
        let should_refresh_native_color = events.iter().any(|event| {
            matches!(event.name.as_str(), "file-loaded" | "video-reconfig")
                || event.property.as_ref().is_some_and(|property| {
                    matches!(
                        property.name.as_str(),
                        "video-params/primaries" | "video-params/gamma"
                    )
                })
        });
        let file_loaded = events.iter().any(|event| event.name == "file-loaded");
        if !events.is_empty()
            || !status.polled_properties.is_empty()
            || !status.track_list.is_empty()
            || !status.playlist.is_empty()
            || !status.audio_devices.is_empty()
            || status.client_running
        {
            let mut player = session.player.lock().map_err(|error| error.to_string())?;
            if !events.is_empty() {
                player.apply_mpv_events(&events);
            }
            if !status.polled_properties.is_empty() {
                player.apply_mpv_property_changes(&status.polled_properties);
            }
            if !status.track_list.is_empty() {
                player.apply_mpv_track_list(&status.track_list);
            }
            if !status.playlist.is_empty() {
                player.apply_mpv_playlist(&status.playlist);
            }
            if !status.audio_devices.is_empty() {
                player.apply_mpv_audio_devices(&status.audio_devices);
            }
            if status.client_running {
                player.apply_mpv_filters(&status.video_filters, &status.audio_filters);
            }
        }
        if should_refresh_native_color {
            native_video::request_color_refresh(&session.label);
        }
        if file_loaded {
            state.record_loaded_media(&session.player)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{restorable_last_playback, AppState};
    use crate::history::mpv_watch_later_md5;
    use crate::key_bindings::KeyBindingRepository;
    use crate::mpv::{mpv_command, set_property, MpvFormat};
    use crate::player::{PlayerCommand, RecentDocument, Track, TrackMetadata};
    use crate::preferences::PreferenceStore;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;

    fn persistence_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "iima-state-persistence-{}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn player_sessions_are_labeled_and_isolated() {
        let state = AppState::default();
        let (first_label, first) = state.create_player_session().unwrap();
        let (second_label, second) = state.create_player_session().unwrap();

        assert_eq!(first_label, "player-0");
        assert_eq!(second_label, "player-1");
        assert!(std::sync::Arc::ptr_eq(
            &first,
            &state.player_session(&first_label).unwrap().unwrap()
        ));
        assert!(!std::sync::Arc::ptr_eq(&first, &second));

        first.player.lock().unwrap().media_title = "First session".to_string();
        assert_eq!(second.player.lock().unwrap().media_title, "IINA");
        first.playlist_cache.lock().unwrap().record_runtime(
            "/tmp/first.mp4",
            60.0,
            15.0,
            "First",
            "",
            "Artist",
        );
        assert_eq!(
            first
                .playlist_cache
                .lock()
                .unwrap()
                .snapshot(["/tmp/first.mp4"])
                .items[0]
                .duration_seconds,
            Some(60.0)
        );
        assert_eq!(
            second
                .playlist_cache
                .lock()
                .unwrap()
                .snapshot(["/tmp/first.mp4"])
                .items[0]
                .duration_seconds,
            None
        );
        assert_eq!(
            state
                .playlist_cache
                .lock()
                .unwrap()
                .snapshot(["/tmp/first.mp4"])
                .items[0]
                .duration_seconds,
            None
        );
        assert_eq!(
            state.player_session_labels().unwrap(),
            vec![first_label, second_label]
        );
        assert!(state.remove_player_session("player-0").unwrap().is_some());
        assert!(state.player_session("player-0").unwrap().is_none());
    }

    #[test]
    fn live_preference_effects_are_queued_for_every_player_session() {
        let state = AppState::default();
        let (_, first) = state.create_player_session().unwrap();
        let (_, second) = state.create_player_session().unwrap();
        let mut preferences = PreferenceStore::default();
        preferences
            .values
            .insert("hardwareDecoder".into(), json!(2));

        assert_eq!(
            state
                .apply_live_preference_effects("hardwareDecoder", &preferences)
                .unwrap(),
            3
        );
        let expected = set_property("hwdec", MpvFormat::String, "auto-copy");
        assert_eq!(
            state.player.lock().unwrap().mpv_operation_log.last(),
            Some(&expected)
        );
        assert_eq!(
            first.player.lock().unwrap().mpv_operation_log.last(),
            Some(&expected)
        );
        assert_eq!(
            second.player.lock().unwrap().mpv_operation_log.last(),
            Some(&expected)
        );
    }

    #[test]
    fn subtitle_encoding_is_set_before_reloading_each_sessions_own_tracks() {
        fn track(id: i64) -> Track {
            Track {
                id,
                title: format!("Subtitle {id}"),
                selected: false,
                metadata: TrackMetadata::default(),
            }
        }

        let state = AppState::default();
        let (_, first) = state.create_player_session().unwrap();
        let (_, second) = state.create_player_session().unwrap();
        state
            .player
            .lock()
            .unwrap()
            .tracks
            .subtitles
            .extend([track(5), track(9)]);
        first.player.lock().unwrap().tracks.subtitles.push(track(3));

        let mut preferences = PreferenceStore::default();
        preferences
            .values
            .insert("defaultEncoding".into(), json!("UTF-16LE"));
        assert_eq!(
            state
                .apply_live_preference_effects("defaultEncoding", &preferences)
                .unwrap(),
            3
        );

        assert_eq!(
            state.player.lock().unwrap().mpv_operation_log,
            vec![
                set_property("sub-codepage", MpvFormat::String, "UTF-16LE"),
                mpv_command("sub-reload", ["5"]),
                mpv_command("sub-reload", ["9"]),
            ]
        );
        assert_eq!(
            first.player.lock().unwrap().mpv_operation_log,
            vec![
                set_property("sub-codepage", MpvFormat::String, "UTF-16LE"),
                mpv_command("sub-reload", ["3"]),
            ]
        );
        assert_eq!(
            second.player.lock().unwrap().mpv_operation_log,
            vec![set_property("sub-codepage", MpvFormat::String, "UTF-16LE")]
        );
    }

    #[test]
    fn startup_only_preferences_refresh_future_configuration_without_hot_apply() {
        let state = AppState::default();
        state
            .preferences
            .lock()
            .unwrap()
            .values
            .insert("enableInitialVolume".into(), json!(true));
        state
            .preferences
            .lock()
            .unwrap()
            .values
            .insert("initialVolume".into(), json!(37));

        state.refresh_mpv_startup_configuration().unwrap();
        let configuration = state.mpv_startup_configuration.lock().unwrap().clone();
        assert!(configuration
            .preference_options
            .iter()
            .any(|option| option.name == "volume" && option.value == "37"));
        let preferences = state.preferences.lock().unwrap().clone();
        assert_eq!(
            state
                .apply_live_preference_effects("initialVolume", &preferences)
                .unwrap(),
            0
        );
        assert!(state.player.lock().unwrap().mpv_operation_log.is_empty());
    }

    #[test]
    fn playlist_auto_add_is_latched_once_for_the_running_application() {
        let state = AppState::default();
        state
            .preferences
            .lock()
            .unwrap()
            .values
            .insert("playlistAutoAdd".into(), json!(false));
        state.capture_general_startup_policy().unwrap();
        assert!(!state.playlist_auto_add_at_startup());

        state
            .preferences
            .lock()
            .unwrap()
            .values
            .insert("playlistAutoAdd".into(), json!(true));
        assert!(
            !state.playlist_auto_add_at_startup(),
            "a persisted UI change must wait for the next application setup"
        );
    }

    #[test]
    fn process_environment_preferences_refresh_future_configuration_without_hot_apply() {
        let state = AppState::default();
        {
            let mut preferences = state.preferences.lock().unwrap();
            preferences
                .values
                .insert("ytdlSearchPath".into(), json!("/opt/custom-ytdl"));
            preferences
                .values
                .insert("httpProxy".into(), json!("127.0.0.1:3128"));
        }

        state.refresh_mpv_startup_configuration().unwrap();
        let configuration = state.mpv_startup_configuration.lock().unwrap().clone();
        let environment = configuration
            .process_environment
            .expect("future mpv environment plan");
        assert_eq!(
            std::env::split_paths(&environment.path).next(),
            Some(PathBuf::from("/opt/custom-ytdl"))
        );
        assert_eq!(
            environment.http_proxy,
            Some(std::ffi::OsString::from("http://127.0.0.1:3128"))
        );

        let preferences = state.preferences.lock().unwrap().clone();
        for key in ["ytdlSearchPath", "httpProxy"] {
            assert_eq!(
                state
                    .apply_live_preference_effects(key, &preferences)
                    .unwrap(),
                0,
                "{key}"
            );
        }
        assert!(state.player.lock().unwrap().mpv_operation_log.is_empty());
    }

    #[test]
    fn initial_launch_action_is_one_shot_and_external_open_suppresses_it() {
        let state = AppState::default();
        assert!(state.claim_initial_launch_action());
        assert!(!state.claim_initial_launch_action());

        let externally_opened = AppState::default();
        externally_opened.note_external_open_request();
        assert!(!externally_opened.claim_initial_launch_action());
        assert!(!externally_opened.claim_initial_launch_action());
    }

    #[test]
    fn playing_and_idle_session_queries_cover_main_and_secondary_players() {
        let state = AppState::default();
        let (first_label, first) = state.create_player_session().unwrap();
        let (second_label, second) = state.create_player_session().unwrap();

        assert!(!state.has_playing_media().unwrap());
        assert_eq!(
            state.idle_player_session_label().unwrap().as_deref(),
            Some("main")
        );

        state.player.lock().unwrap().current_url = Some("/tmp/main.mp4".to_string());
        assert!(state.has_playing_media().unwrap());
        assert_eq!(
            state.idle_player_session_label().unwrap().as_deref(),
            Some(first_label.as_str())
        );

        first.player.lock().unwrap().current_url = Some("/tmp/first.mp4".to_string());
        assert_eq!(
            state.idle_player_session_label().unwrap().as_deref(),
            Some(second_label.as_str())
        );

        second.player.lock().unwrap().current_url = Some("/tmp/second.mp4".to_string());
        assert_eq!(state.idle_player_session_label().unwrap(), None);
    }

    #[test]
    fn last_active_player_tracks_main_secondary_and_mini_player_windows() {
        let state = AppState::default();
        let (label, _) = state.create_player_session().unwrap();

        assert_eq!(state.last_active_player_session_label().unwrap(), "main");
        state.note_player_session_active(&label).unwrap();
        assert_eq!(state.last_active_player_session_label().unwrap(), label);
        state
            .note_player_session_active("mini-player-player-0")
            .unwrap();
        assert_eq!(
            state.last_active_player_session_label().unwrap(),
            "player-0"
        );
        assert!(state.note_player_session_active("player-missing").is_err());

        state.remove_player_session("player-0").unwrap();
        assert_eq!(state.last_active_player_session_label().unwrap(), "main");
    }

    #[test]
    fn window_labels_route_to_the_expected_player_session() {
        let state = AppState::default();
        let (label, _) = state.create_player_session().unwrap();

        assert_eq!(
            state.player_session_for_window("main").unwrap().label(),
            "main"
        );
        assert_eq!(
            state
                .player_session_for_window("mini-player")
                .unwrap()
                .label(),
            "main"
        );
        assert_eq!(
            state.player_session_for_window(&label).unwrap().label(),
            label
        );
        assert_eq!(
            state
                .player_session_for_window("mini-player-player-0")
                .unwrap()
                .label(),
            "player-0"
        );
        assert!(state.player_session_for_window("player-missing").is_err());
    }

    #[test]
    fn shortcut_windows_are_strict_per_player_and_utilities_follow_last_active() {
        let state = AppState::default();
        let (first_label, _) = state.create_player_session().unwrap();
        let (second_label, _) = state.create_player_session().unwrap();

        assert_eq!(state.shortcut_player_session_label("main").unwrap(), "main");
        assert_eq!(
            state.shortcut_player_session_label("mini-player").unwrap(),
            "main"
        );
        assert_eq!(
            state.shortcut_player_session_label(&second_label).unwrap(),
            second_label
        );
        assert_eq!(
            state
                .shortcut_player_session_label("mini-player-player-1")
                .unwrap(),
            "player-1"
        );

        state.note_player_session_active(&first_label).unwrap();
        assert_eq!(
            state.shortcut_player_session_label("preferences").unwrap(),
            first_label
        );
        state
            .note_player_session_active("mini-player-player-1")
            .unwrap();
        assert_eq!(
            state
                .player_session_for_shortcut_window("playback-history")
                .unwrap()
                .label(),
            "player-1"
        );

        assert!(state
            .shortcut_player_session_label("player-missing")
            .is_err());
        assert!(state
            .shortcut_player_session_label("mini-player-player-missing")
            .is_err());
    }

    #[test]
    fn shortcut_session_mutations_do_not_cross_player_boundaries() {
        let state = AppState::default();
        let (first_label, first) = state.create_player_session().unwrap();
        let (second_label, second) = state.create_player_session().unwrap();

        state
            .player_session_for_shortcut_window("mini-player-player-1")
            .unwrap()
            .player()
            .lock()
            .unwrap()
            .apply(PlayerCommand::SetVolume { volume: 31.0 });
        assert_eq!(second.player.lock().unwrap().volume, 31.0);
        assert_eq!(first.player.lock().unwrap().volume, 100.0);
        assert_eq!(state.player.lock().unwrap().volume, 100.0);

        state.note_player_session_active(&first_label).unwrap();
        state
            .player_session_for_shortcut_window("preferences")
            .unwrap()
            .player()
            .lock()
            .unwrap()
            .apply(PlayerCommand::SetVolume { volume: 62.0 });
        assert_eq!(first.player.lock().unwrap().volume, 62.0);
        assert_eq!(second.player.lock().unwrap().volume, 31.0);
        assert_eq!(
            state.shortcut_player_session_label(&second_label).unwrap(),
            second_label
        );
    }

    #[test]
    fn recent_documents_are_shared_with_new_player_sessions() {
        let state = AppState::default();
        state
            .restore_recent_documents(vec![RecentDocument {
                id: 99,
                path: "/tmp/older.mp4".to_string(),
                title: "Older".to_string(),
            }])
            .unwrap();
        let (label, session) = state.create_player_session().unwrap();

        let documents = state
            .record_recent_document("/tmp/newer.mp4".to_string(), "Newer".to_string())
            .unwrap();
        assert_eq!(
            documents
                .iter()
                .map(|document| document.path.as_str())
                .collect::<Vec<_>>(),
            vec!["/tmp/newer.mp4", "/tmp/older.mp4"]
        );
        assert_eq!(state.player.lock().unwrap().recent_documents, documents);
        assert_eq!(session.player.lock().unwrap().recent_documents, documents);
        assert!(state.player_session(&label).unwrap().is_some());
    }

    #[test]
    fn clearing_playback_history_reaches_every_player_session() {
        let state = AppState::default();
        let (_, session) = state.create_player_session().unwrap();
        state
            .player
            .lock()
            .unwrap()
            .open_media("/tmp/main.mp4".to_string(), Err("probe failed".to_string()));
        session.player.lock().unwrap().open_media(
            "/tmp/secondary.mp4".to_string(),
            Err("probe failed".to_string()),
        );
        state
            .record_recent_document("/tmp/main.mp4".to_string(), "Main".to_string())
            .unwrap();

        state.clear_playback_history().unwrap();

        assert!(state.recent_documents().unwrap().is_empty());
        let main = state.player.lock().unwrap();
        assert!(main.recent_documents.is_empty());
        assert!(main.last_playback.is_none());
        drop(main);
        let secondary = session.player.lock().unwrap();
        assert!(secondary.recent_documents.is_empty());
        assert!(secondary.last_playback.is_none());
    }

    #[test]
    fn thumbnail_generations_cancel_stale_session_work() {
        let state = AppState::default();
        let first = state.begin_thumbnail_generation("main").unwrap();
        let secondary = state.begin_thumbnail_generation("player-0").unwrap();

        assert!(state
            .thumbnail_generation_is_current("main", first)
            .unwrap());
        assert!(state
            .thumbnail_generation_is_current("player-0", secondary)
            .unwrap());

        let replacement = state.begin_thumbnail_generation("main").unwrap();
        assert!(!state
            .thumbnail_generation_is_current("main", first)
            .unwrap());
        assert!(state
            .thumbnail_generation_is_current("main", replacement)
            .unwrap());
        assert!(state
            .thumbnail_generation_is_current("player-0", secondary)
            .unwrap());

        state.cancel_thumbnail_generation("main").unwrap();
        assert!(!state
            .thumbnail_generation_is_current("main", replacement)
            .unwrap());
    }

    #[test]
    fn startup_restores_an_existing_last_file_into_every_initial_player() {
        let root = persistence_root("restore");
        let config = root.join("config");
        let data = root.join("data");
        let media = root.join("Last Played.mp4");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&config).unwrap();
        fs::write(&media, b"fixture").unwrap();
        let state = AppState::default();
        {
            let mut preferences = state.preferences.lock().unwrap();
            preferences.values.insert(
                "iinaLastPlayedFilePath".to_string(),
                json!(media.to_string_lossy()),
            );
            preferences
                .values
                .insert("iinaLastPlayedFilePosition".to_string(), json!(37.25));
        }

        state
            .configure_playback_persistence(&config, &data)
            .unwrap();
        let restored = state.player.lock().unwrap().last_playback.clone().unwrap();
        assert_eq!(restored.path, media.to_string_lossy());
        assert_eq!(restored.title, "Last Played.mp4");
        assert_eq!(restored.position_seconds, 37.25);

        let (_, secondary) = state.create_player_session().unwrap();
        assert_eq!(
            secondary.player.lock().unwrap().last_playback,
            Some(restored)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_materializes_the_selected_key_binding_profile_for_every_player() {
        let root = persistence_root("key-bindings");
        let config = root.join("config");
        let data = root.join("data");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&config).unwrap();
        let repository = KeyBindingRepository::new(&config);
        repository
            .create_empty_profile("No Bindings")
            .expect("empty profile should create");
        let state = AppState::default();
        state
            .preferences
            .lock()
            .unwrap()
            .values
            .insert("currentInputConfigName".to_string(), json!("No Bindings"));

        state
            .configure_playback_persistence(&config, &data)
            .unwrap();
        let expected = config.join("input_conf/No Bindings.conf");
        assert_eq!(
            state
                .mpv_startup_configuration
                .lock()
                .unwrap()
                .input_config_path,
            Some(expected.clone())
        );
        assert_eq!(fs::read_to_string(&expected).unwrap(), "");

        let (_, secondary) = state.create_player_session().unwrap();
        assert_eq!(secondary.label, "player-0");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_selected_key_binding_profile_falls_back_to_iina_default() {
        let root = persistence_root("missing-key-bindings");
        let config = root.join("config");
        let data = root.join("data");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&config).unwrap();
        let state = AppState::default();
        state.preferences.lock().unwrap().values.insert(
            "currentInputConfigName".to_string(),
            json!("Removed Profile"),
        );

        state
            .configure_playback_persistence(&config, &data)
            .unwrap();
        let input_config_path = state
            .mpv_startup_configuration
            .lock()
            .unwrap()
            .input_config_path
            .clone()
            .expect("default input profile should materialize");
        assert!(input_config_path.ends_with("input_conf/.builtins/iina-default-input.conf"));
        assert_eq!(
            fs::read_to_string(input_config_path).unwrap(),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../参考/iina/iina/config/iina-default-input.conf"
            ))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_hides_last_playback_when_reference_preconditions_fail() {
        let root = persistence_root("preconditions");
        let media = root.join("current.mp4");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(&media, b"fixture").unwrap();
        let mut preferences = PreferenceStore::default();
        preferences.values.insert(
            "iinaLastPlayedFilePath".to_string(),
            json!(media.to_string_lossy()),
        );

        preferences
            .values
            .insert("recordRecentFiles".to_string(), json!(false));
        assert!(restorable_last_playback(&preferences).is_none());
        preferences
            .values
            .insert("recordRecentFiles".to_string(), json!(true));
        preferences
            .values
            .insert("resumeLastPosition".to_string(), json!(false));
        assert!(restorable_last_playback(&preferences).is_none());
        preferences
            .values
            .insert("resumeLastPosition".to_string(), json!(true));
        preferences.values.insert(
            "iinaLastPlayedFilePath".to_string(),
            json!(root.join("missing.mp4").to_string_lossy()),
        );
        assert!(restorable_last_playback(&preferences).is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn saving_playback_position_persists_iina_keys_and_queues_watch_later() {
        let root = persistence_root("save-position");
        let config = root.join("config");
        let data = root.join("data");
        let media = root.join("current.mp4");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&config).unwrap();
        fs::write(&media, b"fixture").unwrap();
        let state = AppState::default();
        state
            .configure_playback_persistence(&config, &data)
            .unwrap();
        {
            let mut player = state.player.lock().unwrap();
            player.current_url = Some(media.to_string_lossy().to_string());
            player.media_title = "Runtime title".to_string();
            player.position_seconds = 48.5;
        }

        assert!(state.save_playback_position_for_window("main").unwrap());

        let loaded = PreferenceStore::load_from_file(&config.join("preferences.json")).unwrap();
        assert_eq!(
            loaded.values.get("iinaLastPlayedFilePath"),
            Some(&json!(media.to_string_lossy()))
        );
        assert_eq!(
            loaded.values.get("iinaLastPlayedFilePosition"),
            Some(&json!(48.5))
        );
        assert!(state
            .player
            .lock()
            .unwrap()
            .mpv_operation_log
            .iter()
            .any(|operation| matches!(
                operation,
                crate::mpv::MpvClientOperation::Command { command, .. }
                    if command == "write-watch-later-config"
            )));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_loaded_history_persists_and_reads_watch_later_progress() {
        let root = persistence_root("history");
        let config = root.join("config");
        let data = root.join("data");
        let media = root.join("current.mp4");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&config).unwrap();
        fs::write(&media, b"fixture").unwrap();
        let state = AppState::default();
        state
            .configure_playback_persistence(&config, &data)
            .unwrap();
        {
            let mut player = state.player.lock().unwrap();
            player.current_url = Some(media.to_string_lossy().to_string());
            player.media_title = "Current".to_string();
            player.duration_seconds = 120.0;
        }

        state.record_loaded_media(&state.player).unwrap();
        let resume_name = mpv_watch_later_md5(&media.to_string_lossy());
        fs::write(
            data.join("watch_later").join(resume_name),
            "start=42.5\npause=yes\n",
        )
        .unwrap();

        let history = state.playback_history().unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].name, "Current");
        assert_eq!(history[0].duration_seconds, 120.0);
        assert_eq!(history[0].progress_seconds, Some(42.5));
        assert!(fs::read(data.join("history.plist"))
            .unwrap()
            .starts_with(b"bplist00"));

        state
            .preferences
            .lock()
            .unwrap()
            .values
            .insert("recordPlaybackHistory".to_string(), json!(false));
        state.player.lock().unwrap().current_url = Some("/tmp/ignored.mkv".to_string());
        state.record_loaded_media(&state.player).unwrap();
        assert_eq!(state.playback_history().unwrap().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn removing_history_entries_persists_and_only_advances_revision_on_change() {
        let root = persistence_root("remove-history");
        let config = root.join("config");
        let data = root.join("data");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&config).unwrap();
        let state = AppState::default();
        state
            .configure_playback_persistence(&config, &data)
            .unwrap();

        for (path, title) in [
            ("/tmp/history-first.mp4", "First"),
            ("/tmp/history-second.mp4", "Second"),
        ] {
            let mut player = state.player.lock().unwrap();
            player.current_url = Some(path.to_string());
            player.media_title = title.to_string();
            drop(player);
            state.record_loaded_media(&state.player).unwrap();
        }

        let history = state.playback_history().unwrap();
        let removed_id = history[0].id.clone();
        let revision = state.playback_history_revision();
        let remaining = state
            .remove_playback_history_entries(std::slice::from_ref(&removed_id))
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].name, "First");
        assert_eq!(state.playback_history_revision(), revision + 1);

        let persisted = AppState::default();
        persisted
            .configure_playback_persistence(&config, &data)
            .unwrap();
        let reloaded = persisted.playback_history().unwrap();
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].id, remaining[0].id);
        assert_eq!(reloaded[0].path, remaining[0].path);
        assert_eq!(reloaded[0].name, remaining[0].name);
        assert_eq!(reloaded[0].duration_seconds, remaining[0].duration_seconds);
        assert_eq!(
            reloaded[0].added_date.get(..19),
            remaining[0].added_date.get(..19)
        );

        let unchanged_revision = state.playback_history_revision();
        state
            .remove_playback_history_entries(&["missing".to_string()])
            .unwrap();
        assert_eq!(state.playback_history_revision(), unchanged_revision);
        fs::remove_dir_all(root).unwrap();
    }
}
