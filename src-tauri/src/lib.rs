mod about_window;
mod app_logging;
mod auxiliary_player_windows;
mod auxiliary_windows;
mod catalog;
mod commands;
mod history;
mod iina_commands;
mod inspector_window;
mod key_bindings;
mod launch;
mod localization;
mod media;
mod menu;
mod mpv;
mod native_application;
mod native_default_app;
mod native_file;
mod native_font_picker;
mod native_keychain;
mod native_menu;
mod native_open_panel;
mod native_pasteboard;
mod native_prompt;
mod native_recent_documents;
mod native_system_media;
mod native_text_encoding;
mod native_touch_bar;
mod native_updater;
mod native_video;
mod native_window_behavior;
mod online_subtitles;
mod player;
mod playlist_actions;
mod playlist_cache;
mod plugin_developer_tool;
mod plugin_global;
mod plugin_mpv_hooks;
mod plugin_sync;
mod plugin_utils;
mod plugin_websocket;
mod plugin_webview;
mod plugins;
mod preference_effects;
mod preferences;
mod state;
mod subtitle_autoload;
mod subtitle_color;
mod window_lifecycle;
mod window_size;

use about_window::{get_about_runtime, open_about_link, show_about};
use auxiliary_player_windows::{
    get_auxiliary_window_context, hide_auxiliary_window, request_player_plugin_runtime_refresh,
    show_filter_window, show_open_url_window, show_preferences_window,
};
use auxiliary_windows::{
    close_release_highlights, get_log_snapshot, open_iina_website, save_log_records,
    show_release_highlights,
};
use commands::{
    cancel_media_thumbnails, cancel_plugin_permissions, cancel_plugin_reinstall,
    capture_current_screenshot, check_for_updates, check_plugin_github_update,
    choose_advanced_config_directory, choose_screenshot_folder, choose_subtitle_font_dialog,
    claim_pending_plugin_install, clear_playback_history, clear_recent_documents,
    clear_saved_playback_progress, clear_thumbnail_cache, close_mini_player,
    complete_initial_launch, confirm_plugin_permissions, confirm_plugin_reinstall,
    create_key_binding_profile, delete_key_binding_profile, delete_screenshot_file,
    download_online_subtitles, download_plugin_subtitles, duplicate_key_binding_profile,
    enqueue_media_dialog, enqueue_media_paths, export_key_bindings_config,
    generate_media_thumbnails, get_key_binding_profile_path, get_libmpv_runtime_status,
    get_media_runtime, get_mpv_executor_status, get_mpv_observer_contract,
    get_mpv_playback_session_plan, get_native_video_renderer_status, get_playback_history,
    get_player_snapshot, get_player_window_status, get_plugin_page_contents,
    get_plugin_runtime_specs, get_plugins, get_preference_snapshot, get_preferences,
    get_replication_catalog, get_thumbnail_cache_stats, get_updater_status,
    has_pending_plugin_installs, import_key_binding_profile, install_plugin_dialog,
    install_plugin_from_github, jump_to_time_dialog, list_key_binding_profiles,
    load_external_track_dialog, login_opensubtitles_account, logout_opensubtitles_account,
    open_advanced_help, open_browser_extension, open_dropped_media_paths, open_log_directory,
    open_media, open_media_dialog, open_media_dialog_new_window, open_media_in_new_window,
    open_media_paths, open_playback_history_item, open_screenshot_file, player_command,
    playlist_add_url_dialog, playlist_can_paste_filenames, playlist_copy_items,
    playlist_copy_network_urls, playlist_insert_items, playlist_open_items_in_new_window,
    playlist_open_network_items, playlist_paste_items, playlist_play_next, playlist_remove_items,
    playlist_reveal_items, playlist_trash_items, plugin_file_delete, plugin_file_exists,
    plugin_file_handle_close, plugin_file_handle_offset, plugin_file_handle_open,
    plugin_file_handle_read, plugin_file_handle_read_to_end, plugin_file_handle_seek,
    plugin_file_handle_seek_to_end, plugin_file_handle_write, plugin_file_list, plugin_file_move,
    plugin_file_read, plugin_file_show_in_finder, plugin_file_trash, plugin_file_write,
    plugin_http_download, plugin_http_request, plugin_keychain_read, plugin_keychain_write,
    plugin_mpv_command, plugin_mpv_observe_property, plugin_mpv_set, probe_media_file,
    read_http_auth_credentials, read_key_binding_profile, refresh_player_menu,
    remove_playback_history_entries, remove_plugin, reorder_plugin,
    resize_player_window_by_magnification, restore_suppressed_alerts, reveal_key_binding_profile,
    reveal_playback_history_items, reveal_plugin_in_finder, reveal_screenshot_file,
    reveal_screenshot_folder, save_current_playlist, save_downloaded_subtitle_dialog,
    save_key_binding_profile, search_online_subtitles, set_default_application,
    set_mini_player_layout, set_plugin_enabled, set_plugin_menu_items, set_preference,
    set_window_fullscreen, set_window_presentation_mode, show_log_viewer, show_playback_history,
    smoke_libmpv_client, start_player_window_drag, submit_open_url, sync_mpv_executor,
    toggle_music_mode, toggle_picture_in_picture, toggle_window_always_on_top,
    toggle_window_fullscreen, update_plugin_from_github, write_http_auth_credentials,
};
use iina_commands::execute_iina_command;
use inspector_window::{get_inspector_snapshot, set_inspector_watch_properties, show_inspector};
use launch::{handle_dropped_paths, handle_opened_urls};
use menu::{build_iina_menu, handle_iina_menu_event};
use plugin_developer_tool::{
    get_plugin_developer_tool_context, set_plugin_developer_tool_realm_context,
};
use plugin_global::{
    plugin_global_create_player_instance, plugin_global_get_label, plugin_global_post_to_child,
    plugin_global_post_to_controller, plugin_global_register_controller,
    plugin_global_unregister_controller,
};
use plugin_mpv_hooks::{plugin_mpv_add_hook, plugin_mpv_continue_hook, plugin_mpv_remove_hooks};
use plugin_sync::{plugin_sync_prepare_grant, plugin_sync_revoke_grant};
use plugin_utils::{
    plugin_utils_ask, plugin_utils_choose_file, plugin_utils_exec, plugin_utils_file_in_path,
    plugin_utils_open, plugin_utils_prompt, plugin_utils_resolve_path,
};
use plugin_websocket::{
    plugin_websocket_create_server, plugin_websocket_send_text, plugin_websocket_start_server,
    plugin_websocket_stop,
};
use plugin_webview::{
    plugin_standalone_window_close, plugin_standalone_window_is_open,
    plugin_standalone_window_load, plugin_standalone_window_open,
    plugin_standalone_window_post_message, plugin_standalone_window_set_frame,
    plugin_standalone_window_set_property, plugin_standalone_window_set_simple_value,
    plugin_webview_cleanup, plugin_webview_cleanup_role, plugin_webview_prepare_page,
};
use preferences::{preference_file_path, PreferenceStore};
use serde::Serialize;
use state::AppState;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{Emitter, Manager};

const MPV_BACKGROUND_POLL_INTERVAL: Duration = Duration::from_millis(250);
const NATIVE_FILE_DRAG_EVENT: &str = "iima-native-file-drag";
const NATIVE_FILE_DROP_EVENT: &str = "iima-native-file-drop";
static EXIT_CLEANUP_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize)]
struct NativeFileDropPosition {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone, Serialize)]
struct NativeFileDropPayload {
    paths: Vec<String>,
    position: NativeFileDropPosition,
}

#[derive(Debug, Clone, Serialize)]
struct NativeFileDragPayload {
    phase: &'static str,
    paths: Vec<String>,
    position: Option<NativeFileDropPosition>,
    accepted: bool,
    has_playable_files: bool,
}

fn cleanup_before_exit(app: &tauri::AppHandle) {
    if EXIT_CLEANUP_STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    let state = app.state::<AppState>();
    native_window_behavior::remove_all_player_input_monitors();
    plugin_webview::cleanup_all(app);
    plugin_sync::cleanup_all();
    plugin_global::stop_all(app);
    plugin_mpv_hooks::stop_all(state.inner());
    plugin_websocket::stop_all(app);
    let _ = state.save_all_playback_positions();
    native_application::shutdown();
    native_touch_bar::shutdown();
    native_system_media::shutdown();
}

