use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Serialize;
use tauri::{AppHandle, Runtime, Url, WebviewWindow};

use crate::commands;
use crate::localization;
use crate::native_file::{self, FileRemovalMode};
use crate::native_prompt;
use crate::player::{PlayerCommand, PlayerState, SidebarTab};
use crate::state::AppState;
use crate::window_size::{resize_player_window, WindowResizeResult, WindowSizeAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IinaCommand {
    TogglePictureInPicture,
    OpenFile,
    OpenUrl,
    AudioPanel,
    VideoPanel,
    SubtitlePanel,
    PlaylistPanel,
    ChapterPanel,
    ToggleMusicMode,
    ToggleFlip,
    ToggleMirror,
    BiggerWindow,
    SmallerWindow,
    FitToScreen,
    SaveCurrentPlaylist,
    DeleteCurrentFile,
    DeleteCurrentFileHard,
    FindOnlineSubtitles,
    SaveDownloadedSubtitle,
}

impl FromStr for IinaCommand {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "toggle-pip" => Ok(Self::TogglePictureInPicture),
            "open-file" => Ok(Self::OpenFile),
            "open-url" => Ok(Self::OpenUrl),
            "audio-panel" => Ok(Self::AudioPanel),
            "video-panel" => Ok(Self::VideoPanel),
            "sub-panel" => Ok(Self::SubtitlePanel),
            "playlist-panel" => Ok(Self::PlaylistPanel),
            "chapter-panel" => Ok(Self::ChapterPanel),
            "toggle-music-mode" => Ok(Self::ToggleMusicMode),
            "toggle-flip" => Ok(Self::ToggleFlip),
            "toggle-mirror" => Ok(Self::ToggleMirror),
            "bigger-window" => Ok(Self::BiggerWindow),
            "smaller-window" => Ok(Self::SmallerWindow),
            "fit-to-screen" => Ok(Self::FitToScreen),
            "save-playlist" => Ok(Self::SaveCurrentPlaylist),
            "delete-current-file" => Ok(Self::DeleteCurrentFile),
            "delete-current-file-hard" => Ok(Self::DeleteCurrentFileHard),
            "find-online-subs" => Ok(Self::FindOnlineSubtitles),
            "save-downloaded-sub" => Ok(Self::SaveDownloadedSubtitle),
            command => Err(format!("Unknown IINA command: {command}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IinaCommandRoute {
    Player,
    TogglePictureInPicture,
    ToggleMusicMode,
    Window(WindowSizeAction),
    SaveCurrentPlaylist,
    SaveDownloadedSubtitle,
    DeleteCurrentFile(FileRemovalMode),
    Frontend(&'static str),
}

fn command_route(command: IinaCommand) -> IinaCommandRoute {
    match command {
        IinaCommand::AudioPanel
        | IinaCommand::VideoPanel
        | IinaCommand::SubtitlePanel
        | IinaCommand::PlaylistPanel
        | IinaCommand::ChapterPanel
        | IinaCommand::ToggleFlip
        | IinaCommand::ToggleMirror => IinaCommandRoute::Player,
        IinaCommand::TogglePictureInPicture => IinaCommandRoute::TogglePictureInPicture,
        IinaCommand::ToggleMusicMode => IinaCommandRoute::ToggleMusicMode,
        IinaCommand::BiggerWindow => IinaCommandRoute::Window(WindowSizeAction::Bigger),
        IinaCommand::SmallerWindow => IinaCommandRoute::Window(WindowSizeAction::Smaller),
        IinaCommand::FitToScreen => IinaCommandRoute::Window(WindowSizeAction::FitToScreen),
        IinaCommand::SaveCurrentPlaylist => IinaCommandRoute::SaveCurrentPlaylist,
        IinaCommand::SaveDownloadedSubtitle => IinaCommandRoute::SaveDownloadedSubtitle,
        IinaCommand::OpenFile => IinaCommandRoute::Frontend("open"),
        IinaCommand::OpenUrl => IinaCommandRoute::Frontend("open-url"),
        IinaCommand::DeleteCurrentFile => {
            IinaCommandRoute::DeleteCurrentFile(FileRemovalMode::Trash)
        }
        IinaCommand::DeleteCurrentFileHard => {
            IinaCommandRoute::DeleteCurrentFile(FileRemovalMode::Permanent)
        }
        IinaCommand::FindOnlineSubtitles => IinaCommandRoute::Frontend("find-online-subtitles"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeleteCurrentFilePlan {
    playlist_index: usize,
    path: PathBuf,
    mode: FileRemovalMode,
}

impl DeleteCurrentFilePlan {
    fn from_player(player: &PlayerState, mode: FileRemovalMode) -> Option<Self> {
        let playlist_index = usize::try_from(player.mpv_properties.playlist_pos).ok()?;
        player.playlist.get(playlist_index)?;
        let path = local_file_path(player.current_url.as_deref()?)?;
        Some(Self {
            playlist_index,
            path,
            mode,
        })
    }
}

fn local_file_path(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    if value.is_empty() || value == "-" {
        return None;
    }
    match Url::parse(value) {
        Ok(url) if url.scheme() == "file" => url.to_file_path().ok(),
        Ok(_) => None,
        Err(_) => Some(PathBuf::from(value)),
    }
}

pub(crate) fn can_delete_current_file(player: &PlayerState) -> bool {
    DeleteCurrentFilePlan::from_player(player, FileRemovalMode::Trash).is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeleteCurrentFileStep {
    RemovePlaylistItem,
    SyncMpv,
    RemoveFile,
}

#[derive(Debug)]
struct DeleteCurrentFileFailure {
    step: DeleteCurrentFileStep,
    message: String,
}

fn run_delete_current_file_steps(
    plan: &DeleteCurrentFilePlan,
    mut remove_playlist_item: impl FnMut(usize) -> Result<(), String>,
    mut sync_mpv: impl FnMut() -> Result<(), String>,
    mut remove_file: impl FnMut(&Path, FileRemovalMode) -> Result<(), String>,
) -> Result<(), DeleteCurrentFileFailure> {
    remove_playlist_item(plan.playlist_index).map_err(|message| DeleteCurrentFileFailure {
        step: DeleteCurrentFileStep::RemovePlaylistItem,
        message,
    })?;
    sync_mpv().map_err(|message| DeleteCurrentFileFailure {
        step: DeleteCurrentFileStep::SyncMpv,
        message,
    })?;
    remove_file(&plan.path, plan.mode).map_err(|message| DeleteCurrentFileFailure {
        step: DeleteCurrentFileStep::RemoveFile,
        message,
    })
}

pub(crate) fn delete_current_file<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    window: &WebviewWindow<R>,
    mode: FileRemovalMode,
) -> Result<Option<PlayerState>, String> {
    let session = state.player_session_for_window(window.label())?;
    let plan = {
        let player = session.player().lock().map_err(|error| error.to_string())?;
        DeleteCurrentFilePlan::from_player(&player, mode)
    };
    let Some(plan) = plan else {
        return Ok(None);
    };

    let mut removed_from_playlist = false;
    let result = run_delete_current_file_steps(
        &plan,
        |index| {
            let mut player = session.player().lock().map_err(|error| error.to_string())?;
            player.apply(PlayerCommand::RemovePlaylistItem { index });
            removed_from_playlist = true;
            Ok(())
        },
        || session.sync_mpv_executor_from_player().map(|_| ()),
        native_file::remove,
    );

    let snapshot = if removed_from_playlist {
        Some(
            session
                .player()
                .lock()
                .map(|player| player.clone())
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };

    let fatal_error = match result {
        Ok(()) => None,
        Err(DeleteCurrentFileFailure {
            step: DeleteCurrentFileStep::RemoveFile,
            message,
        }) => {
            let template = localization::menu_title("Error deleting: %@");
            let message = template.replacen("%@", &message, 1);
            let _ = native_prompt::show_error(&localization::menu_title("Error"), &message);
            None
        }
        Err(failure) => Some(failure.message),
    };

    if let Some(snapshot) = snapshot.as_ref() {
        commands::emit_player_state_for_session(app, session.label(), snapshot);
        crate::menu::refresh_iina_menu(app)?;
    }
    if let Some(error) = fatal_error {
        return Err(error);
    }
    Ok(snapshot)
}

fn player_command_for(command: IinaCommand, player: &PlayerState) -> Option<PlayerCommand> {
    match command {
        IinaCommand::AudioPanel => Some(PlayerCommand::ShowSidebar {
            tab: SidebarTab::Audio,
        }),
        IinaCommand::VideoPanel => Some(PlayerCommand::ShowSidebar {
            tab: SidebarTab::Video,
        }),
        IinaCommand::SubtitlePanel => Some(PlayerCommand::ShowSidebar {
            tab: SidebarTab::Subtitles,
        }),
        IinaCommand::PlaylistPanel => Some(PlayerCommand::ShowSidebar {
            tab: SidebarTab::Playlist,
        }),
        IinaCommand::ChapterPanel => Some(PlayerCommand::ShowSidebar {
            tab: SidebarTab::Chapters,
        }),
        IinaCommand::ToggleFlip => Some(PlayerCommand::SetVideoFlip {
            enabled: !player.quick_settings.video_flipped,
        }),
        IinaCommand::ToggleMirror => Some(PlayerCommand::SetVideoMirror {
            enabled: !player.quick_settings.video_mirrored,
        }),
        _ => None,
    }
}

fn selected_video_size(player: &PlayerState) -> Option<(f64, f64)> {
    player.video_size_for_display()
}

fn use_physical_resolution(state: &AppState) -> Result<bool, String> {
    state
        .preferences
        .lock()
        .map(|preferences| {
            preferences
                .values
                .get("usePhysicalResolution")
                .and_then(|value| value.as_bool())
                .unwrap_or(true)
        })
        .map_err(|error| error.to_string())
}

pub(crate) fn execute_window_size<R: Runtime>(
    state: &AppState,
    window: &WebviewWindow<R>,
    action: WindowSizeAction,
) -> Result<WindowResizeResult, String> {
    let video_size = {
        let session = state.player_session_for_window(window.label())?;
        let player = session.player().lock().map_err(|error| error.to_string())?;
        selected_video_size(&player)
    };
    resize_player_window(window, action, video_size, use_physical_resolution(state)?)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IinaCommandExecution {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player: Option<PlayerState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontend_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_resize: Option<WindowResizeResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_path: Option<String>,
}

impl IinaCommandExecution {
    fn empty(command: &str) -> Self {
        Self {
            command: command.to_string(),
            player: None,
            frontend_action: None,
            window_resize: None,
            saved_path: None,
        }
    }
}

#[tauri::command]
pub fn execute_iina_command(
    app: AppHandle,
    state: tauri::State<AppState>,
    window: WebviewWindow,
    action: String,
) -> Result<IinaCommandExecution, String> {
    let command = IinaCommand::from_str(&action)?;
    let window = commands::shortcut_player_window(&app, state.inner(), &window)?;
    let mut execution = IinaCommandExecution::empty(action.trim());
    match command_route(command) {
        IinaCommandRoute::Player => {
            let player_command = {
                let session = state.inner().player_session_for_window(window.label())?;
                let player = session.player().lock().map_err(|error| error.to_string())?;
                player_command_for(command, &player)
                    .ok_or_else(|| format!("IINA command is not a player action: {action}"))?
            };
            execution.player = Some(commands::player_command(state, window, player_command)?);
        }
        IinaCommandRoute::TogglePictureInPicture => {
            execution.player = Some(commands::toggle_picture_in_picture(app, state, window)?);
        }
        IinaCommandRoute::ToggleMusicMode => {
            execution.player = Some(commands::toggle_music_mode(app, state, window)?);
        }
        IinaCommandRoute::Window(action) => {
            execution.window_resize = Some(execute_window_size(state.inner(), &window, action)?);
        }
        IinaCommandRoute::SaveCurrentPlaylist => {
            execution.saved_path = commands::save_current_playlist(state, window)?;
        }
        IinaCommandRoute::SaveDownloadedSubtitle => {
            execution.saved_path = commands::save_downloaded_subtitle_dialog(app, state, window)?;
        }
        IinaCommandRoute::DeleteCurrentFile(mode) => {
            execution.player = delete_current_file(&app, state.inner(), &window, mode)?;
        }
        IinaCommandRoute::Frontend(action) => {
            execution.frontend_action = Some(action.to_string());
        }
    }
    Ok(execution)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{PlaylistItem, Track, TrackMetadata};
    use std::cell::RefCell;

    #[test]
    fn parser_covers_the_complete_iina_135_command_surface() {
        let commands = [
            "toggle-pip",
            "open-file",
            "open-url",
            "audio-panel",
            "video-panel",
            "sub-panel",
            "playlist-panel",
            "chapter-panel",
            "toggle-music-mode",
            "toggle-flip",
            "toggle-mirror",
            "bigger-window",
            "smaller-window",
            "fit-to-screen",
            "save-playlist",
            "delete-current-file",
            "delete-current-file-hard",
            "find-online-subs",
            "save-downloaded-sub",
        ];
        assert_eq!(commands.len(), 19);
        assert!(commands
            .iter()
            .all(|command| IinaCommand::from_str(command).is_ok()));
        assert!(IinaCommand::from_str("fit-to-screen now").is_err());
    }

    #[test]
    fn default_raw_gaps_share_window_and_frontend_routes() {
        assert_eq!(
            command_route(IinaCommand::from_str("fit-to-screen").unwrap()),
            IinaCommandRoute::Window(WindowSizeAction::FitToScreen)
        );
        assert_eq!(
            command_route(IinaCommand::from_str("smaller-window").unwrap()),
            IinaCommandRoute::Window(WindowSizeAction::Smaller)
        );
        assert_eq!(
            command_route(IinaCommand::from_str("bigger-window").unwrap()),
            IinaCommandRoute::Window(WindowSizeAction::Bigger)
        );
        assert_eq!(
            command_route(IinaCommand::from_str("find-online-subs").unwrap()),
            IinaCommandRoute::Frontend("find-online-subtitles")
        );
    }

    #[test]
    fn delete_commands_are_backend_routes_with_reference_file_semantics() {
        assert_eq!(
            command_route(IinaCommand::from_str("delete-current-file").unwrap()),
            IinaCommandRoute::DeleteCurrentFile(FileRemovalMode::Trash)
        );
        assert_eq!(
            command_route(IinaCommand::from_str("delete-current-file-hard").unwrap()),
            IinaCommandRoute::DeleteCurrentFile(FileRemovalMode::Permanent)
        );
    }

    fn player_with_current_item(path: &str) -> PlayerState {
        let mut player = PlayerState::default();
        player.current_url = Some(path.to_string());
        player.playlist = vec![PlaylistItem {
            id: 1,
            mpv_id: Some(1),
            path: path.to_string(),
            title: "Current".to_string(),
            duration_seconds: None,
            current: true,
            playing: true,
        }];
        player.mpv_properties.playlist_pos = 0;
        player
    }

    #[test]
    fn deletion_plan_requires_a_local_url_and_valid_current_playlist_position() {
        let player = player_with_current_item("file:///tmp/IINA%20Fixture.mp4");
        let plan = DeleteCurrentFilePlan::from_player(&player, FileRemovalMode::Trash).unwrap();
        assert_eq!(plan.playlist_index, 0);
        assert_eq!(plan.path, PathBuf::from("/tmp/IINA Fixture.mp4"));

        let remote = player_with_current_item("https://example.com/live.m3u8");
        assert!(DeleteCurrentFilePlan::from_player(&remote, FileRemovalMode::Trash).is_none());

        let mut missing = player_with_current_item("/tmp/current.mp4");
        missing.mpv_properties.playlist_pos = -1;
        assert!(DeleteCurrentFilePlan::from_player(&missing, FileRemovalMode::Trash).is_none());
    }

    #[test]
    fn deletion_pipeline_removes_playlist_then_syncs_mpv_then_touches_the_file() {
        let plan = DeleteCurrentFilePlan {
            playlist_index: 3,
            path: PathBuf::from("/tmp/current.mp4"),
            mode: FileRemovalMode::Permanent,
        };
        let events = RefCell::new(Vec::new());
        run_delete_current_file_steps(
            &plan,
            |index| {
                events.borrow_mut().push(format!("playlist:{index}"));
                Ok(())
            },
            || {
                events.borrow_mut().push("sync".to_string());
                Ok(())
            },
            |path, mode| {
                events
                    .borrow_mut()
                    .push(format!("file:{}:{mode:?}", path.display()));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            events.into_inner(),
            ["playlist:3", "sync", "file:/tmp/current.mp4:Permanent"]
        );
    }

    #[test]
    fn selected_video_size_applies_source_minus_user_rotation() {
        let mut player = PlayerState::default();
        player.tracks.video.clear();
        player.tracks.video.push(Track {
            id: 1,
            title: "Video".to_string(),
            selected: true,
            metadata: TrackMetadata {
                demux_width: Some(1920),
                demux_height: Some(1080),
                demux_rotation: Some(270),
                ..TrackMetadata::default()
            },
        });

        assert_eq!(selected_video_size(&player), Some((1080.0, 1920.0)));
        player.quick_settings.video_rotate = 180;
        assert_eq!(selected_video_size(&player), Some((1080.0, 1920.0)));
        player.quick_settings.video_rotate = 270;
        assert_eq!(selected_video_size(&player), Some((1920.0, 1080.0)));
    }
}
