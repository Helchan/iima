#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackDirective {
    Pause,
    Resume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipWindowBehavior {
    DoNothing,
    Hide,
    Minimize,
}

impl From<i64> for PipWindowBehavior {
    fn from(value: i64) -> Self {
        match value {
            1 => Self::Hide,
            2 => Self::Minimize,
            _ => Self::DoNothing,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PipWindowTransition {
    pub hide: bool,
    pub minimize: bool,
    pub show: bool,
    pub deminimize: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipToggleDirective {
    Enter,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowResizeDirective {
    ManuallyOpenedFile,
    AutomaticallyStartedFile,
    VideoReconfigured,
}

/// Per-player-window cause tracking for IINA's General > Pause/resume rules.
///
/// The cause flags are deliberately separate. A window only resumes playback
/// that it paused itself; media which was already paused stays paused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerWindowLifecycle {
    pub focused: bool,
    pub minimized: bool,
    pub fullscreen: bool,
    paused_due_to_inactive: bool,
    paused_due_to_minimized: bool,
    pip_hidden_window: bool,
    pip_minimized_window: bool,
    observed_playing: Option<bool>,
    observed_fullscreen: Option<bool>,
    observed_screen: Option<String>,
    always_float_enabled: bool,
    floating_due_to_preference: bool,
    media_window_presented: bool,
    initial_geometry_applied: bool,
    observed_resize_file_generation: Option<u64>,
    observed_video_reconfiguration_generation: Option<u64>,
}

impl Default for PlayerWindowLifecycle {
    fn default() -> Self {
        Self {
            focused: true,
            minimized: false,
            fullscreen: false,
            paused_due_to_inactive: false,
            paused_due_to_minimized: false,
            pip_hidden_window: false,
            pip_minimized_window: false,
            observed_playing: None,
            observed_fullscreen: None,
            observed_screen: None,
            always_float_enabled: false,
            floating_due_to_preference: false,
            media_window_presented: false,
            initial_geometry_applied: false,
            observed_resize_file_generation: None,
            observed_video_reconfiguration_generation: None,
        }
    }
}

impl PlayerWindowLifecycle {
    /// Tracks the retained Main-window half of IINA's per-PlayerCore window pair.
    ///
    /// IINA closes (but retains) MainWindowController when mpv returns to idle; it does not
    /// replace the visible player with InitialWindowController. The Tauri port reuses one native
    /// window for both surfaces, so the first player -> idle edge must hide that window exactly
    /// once. Explicitly presenting the welcome surface resets the edge without touching player
    /// state.
    pub fn observe_media_window(&mut self, media_window_presented: bool) -> bool {
        if media_window_presented {
            self.media_window_presented = true;
            return false;
        }
        if self.media_window_presented {
            self.media_window_presented = false;
            true
        } else {
            false
        }
    }

    pub fn prepare_initial_window(&mut self) {
        self.media_window_presented = false;
    }

    pub fn observe_screen(&mut self, screen: Option<String>) -> bool {
        let Some(screen) = screen else {
            return false;
        };
        let changed = self
            .observed_screen
            .as_ref()
            .is_some_and(|prior| prior != &screen);
        self.observed_screen = Some(screen);
        changed
    }

    pub fn claim_initial_geometry(&mut self) -> bool {
        if self.initial_geometry_applied {
            false
        } else {
            self.initial_geometry_applied = true;
            true
        }
    }

    pub fn pending_window_resize(
        &self,
        file_generation: u64,
        video_reconfiguration_generation: u64,
        opened_manually: bool,
        video_size: (f64, f64),
    ) -> Option<WindowResizeDirective> {
        valid_video_size_key(video_size)?;
        if self.observed_resize_file_generation != Some(file_generation) {
            return Some(if opened_manually {
                WindowResizeDirective::ManuallyOpenedFile
            } else {
                WindowResizeDirective::AutomaticallyStartedFile
            });
        }
        if self.observed_video_reconfiguration_generation != Some(video_reconfiguration_generation)
        {
            Some(WindowResizeDirective::VideoReconfigured)
        } else {
            None
        }
    }

    pub fn commit_window_resize(
        &mut self,
        file_generation: u64,
        video_reconfiguration_generation: u64,
        video_size: (f64, f64),
    ) -> bool {
        if valid_video_size_key(video_size).is_none() {
            return false;
        }
        self.observed_resize_file_generation = Some(file_generation);
        self.observed_video_reconfiguration_generation = Some(video_reconfiguration_generation);
        true
    }

    pub fn claim_pending_window_resize(
        &mut self,
        file_generation: u64,
        video_reconfiguration_generation: u64,
        opened_manually: bool,
        video_size: (f64, f64),
    ) -> Option<WindowResizeDirective> {
        let directive = self.pending_window_resize(
            file_generation,
            video_reconfiguration_generation,
            opened_manually,
            video_size,
        )?;
        self.commit_window_resize(
            file_generation,
            video_reconfiguration_generation,
            video_size,
        );
        Some(directive)
    }

    pub fn observe_always_float_on_top(
        &mut self,
        enabled: bool,
        playing: bool,
        fullscreen: bool,
    ) -> Option<bool> {
        let first_observation =
            self.observed_playing.is_none() || self.observed_fullscreen.is_none();
        let playback_changed = self.observed_playing.is_some_and(|prior| prior != playing);
        let fullscreen_changed = self
            .observed_fullscreen
            .is_some_and(|prior| prior != fullscreen);
        let preference_changed = self.always_float_enabled != enabled;
        self.observed_playing = Some(playing);
        self.observed_fullscreen = Some(fullscreen);
        self.always_float_enabled = enabled;

        if enabled {
            self.floating_due_to_preference = true;
            if first_observation || playback_changed || fullscreen_changed || preference_changed {
                Some(playing && !fullscreen)
            } else {
                None
            }
        } else if self.floating_due_to_preference {
            self.floating_due_to_preference = false;
            Some(false)
        } else {
            None
        }
    }

    pub fn begin_picture_in_picture(
        &mut self,
        behavior: PipWindowBehavior,
        window_transition_allowed: bool,
    ) -> PipWindowTransition {
        self.pip_hidden_window = false;
        self.pip_minimized_window = false;
        if !window_transition_allowed {
            return PipWindowTransition::default();
        }
        match behavior {
            PipWindowBehavior::DoNothing => PipWindowTransition::default(),
            PipWindowBehavior::Hide => {
                self.pip_hidden_window = true;
                PipWindowTransition {
                    hide: true,
                    ..PipWindowTransition::default()
                }
            }
            PipWindowBehavior::Minimize => {
                self.pip_minimized_window = true;
                PipWindowTransition {
                    minimize: true,
                    ..PipWindowTransition::default()
                }
            }
        }
    }

    pub fn finish_picture_in_picture(&mut self) -> PipWindowTransition {
        let transition = PipWindowTransition {
            show: self.pip_hidden_window,
            deminimize: self.pip_minimized_window,
            ..PipWindowTransition::default()
        };
        self.pip_hidden_window = false;
        self.pip_minimized_window = false;
        transition
    }

    pub fn pip_toggle_for_minimized(
        &self,
        minimized: bool,
        enabled: bool,
        pip_active: bool,
    ) -> Option<PipToggleDirective> {
        if !enabled || self.pip_minimized_window {
            return None;
        }
        match (minimized, pip_active) {
            (true, false) => Some(PipToggleDirective::Enter),
            (false, true) => Some(PipToggleDirective::Exit),
            _ => None,
        }
    }

    pub fn observe_minimized(
        &mut self,
        minimized: bool,
        enabled: bool,
        playing: bool,
    ) -> Option<PlaybackDirective> {
        if self.minimized == minimized {
            return None;
        }
        self.minimized = minimized;
        if minimized {
            if enabled && playing {
                self.paused_due_to_minimized = true;
                Some(PlaybackDirective::Pause)
            } else {
                None
            }
        } else if enabled && self.paused_due_to_minimized {
            self.paused_due_to_minimized = false;
            Some(PlaybackDirective::Resume)
        } else {
            None
        }
    }

    pub fn observe_focus(
        &mut self,
        focused: bool,
        enabled: bool,
        should_pause_when_unfocused: bool,
        playing: bool,
    ) -> Option<PlaybackDirective> {
        if self.focused == focused {
            return None;
        }
        self.focused = focused;
        if focused {
            if enabled && self.paused_due_to_inactive {
                self.paused_due_to_inactive = false;
                Some(PlaybackDirective::Resume)
            } else {
                None
            }
        } else if enabled && should_pause_when_unfocused && playing {
            self.paused_due_to_inactive = true;
            Some(PlaybackDirective::Pause)
        } else {
            None
        }
    }

    pub fn observe_fullscreen(
        &mut self,
        fullscreen: bool,
        play_when_entering: bool,
        pause_when_leaving: bool,
        playing: bool,
    ) -> Option<PlaybackDirective> {
        if self.fullscreen == fullscreen {
            return None;
        }
        self.fullscreen = fullscreen;
        if fullscreen && play_when_entering && !playing {
            Some(PlaybackDirective::Resume)
        } else if !fullscreen && pause_when_leaving && playing {
            Some(PlaybackDirective::Pause)
        } else {
            None
        }
    }
}

fn valid_video_size_key((width, height): (f64, f64)) -> Option<(u64, u64)> {
    (width.is_finite() && width > 0.0 && height.is_finite() && height > 0.0)
        .then_some((width.to_bits(), height.to_bits()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_media_window_hides_once_on_the_player_to_idle_edge() {
        let mut state = PlayerWindowLifecycle::default();
        assert!(!state.observe_media_window(false));
        assert!(!state.observe_media_window(true));
        assert!(!state.observe_media_window(true));
        assert!(state.observe_media_window(false));
        assert!(!state.observe_media_window(false));

        state.observe_media_window(true);
        state.prepare_initial_window();
        assert!(!state.observe_media_window(false));
    }

    #[test]
    fn screen_change_ignores_initial_and_unavailable_observations() {
        let mut state = PlayerWindowLifecycle::default();
        assert!(!state.observe_screen(None));
        assert!(!state.observe_screen(Some("display-a".to_string())));
        assert!(!state.observe_screen(Some("display-a".to_string())));
        assert!(state.observe_screen(Some("display-b".to_string())));
        assert!(!state.observe_screen(None));
        assert!(!state.observe_screen(Some("display-b".to_string())));
    }

    #[test]
    fn minimized_rule_resumes_only_a_pause_it_caused() {
        let mut state = PlayerWindowLifecycle::default();
        assert_eq!(state.observe_minimized(true, true, false), None);
        assert_eq!(state.observe_minimized(false, true, false), None);

        assert_eq!(
            state.observe_minimized(true, true, true),
            Some(PlaybackDirective::Pause)
        );
        assert_eq!(state.observe_minimized(true, true, false), None);
        assert_eq!(
            state.observe_minimized(false, true, false),
            Some(PlaybackDirective::Resume)
        );
    }

    #[test]
    fn inactive_rule_ignores_utility_windows_and_tracks_its_own_cause() {
        let mut state = PlayerWindowLifecycle::default();
        assert_eq!(state.observe_focus(false, true, false, true), None);
        assert_eq!(state.observe_focus(true, true, false, true), None);
        assert_eq!(
            state.observe_focus(false, true, true, true),
            Some(PlaybackDirective::Pause)
        );
        assert_eq!(
            state.observe_focus(true, true, false, false),
            Some(PlaybackDirective::Resume)
        );
    }

    #[test]
    fn fullscreen_rules_are_edge_triggered() {
        let mut state = PlayerWindowLifecycle::default();
        assert_eq!(
            state.observe_fullscreen(true, true, true, false),
            Some(PlaybackDirective::Resume)
        );
        assert_eq!(state.observe_fullscreen(true, true, true, true), None);
        assert_eq!(
            state.observe_fullscreen(false, true, true, true),
            Some(PlaybackDirective::Pause)
        );
        assert_eq!(state.observe_fullscreen(false, true, true, false), None);
    }

    #[test]
    fn pip_window_behavior_is_reversible_and_self_caused_minimize_does_not_toggle() {
        let mut state = PlayerWindowLifecycle::default();
        assert_eq!(
            state.begin_picture_in_picture(PipWindowBehavior::Hide, true),
            PipWindowTransition {
                hide: true,
                ..PipWindowTransition::default()
            }
        );
        assert_eq!(
            state.finish_picture_in_picture(),
            PipWindowTransition {
                show: true,
                ..PipWindowTransition::default()
            }
        );

        assert_eq!(
            state.begin_picture_in_picture(PipWindowBehavior::Minimize, true),
            PipWindowTransition {
                minimize: true,
                ..PipWindowTransition::default()
            }
        );
        assert_eq!(state.pip_toggle_for_minimized(true, true, true), None);
        assert_eq!(
            state.finish_picture_in_picture(),
            PipWindowTransition {
                deminimize: true,
                ..PipWindowTransition::default()
            }
        );
        assert_eq!(
            state.pip_toggle_for_minimized(true, true, false),
            Some(PipToggleDirective::Enter)
        );
        assert_eq!(
            state.pip_toggle_for_minimized(false, true, true),
            Some(PipToggleDirective::Exit)
        );
    }

    #[test]
    fn pip_does_not_hide_or_minimize_an_already_fullscreen_or_minimized_window() {
        let mut state = PlayerWindowLifecycle::default();
        assert_eq!(
            state.begin_picture_in_picture(PipWindowBehavior::Hide, false),
            PipWindowTransition::default()
        );
        assert_eq!(
            state.finish_picture_in_picture(),
            PipWindowTransition::default()
        );
    }

    #[test]
    fn always_float_tracks_playback_edges_without_overwriting_manual_state_when_disabled() {
        let mut state = PlayerWindowLifecycle::default();
        assert_eq!(state.observe_always_float_on_top(false, true, false), None);
        assert_eq!(
            state.observe_always_float_on_top(true, true, false),
            Some(true)
        );
        assert_eq!(state.observe_always_float_on_top(true, true, false), None);
        assert_eq!(
            state.observe_always_float_on_top(true, false, false),
            Some(false)
        );
        assert_eq!(
            state.observe_always_float_on_top(true, true, false),
            Some(true)
        );
        assert_eq!(
            state.observe_always_float_on_top(true, true, true),
            Some(false)
        );
        assert_eq!(
            state.observe_always_float_on_top(true, true, false),
            Some(true)
        );
        assert_eq!(
            state.observe_always_float_on_top(false, true, false),
            Some(false)
        );
        assert_eq!(state.observe_always_float_on_top(false, false, false), None);
    }

    #[test]
    fn initial_geometry_is_claimed_once_per_player_window() {
        let mut state = PlayerWindowLifecycle::default();
        assert!(state.claim_initial_geometry());
        assert!(!state.claim_initial_geometry());
    }

    #[test]
    fn window_resize_distinguishes_manual_and_automatic_file_starts() {
        let mut state = PlayerWindowLifecycle::default();
        assert_eq!(
            state.pending_window_resize(1, 0, true, (1920.0, 1080.0)),
            Some(WindowResizeDirective::ManuallyOpenedFile)
        );
        assert!(state.commit_window_resize(1, 0, (1920.0, 1080.0)));
        assert_eq!(
            state.pending_window_resize(1, 0, true, (1920.0, 1080.0)),
            None
        );
        assert_eq!(
            state.pending_window_resize(2, 0, false, (1280.0, 720.0)),
            Some(WindowResizeDirective::AutomaticallyStartedFile)
        );
    }

    #[test]
    fn window_resize_detects_video_reconfiguration_once() {
        let mut state = PlayerWindowLifecycle::default();
        assert!(state.commit_window_resize(3, 4, (1920.0, 1080.0)));
        assert_eq!(
            state.claim_pending_window_resize(3, 5, false, (1920.0, 1080.0)),
            Some(WindowResizeDirective::VideoReconfigured)
        );
        assert_eq!(
            state.pending_window_resize(3, 5, false, (1920.0, 1080.0)),
            None
        );

        assert_eq!(
            state.pending_window_resize(3, 5, false, (1080.0, 1920.0)),
            None
        );
    }

    #[test]
    fn invalid_video_sizes_are_never_committed_or_resized() {
        let mut state = PlayerWindowLifecycle::default();
        assert_eq!(state.pending_window_resize(1, 0, true, (0.0, 1080.0)), None);
        assert!(!state.commit_window_resize(1, 0, (f64::NAN, 1080.0)));
        assert_eq!(
            state.pending_window_resize(1, 0, false, (1920.0, 1080.0)),
            Some(WindowResizeDirective::AutomaticallyStartedFile)
        );
    }
}