pub fn run() {
    let command_line_launch = match launch::command_line_request_from_args(std::env::args().skip(1))
    {
        Ok(request) => request,
        Err(error) => {
            eprintln!("iima: {error}");
            return;
        }
    };
    let app = tauri::Builder::default()
        .register_uri_scheme_protocol(plugin_webview::PLUGIN_WEBVIEW_SCHEME, |context, request| {
            plugin_webview::handle_protocol(context, request)
        })
        .register_uri_scheme_protocol(plugin_sync::PLUGIN_SYNC_SCHEME, |context, request| {
            plugin_sync::handle_protocol(context, request)
        })
        .plugin(tauri_plugin_dialog::init())
        .enable_macos_default_menu(false)
        .menu(build_iina_menu)
        .on_menu_event(|app, event| handle_iina_menu_event(app, event.id().as_ref()))
        .manage(AppState::default())
        .setup(move |app| {
            let config_directory = app.path().app_config_dir()?;
            let data_directory = app.path().app_data_dir()?;
            let preferences_path = preference_file_path(&config_directory);
            let preferences =
                PreferenceStore::load_compatible(&preferences_path).map_err(|error| {
                    std::io::Error::other(format!("failed to load preferences: {error}"))
                })?;
            commands::prepare_advanced_logging_directory(app.handle(), &preferences)
                .map_err(std::io::Error::other)?;
            let native_video_color_settings =
                native_video::color_settings_from_preferences(&preferences);
            let native_video_surface_settings =
                native_video::surface_settings_from_preferences(&preferences);
            let state = app.state::<AppState>();
            *state.preferences.lock().map_err(|error| {
                std::io::Error::other(format!("failed to lock preferences: {error}"))
            })? = preferences;
            state
                .capture_general_startup_policy()
                .map_err(std::io::Error::other)?;
            native_window_behavior::install_system_sleep_observer(app.handle());
            #[cfg(target_os = "macos")]
            {
                let (receive_beta, automatic_checks, check_interval) = state
                    .preferences
                    .lock()
                    .map_err(|error| {
                        std::io::Error::other(format!("failed to lock preferences: {error}"))
                    })
                    .map(|preferences| {
                        (
                            preferences
                                .values
                                .get("receiveBetaUpdate")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false),
                            preferences
                                .values
                                .get("updaterAutomaticallyChecks")
                                .and_then(serde_json::Value::as_bool),
                            preferences
                                .values
                                .get("updaterCheckInterval")
                                .and_then(serde_json::Value::as_f64),
                        )
                    })?;
                match native_updater::initialize(receive_beta) {
                    Ok(()) => {
                        if let Some(enabled) = automatic_checks {
                            native_updater::set_automatic_checks(enabled)
                                .map_err(std::io::Error::other)?;
                        }
                        if let Some(interval) = check_interval
                            .and_then(|value| native_updater::validated_update_interval(value).ok())
                        {
                            native_updater::set_check_interval(interval)
                                .map_err(std::io::Error::other)?;
                        }
                    }
                    Err(error) => eprintln!("iima: updater unavailable: {error}"),
                }
            }
            let (persisted_recent_documents, record_recent_files) = state
                .preferences
                .lock()
                .map_err(|error| {
                    std::io::Error::other(format!("failed to lock preferences: {error}"))
                })
                .map(|preferences| {
                    (
                        preferences
                            .values
                            .get("recentDocuments")
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!([])),
                        preferences
                            .values
                            .get("recordRecentFiles")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(true),
                    )
                })?;
            #[cfg(target_os = "macos")]
            {
                if !record_recent_files {
                    native_recent_documents::clear().map_err(std::io::Error::other)?;
                }
                let restore_report = if record_recent_files {
                    native_recent_documents::restore_if_empty(&persisted_recent_documents)
                        .map_err(std::io::Error::other)?
                } else {
                    native_recent_documents::RestoreReport::default()
                };
                let native_documents =
                    native_recent_documents::snapshot().map_err(std::io::Error::other)?;
                state
                    .restore_recent_documents(native_recent_documents::player_documents(
                        &native_documents,
                    ))
                    .map_err(std::io::Error::other)?;
                if !record_recent_files
                    || restore_report.restored
                    || restore_report.found_stale_bookmark
                {
                    let preferences = {
                        let mut preferences = state.preferences.lock().map_err(|error| {
                            std::io::Error::other(format!("failed to lock preferences: {error}"))
                        })?;
                        preferences.values.insert(
                            "recentDocuments".to_string(),
                            if record_recent_files {
                                native_recent_documents::persistence_value(&native_documents)
                            } else {
                                serde_json::json!([])
                            },
                        );
                        preferences.clone()
                    };
                    preferences
                        .save_to_file(&preferences_path)
                        .map_err(std::io::Error::other)?;
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                let recent_documents = if record_recent_files {
                    serde_json::from_value(persisted_recent_documents).unwrap_or_default()
                } else {
                    state
                        .preferences
                        .lock()
                        .map_err(|error| {
                            std::io::Error::other(format!("failed to lock preferences: {error}"))
                        })?
                        .values
                        .insert("recentDocuments".to_string(), serde_json::json!([]));
                    Vec::new()
                };
                state
                    .restore_recent_documents(recent_documents)
                    .map_err(std::io::Error::other)?;
            }
            state
                .configure_playback_persistence(&config_directory, &data_directory)
                .map_err(std::io::Error::other)?;
            menu::refresh_iina_menu(app.handle()).map_err(std::io::Error::other)?;
            native_application::install(app.handle()).map_err(std::io::Error::other)?;
            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("main") {
                // Tauri may expose the NSView one run-loop turn before AppKit assigns its
                // NSWindow. Preserve the configured player and let the visible open path perform
                // the required strict install before any queued libmpv operation is consumed.
                let _ = native_video::install_if_ready(
                    window.ns_view().map_err(std::io::Error::other)?,
                    "main",
                    &native_video_surface_settings,
                )
                .map_err(std::io::Error::other)?;
                native_video::configure_color(&native_video_color_settings, "main");
                commands::configure_player_window_behavior(state.inner(), &window)
                    .map_err(std::io::Error::other)?;
                native_window_behavior::install_player_input_monitor(
                    app.handle(),
                    window.ns_window().map_err(std::io::Error::other)?,
                    "main",
                )
                .map_err(std::io::Error::other)?;
            }
            native_system_media::initialize(app.handle(), state.inner())
                .map_err(std::io::Error::other)?;
            native_touch_bar::initialize(app.handle(), state.inner())
                .map_err(std::io::Error::other)?;
            start_mpv_executor_background_poll(app.handle().clone())?;
            if let Some(request) = command_line_launch {
                launch::handle_command_line_launch(app.handle(), request)
                    .map_err(std::io::Error::other)?;
            }
            auxiliary_windows::show_first_run_release_highlights(app.handle(), &data_directory)
                .map_err(std::io::Error::other)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_replication_catalog,
            get_media_runtime,
            get_mpv_observer_contract,
            get_mpv_playback_session_plan,
            get_libmpv_runtime_status,
            smoke_libmpv_client,
            get_mpv_executor_status,
            get_native_video_renderer_status,
            execute_iina_command,
            sync_mpv_executor,
            probe_media_file,
            generate_media_thumbnails,
            cancel_media_thumbnails,
            get_thumbnail_cache_stats,
            clear_thumbnail_cache,
            capture_current_screenshot,
            clear_recent_documents,
            clear_saved_playback_progress,
            clear_playback_history,
            get_playback_history,
            show_playback_history,
            remove_playback_history_entries,
            open_playback_history_item,
            reveal_playback_history_items,
            restore_suppressed_alerts,
            set_default_application,
            open_browser_extension,
            choose_advanced_config_directory,
            open_log_directory,
            show_log_viewer,
            show_about,
            get_about_runtime,
            open_about_link,
            show_inspector,
            get_inspector_snapshot,
            set_inspector_watch_properties,
            get_log_snapshot,
            save_log_records,
            show_release_highlights,
            close_release_highlights,
            open_iina_website,
            open_advanced_help,
            choose_screenshot_folder,
            export_key_bindings_config,
            list_key_binding_profiles,
            read_key_binding_profile,
            create_key_binding_profile,
            duplicate_key_binding_profile,
            import_key_binding_profile,
            save_key_binding_profile,
            delete_key_binding_profile,
            get_key_binding_profile_path,
            reveal_key_binding_profile,
            get_player_snapshot,
            get_player_window_status,
            refresh_player_menu,
            get_preferences,
            get_preference_snapshot,
            show_open_url_window,
            show_filter_window,
            show_preferences_window,
            get_auxiliary_window_context,
            hide_auxiliary_window,
            request_player_plugin_runtime_refresh,
            get_plugin_developer_tool_context,
            set_plugin_developer_tool_realm_context,
            get_updater_status,
            check_for_updates,
            read_http_auth_credentials,
            write_http_auth_credentials,
            login_opensubtitles_account,
            logout_opensubtitles_account,
            complete_initial_launch,
            set_preference,
            open_media,
            submit_open_url,
            open_media_paths,
            open_dropped_media_paths,
            open_media_dialog,
            open_media_dialog_new_window,
            open_media_in_new_window,
            enqueue_media_paths,
            enqueue_media_dialog,
            playlist_play_next,
            playlist_remove_items,
            playlist_insert_items,
            playlist_copy_items,
            playlist_can_paste_filenames,
            playlist_paste_items,
            playlist_add_url_dialog,
            playlist_open_items_in_new_window,
            playlist_trash_items,
            playlist_open_network_items,
            playlist_copy_network_urls,
            playlist_reveal_items,
            load_external_track_dialog,
            choose_subtitle_font_dialog,
            search_online_subtitles,
            download_online_subtitles,
            download_plugin_subtitles,
            jump_to_time_dialog,
            save_current_playlist,
            save_downloaded_subtitle_dialog,
            get_plugins,
            get_plugin_page_contents,
            get_plugin_runtime_specs,
            install_plugin_dialog,
            install_plugin_from_github,
            confirm_plugin_permissions,
            cancel_plugin_permissions,
            confirm_plugin_reinstall,
            cancel_plugin_reinstall,
            claim_pending_plugin_install,
            has_pending_plugin_installs,
            check_plugin_github_update,
            update_plugin_from_github,
            set_plugin_enabled,
            reorder_plugin,
            reveal_plugin_in_finder,
            remove_plugin,
            set_plugin_menu_items,
            plugin_http_request,
            plugin_http_download,
            plugin_keychain_read,
            plugin_keychain_write,
            plugin_file_exists,
            plugin_file_list,
            plugin_file_read,
            plugin_file_write,
            plugin_file_delete,
            plugin_file_trash,
            plugin_file_move,
            plugin_file_show_in_finder,
            plugin_file_handle_open,
            plugin_file_handle_offset,
            plugin_file_handle_seek,
            plugin_file_handle_seek_to_end,
            plugin_file_handle_read,
            plugin_file_handle_read_to_end,
            plugin_file_handle_write,
            plugin_file_handle_close,
            plugin_utils_file_in_path,
            plugin_utils_resolve_path,
            plugin_utils_exec,
            plugin_utils_ask,
            plugin_utils_prompt,
            plugin_utils_choose_file,
            plugin_utils_open,
            plugin_webview_prepare_page,
            plugin_webview_cleanup,
            plugin_webview_cleanup_role,
            plugin_sync_prepare_grant,
            plugin_sync_revoke_grant,
            plugin_standalone_window_load,
            plugin_standalone_window_open,
            plugin_standalone_window_close,
            plugin_standalone_window_is_open,
            plugin_standalone_window_set_property,
            plugin_standalone_window_set_frame,
            plugin_standalone_window_post_message,
            plugin_standalone_window_set_simple_value,
            plugin_global_register_controller,
            plugin_global_unregister_controller,
            plugin_global_create_player_instance,
            plugin_global_get_label,
            plugin_global_post_to_controller,
            plugin_global_post_to_child,
            plugin_websocket_create_server,
            plugin_websocket_start_server,
            plugin_websocket_send_text,
            plugin_websocket_stop,
            plugin_mpv_command,
            plugin_mpv_set,
            plugin_mpv_observe_property,
            plugin_mpv_add_hook,
            plugin_mpv_continue_hook,
            plugin_mpv_remove_hooks,
            player_command,
            toggle_music_mode,
            close_mini_player,
            toggle_picture_in_picture,
            reveal_screenshot_folder,
            reveal_screenshot_file,
            open_screenshot_file,
            delete_screenshot_file,
            set_mini_player_layout,
            set_window_presentation_mode,
            resize_player_window_by_magnification,
            start_player_window_drag,
            toggle_window_always_on_top,
            set_window_fullscreen,
            toggle_window_fullscreen
        ])
        .build(tauri::generate_context!())
        .expect("failed to build IINA Tauri replica");

    app.run(|app, event| {
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
        match event {
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::Focused(focused),
                ..
            } if label == "main"
                || label.starts_with("player-")
                || label.starts_with("mini-player") =>
            {
                let state = app.state::<AppState>();
                if focused {
                    let _ = state.note_player_session_active(&label);
                }
                let _ = commands::observe_player_window_lifecycle(
                    app,
                    state.inner(),
                    &label,
                    Some(focused),
                );
                let _ = native_system_media::sync(app, state.inner());
                let _ = native_touch_bar::sync_all(app, state.inner());
                if !label.starts_with("mini-player") {
                    commands::emit_plugin_host_event(
                        app,
                        &label,
                        "iina.window-main.changed",
                        serde_json::json!([focused]),
                    );
                }
                let _ = menu::refresh_iina_menu(app);
            }
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::Resized(_),
                ..
            } if label == "main" || label.starts_with("player-") => {
                let state = app.state::<AppState>();
                let _ = commands::observe_player_window_lifecycle(app, state.inner(), &label, None);
                if let Some(window) = app.get_webview_window(&label) {
                    commands::emit_plugin_window_rect_event(app, &window, "iina.window-resized");
                }
            }
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::Moved(_),
                ..
            } if label == "main" || label.starts_with("player-") => {
                let state = app.state::<AppState>();
                let _ = commands::observe_player_window_lifecycle(app, state.inner(), &label, None);
                if let Some(window) = app.get_webview_window(&label) {
                    commands::emit_plugin_window_rect_event(app, &window, "iina.window-moved");
                }
            }
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::ScaleFactorChanged { .. },
                ..
            } if label == "main" || label.starts_with("player-") => {
                let state = app.state::<AppState>();
                let _ = commands::observe_player_window_lifecycle(app, state.inner(), &label, None);
            }
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } if auxiliary_player_windows::is_reusable_auxiliary_window_label(&label) => {
                api.prevent_close();
                if let Some(window) = app.get_webview_window(&label) {
                    let _ = window.hide();
                }
            }
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } if label == about_window::ABOUT_WINDOW_LABEL => {
                api.prevent_close();
                if let Some(window) = app.get_webview_window(&label) {
                    let _ = window.hide();
                }
            }
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } if label == inspector_window::INSPECTOR_WINDOW_LABEL => {
                api.prevent_close();
                if let Some(window) = app.get_webview_window(&label) {
                    let _ = window.hide();
                }
            }
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } if plugin_developer_tool::is_plugin_developer_tool_label(&label) => {
                api.prevent_close();
                if let Some(window) = app.get_webview_window(&label) {
                    let _ = window.hide();
                }
            }
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } if plugin_webview::is_standalone_window_label(&label) => {
                api.prevent_close();
                if let Some(window) = app.get_webview_window(&label) {
                    let _ = window.hide();
                }
            }
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } if commands::mini_player_session_label(&label).is_some() => {
                api.prevent_close();
                let state = app.state::<AppState>();
                if let Some(session_label) = commands::mini_player_session_label(&label) {
                    let _ = commands::close_mini_player_window_for_session(
                        app,
                        state.inner(),
                        session_label,
                    );
                }
            }
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } if label == "main" || label.starts_with("player-") => {
                let state = app.state::<AppState>();
                commands::emit_plugin_host_event(
                    app,
                    &label,
                    "iina.window-will-close",
                    serde_json::json!([]),
                );
                let managed_by_plugin = app
                    .get_webview_window(&label)
                    .as_ref()
                    .is_some_and(commands::is_plugin_managed_player_window);
                if managed_by_plugin {
                    // JavascriptAPIGlobal owns these cores outside PlayerCore.playerCores. Plugin
                    // teardown must still destroy them instead of leaking an idle reusable player.
                    if let Some(window) = app.get_webview_window(&label) {
                        if let Ok(native_window) = window.ns_window() {
                            native_window_behavior::remove_player_input_monitor(native_window);
                        }
                    }
                    plugin_webview::cleanup_owner(app, &label);
                    plugin_sync::cleanup_owner(&label);
                    plugin_mpv_hooks::stop_window(state.inner(), &label);
                    plugin_websocket::stop_window(app, &label);
                    let _ = state.save_playback_position_for_window(&label);
                    commands::remove_player_window_lifecycle(app, state.inner(), &label);
                    return;
                }

                // IINA's MainWindowController is releasedWhenClosed=NO. Closing it stops media
                // and retains the now-idle PlayerCore so PlayerCore.newPlayerCore can reuse it.
                api.prevent_close();
                let should_quit =
                    commands::should_quit_after_closing_window(app, state.inner(), &label)
                        .unwrap_or(false);
                if let Err(error) =
                    commands::close_retained_player_window(app, state.inner(), &label)
                {
                    app_logging::log(
                        "window",
                        2,
                        &format!("Unable to close retained player window {label}: {error}"),
                    );
                    if let Some(window) = app.get_webview_window(&label) {
                        let _ = window.hide();
                    }
                }
                if should_quit {
                    app.exit(0);
                }
            }
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::Destroyed,
                ..
            } if label.starts_with("player-") => {
                let state = app.state::<AppState>();
                plugin_global::remove_window(&label);
                plugin_mpv_hooks::stop_window(state.inner(), &label);
                plugin_websocket::stop_window(app, &label);
                commands::remove_player_window_lifecycle(app, state.inner(), &label);
                if let Some(mini_window) =
                    app.get_webview_window(&commands::mini_player_label_for_session(&label))
                {
                    let _ = mini_window.hide();
                }
                let _ = state.remove_player_session(&label);
                native_video::remove_session(&label);
                native_touch_bar::remove_session(&label);
                let _ = native_system_media::sync(app, state.inner());
            }
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::Destroyed,
                ..
            } if label == "main" => {
                let state = app.state::<AppState>();
                native_touch_bar::remove_session("main");
                let _ = native_system_media::sync(app, state.inner());
            }
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit => {
                // macOS AppleEvent termination can proceed directly to RunEvent::Exit. Keep one
                // idempotent path for both events so grants, handles, sockets and native observers
                // are released on every graceful exit route.
                cleanup_before_exit(app);
            }
            tauri::RunEvent::Reopen {
                has_visible_windows,
                ..
            } => {
                let _ = commands::handle_application_reopen(app, has_visible_windows);
            }
            tauri::RunEvent::Opened { urls } => handle_opened_urls(app, urls),
            tauri::RunEvent::WindowEvent {
                label,
                event:
                    tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Enter {
                        paths,
                        position,
                    }),
                ..
            } => handle_native_file_drag(app, &label, "enter", paths, Some(position)),
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Over { position }),
                ..
            } => handle_native_file_drag(app, &label, "over", Vec::new(), Some(position)),
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Leave),
                ..
            } => handle_native_file_drag(app, &label, "leave", Vec::new(), None),
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, position }),
                ..
            } => handle_native_file_drop(app, &label, paths, position),
            _ => {}
        }
    });
}

fn native_file_drop_position(
    app: &tauri::AppHandle,
    window_label: &str,
    position: tauri::PhysicalPosition<f64>,
) -> NativeFileDropPosition {
    let scale_factor = app
        .get_webview_window(window_label)
        .and_then(|window| window.scale_factor().ok())
        .filter(|scale| *scale > 0.0)
        .unwrap_or(1.0);
    NativeFileDropPosition {
        x: position.x / scale_factor,
        y: position.y / scale_factor,
    }
}

fn native_file_drop_targets(paths: &[std::path::PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .filter(|path| !path.is_empty())
        .collect()
}

fn handle_native_file_drag(
    app: &tauri::AppHandle,
    window_label: &str,
    phase: &'static str,
    paths: Vec<std::path::PathBuf>,
    position: Option<tauri::PhysicalPosition<f64>>,
) {
    let targets = native_file_drop_targets(&paths);
    let plan = playlist_actions::plan_dropped_media(&targets);
    let accepted = !matches!(&plan, playlist_actions::DroppedMediaPlan::None);
    let has_playable_files = matches!(&plan, playlist_actions::DroppedMediaPlan::Open(_));
    let payload = NativeFileDragPayload {
        phase,
        paths: targets,
        position: position.map(|position| native_file_drop_position(app, window_label, position)),
        accepted,
        has_playable_files,
    };
    let _ = app.emit_to(window_label, NATIVE_FILE_DRAG_EVENT, payload);
}

fn handle_native_file_drop(
    app: &tauri::AppHandle,
    window_label: &str,
    paths: Vec<std::path::PathBuf>,
    position: tauri::PhysicalPosition<f64>,
) {
    if paths.iter().any(|path| {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("iinaplgz"))
    }) {
        handle_dropped_paths(app, window_label, paths);
        return;
    }
    let targets = native_file_drop_targets(&paths);
    if targets.is_empty() {
        return;
    }
    let payload = NativeFileDropPayload {
        paths: targets,
        position: native_file_drop_position(app, window_label, position),
    };
    if app
        .emit_to(window_label, NATIVE_FILE_DROP_EVENT, payload)
        .is_err()
    {
        handle_dropped_paths(app, window_label, paths);
    }
}

fn start_mpv_executor_background_poll(app: tauri::AppHandle) -> Result<(), std::io::Error> {
    std::thread::Builder::new()
        .name("iina-mpv-executor-poll".to_string())
        .spawn(move || {
            let mut emitted_history_revision = app.state::<AppState>().playback_history_revision();
            let mut emitted_plugin_mpv_event_cursors = HashMap::new();
            loop {
                let state = app.state::<AppState>();
                state.wait_for_mpv_wakeup(MPV_BACKGROUND_POLL_INTERVAL);
                let _ = state.sync_all_mpv_executors_from_players();
                let _ = native_system_media::sync(&app, state.inner());
                plugin_mpv_hooks::dispatch_pending(&app, state.inner());
                let _ = commands::emit_all_player_mpv_event_batches(
                    &app,
                    &mut emitted_plugin_mpv_event_cursors,
                );
                let _ = commands::sync_all_player_window_video_sizes(&app);
                let history_revision = state.playback_history_revision();
                if history_revision != emitted_history_revision {
                    emitted_history_revision = history_revision;
                    let _ = app.emit("iima-history-updated", history_revision);
                }
            }
        })
        .map(|_| ())
        .map_err(|error| {
            std::io::Error::other(format!("failed to start mpv executor poll thread: {error}"))
        })
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_jump_and_playlist_commands_are_registered_for_webviews() {
        let command_source = include_str!("commands.rs");
        let lib_source = include_str!("lib.rs");
        for command in [
            "jump_to_time_dialog",
            "save_current_playlist",
            "playlist_play_next",
            "playlist_remove_items",
            "playlist_insert_items",
            "playlist_copy_items",
            "playlist_paste_items",
            "playlist_add_url_dialog",
            "playlist_open_items_in_new_window",
            "playlist_trash_items",
            "playlist_open_network_items",
            "playlist_copy_network_urls",
            "playlist_reveal_items",
        ] {
            assert!(
                command_source.contains(&format!("pub fn {command}(")),
                "{command} must remain a Tauri command implementation"
            );
            assert!(
                lib_source.matches(command).count() >= 2,
                "{command} must be imported and registered in generate_handler"
            );
        }
        assert!(lib_source.contains("iima-native-file-drop"));
        assert!(lib_source.contains("position.x / scale_factor"));
        assert!(command_source.contains("playlist_can_paste_filenames"));
        let frontend = include_str!("../../src/main.js");
        assert!(frontend.contains(
            "event.stopPropagation();\n      await insertPlaylistItems(resolvedTargets, destination);"
        ));
        assert!(frontend.contains("playlistPasteDestination"));
    }

    #[test]
    fn plugin_reinstall_confirmation_is_single_owner_replayable_and_shared_by_all_entry_points() {
        let command_source = include_str!("commands.rs");
        let lib_source = include_str!("lib.rs");
        let launch_source = include_str!("launch.rs");
        let plugin_source = include_str!("plugins.rs");
        let frontend = include_str!("../../src/main.js");

        for command in [
            "confirm_plugin_permissions",
            "cancel_plugin_permissions",
            "confirm_plugin_reinstall",
            "cancel_plugin_reinstall",
            "claim_pending_plugin_install",
            "has_pending_plugin_installs",
        ] {
            assert!(
                command_source.contains(&format!("pub fn {command}(")),
                "{command} must remain a Tauri command implementation"
            );
            assert!(
                lib_source.matches(command).count() >= 2,
                "{command} must be imported and registered in generate_handler"
            );
        }
        let claim_command = command_source
            .split("pub fn claim_pending_plugin_install(")
            .nth(1)
            .and_then(|source| source.split("#[tauri::command]").next())
            .expect("claim command source");
        assert!(claim_command.contains("window: WebviewWindow"));
        assert!(claim_command.contains("PREFERENCES_WINDOW_LABEL"));

        let install_launch = launch_source
            .split("fn install_plugin_package(")
            .nth(1)
            .and_then(|source| source.split("fn is_plugin_package_path(").next())
            .expect("plugin launch handler source");
        let enqueue = install_launch
            .find("plugins::enqueue_install_notification(notification)")
            .expect("Finder install result must be queued");
        let existing_main = install_launch
            .find("app.get_webview_window(\"main\")")
            .expect("Finder install must only notify an existing main window");
        let emit = install_launch
            .find("app.emit_to(\"main\", PLUGIN_PACKAGE_EVENT, ())")
            .expect("Finder install must notify only the main window");
        assert!(enqueue < existing_main && existing_main < emit);
        assert!(install_launch.contains("app.get_webview_window(\"main\").is_some()"));
        assert!(!install_launch.contains(".show()"));
        assert!(!install_launch.contains(".set_focus()"));
        assert!(!install_launch.contains("app.emit(PLUGIN_PACKAGE_EVENT"));
        assert!(launch_source.matches("install_plugin_package(app,").count() >= 2);

        let drain_source = frontend
            .split("async function drainPendingPluginInstallNotifications()")
            .nth(1)
            .and_then(|source| {
                source
                    .split("async function installTauriMenuListeners()")
                    .next()
            })
            .expect("plugin install drain source");
        assert!(drain_source.contains("isPreferencesAuxiliaryWindow"));
        assert!(drain_source.contains("invoke(\"claim_pending_plugin_install\")"));
        assert!(drain_source.contains("drainingPluginInstallNotifications"));
        assert!(drain_source.contains("pluginInstallDrainRequested"));
        assert!(drain_source.contains("confirmationToken"));
        assert!(drain_source.contains("cancelConfirmationCommand"));
        assert!(drain_source.contains("\"cancel_plugin_permissions\""));
        assert!(drain_source.contains("\"cancel_plugin_reinstall\""));
        assert!(drain_source
            .contains("invoke(cancelConfirmationCommand, { token: confirmationToken })"));
        assert!(drain_source.contains("invoke(\"has_pending_plugin_installs\")"));
        assert!(drain_source.contains("drainPendingPluginInstalls: true"));

        let listener_source = frontend
            .split("async function installTauriMenuListeners()")
            .nth(1)
            .and_then(|source| {
                source
                    .split("async function refreshPluginRuntimes()")
                    .next()
            })
            .expect("Tauri listener source");
        let listen = listener_source
            .find("await tauriListen(\"iima-plugin-package\"")
            .expect("plugin package listener");
        let cold_replay = listener_source
            .find("await requestPendingPluginInstallDrain();")
            .expect("cold-start plugin notification replay");
        assert!(listen < cold_replay);
        let package_listener = &listener_source[listen..];
        assert!(!package_listener.contains("event.payload?.result"));

        let github_submit = frontend
            .split("async function submitPluginGithubPanel()")
            .nth(1)
            .and_then(|source| source.split("function renderPreferences()").next())
            .expect("GitHub plugin submit source");
        let hide = github_submit
            .find("els.pluginGithubModal.hidden = true")
            .expect("GitHub sheet closes before confirmation");
        let confirm = github_submit
            .find("await resolvePluginInstallResult(result)")
            .expect("GitHub duplicate must use shared confirmation");
        assert!(hide < confirm);
        assert!(github_submit.contains("els.pluginGithubModal.hidden = false"));

        assert!(plugin_source.contains("prepare_package_install_in_root"));
        assert!(plugin_source.contains("prepare_package_permission_install_in_root"));
        assert!(plugin_source.contains("prepare_from_github_permission_install_in_root"));
        assert!(frontend.contains("showPluginPermissionConfirmation"));
        assert!(frontend.contains("invoke(\"confirm_plugin_permissions\""));
        assert!(frontend.contains("invoke(\"cancel_plugin_permissions\""));
        let confirm_backend = plugin_source
            .split("fn confirm_plugin_reinstall_in_root(")
            .nth(1)
            .and_then(|source| source.split("fn validate_staged_plugin(").next())
            .expect("reinstall confirmation backend source");
        let transaction_lock = confirm_backend
            .find("plugin_filesystem_transaction_lock()")
            .expect("confirmation transaction lock");
        let current_set_check = confirm_backend
            .find("plugin_roots_for_identifier(")
            .expect("confirmation current plugin check");
        let atomic_replace = confirm_backend
            .find("replace_plugin_entry_locked(")
            .expect("confirmation locked replacement");
        assert!(transaction_lock < current_set_check && current_set_check < atomic_replace);
        assert!(frontend.contains("invoke(\"install_plugin_dialog\")"));
        assert!(frontend.contains("invoke(\"install_plugin_from_github\""));
    }

    #[test]
    fn preference_webviews_reconcile_after_listener_install_without_duplicate_plugin_refresh() {
        let command_source = include_str!("commands.rs");
        let lib_source = include_str!("lib.rs");
        let frontend = include_str!("../../src/main.js");

        assert!(command_source.contains("pub fn get_preference_snapshot("));
        assert!(lib_source.matches("get_preference_snapshot").count() >= 2);
        let snapshot_source = command_source
            .split("fn current_preferences_with_revision(")
            .nth(1)
            .and_then(|source| source.split("pub fn get_updater_status(").next())
            .expect("revisioned preference snapshot source");
        let lock = snapshot_source
            .find(".preferences")
            .expect("preference snapshot lock");
        let revision = snapshot_source
            .find("PREFERENCE_CHANGE_SEQUENCE")
            .expect("preference snapshot revision");
        assert!(lock < revision);
        assert!(snapshot_source.contains("load(Ordering::Acquire)"));
        assert!(snapshot_source.contains("saturating_sub(1)"));

        let listener_install = frontend
            .find("await installTauriMenuListeners();")
            .expect("listener install startup step");
        let reconcile = frontend
            .find("await reconcilePreferencesAfterListenerInstall();")
            .expect("post-listener preference handshake");
        assert!(listener_install < reconcile);
        let reconcile_source = frontend
            .split("async function reconcilePreferencesAfterListenerInstall()")
            .nth(1)
            .and_then(|source| {
                source
                    .split("function applyFrontendPreferenceChange(")
                    .next()
            })
            .expect("preference handshake source");
        assert!(reconcile_source.contains("invoke(\"get_preference_snapshot\")"));
        assert!(reconcile_source.contains("revision < lastPreferenceChangeRevision"));
        assert!(reconcile_source.contains("refreshPluginRuntime: false"));
        assert!(!reconcile_source.contains("queuePluginRuntimeRefresh"));
    }

    #[test]
    fn advanced_preference_native_commands_are_registered_for_webviews() {
        let command_source = include_str!("commands.rs");
        let lib_source = include_str!("lib.rs");
        for command in [
            "choose_advanced_config_directory",
            "open_log_directory",
            "show_log_viewer",
            "open_advanced_help",
            "get_player_window_status",
        ] {
            assert!(
                command_source.contains(&format!("pub fn {command}(")),
                "{command} must remain a native command implementation"
            );
            assert!(
                lib_source.matches(command).count() >= 2,
                "{command} must be imported and registered in generate_handler"
            );
        }
        assert!(command_source.contains("set_title(\"Choose config directory\")"));
        assert!(command_source.contains("set_can_create_directories(false)"));
        assert!(command_source.contains("auxiliary_windows::show_log_viewer_window"));
        assert!(lib_source.contains("get_log_snapshot"));
        assert!(include_str!("auxiliary_windows.rs").contains("log.html"));
        assert!(include_str!("preference_effects.rs")
            .contains("https://github.com/iina/iina/wiki/MPV-Options-and-Properties"));
    }

    #[test]
    fn packaged_webview_uses_the_real_tauri_command_and_event_api() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let manifest = include_str!("../Cargo.toml");
        let package_script = include_str!("../../scripts/package-macos.mjs");

        assert_eq!(
            config
                .get("app")
                .and_then(|app| app.get("withGlobalTauri"))
                .and_then(serde_json::Value::as_bool),
            Some(true),
            "release WebViews must not fall back to the browser mock backend"
        );
        assert!(manifest.contains("custom-protocol = [\"tauri/custom-protocol\"]"));
        assert!(manifest.contains("\"macos-private-api\", \"protocol-asset\""));
        assert_eq!(
            config
                .pointer("/app/security/assetProtocol/enable")
                .and_then(serde_json::Value::as_bool),
            Some(true),
            "local screenshot and thumbnail previews require Tauri's asset protocol"
        );
        assert!(
            package_script.matches("\"custom-protocol\"").count() >= 2,
            "every packaged Rust binary build must enable the production asset protocol"
        );
    }

    #[test]
    fn safari_extension_is_built_from_the_iina_135_contract_and_embedded() {
        let swift = include_str!("../../browser/Safari_Open_In_IINA/SafariExtensionHandler.swift");
        let script = include_str!("../../browser/Safari_Open_In_IINA/open-in-iina.js");
        let info = include_str!("../../browser/Safari_Open_In_IINA/Info.plist");
        let entitlements =
            include_str!("../../browser/Safari_Open_In_IINA/OpenInIINA.entitlements");
        let package = include_str!("../../scripts/package-macos.mjs");
        let builder = include_str!("../../scripts/build-safari-extension.mjs");

        for contract in [
            "toolbarItemClicked(in window: SFSafariWindow)",
            "OpenInIINA",
            "OpenLinkInIINA",
            "iina://weblink?url=",
            "NSWorkspace.shared.open(url)",
        ] {
            assert!(
                swift.contains(contract),
                "missing Safari Swift contract: {contract}"
            );
        }
        assert!(script.contains("safari.extension.setContextMenuEventUserInfo"));
        assert!(script.contains("document.addEventListener(\"contextmenu\""));
        for contract in [
            "com.apple.Safari.extension",
            "OpenInIINA.SafariExtensionHandler",
            "open-in-iina.js",
            "ToolbarItemIcon.pdf",
        ] {
            assert!(
                info.contains(contract),
                "missing Safari plist contract: {contract}"
            );
        }
        assert!(entitlements.contains("com.apple.security.app-sandbox"));
        for contract in [
            "buildSafariExtension",
            "OpenInIINA.appex",
            "arm64",
            "x86_64",
            "safariExtensionEntitlements",
            "verifySafariExtension",
        ] {
            assert!(
                package.contains(contract),
                "missing Safari package contract: {contract}"
            );
        }
        assert!(builder.contains("_NSExtensionMain"));
    }

    #[test]
    fn player_osc_preserves_iina_135_reference_geometry_and_icons() {
        let html = include_str!("../../src/index.html");
        let css = include_str!("../../src/styles.css");
        let frontend = include_str!("../../src/main.js");
        let preferences = include_str!("preferences.rs");
        let settings_icon = include_bytes!("../../src/assets/iina/icons/settings.png");
        let previous_icon = include_bytes!("../../src/assets/iina/icons/previous.png");
        let next_icon = include_bytes!("../../src/assets/iina/icons/next.png");

        for icon in [
            "icons/volume.png",
            "icons/speed-backward.png",
            "icons/play.png",
            "icons/speed-forward.png",
            "icons/pip.png",
            "icons/playlist.png",
            "icons/settings.png",
        ] {
            assert!(
                html.contains(icon),
                "OSC must use the IINA icon asset {icon}"
            );
        }
        for placeholder in [
            "id=\"pin-button\"",
            ">PIN<",
            ">PIP<",
            ">VOL<",
            ">LIST<",
            ">SET<",
            ">MINI<",
            ">FS<",
        ] {
            assert!(
                !html.contains(placeholder),
                "OSC must not regress to the placeholder control {placeholder}"
            );
        }

        for reference_geometry in [
            "width: min(440px, calc(100vw - 20px));",
            "height: 67px;",
            "grid-template-columns: 104px minmax(0, 1fr) 148px minmax(0, 1fr) 72px;",
            "grid-template-columns: 24px 70px;",
            "width: 148px;",
            "width: 72px;",
            "grid-template-columns: minmax(50px, max-content) minmax(0, 1fr) minmax(50px, max-content);",
        ] {
            assert!(
                css.contains(reference_geometry),
                "OSC reference geometry is missing: {reference_geometry}"
            );
        }

        assert!(html.contains("assets/iina/icons/playing-in-pip.png"));
        assert!(html.contains("This video is playing in picture in picture"));
        assert!(settings_icon.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(
            u32::from_be_bytes(settings_icon[16..20].try_into().unwrap()),
            64
        );
        assert_eq!(
            u32::from_be_bytes(settings_icon[20..24].try_into().unwrap()),
            64
        );

        for reference_behavior in [
            "const OSC_SPEED_VALUES = [0.03125, 0.0625, 0.125, 0.25, 0.5, 1, 2, 4, 8, 16, 32];",
            "async function handleOscArrowButton(direction)",
            "direction < 0 ? \"playlist-prev\" : \"playlist-next\"",
            "direction < 0 ? -10 : 10",
            "async function toggleRemainingTimeDisplay(event)",
            "[\"1s\", \"100ms\", \"10ms\", \"1ms\"]",
            "function startOscTimeDisplayTicker()",
            "function estimatedOscPosition()",
            "const interval = timeDisplayPrecision() >= 2 ? OSC_TIME_PRECISE_REFRESH_MS : RUNTIME_SNAPSHOT_POLL_MS;",
            "const RUNTIME_SNAPSHOT_POLL_MS = 100;",
            "const OSC_TIME_PRECISE_REFRESH_MS = 40;",
        ] {
            assert!(
                frontend.contains(reference_behavior),
                "OSC reference behavior is missing: {reference_behavior}"
            );
        }
        for preference_default in [
            "values.insert(\"arrowBtnAction\".into(), json!(0));",
            "values.insert(\"showRemainingTime\".into(), json!(false));",
            "values.insert(\"timeDisplayPrecision\".into(), json!(0));",
        ] {
            assert!(preferences.contains(preference_default));
        }
        assert!(html.contains("id=\"left-arrow-label\""));
        assert!(html.contains("id=\"right-arrow-label\""));
        assert!(html.contains("time-label--switchable"));
        assert!(css.contains(".osc-arrow-label--left"));
        assert!(css.contains(".osc-arrow-label--right"));
        assert!(css.contains(".time-context-menu"));
        for icon in [previous_icon.as_slice(), next_icon.as_slice()] {
            assert!(icon.starts_with(b"\x89PNG\r\n\x1a\n"));
            assert_eq!(u32::from_be_bytes(icon[16..20].try_into().unwrap()), 48);
            assert_eq!(u32::from_be_bytes(icon[20..24].try_into().unwrap()), 48);
        }
    }

    #[test]
    fn initial_window_preserves_iina_135_geometry_and_reference_assets() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let html = include_str!("../../src/index.html");
        let css = include_str!("../../src/styles.css");
        let initial_icon = include_bytes!("../../src/assets/iina/initial-window-icon.png");
        let history_icon = include_bytes!("../../src/assets/iina/icons/history.png");
        let frontend = include_str!("../../src/main.js");
        let commands = include_str!("commands.rs");
        let library = include_str!("lib.rs");
        let native_open_panel = include_str!("native_open_panel.m");
        let window = &config["app"]["windows"][0];

        assert_eq!(window["width"], 640);
        assert_eq!(window["height"], 400);
        assert_eq!(window["minWidth"], 640);
        assert_eq!(window["minHeight"], 400);
        assert_eq!(window["resizable"], false);
        assert_eq!(window["center"], true);
        assert_eq!(window["visible"], false);
        assert!(frontend.contains("await completeInitialLaunch();"));
        assert!(frontend.contains("invoke(\"complete_initial_launch\")"));
        assert!(commands.contains("std::thread::sleep(Duration::from_millis(100))"));
        assert!(library.contains("complete_initial_launch,"));
        assert!(library.contains("RunEvent::Reopen"));
        assert!(native_open_panel.contains("panel.canChooseFiles = YES;"));
        assert!(native_open_panel.contains("panel.canChooseDirectories = YES;"));
        assert!(native_open_panel.contains("panel.allowsMultipleSelection = YES;"));

        for reference_markup in [
            "assets/iina/initial-window-icon.png",
            "assets/iina/icons/history.png",
            "<span>Open...</span>",
            "<kbd>&#8984;O</kbd>",
            "<span>Open URL...</span>",
            "<kbd>&#8679;&#8984;O</kbd>",
        ] {
            assert!(html.contains(reference_markup));
        }
        assert!(html.contains("class=\"last-file-prefix\""));
        assert!(html.contains("data-i18n-table=\"InitialWindowController\""));
        assert!(html.contains("data-i18n-key=\"KWZ-BM-GBN.title\">Resume</span>"));
        assert!(!html.contains("class=\"action-symbol\""));
        assert!(!html.contains("class=\"recent-title\""));
        assert!(!html.contains("No recent files"));

        for reference_geometry in [
            "grid-template-columns: 180px minmax(0, 1fr);",
            "top: 32px;\n  left: 24px;\n  width: 128px;\n  height: 128px;",
            "right: 44px;\n  left: 24px;",
            "height: 28px;",
            "top: 94px;",
            ".initial-window--has-last-playback .recent-panel {\n  top: 130px;",
        ] {
            assert!(css.contains(reference_geometry));
        }

        assert!(initial_icon.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(
            u32::from_be_bytes(initial_icon[16..20].try_into().unwrap()),
            1024
        );
        assert_eq!(
            u32::from_be_bytes(initial_icon[20..24].try_into().unwrap()),
            1024
        );
        assert!(history_icon.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(
            u32::from_be_bytes(history_icon[16..20].try_into().unwrap()),
            56
        );
        assert_eq!(
            u32::from_be_bytes(history_icon[20..24].try_into().unwrap()),
            56
        );
    }

    #[test]
    fn native_menu_bridge_preserves_iina_option_alternates() {
        let build = include_str!("../build.rs");
        let menu = include_str!("menu.rs");
        let native = include_str!("native_menu.m");

        assert!(build.contains(".file(\"src/native_menu.m\")"));
        assert!(menu.contains("crate::native_menu::configure_alternate_items"));
        assert!(menu.contains("native_alternate_menu_items_for_titles"));
        assert!(menu.contains("app.run_on_main_thread(move ||"));
        assert!(native.contains("item.alternate = YES;"));
        assert!(native
            .contains("item.keyEquivalentModifierMask = modifiers | NSEventModifierFlagOption;"));
        assert!(native.contains("dispatch_sync(dispatch_get_main_queue(), configure);"));
    }

    #[test]
    fn player_osd_preserves_iina_135_geometry_timing_and_accessories() {
        let frontend = include_str!("../../src/main.js");
        let css = include_str!("../../src/styles.css");
        let preferences = include_str!("preferences.rs");

        for reference_geometry in [
            "top: 30px;",
            "left: 8px;",
            "max-width: calc(100vw - 16px);",
            "padding: 8px 16px;",
            "border-radius: 10px;",
            "transition: opacity 0.5s ease;",
            "gap: 2px;",
            "min-width: 150px;",
            "height: 12px;",
            "padding: 4px 0;",
        ] {
            assert!(
                css.contains(reference_geometry),
                "OSD reference geometry is missing: {reference_geometry}"
            );
        }

        for reference_behavior in [
            "function showPlayerOsd(message, nextState)",
            "function playerOsdPresentation(rawMessage, nextState)",
            "if (message === \"Paused\") return { message: trKey(\"Localizable\", \"osd.pause\", \"Pause\"), detail: timeDetail };",
            "if (message.startsWith(\"Seek \"))",
            "progress.className = \"osd-progress\";",
            "accessory.className = \"screenshot-osd-accessory\";",
            "const minuteText = mins.toString().padStart(2, \"0\");",
            "return `${minuteText}:${secondText}`;",
            "}, 500);",
        ] {
            assert!(
                frontend.contains(reference_behavior),
                "OSD reference behavior is missing: {reference_behavior}"
            );
        }

        assert!(preferences.contains("values.insert(\"osdAutoHideTimeout\".into(), json!(1.0));"));
        assert!(preferences.contains("values.insert(\"osdTextSize\".into(), json!(20.0));"));
    }

    #[test]
    fn osc_thumbnails_preserve_iina_135_generation_cache_and_hover_contract() {
        let media = include_str!("media.rs");
        let commands = include_str!("commands.rs");
        let state = include_str!("state.rs");
        let native = include_str!("native_video.m");
        let preferences = include_str!("preferences.rs");
        let frontend = include_str!("../../src/main.js");
        let css = include_str!("../../src/styles.css");

        for contract in [
            "const THUMBNAIL_CACHE_VERSION: u8 = 2;",
            "const THUMBNAIL_DEFAULT_WIDTH: u32 = 240;",
            "const THUMBNAIL_DEFAULT_COUNT: usize = 100;",
            "const THUMBNAIL_PARTIAL_BATCH_SIZE: usize = 10;",
            "Duration::from_millis(200)",
            "Duration::from_secs(1)",
            "thumbnail_source_signature",
            "source_size.to_ne_bytes()",
            "source_timestamp.to_ne_bytes()",
            "thumbnail.time_seconds.to_ne_bytes()",
            "clear_old_thumbnail_cache(cache_directory, max_cache_size_bytes / 2)",
        ] {
            assert!(
                media.contains(contract),
                "thumbnail media contract is missing: {contract}"
            );
        }
        for contract in [
            "pub async fn generate_media_thumbnails(",
            "tauri::async_runtime::spawn_blocking(move ||",
            "const THUMBNAIL_PROGRESS_EVENT: &str = \"iima-thumbnail-progress\";",
            "begin_thumbnail_generation(&session_label)",
            "thumbnail_generation_is_current(&session_label, generation_id)",
            "enableThumbnailForRemoteFiles",
            "native_video::path_is_on_local_volume(&path)",
        ] {
            assert!(
                commands.contains(contract),
                "thumbnail command contract is missing: {contract}"
            );
        }
        assert!(state.contains("thumbnail_generations: Mutex<BTreeMap<String, u64>>"));
        assert!(native.contains("NSURLVolumeIsLocalKey"));

        for preference in [
            "values.insert(\"enableThumbnailPreview\".into(), json!(true));",
            "values.insert(\"maxThumbnailPreviewCacheSize\".into(), json!(500));",
            "values.insert(\"enableThumbnailForRemoteFiles\".into(), json!(false));",
            "values.insert(\"thumbnailWidth\".into(), json!(240));",
        ] {
            assert!(preferences.contains(preference));
        }
        for behavior in [
            "invoke(\"generate_media_thumbnails\", { path: source })",
            "tauriListen(\"iima-thumbnail-progress\"",
            "thumbnailSet?.ready ? nearestThumbnail(time) : undefined",
            "thumbnail = thumbnails[index === 0 ? 0 : index - 1];",
            "function renderUtilityCacheControl(control)",
            "invoke(\"clear_thumbnail_cache\")",
        ] {
            assert!(
                frontend.contains(behavior),
                "thumbnail frontend contract is missing: {behavior}"
            );
        }
        for geometry in [
            "width: 120px;",
            "border-radius: 4px;",
            "border: 1px solid rgba(153, 153, 153, 0.5);",
            "font-size: 10px;",
        ] {
            assert!(
                css.contains(geometry),
                "thumbnail hover geometry is missing: {geometry}"
            );
        }
    }

    #[test]
    fn mini_player_preserves_iina_135_layout_assets_and_window_behavior() {
        let html = include_str!("../../src/index.html");
        let css = include_str!("../../src/styles.css");
        let frontend = include_str!("../../src/main.js");
        let preferences = include_str!("preferences.rs");
        let commands = include_str!("commands.rs");
        let native = include_str!("native_video.m");
        let native_window = include_str!("native_window.m");
        let album_art = include_bytes!("../../src/assets/iina/icons/default-album-art.png");
        let back = include_bytes!("../../src/assets/iina/icons/back.png");
        let toggle_album_art = include_bytes!("../../src/assets/iina/icons/toggle-album-art.png");

        for control in [
            "id=\"mini-player-ui\"",
            "id=\"mini-volume-button\"",
            "id=\"mini-previous-button\"",
            "id=\"mini-play-button\"",
            "id=\"mini-next-button\"",
            "id=\"mini-playlist-button\"",
            "id=\"mini-album-art-button\"",
            "id=\"mini-play-slider\"",
            "id=\"mini-volume-popover\"",
            "id=\"mini-playlist-list\"",
            "id=\"mini-close-button\"",
            "id=\"mini-back-button\"",
        ] {
            assert!(
                html.contains(control),
                "Mini Player control is missing: {control}"
            );
        }

        for reference_geometry in [
            "grid-template-rows: var(--mini-video-height, 300px) 72px minmax(0, 1fr);",
            "min-width: 300px;",
            "height: 72px;",
            "grid-row: 1;",
            "grid-row: 2;",
            "grid-row: 3;",
            "left: calc(50% - 101px);",
            "left: calc(50% - 64px);",
            "left: calc(50% - 12px);",
            "left: calc(50% + 36px);",
            "left: calc(50% + 84px);",
            "left: calc(50% + 110px);",
            "grid-template-columns: 17px 100px 30px;",
            "width: 183px;",
            "height: 29px;",
            ".mini-playlist {\n  grid-row: 3;\n  min-height: 200px;",
            "transition: opacity 200ms ease;",
            "filter: brightness(0) invert(1);",
        ] {
            assert!(
                css.contains(reference_geometry),
                "Mini Player reference geometry is missing: {reference_geometry}"
            );
        }

        for reference_behavior in [
            "const MINI_PLAYER_CONTROL_HEIGHT = 72;",
            "const MINI_PLAYER_PLAYLIST_HEIGHT = 300;",
            "const MINI_PLAYER_AUTO_HIDE_PLAYLIST_HEIGHT = 200;",
            "function renderMiniPlayer(nextState, position, duration, hasMedia, hasAudio, hasVideo)",
            "function miniPlayerArtistAlbum(nextState)",
            "function requestMiniPlayerLayout(aspect = miniPlayerVideoAspect(state))",
            "function applyNativeMiniPlayerLayout(layout)",
            "tauriListen(\"iima-native-mini-player-layout\"",
            "async function toggleMiniPlaylist(visible = !miniPlaylistVisible)",
            "async function toggleMiniVideo(visible = !miniVideoVisible)",
            "function toggleMiniVolumePopover(event)",
            "async function closeMiniPlayer()",
            "invoke(\"close_mini_player\")",
            "music_title: nextState.music_title",
            "music_album: nextState.music_album",
            "music_artist: nextState.music_artist",
        ] {
            assert!(
                frontend.contains(reference_behavior),
                "Mini Player reference behavior is missing: {reference_behavior}"
            );
        }

        for preference_default in [
            "values.insert(\"autoSwitchToMusicMode\".into(), json!(true));",
            "values.insert(\"musicModeShowPlaylist\".into(), json!(false));",
            "values.insert(\"musicModeShowAlbumArt\".into(), json!(true));",
        ] {
            assert!(preferences.contains(preference_default));
        }

        for native_window_contract in [
            ".inner_size(initial_layout.width, initial_layout.height)",
            ".min_inner_size(MINI_PLAYER_INITIAL_WIDTH, MINI_PLAYER_CONTROL_HEIGHT)",
            ".always_on_top(always_on_top)",
            ".set_always_on_top(always_on_top)",
            "native_video::configure_mini_player_window(",
            "native_window_behavior::install_mini_player_layout_observer(",
            "native_window_behavior::apply_mini_player_layout(",
            "fn automatically_sync_music_mode<R: Runtime>(",
            "bool_preference(&preferences.values, \"autoSwitchToMusicMode\", true)",
            "pub(crate) fn close_mini_player_window_for_session<R: Runtime>(",
        ] {
            assert!(commands.contains(native_window_contract));
        }
        for appkit_contract in [
            "window.titleVisibility = NSWindowTitleHidden;",
            "window.titlebarAppearsTransparent = YES;",
            "window.movableByWindowBackground = YES;",
            "window.tabbingMode = NSWindowTabbingModeDisallowed;",
            "button.hidden = YES;",
            "button.frame = NSZeroRect;",
        ] {
            assert!(native.contains(appkit_contract));
        }
        for live_resize_contract in [
            "NSWindowWillStartLiveResizeNotification",
            "NSWindowDidEndLiveResizeNotification",
            "IIMAMiniPlayerAutoHidePlaylistThreshold = 200.0",
            "IIMAMiniPlayerDefaultPlaylistHeight = 300.0",
            "frame.origin.y += frame.size.height - targetHeight",
            "[window setFrame:frame display:YES animate:YES]",
            "[window setFrame:frame display:YES animate:NO]",
        ] {
            assert!(
                native_window.contains(live_resize_contract),
                "Mini Player live-resize contract is missing: {live_resize_contract}"
            );
        }

        assert!(album_art.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(
            u32::from_be_bytes(album_art[16..20].try_into().unwrap()),
            600
        );
        assert_eq!(
            u32::from_be_bytes(album_art[20..24].try_into().unwrap()),
            600
        );
        for icon in [back.as_slice(), toggle_album_art.as_slice()] {
            assert!(icon.starts_with(b"\x89PNG\r\n\x1a\n"));
            assert_eq!(u32::from_be_bytes(icon[16..20].try_into().unwrap()), 48);
            assert_eq!(u32::from_be_bytes(icon[20..24].try_into().unwrap()), 48);
        }
    }

    #[test]
    fn utilities_preferences_preserve_iina_135_actions_and_copy() {
        let html = include_str!("../../src/index.html");
        let frontend = include_str!("../../src/main.js");
        let preference_panes = include_str!("../../src/preference-panes.js");
        let preference_frontend = format!("{frontend}\n{preference_panes}");
        let css = include_str!("../../src/styles.css");
        let commands = include_str!("commands.rs");
        let state = include_str!("state.rs");
        let preferences = include_str!("preferences.rs");
        let native = include_str!("native_default_app.m");
        let info_plist = include_str!("../IINA-Info.plist");

        assert!(html.contains("id=\"preference-sheet-layer\""));
        for reference_copy in [
            "Set IINA as the Default Application…",
            "Restore Suppressed Alerts…",
            "Clear Saved Playback Progress…",
            "Clear Playback History…",
            "Clear Thumbnail Cache…",
            "Get Browser Extensions for IINA",
            "Please select the media types that you want to make IINA as the default Application for.",
            "Are you sure you want to restore suppressed alerts?",
            "Are you sure to delete all saved playback progress",
            "Are you sure to delete all playback history?",
            "Are you sure to clear all thumbnail Cache?",
            "trFormat(\"Finished with %d success and %d failed.\"",
        ] {
            assert!(
                preference_frontend.contains(reference_copy),
                "missing Utilities copy: {reference_copy}"
            );
        }
        for command in [
            "clear_saved_playback_progress",
            "clear_playback_history",
            "restore_suppressed_alerts",
            "set_default_application",
            "open_browser_extension",
        ] {
            assert!(frontend.contains(&format!("invoke(\"{command}\"")));
            assert!(commands.contains(&format!("pub fn {command}")));
        }
        for native_contract in [
            "UTImportedTypeDeclarations",
            "UTTypeCreatePreferredIdentifierForTag",
            "LSSetDefaultRoleHandlerForContentType",
            "kLSRolesAll",
            "@\"public.movie\"",
            "@\"public.audio\"",
            "@\"public.text\"",
        ] {
            assert!(native.contains(native_contract));
        }
        assert!(info_plist.contains("<key>UTImportedTypeDeclarations</key>"));
        assert!(commands.contains(".join(\"watch_later\")"));
        assert!(commands.contains(".join(\"history.plist\")"));
        assert!(state.contains("pub fn clear_playback_history(&self)"));
        assert!(preferences.contains(
            "values.insert(\"suppressCannotPreventDisplaySleep\".into(), json!(false));"
        ));
        assert!(css.contains(".preference-default-sheet"));
        assert!(css.contains("height: 192px;"));
    }

    #[test]
    fn playback_history_window_preserves_iina_135_structure_and_actions() {
        let html = include_str!("../../src/history.html");
        let frontend = include_str!("../../src/history.js");
        let css = include_str!("../../src/history.css");
        let commands = include_str!("commands.rs");
        let state = include_str!("state.rs");
        let history = include_str!("history.rs");
        let mpv = include_str!("mpv.rs");

        for copy in [
            "Playback History",
            "Group by:",
            "Date",
            "Folder",
            "File",
            "Progress",
            "Played at",
            "Play in New Window",
            "Show in Finder",
            "Delete…",
            "Delete Playback History",
            "Are you sure to delete selected entries?",
        ] {
            assert!(html.contains(copy), "missing playback history copy: {copy}");
        }
        for geometry in [
            ".inner_size(600.0, 400.0)",
            ".min_inner_size(400.0, 200.0)",
            "grid-template-rows: 32px minmax(0, 1fr);",
            "grid-template-columns: minmax(200px, 1fr) 110px 60px;",
            "grid-template-columns: minmax(200px, 1fr) 110px 145px;",
            "height: 22px;",
            "flex: 0 0 200px;",
        ] {
            assert!(
                commands.contains(geometry) || css.contains(geometry),
                "missing playback history geometry: {geometry}"
            );
        }
        for behavior in [
            "get_playback_history",
            "remove_playback_history_entries",
            "open_playback_history_item",
            "reveal_playback_history_items",
            "event.key.toLowerCase() === \"f\"",
            "event.key.toLowerCase() === \"a\"",
            "event.key === \"Delete\" || event.key === \"Backspace\"",
            "addEventListener(\"dblclick\"",
            "addEventListener(\"contextmenu\"",
            "data-search-option=\"filename\"",
            "data-search-option=\"path\"",
        ] {
            assert!(
                frontend.contains(behavior) || html.contains(behavior),
                "missing playback history behavior: {behavior}"
            );
        }
        for persistence_contract in [
            "event.name == \"file-loaded\"",
            "recordPlaybackHistory",
            "iinaLastPlayedFilePath",
            "iinaLastPlayedFilePosition",
            "write-watch-later-config",
            "save-position-on-quit",
            "resume-playback",
        ] {
            assert!(
                state.contains(persistence_contract)
                    || history.contains(persistence_contract)
                    || mpv.contains(persistence_contract),
                "missing playback persistence contract: {persistence_contract}"
            );
        }
        for archive_key in [
            "IINAPHUrl",
            "IINAPHNme",
            "IINAPHMpvmd5",
            "IINAPHPlayed",
            "IINAPHDate",
            "IINAPHDuration",
        ] {
            assert!(history.contains(archive_key));
        }
    }

    #[test]
    fn open_url_http_auth_uses_the_iina_keychain_contract() {
        let native = include_str!("native_keychain.m");
        let rust = include_str!("native_keychain.rs");
        let commands = include_str!("commands.rs");
        let frontend = include_str!("../../src/main.js");
        let html = include_str!("../../src/index.html");
        let build = include_str!("../build.rs");

        for contract in [
            "IINA Saved HTTP Password",
            "kSecClassInternetPassword",
            "kSecAttrServer",
            "kSecAttrPort",
            "SecItemCopyMatching",
            "SecItemUpdate",
            "SecItemAdd",
        ] {
            assert!(
                native.contains(contract),
                "missing Keychain contract: {contract}"
            );
        }
        for contract in [
            "pub async fn read_http_auth_credentials",
            "pub async fn write_http_auth_credentials",
            "fn http_auth_key_from_url",
        ] {
            assert!(commands.contains(contract));
        }
        for contract in [
            "invoke(\"read_http_auth_credentials\"",
            "invoke(\"write_http_auth_credentials\"",
            "els.urlRemember.checked = false;",
            "function scheduleOpenUrlCredentialLookup(url)",
        ] {
            assert!(frontend.contains(contract));
        }
        assert!(html.contains("id=\"url-remember\" type=\"checkbox\" />"));
        assert!(rust.contains("pub struct HttpAuthCredentials"));
        assert!(build.contains("cargo:rustc-link-lib=framework=Security"));
    }

    #[test]
    fn opensubtitles_account_uses_keychain_and_authenticated_api_contracts() {
        let native = include_str!("native_keychain.m");
        let rust = include_str!("native_keychain.rs");
        let commands = include_str!("commands.rs");
        let online = include_str!("online_subtitles.rs");
        let preferences = include_str!("preferences.rs");
        let frontend = include_str!("../../src/main.js");

        for contract in [
            "IINA OpenSubtitles Account",
            "kSecClassGenericPassword",
            "iima_keychain_read_opensubtitles",
            "iima_keychain_write_opensubtitles",
        ] {
            assert!(
                native.contains(contract),
                "missing OpenSubtitles Keychain contract: {contract}"
            );
        }
        for contract in [
            "read_opensubtitles_password",
            "write_opensubtitles_password",
        ] {
            assert!(rust.contains(contract));
        }
        for contract in [
            "pub async fn login_opensubtitles_account",
            "pub fn logout_opensubtitles_account",
            "persist_open_sub_username",
        ] {
            assert!(commands.contains(contract));
        }
        for contract in [
            "OPENSUBTITLES_SESSION_LIFETIME",
            "OpenSubtitlesRateLimiter",
            "opensubtitles_session_for_preferences",
            "search_with_opensubtitles_session",
            "ratelimit-remaining",
            "OPEN_SUBTITLES_INVALID_TOKEN",
            "download_opensubtitles_file",
            "Authorization",
            "--data-binary",
        ] {
            assert!(online.contains(contract));
        }
        assert!(commands.contains("abandon_rejected_opensubtitles_session"));
        assert!(preferences.contains("openSubUsername"));
        for contract in [
            "showOpenSubtitlesLoginPanel",
            "login_opensubtitles_account",
            "logout_opensubtitles_account",
            "Logged in as %@",
            "Not logged in",
            "OPEN_SUBTITLES_INVALID_TOKEN:",
        ] {
            assert!(frontend.contains(contract));
        }
    }

    #[test]
    fn recent_documents_use_appkit_and_sonoma_bookmark_recovery() {
        let native = include_str!("native_recent_documents.m");
        let rust = include_str!("native_recent_documents.rs");
        let commands = include_str!("commands.rs");
        let build = include_str!("../build.rs");

        for contract in [
            "NSDocumentController.sharedDocumentController.recentDocumentURLs",
            "noteNewRecentDocumentURL",
            "clearRecentDocuments",
            "bookmarkDataWithOptions",
            "URLByResolvingBookmarkData",
            "@available(macOS 14.0, *)",
        ] {
            assert!(
                native.contains(contract),
                "missing recent-document contract: {contract}"
            );
        }
        for contract in [
            "RecentDocumentSource::OpenPanel",
            "RecentDocumentSource::FileLoaded",
            "trackAllFilesInRecentOpenMenu",
            "synchronize_recent_documents_from_native",
            "persist_native_recent_documents",
        ] {
            assert!(commands.contains(contract));
        }
        assert!(rust.contains("pub fn persistence_value"));
        assert!(build.contains("src/native_recent_documents.m"));
    }

    #[test]
    fn updater_preserves_the_iina_135_sparkle_contract() {
        let native = include_str!("native_updater.m");
        let rust = include_str!("native_updater.rs");
        let commands = include_str!("commands.rs");
        let menu = include_str!("menu.rs");
        let frontend = include_str!("../../src/main.js");
        let preference_panes = include_str!("../../src/preference-panes.js");
        let preference_frontend = format!("{frontend}\n{preference_panes}");
        let package = include_str!("../../scripts/package-macos.mjs");
        let info_plist = include_str!("../IINA-Info.plist");
        let tauri_config = include_str!("../tauri.conf.json");
        let build = include_str!("../build.rs");

        for contract in [
            "SPUStandardUpdaterController",
            "feedURLStringForUpdater:",
            "clearFeedURLFromUserDefaults",
            "automaticallyChecksForUpdates",
            "setUpdateCheckInterval:",
            "checkForUpdates:",
            "https://www.iina.io/appcast.xml",
            "https://www.iina.io/appcast-beta.xml",
            "if (candidate == nil)",
            "return iima_updater_is_owned_channel() ? nil : fallback;",
            "iima_updater_guarded_on_main",
            "iima_updater_record_exception",
            "iima_updater_reset_runtime",
            "Commit the runtime only after every potentially throwing validation call",
            "@catch (NSException *exception)",
        ] {
            assert!(
                native.contains(contract),
                "missing Sparkle contract: {contract}"
            );
        }
        for contract in [
            "UPDATE_CHECK_INTERVALS: [f64; 4]",
            "3600.0, 86400.0, 604800.0, 2_629_800.0",
            "pub fn validated_update_interval",
            "pub fn check_for_updates",
        ] {
            assert!(rust.contains(contract));
        }
        for contract in [
            "pub fn get_updater_status",
            "pub fn check_for_updates",
            "receiveBetaUpdate",
            "updaterAutomaticallyChecks",
            "updaterCheckInterval",
        ] {
            assert!(commands.contains(contract));
        }
        for contract in [
            "\"iina.check-updates\" => emit_request(app, &target, \"check-updates\")",
            "\"Check for Updates...\"",
        ] {
            assert!(menu.contains(contract));
        }
        for contract in [
            "type: \"updater-check\"",
            "[3600, \"Hourly\"]",
            "[86400, \"Daily\"]",
            "[604800, \"Weekly\"]",
            "[2629800, \"Monthly\"]",
            "Receive beta updates",
            "invoke(\"check_for_updates\")",
        ] {
            assert!(preference_frontend.contains(contract));
        }
        for contract in [
            "Sparkle.xcframework",
            "Expected Sparkle 2.9.4",
            "SUPublicDSAKeyFile",
            "SUPublicEDKey",
            "codesign",
        ] {
            assert!(package.contains(contract));
        }
        for contract in [
            "<key>SUFeedURL</key>",
            "<key>SUPublicDSAKeyFile</key>",
            "<key>SUPublicEDKey</key>",
            "UpwCRYfYOg0OGgQHY6RUdrV29yPcdkvxGlEfq46r6a0=",
        ] {
            assert!(info_plist.contains(contract));
        }
        assert!(tauri_config.contains("Resources/dsa_pub.pem"));
        assert!(build.contains("src/native_updater.m"));
    }

    #[test]
    fn localization_catalogs_are_generated_from_iina_135_resources() {
        let generator = include_str!("../../scripts/generate-localizations.mjs");
        let runtime = include_str!("../../src/localization.js");
        let native_runtime = include_str!("localization.rs");
        let native_font_picker = include_str!("native_font_picker.m");
        let package = include_str!("../../scripts/package-macos.mjs");
        let frontend = include_str!("../../src/main.js");
        let history = include_str!("../../src/history.js");
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../src/locales/manifest.json")).unwrap();
        let simplified_chinese: serde_json::Value =
            serde_json::from_str(include_str!("../../src/locales/zh-Hans.json")).unwrap();

        assert!(generator.contains("参考\", \"iina\", \"iina"));
        assert!(generator.contains("entry.name.endsWith(\".lproj\")"));
        assert!(generator.contains("entry.name.endsWith(\".strings\")"));
        assert!(generator.contains("source.replaceAll(\"…\", \"...\")"));
        for contract in [
            "navigator.languages",
            "Intl.getCanonicalLocales",
            "zh-hans",
            "zh-hant",
            "document.documentElement.dir",
            "localizeStaticDocument",
        ] {
            assert!(runtime.contains(contract));
        }
        assert!(frontend.starts_with("import {\n  initializeLocalization,"));
        for helper in ["tr,", "trFormat,", "trKey,", "trKeyFormat,"] {
            assert!(frontend.contains(helper));
        }
        assert!(frontend.contains("} from \"./localization.js\";"));
        assert!(history.starts_with("import { initializeLocalization, tr }"));
        assert!(generator.contains("native-menu-locales.json"));
        assert!(native_runtime.contains("iima_native_preferred_languages_json"));
        assert!(native_runtime.contains("menu_number_for_locale"));
        assert!(native_font_picker.contains("initWithLabels:labels"));
        assert!(package.contains("npm\", [\"run\", \"locales:check\"]"));
        assert!(package.contains("_iima_native_preferred_languages_json"));
        assert!(package.contains("Verified packaged frontend and native localization runtimes"));

        let locales = manifest["locales"].as_array().unwrap();
        assert_eq!(manifest["defaultLocale"], "en");
        assert_eq!(locales.len(), 56);
        assert_eq!(
            locales
                .iter()
                .filter_map(|locale| locale["entries"].as_u64())
                .sum::<u64>(),
            33_847
        );
        assert_eq!(
            simplified_chinese["translations"]["Check for Updates..."],
            "检查更新…"
        );
        assert_eq!(simplified_chinese["translations"]["Cancel"], "取消");
    }
}
