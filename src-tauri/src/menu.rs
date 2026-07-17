use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::auxiliary_windows::IinaExternalPage;
use crate::iina_commands::{can_delete_current_file, delete_current_file, execute_window_size};
use crate::key_bindings::{
    active_key_bindings_from_preference, normalize_mpv_key, ActiveKeyBinding,
};
use crate::localization;
use crate::native_file::FileRemovalMode;
use crate::native_menu::{
    NativeMenuItemState, NativeMenuKeyEquivalent, NativeMenuResponderAction, NativeMenuVisibility,
};
use crate::player::{
    FilterKind, LoopMode, PlayerCommand, PlayerMode, PlayerState, RelativeSeekOption, SidebarTab,
    Track, TrackSelectionKind, IINA_SUBTITLE_ENCODINGS,
};
use crate::plugins::{PluginMenuDefinition, PluginMenuItemDefinition};
use crate::preferences::{SavedFilter, SAVED_AUDIO_FILTERS_KEY, SAVED_VIDEO_FILTERS_KEY};
use crate::state::{player_session_label_for_window, AppState};
use crate::window_size::WindowSizeAction;

const MENU_EVENT_PLAYER_STATE: &str = "iima-player-state";
const MENU_EVENT_REQUEST: &str = "iima-menu-request";
const PLUGIN_MENU_EVENT: &str = "iima-plugin-menu-action";
const PLUGIN_RUNTIME_RELOAD_ALL_EVENT: &str = "iima-plugin-runtime-reload-all";
const PLUGIN_MENU_ID_PREFIX: &str = "iina.plugin-menu.";
const PLUGIN_DEVELOPER_TOOL_ID_PREFIX: &str = "iina.plugin-developer-tool.";
const PLUGIN_MENU_FIRST_LEVEL_LIMIT: usize = 5;
const SUBTITLE_PROVIDER_MENU_ID_PREFIX: &str = "iina.find-online-subtitle-provider.";
const OTHER_KEY_BINDINGS_MENU_ID: &str = "iina.menu.other-key-bindings";
const OTHER_KEY_BINDINGS_MENU_TITLE: &str = "Other Actions From Key Bindings";
const OTHER_KEY_BINDING_ITEM_ID_PREFIX: &str = "iina.other-key-binding.";
const CUSTOM_TOUCH_BAR_MENU_ID: &str = "iina.custom-touch-bar";
const BUILT_IN_SUBTITLE_PROVIDERS: [(&str, &str); 3] = [
    (":opensubtitles", "opensubtitles.com"),
    (":assrt", "assrt.net"),
    (":shooter", "shooter.cn"),
];
const IINA_ASPECTS: [&str; 9] = [
    "4:3", "5:4", "16:9", "16:10", "1:1", "3:2", "2.21:1", "2.35:1", "2.39:1",
];
const IINA_ROTATIONS: [i64; 4] = [0, 90, 180, 270];
const EDIT_RESPONDER_ACTIONS: [(&str, &str, &str); 6] = [
    ("iina.delete", "delete:", "\u{0008}"),
    ("iina.menu.transformations.0", "uppercaseWord:", ""),
    ("iina.menu.transformations.1", "lowercaseWord:", ""),
    ("iina.menu.transformations.2", "capitalizeWord:", ""),
    ("iina.menu.speech.0", "startSpeaking:", ""),
    ("iina.menu.speech.1", "stopSpeaking:", ""),
];

#[derive(Debug, Clone, Copy)]
enum NumericActionMatch {
    Exact,
    Range { minimum: f64, maximum: f64 },
}

#[derive(Debug, Clone, Copy)]
struct MenuBindingSpec {
    id: &'static str,
    iina_command: bool,
    action: &'static [&'static str],
    numeric: Option<NumericActionMatch>,
    title_template: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq)]
struct ResolvedMenuBinding {
    key_equivalent: String,
    modifier_mask: u32,
    numeric_value: Option<f64>,
    exact_seek: bool,
    title: Option<String>,
}

const NATIVE_MODIFIER_COMMAND: u32 = 1 << 0;
const NATIVE_MODIFIER_CONTROL: u32 = 1 << 1;
const NATIVE_MODIFIER_OPTION: u32 = 1 << 2;
const NATIVE_MODIFIER_SHIFT: u32 = 1 << 3;

const MENU_BINDING_SPECS: &[MenuBindingSpec] = &[
    binding("iina.delete-current-file", true, &["delete-current-file"]),
    binding("iina.save-current-playlist", true, &["save-playlist"]),
    binding("iina.show-video-panel", true, &["video-panel"]),
    binding("iina.show-audio-panel", true, &["audio-panel"]),
    binding("iina.show-subtitles-panel", true, &["sub-panel"]),
    binding("iina.show-playlist", true, &["playlist-panel"]),
    binding("iina.show-chapters", true, &["chapter-panel"]),
    binding("iina.find-online-subtitles", true, &["find-online-subs"]),
    binding(
        "iina.save-downloaded-subtitle",
        true,
        &["save-downloaded-sub"],
    ),
    binding("iina.bigger-size", true, &["bigger-window"]),
    binding("iina.smaller-size", true, &["smaller-window"]),
    binding("iina.fit-screen", true, &["fit-to-screen"]),
    binding("iina.enter-music-mode", true, &["toggle-music-mode"]),
    binding("iina.picture-in-picture", true, &["toggle-pip"]),
    binding("iina.cycle-video-tracks", false, &["cycle", "video"]),
    binding("iina.cycle-audio-tracks", false, &["cycle", "audio"]),
    binding("iina.cycle-subtitles", false, &["cycle", "sub"]),
    numeric_binding(
        "iina.next-chapter",
        false,
        &["add", "chapter", "1"],
        NumericActionMatch::Exact,
        None,
    ),
    numeric_binding(
        "iina.previous-chapter",
        false,
        &["add", "chapter", "-1"],
        NumericActionMatch::Exact,
        None,
    ),
    binding("iina.toggle-pause", false, &["cycle", "pause"]),
    binding("iina.stop", false, &["stop"]),
    numeric_binding(
        "iina.seek-forward-5",
        false,
        &["seek", "5"],
        NumericActionMatch::Range {
            minimum: 5.0,
            maximum: 60.0,
        },
        Some("Step Forward %.0fs"),
    ),
    numeric_binding(
        "iina.seek-backward-5",
        false,
        &["seek", "-5"],
        NumericActionMatch::Range {
            minimum: -60.0,
            maximum: -5.0,
        },
        Some("Step Backward %.0fs"),
    ),
    binding("iina.frame-step-forward", false, &["frame-step"]),
    binding("iina.frame-step-backward", false, &["frame-back-step"]),
    binding("iina.next-media", false, &["playlist-next"]),
    binding("iina.previous-media", false, &["playlist-prev"]),
    numeric_binding(
        "iina.speed-2x",
        false,
        &["multiply", "speed", "2.0"],
        NumericActionMatch::Range {
            minimum: 1.5,
            maximum: 3.0,
        },
        Some("Speed Up by %.1fx"),
    ),
    numeric_binding(
        "iina.speed-1_1x",
        false,
        &["multiply", "speed", "1.1"],
        NumericActionMatch::Range {
            minimum: 1.01,
            maximum: 1.49,
        },
        Some("Speed Up by %.1fx"),
    ),
    numeric_binding(
        "iina.speed-0_5x",
        false,
        &["multiply", "speed", "0.5"],
        NumericActionMatch::Range {
            minimum: 0.0,
            maximum: 0.7,
        },
        Some("Speed Down by %.1fx"),
    ),
    numeric_binding(
        "iina.speed-0_9x",
        false,
        &["multiply", "speed", "0.9"],
        NumericActionMatch::Range {
            minimum: 0.71,
            maximum: 0.99,
        },
        Some("Speed Down by %.1fx"),
    ),
    numeric_binding(
        "iina.speed-reset",
        false,
        &["set", "speed", "1.0"],
        NumericActionMatch::Exact,
        None,
    ),
    binding("iina.ab-loop", false, &["ab-loop"]),
    binding(
        "iina.file-loop",
        false,
        &["cycle-values", "loop", "\"inf\"", "\"no\""],
    ),
    binding("iina.screenshot", false, &["screenshot"]),
    numeric_binding(
        "iina.half-size",
        false,
        &["set", "window-scale", "0.5"],
        NumericActionMatch::Exact,
        None,
    ),
    numeric_binding(
        "iina.normal-size",
        false,
        &["set", "window-scale", "1"],
        NumericActionMatch::Exact,
        None,
    ),
    numeric_binding(
        "iina.double-size",
        false,
        &["set", "window-scale", "2"],
        NumericActionMatch::Exact,
        None,
    ),
    binding("iina.fullscreen", false, &["cycle", "fullscreen"]),
    binding("iina.float-on-top", false, &["cycle", "ontop"]),
    binding("iina.mute", false, &["cycle", "mute"]),
    numeric_binding(
        "iina.volume-up-5",
        false,
        &["add", "volume", "5"],
        NumericActionMatch::Range {
            minimum: 5.0,
            maximum: 10.0,
        },
        Some("Volume + %.0f%%"),
    ),
    numeric_binding(
        "iina.volume-down-5",
        false,
        &["add", "volume", "-5"],
        NumericActionMatch::Range {
            minimum: -10.0,
            maximum: -5.0,
        },
        Some("Volume - %.0f%%"),
    ),
    numeric_binding(
        "iina.volume-up-1",
        false,
        &["add", "volume", "1"],
        NumericActionMatch::Range {
            minimum: 1.0,
            maximum: 2.0,
        },
        Some("Volume + %.0f%%"),
    ),
    numeric_binding(
        "iina.volume-down-1",
        false,
        &["add", "volume", "-1"],
        NumericActionMatch::Range {
            minimum: -2.0,
            maximum: -1.0,
        },
        Some("Volume - %.0f%%"),
    ),
    numeric_binding(
        "iina.audio-delay-down-0_5",
        false,
        &["add", "audio-delay", "-0.5"],
        NumericActionMatch::Exact,
        Some("Audio Delay - %.1fs"),
    ),
    numeric_binding(
        "iina.audio-delay-down-0_1",
        false,
        &["add", "audio-delay", "-0.1"],
        NumericActionMatch::Exact,
        Some("Audio Delay - %.1fs"),
    ),
    numeric_binding(
        "iina.audio-delay-up-0_5",
        false,
        &["add", "audio-delay", "0.5"],
        NumericActionMatch::Exact,
        Some("Audio Delay + %.1fs"),
    ),
    numeric_binding(
        "iina.audio-delay-up-0_1",
        false,
        &["add", "audio-delay", "0.1"],
        NumericActionMatch::Exact,
        Some("Audio Delay + %.1fs"),
    ),
    numeric_binding(
        "iina.audio-delay-reset",
        false,
        &["set", "audio-delay", "0"],
        NumericActionMatch::Exact,
        None,
    ),
    numeric_binding(
        "iina.subtitle-delay-down-0_5",
        false,
        &["add", "sub-delay", "-0.5"],
        NumericActionMatch::Exact,
        Some("Subtitle Delay - %.1fs"),
    ),
    numeric_binding(
        "iina.subtitle-delay-down-0_1",
        false,
        &["add", "sub-delay", "-0.1"],
        NumericActionMatch::Exact,
        Some("Subtitle Delay - %.1fs"),
    ),
    numeric_binding(
        "iina.subtitle-delay-up-0_5",
        false,
        &["add", "sub-delay", "0.5"],
        NumericActionMatch::Exact,
        Some("Subtitle Delay + %.1fs"),
    ),
    numeric_binding(
        "iina.subtitle-delay-up-0_1",
        false,
        &["add", "sub-delay", "0.1"],
        NumericActionMatch::Exact,
        Some("Subtitle Delay + %.1fs"),
    ),
    numeric_binding(
        "iina.subtitle-delay-reset",
        false,
        &["set", "sub-delay", "0"],
        NumericActionMatch::Exact,
        None,
    ),
    numeric_binding(
        "iina.subtitle-scale-up",
        false,
        &["multiply", "sub-scale", "1.1"],
        NumericActionMatch::Range {
            minimum: 1.01,
            maximum: 1.49,
        },
        None,
    ),
    numeric_binding(
        "iina.subtitle-scale-down",
        false,
        &["multiply", "sub-scale", "0.9"],
        NumericActionMatch::Range {
            minimum: 0.71,
            maximum: 0.99,
        },
        None,
    ),
    numeric_binding(
        "iina.subtitle-scale-reset",
        false,
        &["set", "sub-scale", "1"],
        NumericActionMatch::Exact,
        None,
    ),
];

const fn binding(
    id: &'static str,
    iina_command: bool,
    action: &'static [&'static str],
) -> MenuBindingSpec {
    MenuBindingSpec {
        id,
        iina_command,
        action,
        numeric: None,
        title_template: None,
    }
}

const fn numeric_binding(
    id: &'static str,
    iina_command: bool,
    action: &'static [&'static str],
    numeric: NumericActionMatch,
    title_template: Option<&'static str>,
) -> MenuBindingSpec {
    MenuBindingSpec {
        id,
        iina_command,
        action,
        numeric: Some(numeric),
        title_template,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
struct MenuRequest {
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(rename = "providerId", skip_serializing_if = "Option::is_none")]
    provider_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SubtitleProviderMenuItem {
    id: String,
    name: String,
    menu_title: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginMenuRequest {
    owner_label: String,
    role: String,
    identifier: String,
    item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PluginDeveloperToolRequest {
    identifier: String,
    role: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlannedPluginShortcut {
    key_equivalent: String,
    modifier_mask: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PlannedPluginMenuNode {
    Separator,
    Item {
        id: String,
        title: String,
        enabled: bool,
        selected: bool,
        submenu: bool,
        shortcut: Option<PlannedPluginShortcut>,
        children: Vec<PlannedPluginMenuNode>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PluginMenuPlan {
    nodes: Vec<PlannedPluginMenuNode>,
    shortcut_conflicts: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RoutedPluginMenuItem {
    owner_label: String,
    role: String,
    item: PluginMenuItemDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActivePluginMenuDefinition {
    order_index: usize,
    identifier: String,
    name: String,
    has_global_instance: bool,
    items: Vec<RoutedPluginMenuItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuxiliaryWindowMenuAction {
    Inspector,
    LogViewer,
}

pub fn build_iina_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    build_iina_menu_with_plugin_items(app, &[])
}

pub fn refresh_iina_menu<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let plugin_items = app
        .state::<AppState>()
        .plugin_menus
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let menu =
        build_iina_menu_with_plugin_items(app, &plugin_items).map_err(|error| error.to_string())?;
    let active_plugin_owner = active_plugin_owner_window_label(app);
    let active_plugin_menus = active_plugin_menu_definitions(&plugin_items, &active_plugin_owner);
    let active_key_bindings = active_key_bindings_for_app(app);
    let plugin_plan = plugin_menu_plan(
        &active_plugin_menus,
        &active_key_bindings,
        crate::native_menu::plugin_developer_tool_available(),
    );
    let resolved_binding_groups = resolved_menu_binding_groups_for_app(app);
    let resolved_bindings = primary_menu_bindings(&resolved_binding_groups);
    let (saved_video_filters, saved_audio_filters) = saved_filters_for_menu(app);
    let mut key_equivalents = native_menu_key_equivalents(
        &menu,
        &resolved_bindings,
        &saved_video_filters,
        &saved_audio_filters,
        &resolved_binding_groups,
    )?;
    key_equivalents.extend(plugin_menu_key_equivalents(&menu, &plugin_plan)?);
    let plugin_item_states = plugin_menu_item_states(&menu, &plugin_plan)?;
    let alternate_items = native_alternate_menu_items(&menu);
    let responder_actions = native_edit_responder_actions(&menu)?;
    let visibility = native_hidden_menu_items(&menu, &resolved_binding_groups)?;
    app.set_menu(menu).map_err(|error| error.to_string())?;
    app.run_on_main_thread(move || {
        if let Err(error) = crate::native_menu::configure_key_equivalents(&key_equivalents) {
            eprintln!("failed to configure IINA key equivalents: {error}");
        }
        if let Err(error) = crate::native_menu::configure_alternate_items(&alternate_items) {
            eprintln!("failed to configure IINA alternate menu items: {error}");
        }
        if let Err(error) = crate::native_menu::configure_item_states(&plugin_item_states) {
            eprintln!("failed to configure IINA plugin menu states: {error}");
        }
        if let Err(error) = crate::native_menu::configure_responder_actions(&responder_actions) {
            eprintln!("failed to configure IINA responder-chain menu actions: {error}");
        }
        if let Err(error) = crate::native_menu::configure_visibility(&visibility) {
            eprintln!("failed to configure IINA hidden menu items: {error}");
        }
    })
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn build_iina_menu_with_plugin_items<R: Runtime>(
    app: &AppHandle<R>,
    plugin_items: &[PluginMenuDefinition],
) -> tauri::Result<Menu<R>> {
    let player = active_player_snapshot(app);
    let (saved_video_filters, saved_audio_filters) = saved_filters_for_menu(app);
    let resolved_binding_groups = resolved_menu_binding_groups_for_app(app);
    let resolved_bindings = primary_menu_bindings(&resolved_binding_groups);
    let menu = Menu::new(app)?;
    menu.append(&app_menu(app)?)?;
    let file_menu = file_menu(app, &player)?;
    menu.append(&file_menu)?;
    menu.append(&edit_menu(app)?)?;
    menu.append(&playback_menu(app, &player)?)?;
    menu.append(&video_menu(app, &player, &saved_video_filters)?)?;
    menu.append(&audio_menu(app, &player, &saved_audio_filters)?)?;
    menu.append(&subtitles_menu(app, &player)?)?;
    let active_plugin_owner = active_plugin_owner_window_label(app);
    let active_plugin_menus = active_plugin_menu_definitions(plugin_items, &active_plugin_owner);
    let active_key_bindings = active_key_bindings_for_app(app);
    let plugin_plan = plugin_menu_plan(
        &active_plugin_menus,
        &active_key_bindings,
        crate::native_menu::plugin_developer_tool_available(),
    );
    menu.append(&plugin_menu(app, &plugin_plan)?)?;
    menu.append(&window_menu(app, &player)?)?;
    menu.append(&help_menu(app)?)?;
    apply_resolved_menu_titles(&menu, &resolved_bindings)?;
    append_other_key_bindings_menu(app, &menu, &file_menu, &resolved_binding_groups)?;
    Ok(menu)
}

fn resolved_menu_bindings_for_app<R: Runtime>(
    app: &AppHandle<R>,
) -> HashMap<&'static str, ResolvedMenuBinding> {
    primary_menu_bindings(&resolved_menu_binding_groups_for_app(app))
}

fn resolved_menu_binding_groups_for_app<R: Runtime>(
    app: &AppHandle<R>,
) -> HashMap<&'static str, Vec<ResolvedMenuBinding>> {
    let modeled = app.try_state::<AppState>().and_then(|state| {
        state
            .preferences
            .lock()
            .ok()
            .and_then(|preferences| preferences.values.get("modeledKeyBindings").cloned())
    });
    resolve_menu_binding_groups(&active_key_bindings_from_preference(modeled.as_ref()))
}

fn active_key_bindings_for_app<R: Runtime>(app: &AppHandle<R>) -> Vec<ActiveKeyBinding> {
    let modeled = app.try_state::<AppState>().and_then(|state| {
        state
            .preferences
            .lock()
            .ok()
            .and_then(|preferences| preferences.values.get("modeledKeyBindings").cloned())
    });
    active_key_bindings_from_preference(modeled.as_ref())
}

#[cfg(test)]
fn resolve_menu_bindings(
    bindings: &[ActiveKeyBinding],
) -> HashMap<&'static str, ResolvedMenuBinding> {
    primary_menu_bindings(&resolve_menu_binding_groups(bindings))
}

fn primary_menu_bindings(
    groups: &HashMap<&'static str, Vec<ResolvedMenuBinding>>,
) -> HashMap<&'static str, ResolvedMenuBinding> {
    groups
        .iter()
        .filter_map(|(id, bindings)| bindings.first().cloned().map(|binding| (*id, binding)))
        .collect()
}

fn resolve_menu_binding_groups(
    bindings: &[ActiveKeyBinding],
) -> HashMap<&'static str, Vec<ResolvedMenuBinding>> {
    MENU_BINDING_SPECS
        .iter()
        .filter_map(|spec| {
            let resolved = bindings
                .iter()
                .filter_map(|binding| resolve_menu_binding(binding, spec))
                .collect::<Vec<_>>();
            (!resolved.is_empty()).then_some((spec.id, resolved))
        })
        .collect()
}

fn resolve_menu_binding(
    binding: &ActiveKeyBinding,
    spec: &MenuBindingSpec,
) -> Option<ResolvedMenuBinding> {
    if binding.is_iina_command != spec.iina_command {
        return None;
    }
    let (numeric_value, exact_seek) = matching_action(binding, spec)?;
    let (key_equivalent, modifier_mask) = native_key_equivalent(&binding.normalized_mpv_key)?;
    let title = spec.title_template.and_then(|template| {
        numeric_value
            .map(f64::abs)
            .map(|value| localization::menu_number(template, value))
    });
    Some(ResolvedMenuBinding {
        key_equivalent,
        modifier_mask,
        numeric_value,
        exact_seek,
        title,
    })
}

fn matching_action(
    binding: &ActiveKeyBinding,
    spec: &MenuBindingSpec,
) -> Option<(Option<f64>, bool)> {
    let (action, exact_seek) = normalized_seek_action(&binding.action, spec.action);
    if action.len() != spec.action.len() || action.is_empty() {
        return None;
    }
    let Some(numeric_match) = spec.numeric else {
        return (action
            .iter()
            .zip(spec.action)
            .all(|(actual, expected)| actual == expected))
        .then_some((None, exact_seek));
    };
    if !action[..action.len() - 1]
        .iter()
        .zip(&spec.action[..spec.action.len() - 1])
        .all(|(actual, expected)| actual == expected)
    {
        return None;
    }
    let actual = action.last()?.parse::<f64>().ok()?;
    let expected = spec.action.last()?.parse::<f64>().ok()?;
    let matches = match numeric_match {
        NumericActionMatch::Exact => actual == expected,
        NumericActionMatch::Range { minimum, maximum } => (minimum..=maximum).contains(&actual),
    };
    matches.then_some((Some(actual), exact_seek))
}

fn normalized_seek_action(action: &[String], expected: &[&str]) -> (Vec<String>, bool) {
    let mut action = action.to_vec();
    let mut exact_seek = false;
    if action.first().map(String::as_str) != Some("seek")
        || expected.first().copied() != Some("seek")
        || action.len() <= 2
    {
        return (action, exact_seek);
    }
    if action.len() == 4 {
        action[2] = format!("{}+{}", action[2], action[3]);
        action.pop();
    }
    let mut flags = action
        .last()
        .map(|flags| flags.split('+').map(str::to_string).collect::<Vec<_>>())
        .unwrap_or_default();
    flags.retain(|flag| {
        if flag == "relative" {
            return false;
        }
        if flag == "exact" {
            exact_seek = true;
            return false;
        }
        true
    });
    if flags.is_empty() {
        action.pop();
    } else if let Some(last) = action.last_mut() {
        *last = flags.join("+");
    }
    (action, exact_seek)
}

fn native_key_equivalent(normalized_mpv_key: &str) -> Option<(String, u32)> {
    if normalized_mpv_key.matches('-').count() > 1 {
        return None;
    }
    let mut parts = normalized_mpv_key.split('+').collect::<Vec<_>>();
    let key = parts.pop()?;
    let mut modifier_mask = 0;
    for modifier in parts {
        modifier_mask |= match modifier {
            "Meta" => NATIVE_MODIFIER_COMMAND,
            "Ctrl" => NATIVE_MODIFIER_CONTROL,
            "Alt" => NATIVE_MODIFIER_OPTION,
            "Shift" => NATIVE_MODIFIER_SHIFT,
            _ => 0,
        };
    }
    let key_equivalent = native_key_character(key)?;
    (!key_equivalent.contains('\0')).then_some((key_equivalent, modifier_mask))
}

fn native_key_character(key: &str) -> Option<String> {
    let mapped = match key {
        "LEFT" => "\u{f702}",
        "RIGHT" => "\u{f703}",
        "UP" => "\u{f700}",
        "DOWN" => "\u{f701}",
        "BS" => "\u{0008}",
        "KP_DEL" | "DEL" => "\u{007f}",
        "KP_INS" | "INS" => "\u{f727}",
        "HOME" => "\u{f729}",
        "END" => "\u{f72b}",
        "PGUP" => "\u{f72c}",
        "PGDWN" => "\u{f72d}",
        "PRINT" => "\u{f72e}",
        "SPACE" => " ",
        "IDEOGRAPHIC_SPACE" => "\u{3000}",
        "SHARP" => "#",
        "ENTER" | "KP_ENTER" => "\r",
        "ESC" => "\u{001b}",
        "KP_DEC" => ".",
        "KP0" => "0",
        "KP1" => "1",
        "KP2" => "2",
        "KP3" => "3",
        "KP4" => "4",
        "KP5" => "5",
        "KP6" => "6",
        "KP7" => "7",
        "KP8" => "8",
        "KP9" => "9",
        "PLUS" => "+",
        _ => {
            if let Some(function) = key
                .strip_prefix('F')
                .and_then(|value| value.parse::<u32>().ok())
            {
                if (1..=12).contains(&function) {
                    return char::from_u32(0xf703 + function).map(|key| key.to_string());
                }
            }
            return (key.chars().count() == 1).then(|| key.to_string());
        }
    };
    Some(mapped.to_string())
}

fn native_saved_filter_key_equivalent(filter: &SavedFilter) -> Option<(String, u32)> {
    let key = filter.shortcut_key.trim();
    if key.is_empty() || key.contains('\0') {
        return None;
    }
    let key_equivalent = native_key_character(key)
        .or_else(|| (key.chars().count() == 1).then(|| key.to_string()))?;
    let mut modifier_mask = 0;
    for modifier in filter.shortcut_key_modifiers.chars() {
        modifier_mask |= match modifier {
            'm' => NATIVE_MODIFIER_COMMAND,
            'c' => NATIVE_MODIFIER_CONTROL,
            'o' => NATIVE_MODIFIER_OPTION,
            's' => NATIVE_MODIFIER_SHIFT,
            _ => 0,
        };
    }
    Some((key_equivalent, modifier_mask))
}

fn apply_resolved_menu_titles<R: Runtime>(
    menu: &Menu<R>,
    resolved: &HashMap<&'static str, ResolvedMenuBinding>,
) -> tauri::Result<()> {
    for (id, binding) in resolved {
        let Some(title) = &binding.title else {
            continue;
        };
        let Some(item) = find_menu_item(menu.items()?, id) else {
            continue;
        };
        match item {
            MenuItemKind::MenuItem(item) => item.set_text(title)?,
            MenuItemKind::Check(item) => item.set_text(title)?,
            MenuItemKind::Icon(item) => item.set_text(title)?,
            MenuItemKind::Submenu(_) | MenuItemKind::Predefined(_) => {}
        }
    }
    Ok(())
}

fn find_menu_item<R: Runtime>(items: Vec<MenuItemKind<R>>, id: &str) -> Option<MenuItemKind<R>> {
    for item in items {
        if item.id().as_ref() == id {
            return Some(item);
        }
        if let MenuItemKind::Submenu(submenu) = &item {
            if let Some(found) = find_menu_item(submenu.items().ok()?, id) {
                return Some(found);
            }
        }
    }
    None
}

fn root_menu_and_item<R: Runtime>(menu: &Menu<R>, id: &str) -> Option<(String, String)> {
    for root in menu.items().ok()? {
        let MenuItemKind::Submenu(submenu) = root else {
            continue;
        };
        let Some(item) = find_menu_item(submenu.items().ok()?, id) else {
            continue;
        };
        let item_title = match item {
            MenuItemKind::MenuItem(item) => item.text().ok()?,
            MenuItemKind::Check(item) => item.text().ok()?,
            MenuItemKind::Icon(item) => item.text().ok()?,
            MenuItemKind::Submenu(item) => item.text().ok()?,
            MenuItemKind::Predefined(_) => continue,
        };
        return Some((submenu.text().ok()?, item_title));
    }
    None
}

fn menu_item_title<R: Runtime>(item: &MenuItemKind<R>) -> Option<String> {
    match item {
        MenuItemKind::MenuItem(item) => item.text().ok(),
        MenuItemKind::Check(item) => item.text().ok(),
        MenuItemKind::Icon(item) => item.text().ok(),
        MenuItemKind::Submenu(item) => item.text().ok(),
        MenuItemKind::Predefined(_) => None,
    }
}

fn submenu_path_for_id<R: Runtime>(items: Vec<MenuItemKind<R>>, id: &str) -> Option<Vec<String>> {
    for item in items {
        if item.id().as_ref() == id {
            return menu_item_title(&item).map(|title| vec![title]);
        }
        if let MenuItemKind::Submenu(submenu) = &item {
            let Some(mut path) = submenu_path_for_id(submenu.items().ok()?, id) else {
                continue;
            };
            path.insert(0, submenu.text().ok()?);
            return Some(path);
        }
    }
    None
}

fn menu_path_for_id<R: Runtime>(menu: &Menu<R>, id: &str) -> Option<Vec<String>> {
    for root in menu.items().ok()? {
        let MenuItemKind::Submenu(submenu) = root else {
            continue;
        };
        let Some(mut path) = submenu_path_for_id(submenu.items().ok()?, id) else {
            continue;
        };
        path.insert(0, submenu.text().ok()?);
        return Some(path);
    }
    None
}

fn native_edit_responder_actions<R: Runtime>(
    menu: &Menu<R>,
) -> Result<Vec<NativeMenuResponderAction>, String> {
    let mut actions = EDIT_RESPONDER_ACTIONS
        .iter()
        .map(|(id, selector, key_equivalent)| {
            let path = menu_path_for_id(menu, id)
                .ok_or_else(|| format!("native responder target {id} is missing"))?;
            match path.as_slice() {
                [menu_title, item_title] => Ok(NativeMenuResponderAction::item(
                    menu_title.clone(),
                    item_title.clone(),
                    *selector,
                    *key_equivalent,
                )),
                [menu_title, submenu_title, item_title] => {
                    Ok(NativeMenuResponderAction::submenu_item(
                        menu_title.clone(),
                        submenu_title.clone(),
                        item_title.clone(),
                        *selector,
                    ))
                }
                _ => Err(format!(
                    "native responder target {id} has unsupported menu depth {}",
                    path.len()
                )),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let path = menu_path_for_id(menu, CUSTOM_TOUCH_BAR_MENU_ID)
        .ok_or_else(|| "native Custom Touch Bar responder target is missing".to_string())?;
    match path.as_slice() {
        [menu_title, item_title] => actions.push(NativeMenuResponderAction::item(
            menu_title.clone(),
            item_title.clone(),
            "toggleTouchBarCustomizationPalette:",
            "",
        )),
        _ => {
            return Err(format!(
                "native Custom Touch Bar target has unsupported menu depth {}",
                path.len()
            ));
        }
    }
    Ok(actions)
}

fn duplicate_menu_binding_entries<'a>(
    groups: &'a HashMap<&'static str, Vec<ResolvedMenuBinding>>,
) -> Vec<(usize, usize, &'static str, &'a ResolvedMenuBinding)> {
    let mut duplicates = Vec::new();
    for (spec_index, spec) in MENU_BINDING_SPECS.iter().enumerate() {
        let Some(bindings) = groups.get(spec.id) else {
            continue;
        };
        for (binding_index, binding) in bindings.iter().enumerate().skip(1) {
            duplicates.push((spec_index, binding_index, spec.id, binding));
        }
    }
    duplicates
}

fn duplicate_menu_binding_id(spec_index: usize, binding_index: usize) -> String {
    format!("{OTHER_KEY_BINDING_ITEM_ID_PREFIX}{spec_index}.{binding_index}")
}

fn duplicate_menu_binding_from_id<'a>(
    id: &str,
    groups: &'a HashMap<&'static str, Vec<ResolvedMenuBinding>>,
) -> Option<(&'static str, &'a ResolvedMenuBinding)> {
    let suffix = id.strip_prefix(OTHER_KEY_BINDING_ITEM_ID_PREFIX)?;
    let (spec_index, binding_index) = suffix.split_once('.')?;
    let spec_index = spec_index.parse::<usize>().ok()?;
    let binding_index = binding_index.parse::<usize>().ok()?;
    let spec = MENU_BINDING_SPECS.get(spec_index)?;
    (binding_index > 0)
        .then(|| {
            groups
                .get(spec.id)?
                .get(binding_index)
                .map(|binding| (spec.id, binding))
        })
        .flatten()
}

fn append_other_key_bindings_menu<R: Runtime>(
    app: &AppHandle<R>,
    menu: &Menu<R>,
    file_menu: &Submenu<R>,
    groups: &HashMap<&'static str, Vec<ResolvedMenuBinding>>,
) -> tauri::Result<()> {
    let submenu = localized_submenu(
        app,
        OTHER_KEY_BINDINGS_MENU_ID,
        OTHER_KEY_BINDINGS_MENU_TITLE,
        true,
    )?;
    for (spec_index, binding_index, target_id, binding) in duplicate_menu_binding_entries(groups) {
        let title = binding
            .title
            .clone()
            .or_else(|| root_menu_and_item(menu, target_id).map(|(_, item_title)| item_title))
            .unwrap_or_else(|| target_id.to_string());
        add_literal_item(
            &submenu,
            app,
            &duplicate_menu_binding_id(spec_index, binding_index),
            &title,
            None,
            true,
        )?;
    }
    file_menu.append(&submenu)
}

fn native_hidden_menu_items<R: Runtime>(
    menu: &Menu<R>,
    _groups: &HashMap<&'static str, Vec<ResolvedMenuBinding>>,
) -> Result<Vec<NativeMenuVisibility>, String> {
    let path = menu_path_for_id(menu, OTHER_KEY_BINDINGS_MENU_ID)
        .ok_or_else(|| "hidden other-key-bindings menu is missing".to_string())?;
    let [menu_title, item_title] = path.as_slice() else {
        return Err(format!(
            "hidden other-key-bindings menu has unsupported depth {}",
            path.len()
        ));
    };
    let mut hidden = vec![NativeMenuVisibility::hidden(
        menu_title.clone(),
        item_title.clone(),
    )];
    let custom_path = menu_path_for_id(menu, CUSTOM_TOUCH_BAR_MENU_ID)
        .ok_or_else(|| "hidden Custom Touch Bar menu item is missing".to_string())?;
    match custom_path.as_slice() {
        [menu_title, item_title] => hidden.push(NativeMenuVisibility::hidden(
            menu_title.clone(),
            item_title.clone(),
        )),
        _ => {
            return Err(format!(
                "hidden Custom Touch Bar menu item has unsupported depth {}",
                custom_path.len()
            ));
        }
    }
    Ok(hidden)
}

fn native_menu_key_equivalents<R: Runtime>(
    menu: &Menu<R>,
    resolved: &HashMap<&'static str, ResolvedMenuBinding>,
    saved_video_filters: &[SavedFilter],
    saved_audio_filters: &[SavedFilter],
    groups: &HashMap<&'static str, Vec<ResolvedMenuBinding>>,
) -> Result<Vec<NativeMenuKeyEquivalent>, String> {
    let mut plan = Vec::new();
    for spec in MENU_BINDING_SPECS {
        let (menu_title, item_title) = root_menu_and_item(menu, spec.id)
            .ok_or_else(|| format!("menu binding target {} is missing", spec.id))?;
        let binding = resolved.get(spec.id);
        plan.push(NativeMenuKeyEquivalent::item(
            menu_title,
            item_title,
            binding
                .map(|binding| binding.key_equivalent.clone())
                .unwrap_or_default(),
            binding.map(|binding| binding.modifier_mask).unwrap_or(0),
        ));
    }
    for (menu_title, submenu_title, filters) in [
        (
            localization::menu_title_key("MainMenu", "H8h-7b-M4v.title", "Video"),
            localization::menu_title("Saved Video Filters"),
            saved_video_filters,
        ),
        (
            localization::menu_title_key("MainMenu", "lYN-0x-lzT.title", "Audio"),
            localization::menu_title("Saved Audio Filters"),
            saved_audio_filters,
        ),
    ] {
        for (index, filter) in filters.iter().enumerate() {
            let (key_equivalent, modifier_mask) =
                native_saved_filter_key_equivalent(filter).unwrap_or_default();
            plan.push(NativeMenuKeyEquivalent::submenu_index(
                menu_title.clone(),
                submenu_title.clone(),
                index,
                key_equivalent,
                modifier_mask,
            ));
        }
    }
    let hidden_menu_path = menu_path_for_id(menu, OTHER_KEY_BINDINGS_MENU_ID)
        .ok_or_else(|| "hidden other-key-bindings menu is missing".to_string())?;
    let [menu_title, submenu_title] = hidden_menu_path.as_slice() else {
        return Err(format!(
            "hidden other-key-bindings menu has unsupported depth {}",
            hidden_menu_path.len()
        ));
    };
    for (item_index, (_, _, _, binding)) in duplicate_menu_binding_entries(groups)
        .into_iter()
        .enumerate()
    {
        plan.push(NativeMenuKeyEquivalent::submenu_index(
            menu_title.clone(),
            submenu_title.clone(),
            item_index,
            binding.key_equivalent.clone(),
            binding.modifier_mask,
        ));
    }
    Ok(plan)
}

fn plugin_menu_key_equivalents<R: Runtime>(
    menu: &Menu<R>,
    plugin_plan: &PluginMenuPlan,
) -> Result<Vec<NativeMenuKeyEquivalent>, String> {
    let menu_title = menu
        .items()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find_map(|item| match item {
            MenuItemKind::Submenu(submenu) if submenu.id().as_ref() == "iina.menu.plugin" => {
                submenu.text().ok()
            }
            _ => None,
        })
        .ok_or_else(|| "Plugin menu is missing".to_string())?;
    let mut key_equivalents = Vec::new();
    for (index, node) in plugin_plan.nodes.iter().enumerate() {
        collect_plugin_menu_key_equivalents(
            &menu_title,
            node,
            &mut vec![index],
            &mut key_equivalents,
        );
    }
    Ok(key_equivalents)
}

fn collect_plugin_menu_key_equivalents(
    menu_title: &str,
    node: &PlannedPluginMenuNode,
    path: &mut Vec<usize>,
    key_equivalents: &mut Vec<NativeMenuKeyEquivalent>,
) {
    let PlannedPluginMenuNode::Item {
        shortcut, children, ..
    } = node
    else {
        return;
    };
    if let Some(shortcut) = shortcut {
        key_equivalents.push(NativeMenuKeyEquivalent::path(
            menu_title.to_string(),
            path.clone(),
            shortcut.key_equivalent.clone(),
            shortcut.modifier_mask,
        ));
    }
    for (index, child) in children.iter().enumerate() {
        path.push(index);
        collect_plugin_menu_key_equivalents(menu_title, child, path, key_equivalents);
        path.pop();
    }
}

fn plugin_menu_item_states<R: Runtime>(
    menu: &Menu<R>,
    plugin_plan: &PluginMenuPlan,
) -> Result<Vec<NativeMenuItemState>, String> {
    let menu_title = menu
        .items()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find_map(|item| match item {
            MenuItemKind::Submenu(submenu) if submenu.id().as_ref() == "iina.menu.plugin" => {
                submenu.text().ok()
            }
            _ => None,
        })
        .ok_or_else(|| "Plugin menu is missing".to_string())?;
    let mut item_states = Vec::new();
    for (index, node) in plugin_plan.nodes.iter().enumerate() {
        collect_plugin_menu_item_states(&menu_title, node, &mut vec![index], &mut item_states);
    }
    Ok(item_states)
}

fn collect_plugin_menu_item_states(
    menu_title: &str,
    node: &PlannedPluginMenuNode,
    path: &mut Vec<usize>,
    item_states: &mut Vec<NativeMenuItemState>,
) {
    let PlannedPluginMenuNode::Item {
        selected,
        submenu,
        children,
        ..
    } = node
    else {
        return;
    };
    if *submenu || !children.is_empty() {
        item_states.push(NativeMenuItemState::path(
            menu_title.to_string(),
            path.clone(),
            *selected,
        ));
    }
    for (index, child) in children.iter().enumerate() {
        path.push(index);
        collect_plugin_menu_item_states(menu_title, child, path, item_states);
        path.pop();
    }
}

pub fn handle_iina_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let target = active_player_window_label(app);
    if id == "iina.about" {
        if let Err(error) = crate::about_window::show_about_window(app) {
            eprintln!("failed to show About IINA window: {error}");
        }
        return;
    }
    if id.starts_with(OTHER_KEY_BINDING_ITEM_ID_PREFIX) {
        let groups = resolved_menu_binding_groups_for_app(app);
        if let Some((target_id, binding)) = duplicate_menu_binding_from_id(id, &groups) {
            if !apply_player_menu_command_with_binding(app, target_id, &target, Some(binding)) {
                handle_iina_menu_event(app, target_id);
            }
        }
        return;
    }
    if let Some(provider_id) = subtitle_provider_id_from_menu_item(id) {
        emit_provider_request(app, &target, "find-online-subtitles", provider_id);
        return;
    }
    if let Some(path) = recent_document_path_for_menu_item(app, id) {
        emit_request_with_path(app, &target, "open-recent", path);
        return;
    }
    if let Some(request) = plugin_menu_request_from_id(id) {
        let owner = request.owner_label.clone();
        let _ = app.emit_to(&owner, PLUGIN_MENU_EVENT, request);
        return;
    }
    if let Some(request) = plugin_developer_tool_request_from_id(id) {
        let plugin_owner = active_plugin_owner_window_label(app);
        if let Err(error) = crate::plugin_developer_tool::show_for_owner(
            app,
            &plugin_owner,
            &request.identifier,
            &request.role,
        ) {
            eprintln!("failed to show plugin Developer Tool: {error}");
        }
        return;
    }
    if let Some(action) = frontend_request_for_menu_item(id) {
        emit_request(app, &target, action);
        return;
    }
    if let Some(action) = auxiliary_window_action_for_menu_item(id) {
        match action {
            AuxiliaryWindowMenuAction::Inspector => {
                if let Err(error) = crate::inspector_window::show_inspector_window(app) {
                    eprintln!("failed to show inspector window: {error}");
                }
            }
            AuxiliaryWindowMenuAction::LogViewer => {
                if let Err(error) = crate::auxiliary_windows::show_log_viewer_window(app) {
                    eprintln!("failed to show log viewer window: {error}");
                }
            }
        }
        return;
    }
    if let Some(page) = external_page_for_menu_item(id) {
        if let Err(error) = crate::auxiliary_windows::open_iina_external_page(page) {
            eprintln!("failed to open IINA page {}: {error}", page.url());
        }
        return;
    }
    if let Some(action) = window_size_action_for_menu_item(id) {
        resize_player_window(app, &target, action);
        return;
    }
    match id {
        "iina.new-window" => {
            let state = app.state::<AppState>();
            if let Err(error) = crate::commands::open_new_empty_player_window(app, state.inner()) {
                eprintln!("failed to open a new IINA window: {error}");
            }
        }
        "iina.open" => emit_request(app, &target, "open"),
        "iina.open-new-window" => emit_request(app, &target, "open-new-window"),
        "iina.open-url" => {
            if let Err(error) =
                crate::auxiliary_player_windows::show_open_url_for_owner(app, &target, false, false)
            {
                eprintln!("failed to show Open URL window: {error}");
            }
        }
        "iina.open-url-new-window" => {
            if let Err(error) =
                crate::auxiliary_player_windows::show_open_url_for_owner(app, &target, true, false)
            {
                eprintln!("failed to show alternate Open URL window: {error}");
            }
        }
        "iina.clear-recent" => emit_request(app, &target, "clear-recent"),
        "iina.preferences" => {
            if let Err(error) =
                crate::auxiliary_player_windows::show_preferences_for_pane(app, None, None, false)
            {
                eprintln!("failed to show Preferences window: {error}");
            }
        }
        "iina.check-updates" => emit_request(app, &target, "check-updates"),
        "iina.playback-history" => {
            let _ = crate::commands::show_playback_history_window(app);
        }
        "iina.delete-current-file" => {
            if let Some(window) = app.get_webview_window(&target) {
                let state = app.state::<AppState>();
                if let Err(error) =
                    delete_current_file(app, state.inner(), &window, FileRemovalMode::Trash)
                {
                    eprintln!("failed to delete the current file: {error}");
                }
            }
        }
        "iina.screenshot" => emit_request(app, &target, "screenshot"),
        "iina.goto-screenshot-folder" => emit_request(app, &target, "goto-screenshot-folder"),
        "iina.video-filters" => {
            if let Err(error) =
                crate::auxiliary_player_windows::show_filter_for_owner(app, &target, "video")
            {
                eprintln!("failed to show Video Filters window: {error}");
            }
        }
        "iina.audio-filters" => {
            if let Err(error) =
                crate::auxiliary_player_windows::show_filter_for_owner(app, &target, "audio")
            {
                eprintln!("failed to show Audio Filters window: {error}");
            }
        }
        "iina.load-external-audio" => emit_request(app, &target, "load-external-audio"),
        "iina.load-external-subtitle" => emit_request(app, &target, "load-external-subtitle"),
        "iina.custom-crop" => emit_request(app, &target, "custom-crop"),
        "iina.find-online-subtitles" => emit_request(app, &target, "find-online-subtitles"),
        "iina.save-downloaded-subtitle" => emit_request(app, &target, "save-downloaded-subtitle"),
        "iina.manage-plugins" => emit_request(app, &target, "manage-plugins"),
        "iina.reload-all-plugins" => {
            let plugin_owner = active_plugin_owner_window_label(app);
            let _ = app.emit_to(&plugin_owner, PLUGIN_RUNTIME_RELOAD_ALL_EVENT, ());
        }
        "iina.release-highlights" => {
            if let Err(error) = crate::auxiliary_windows::show_release_highlights_window(app) {
                eprintln!("failed to show release highlights: {error}");
            }
        }
        "iina.enter-music-mode" => emit_request(app, &target, "music-mode"),
        "iina.picture-in-picture" => emit_request(app, &target, "picture-in-picture"),
        "iina.fullscreen" => toggle_fullscreen(app, &target),
        "iina.float-on-top" => toggle_always_on_top(app, &target),
        _ => apply_player_menu_command(app, id, &target),
    }
}

fn window_size_action_for_menu_item(id: &str) -> Option<WindowSizeAction> {
    match id {
        "iina.half-size" => Some(WindowSizeAction::Half),
        "iina.normal-size" => Some(WindowSizeAction::Normal),
        "iina.double-size" => Some(WindowSizeAction::Double),
        "iina.fit-screen" => Some(WindowSizeAction::FitToScreen),
        "iina.bigger-size" => Some(WindowSizeAction::Bigger),
        "iina.smaller-size" => Some(WindowSizeAction::Smaller),
        _ => None,
    }
}

fn frontend_request_for_menu_item(id: &str) -> Option<&'static str> {
    match id {
        "iina.jump-to" => Some("jump-to"),
        "iina.save-current-playlist" => Some("save-current-playlist"),
        "iina.subtitle-font" => Some("subtitle-font"),
        "iina.delogo" => Some("delogo"),
        _ => None,
    }
}

fn auxiliary_window_action_for_menu_item(id: &str) -> Option<AuxiliaryWindowMenuAction> {
    match id {
        "iina.inspector" => Some(AuxiliaryWindowMenuAction::Inspector),
        "iina.log-viewer" => Some(AuxiliaryWindowMenuAction::LogViewer),
        _ => None,
    }
}

fn external_page_for_menu_item(id: &str) -> Option<IinaExternalPage> {
    match id {
        "iina.help" => Some(IinaExternalPage::Help),
        "iina.github" => Some(IinaExternalPage::GitHub),
        "iina.website" => Some(IinaExternalPage::Website),
        _ => None,
    }
}

fn app_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(app, "iina.menu.app", "IINA", true)?;
    add_item_key(
        &submenu,
        app,
        "iina.about",
        "MainMenu",
        "5kV-Vb-QxS.title",
        "About IINA",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.check-updates",
        "Check for Updates...",
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.preferences",
        "Preferences...",
        Some("CmdOrCtrl+,"),
        true,
    )?;
    add_separator(&submenu, app)?;
    submenu.append(&PredefinedMenuItem::services(
        app,
        Some(&localization::menu_title("Services")),
    )?)?;
    add_separator(&submenu, app)?;
    submenu.append(&PredefinedMenuItem::hide(
        app,
        Some(&localization::menu_title("Hide IINA")),
    )?)?;
    submenu.append(&PredefinedMenuItem::hide_others(
        app,
        Some(&localization::menu_title("Hide Others")),
    )?)?;
    submenu.append(&PredefinedMenuItem::show_all(
        app,
        Some(&localization::menu_title("Show All")),
    )?)?;
    add_separator(&submenu, app)?;
    submenu.append(&PredefinedMenuItem::quit(
        app,
        Some(&localization::menu_title("Quit IINA")),
    )?)?;
    Ok(submenu)
}

fn file_menu<R: Runtime>(app: &AppHandle<R>, player: &PlayerState) -> tauri::Result<Submenu<R>> {
    let titles = open_menu_titles_for_app(app);
    let submenu = localized_submenu(app, "iina.menu.file", "File", true)?;
    add_item(
        &submenu,
        app,
        "iina.open",
        titles.open,
        Some("CmdOrCtrl+O"),
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.open-new-window",
        titles.open_alternative,
        Some("CmdOrCtrl+Alt+O"),
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.open-url",
        titles.open_url,
        Some("CmdOrCtrl+Shift+O"),
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.open-url-new-window",
        titles.open_url_alternative,
        Some("CmdOrCtrl+Alt+Shift+O"),
        true,
    )?;
    submenu.append(&open_recent_menu(app)?)?;
    if enable_cmd_n_for_app(app) {
        // IINA 1.3.5 keeps both of these XIB rows hidden until enableCmdN is
        // enabled: a separator, then New Window with Command-N.
        add_separator(&submenu, app)?;
        add_item(
            &submenu,
            app,
            "iina.new-window",
            &localization::menu_title("New Window"),
            Some("CmdOrCtrl+N"),
            true,
        )?;
    }
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.playback-history",
        "Playback History",
        Some("CmdOrCtrl+Shift+H"),
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.delete-current-file",
        "Delete Current File",
        None,
        can_delete_current_file(player),
    )?;
    add_item(
        &submenu,
        app,
        "iina.save-current-playlist",
        "Save Current Playlist...",
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    submenu.append(&PredefinedMenuItem::close_window(
        app,
        Some(&localization::menu_title("Close")),
    )?)?;
    Ok(submenu)
}

fn enable_cmd_n_for_app<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<AppState>()
        .and_then(|state| {
            state
                .preferences
                .lock()
                .ok()
                .map(|preferences| enable_cmd_n_value(preferences.values.get("enableCmdN")))
        })
        .unwrap_or(false)
}

fn enable_cmd_n_value(value: Option<&serde_json::Value>) -> bool {
    value.and_then(serde_json::Value::as_bool).unwrap_or(false)
}

fn open_menu_titles_for_app<R: Runtime>(app: &AppHandle<R>) -> OpenMenuTitles {
    let has_playing_media = app
        .try_state::<AppState>()
        .and_then(|state| state.has_playing_media().ok())
        .unwrap_or(false);
    let always_open_in_new_window = app
        .try_state::<AppState>()
        .and_then(|state| {
            state.preferences.lock().ok().and_then(|preferences| {
                preferences
                    .values
                    .get("alwaysOpenInNewWindow")
                    .and_then(serde_json::Value::as_bool)
            })
        })
        .unwrap_or(true);
    open_menu_titles(always_open_in_new_window, has_playing_media)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenMenuTitles {
    open: &'static str,
    open_alternative: &'static str,
    open_url: &'static str,
    open_url_alternative: &'static str,
}

fn open_menu_titles(always_open_in_new_window: bool, has_playing_media: bool) -> OpenMenuTitles {
    if !has_playing_media {
        return OpenMenuTitles {
            open: "Open...",
            open_alternative: "Open...",
            open_url: "Open URL...",
            open_url_alternative: "Open URL...",
        };
    }
    if always_open_in_new_window {
        OpenMenuTitles {
            open: "Open in New Window...",
            open_alternative: "Open...",
            open_url: "Open URL in New Window...",
            open_url_alternative: "Open URL...",
        }
    } else {
        OpenMenuTitles {
            open: "Open...",
            open_alternative: "Open in New Window...",
            open_url: "Open URL...",
            open_url_alternative: "Open URL in New Window...",
        }
    }
}

fn native_alternate_menu_items<R: Runtime>(menu: &Menu<R>) -> Vec<(String, String, bool)> {
    [
        ("iina.open-new-window", true),
        ("iina.open-url-new-window", true),
        ("iina.frame-step-forward", false),
        ("iina.frame-step-backward", false),
        ("iina.speed-1_1x", false),
        ("iina.speed-0_9x", false),
        ("iina.volume-up-1", false),
        ("iina.volume-down-1", false),
        ("iina.audio-delay-up-0_1", false),
        ("iina.audio-delay-down-0_1", false),
        ("iina.subtitle-delay-up-0_1", false),
        ("iina.subtitle-delay-down-0_1", false),
    ]
    .into_iter()
    .filter_map(|(id, require_option_accelerator)| {
        root_menu_and_item(menu, id)
            .map(|(menu_title, item_title)| (menu_title, item_title, require_option_accelerator))
    })
    .collect()
}

#[cfg(test)]
fn native_alternate_menu_items_for_titles(
    open_titles: OpenMenuTitles,
) -> Vec<(&'static str, &'static str, bool)> {
    let mut items = vec![
        ("File", open_titles.open_alternative, true),
        ("File", open_titles.open_url_alternative, true),
    ];
    items.extend([
        ("Playback", "Next Frame", false),
        ("Playback", "Previous Frame", false),
        ("Playback", "Speed Up to 1.1x", false),
        ("Playback", "Speed Down to 0.9x", false),
        ("Audio", "Volume + 1%", false),
        ("Audio", "Volume - 1%", false),
        ("Audio", "Audio Delay + 0.1s", false),
        ("Audio", "Audio Delay - 0.1s", false),
        ("Subtitles", "Subtitle Delay + 0.1s", false),
        ("Subtitles", "Subtitle Delay - 0.1s", false),
    ]);
    items
}

fn edit_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(app, "iina.menu.edit", "Edit", true)?;
    submenu.append(&PredefinedMenuItem::undo(
        app,
        Some(&localization::menu_title("Undo")),
    )?)?;
    submenu.append(&PredefinedMenuItem::redo(
        app,
        Some(&localization::menu_title("Redo")),
    )?)?;
    add_separator(&submenu, app)?;
    submenu.append(&PredefinedMenuItem::cut(
        app,
        Some(&localization::menu_title("Cut")),
    )?)?;
    submenu.append(&PredefinedMenuItem::copy(
        app,
        Some(&localization::menu_title("Copy")),
    )?)?;
    submenu.append(&PredefinedMenuItem::paste(
        app,
        Some(&localization::menu_title("Paste")),
    )?)?;
    add_item(&submenu, app, "iina.delete", "Delete", None, false)?;
    submenu.append(&PredefinedMenuItem::select_all(
        app,
        Some(&localization::menu_title("Select All")),
    )?)?;
    add_submenu_placeholder(
        &submenu,
        app,
        "iina.menu.transformations",
        "Transformations",
        &["Make Upper Case", "Make Lower Case", "Capitalize"],
    )?;
    add_submenu_placeholder(
        &submenu,
        app,
        "iina.menu.speech",
        "Speech",
        &["Start Speaking", "Stop Speaking"],
    )?;
    Ok(submenu)
}

fn playback_menu<R: Runtime>(
    app: &AppHandle<R>,
    player: &PlayerState,
) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(app, "iina.menu.playback", "Playback", true)?;
    add_item_key(
        &submenu,
        app,
        "iina.toggle-pause",
        "Localizable",
        if player.paused {
            "menu.resume"
        } else {
            "menu.pause"
        },
        if player.paused { "Resume" } else { "Pause" },
        Some("Space"),
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.stop",
        "Stop and Clear Playlists",
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.seek-forward-5",
        "Step Forward 5s",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.frame-step-forward",
        "Next Frame",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.seek-backward-5",
        "Step Backward 5s",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.frame-step-backward",
        "Previous Frame",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.jump-beginning",
        "Jump to Beginning",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.jump-to",
        "Jump to...",
        Some("CmdOrCtrl+J"),
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.speed-label",
        &localization::menu_number_key("Localizable", "menu.speed", "Speed: %.2fx", player.speed),
        None,
        false,
    )?;
    add_item(&submenu, app, "iina.speed-2x", "Speed Up to 2x", None, true)?;
    add_item(
        &submenu,
        app,
        "iina.speed-1_1x",
        "Speed Up to 1.1x",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.speed-0_5x",
        "Speed Down to 0.5x",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.speed-0_9x",
        "Speed Down to 0.9x",
        None,
        true,
    )?;
    add_item(&submenu, app, "iina.speed-reset", "Reset Speed", None, true)?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.screenshot",
        "Take a Screenshot",
        Some("CmdOrCtrl+Shift+S"),
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.goto-screenshot-folder",
        "Go to Screenshot Folder",
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    add_check_item(
        &submenu,
        app,
        "iina.ab-loop",
        "A-B Loop",
        None,
        true,
        player.ab_loop.is_active(),
    )?;
    add_check_item(
        &submenu,
        app,
        "iina.file-loop",
        "File Loop",
        None,
        true,
        player.loop_mode == LoopMode::File,
    )?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.show-playlist",
        if player.sidebar.visible && matches!(player.sidebar.tab, SidebarTab::Playlist) {
            "Hide Playlist Panel"
        } else {
            "Show Playlist Panel"
        },
        None,
        true,
    )?;
    add_check_item(
        &submenu,
        app,
        "iina.playlist-loop",
        "Playlist Loop",
        None,
        true,
        player.loop_mode == LoopMode::Playlist,
    )?;
    submenu.append(&playlist_submenu(app, player)?)?;
    add_separator(&submenu, app)?;
    add_item(&submenu, app, "iina.next-media", "Next Media", None, true)?;
    add_item(
        &submenu,
        app,
        "iina.previous-media",
        "Previous Media",
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.show-chapters",
        if player.sidebar.visible && matches!(player.sidebar.tab, SidebarTab::Chapters) {
            "Hide Chapters Panel"
        } else {
            "Show Chapters Panel"
        },
        None,
        true,
    )?;
    submenu.append(&chapters_submenu(app, player)?)?;
    add_item(
        &submenu,
        app,
        "iina.next-chapter",
        "Next Chapter",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.previous-chapter",
        "Previous Chapter",
        None,
        true,
    )?;
    Ok(submenu)
}

fn video_menu<R: Runtime>(
    app: &AppHandle<R>,
    player: &PlayerState,
    saved_filters: &[SavedFilter],
) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu_key(
        app,
        "iina.menu.video",
        "MainMenu",
        "H8h-7b-M4v.title",
        "Video",
        true,
    )?;
    let video_tracks = if player.current_url.is_some() {
        player.tracks.video.as_slice()
    } else {
        &[]
    };
    add_item(
        &submenu,
        app,
        "iina.show-video-panel",
        if player.sidebar.visible && matches!(player.sidebar.tab, SidebarTab::Video) {
            "Hide Video Panel"
        } else {
            "Show Video Panel"
        },
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.cycle-video-tracks",
        "Cycle Video Tracks",
        None,
        true,
    )?;
    submenu.append(&track_submenu(
        app,
        "iina.menu.video-track",
        "Video Track",
        "iina.select-video-track",
        video_tracks,
        selected_track_id(video_tracks),
    )?)?;
    add_separator(&submenu, app)?;
    add_item(&submenu, app, "iina.half-size", "Half Size", None, true)?;
    add_item(&submenu, app, "iina.normal-size", "Normal Size", None, true)?;
    add_item(&submenu, app, "iina.double-size", "Double Size", None, true)?;
    add_item(
        &submenu,
        app,
        "iina.fit-screen",
        "Fit to Screen",
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item(&submenu, app, "iina.bigger-size", "Bigger Size", None, true)?;
    add_item(
        &submenu,
        app,
        "iina.smaller-size",
        "Smaller Size",
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item_key(
        &submenu,
        app,
        "iina.fullscreen",
        "Localizable",
        if active_player_window_is_fullscreen(app) {
            "menu.exit_fullscreen"
        } else {
            "menu.fullscreen"
        },
        if active_player_window_is_fullscreen(app) {
            "Exit Full Screen"
        } else {
            "Enter Full Screen"
        },
        Some("CmdOrCtrl+F"),
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.picture-in-picture",
        if player.pip_active {
            "Exit Picture-in-Picture"
        } else {
            "Enter Picture in Picture"
        },
        Some("CmdOrCtrl+Ctrl+P"),
        true,
    )?;
    add_check_item(
        &submenu,
        app,
        "iina.float-on-top",
        "Float on Top",
        None,
        true,
        active_player_window_is_always_on_top(app),
    )?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.enter-music-mode",
        if matches!(player.mode, PlayerMode::MiniPlayer) {
            "Exit Music Mode"
        } else {
            "Enter Music Mode"
        },
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    submenu.append(&aspect_submenu(app, player)?)?;
    submenu.append(&crop_submenu(app, player)?)?;
    submenu.append(&rotation_submenu(app, player)?)?;
    submenu.append(&flip_submenu(app, player)?)?;
    add_check_item(
        &submenu,
        app,
        "iina.deinterlace",
        "Deinterlace",
        None,
        true,
        player.quick_settings.deinterlace,
    )?;
    add_check_item_key(
        &submenu,
        app,
        "iina.delogo",
        "MainMenu",
        "ArE-2s-JIV.title",
        "Delogo",
        None,
        has_current_video(player),
        has_iina_delogo_filter(player),
    )?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.video-filters",
        "Video Filters...",
        Some("CmdOrCtrl+Shift+F"),
        true,
    )?;
    add_saved_filters_submenu(
        &submenu,
        app,
        "iina.menu.saved-video-filters",
        "Saved Video Filters",
        "iina.toggle-saved-video-filter",
        FilterKind::Video,
        saved_filters,
        player,
    )?;
    Ok(submenu)
}

fn audio_menu<R: Runtime>(
    app: &AppHandle<R>,
    player: &PlayerState,
    saved_filters: &[SavedFilter],
) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu_key(
        app,
        "iina.menu.audio",
        "MainMenu",
        "lYN-0x-lzT.title",
        "Audio",
        true,
    )?;
    let audio_tracks = if player.current_url.is_some() {
        player.tracks.audio.as_slice()
    } else {
        &[]
    };
    add_item(
        &submenu,
        app,
        "iina.show-audio-panel",
        if player.sidebar.visible && matches!(player.sidebar.tab, SidebarTab::Audio) {
            "Hide Audio Panel"
        } else {
            "Show Audio Panel"
        },
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.cycle-audio-tracks",
        "Cycle Audio Tracks",
        None,
        true,
    )?;
    submenu.append(&track_submenu(
        app,
        "iina.menu.audio-track",
        "Audio Track",
        "iina.select-audio-track",
        audio_tracks,
        selected_track_id(audio_tracks),
    )?)?;
    add_item(
        &submenu,
        app,
        "iina.load-external-audio",
        "Load External Audio...",
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    let volume_title = if player.muted {
        localization::menu_number_key(
            "Localizable",
            "menu.volume_muted",
            "Volume: %d (Muted)",
            player.volume,
        )
    } else {
        localization::menu_number_key("Localizable", "menu.volume", "Volume: %d", player.volume)
    };
    add_item(
        &submenu,
        app,
        "iina.volume-label",
        &volume_title,
        None,
        false,
    )?;
    add_item(&submenu, app, "iina.volume-up-5", "Volume + 5%", None, true)?;
    add_item(&submenu, app, "iina.volume-up-1", "Volume + 1%", None, true)?;
    add_item(
        &submenu,
        app,
        "iina.volume-down-5",
        "Volume - 5%",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.volume-down-1",
        "Volume - 1%",
        None,
        true,
    )?;
    add_check_item(&submenu, app, "iina.mute", "Mute", None, true, player.muted)?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.audio-delay-label",
        &localization::menu_number_key(
            "Localizable",
            "menu.audio_delay",
            "Audio Delay: %.2fs",
            player.quick_settings.audio_delay,
        ),
        None,
        false,
    )?;
    add_item(
        &submenu,
        app,
        "iina.audio-delay-up-0_5",
        "Audio Delay + 0.5s",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.audio-delay-up-0_1",
        "Audio Delay + 0.1s",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.audio-delay-down-0_5",
        "Audio Delay - 0.5s",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.audio-delay-down-0_1",
        "Audio Delay - 0.1s",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.audio-delay-reset",
        "Reset Audio Delay",
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    submenu.append(&audio_device_submenu(app, player)?)?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.audio-filters",
        "Audio Filters...",
        Some("CmdOrCtrl+Shift+G"),
        true,
    )?;
    add_saved_filters_submenu(
        &submenu,
        app,
        "iina.menu.saved-audio-filters",
        "Saved Audio Filters",
        "iina.toggle-saved-audio-filter",
        FilterKind::Audio,
        saved_filters,
        player,
    )?;
    Ok(submenu)
}

fn subtitles_menu<R: Runtime>(
    app: &AppHandle<R>,
    player: &PlayerState,
) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(app, "iina.menu.subtitles", "Subtitles", true)?;
    let subtitle_providers = online_subtitle_providers_for_menu(app);
    let default_provider_id = app
        .try_state::<AppState>()
        .and_then(|state| {
            state.preferences.lock().ok().and_then(|preferences| {
                preferences
                    .values
                    .get("onlineSubProvider")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            })
        })
        .unwrap_or_else(|| BUILT_IN_SUBTITLE_PROVIDERS[0].0.to_string());
    let default_provider_name = subtitle_providers
        .iter()
        .find(|provider| provider.id == default_provider_id)
        .map(|provider| provider.name.as_str())
        .unwrap_or(BUILT_IN_SUBTITLE_PROVIDERS[0].1);
    let find_online_title = localization::menu_title_key(
        "Localizable",
        "menu.find_online_sub",
        "Find Online Subtitles from %@",
    )
    .replacen("%@", default_provider_name, 1);
    let subtitle_tracks = if player.current_url.is_some() {
        player.tracks.subtitles.as_slice()
    } else {
        &[]
    };
    add_item(
        &submenu,
        app,
        "iina.show-subtitles-panel",
        if player.sidebar.visible && matches!(player.sidebar.tab, SidebarTab::Subtitles) {
            "Hide Subtitles Panel"
        } else {
            "Show Subtitles Panel"
        },
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.cycle-subtitles",
        "Cycle Subtitles",
        None,
        true,
    )?;
    submenu.append(&track_submenu(
        app,
        "iina.menu.subtitle",
        "Subtitle",
        "iina.select-subtitle-track",
        subtitle_tracks,
        selected_track_id(subtitle_tracks),
    )?)?;
    submenu.append(&track_submenu(
        app,
        "iina.menu.second-subtitle",
        "Second Subtitle",
        "iina.select-second-subtitle-track",
        subtitle_tracks,
        player.second_subtitle_id,
    )?)?;
    add_item(
        &submenu,
        app,
        "iina.load-external-subtitle",
        "Load External Subtitle...",
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.find-online-subtitles",
        &find_online_title,
        None,
        true,
    )?;
    submenu.append(&online_subtitle_provider_submenu(app, &subtitle_providers)?)?;
    add_item(
        &submenu,
        app,
        "iina.save-downloaded-subtitle",
        "Save Downloaded Subtitle...",
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    submenu.append(&subtitle_encoding_submenu(
        app,
        &player.quick_settings.sub_encoding,
    )?)?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.subtitle-scale-up",
        "Scale Up",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.subtitle-scale-down",
        "Scale Down",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.subtitle-scale-reset",
        "Reset Subtitle Scale",
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.subtitle-delay-label",
        &localization::menu_number_key(
            "Localizable",
            "menu.sub_delay",
            "Subtitle Delay: %.2fs",
            player.quick_settings.sub_delay,
        ),
        None,
        false,
    )?;
    add_item(
        &submenu,
        app,
        "iina.subtitle-delay-up-0_5",
        "Subtitle Delay + 0.5s",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.subtitle-delay-up-0_1",
        "Subtitle Delay + 0.1s",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.subtitle-delay-down-0_5",
        "Subtitle Delay - 0.5s",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.subtitle-delay-down-0_1",
        "Subtitle Delay - 0.1s",
        None,
        true,
    )?;
    add_item(
        &submenu,
        app,
        "iina.subtitle-delay-reset",
        "Reset Subtitle Delay",
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item(&submenu, app, "iina.subtitle-font", "Font...", None, true)?;
    Ok(submenu)
}

fn plugin_menu<R: Runtime>(app: &AppHandle<R>, plan: &PluginMenuPlan) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(app, "iina.menu.plugin", "Plugin", true)?;
    for node in &plan.nodes {
        add_planned_plugin_menu_node(&submenu, app, node)?;
    }
    Ok(submenu)
}

fn add_planned_plugin_menu_node<R: Runtime>(
    parent: &Submenu<R>,
    app: &AppHandle<R>,
    node: &PlannedPluginMenuNode,
) -> tauri::Result<()> {
    match node {
        PlannedPluginMenuNode::Separator => add_separator(parent, app),
        PlannedPluginMenuNode::Item {
            id,
            title,
            enabled,
            submenu,
            children,
            ..
        } if *submenu || !children.is_empty() => {
            let submenu = Submenu::with_id(app, id, title, *enabled)?;
            for child in children {
                add_planned_plugin_menu_node(&submenu, app, child)?;
            }
            parent.append(&submenu)
        }
        PlannedPluginMenuNode::Item {
            id,
            title,
            enabled,
            selected: true,
            ..
        } => {
            let menu_item = CheckMenuItem::with_id(app, id, title, *enabled, true, None::<&str>)?;
            parent.append(&menu_item)
        }
        PlannedPluginMenuNode::Item {
            id, title, enabled, ..
        } => {
            let menu_item = MenuItem::with_id(app, id, title, *enabled, None::<&str>)?;
            parent.append(&menu_item)
        }
    }
}

fn active_plugin_menu_definitions(
    definitions: &[PluginMenuDefinition],
    active_owner: &str,
) -> Vec<ActivePluginMenuDefinition> {
    let is_active = |definition: &&PluginMenuDefinition| {
        (definition.role == "global" && definition.owner_label == "main")
            || (definition.role == "entry" && definition.owner_label == active_owner)
    };
    let mut active = Vec::<ActivePluginMenuDefinition>::new();
    for definition in definitions.iter().filter(is_active) {
        if active
            .iter()
            .any(|candidate| candidate.identifier == definition.identifier)
        {
            continue;
        }
        active.push(ActivePluginMenuDefinition {
            order_index: definition.order_index,
            identifier: definition.identifier.clone(),
            name: definition.name.clone(),
            has_global_instance: definition.has_global_instance,
            items: Vec::new(),
        });
    }
    // IINA concatenates globalInstance.menuItems before the owning PlayerCore instance menu.
    for role in ["global", "entry"] {
        for definition in definitions
            .iter()
            .filter(is_active)
            .filter(|definition| definition.role == role)
        {
            let Some(plugin) = active
                .iter_mut()
                .find(|candidate| candidate.identifier == definition.identifier)
            else {
                continue;
            };
            plugin.has_global_instance |= definition.has_global_instance;
            plugin.order_index = plugin.order_index.min(definition.order_index);
            plugin
                .items
                .extend(
                    definition
                        .items
                        .iter()
                        .cloned()
                        .map(|item| RoutedPluginMenuItem {
                            owner_label: definition.owner_label.clone(),
                            role: definition.role.clone(),
                            item,
                        }),
                );
        }
    }
    active.sort_by(|left, right| {
        left.order_index
            .cmp(&right.order_index)
            .then_with(|| left.identifier.cmp(&right.identifier))
    });
    active
}

fn plugin_menu_plan(
    plugins: &[ActivePluginMenuDefinition],
    active_key_bindings: &[ActiveKeyBinding],
    developer_tool_available: bool,
) -> PluginMenuPlan {
    let active_keys = active_key_bindings
        .iter()
        .map(|binding| binding.normalized_mpv_key.clone())
        .collect::<HashSet<_>>();
    let mut nodes = vec![
        planned_plugin_item(
            "iina.manage-plugins",
            "Manage Plugins…",
            true,
            false,
            None,
            Vec::new(),
        ),
        PlannedPluginMenuNode::Separator,
    ];
    let mut shortcut_conflicts = Vec::new();
    let mut developer_tool_items = Vec::new();
    for (plugin_index, plugin) in plugins.iter().enumerate() {
        if plugin.items.is_empty() {
            continue;
        }
        // Match MenuController's enumerate index, including its observable extra separator when
        // earlier enabled plugins have no menu items.
        if plugin_index != 0 {
            nodes.push(PlannedPluginMenuNode::Separator);
        }
        nodes.push(planned_plugin_item(
            format!("iina.plugin-title.{}", plugin.identifier),
            plugin.name.clone(),
            false,
            false,
            None,
            Vec::new(),
        ));

        let mut first_level = plugin
            .items
            .iter()
            .map(|item| plan_plugin_menu_item(plugin, item, &active_keys, &mut shortcut_conflicts))
            .collect::<Vec<_>>();
        if first_level.len() > PLUGIN_MENU_FIRST_LEVEL_LIMIT {
            let overflow = first_level.split_off(PLUGIN_MENU_FIRST_LEVEL_LIMIT);
            nodes.extend(first_level);
            nodes.push(planned_plugin_item(
                format!("iina.plugin-more.{}", plugin.identifier),
                "More…",
                true,
                false,
                None,
                overflow,
            ));
        } else {
            nodes.extend(first_level);
        }

        if developer_tool_available {
            developer_tool_items.push(planned_plugin_item(
                plugin_developer_tool_item_id(&plugin.identifier, "entry"),
                plugin.name.clone(),
                true,
                false,
                None,
                Vec::new(),
            ));
            if plugin.has_global_instance {
                developer_tool_items.push(planned_plugin_item(
                    plugin_developer_tool_item_id(&plugin.identifier, "global"),
                    format!("{} Global", plugin.name),
                    true,
                    false,
                    None,
                    Vec::new(),
                ));
            }
        }
    }

    if !shortcut_conflicts.is_empty() {
        nodes.insert(
            0,
            planned_plugin_item(
                "iina.plugin-shortcut-conflicts",
                "⚠︎ Conflicting key shortcuts…",
                false,
                false,
                None,
                Vec::new(),
            ),
        );
    }
    nodes.push(PlannedPluginMenuNode::Separator);
    if developer_tool_available {
        nodes.push(planned_plugin_submenu(
            "iina.plugin-developer-tool",
            "Developer Tool",
            true,
            false,
            None,
            developer_tool_items,
        ));
    }
    nodes.push(planned_plugin_item(
        "iina.reload-all-plugins",
        "Reload all plugins",
        true,
        false,
        None,
        Vec::new(),
    ));
    PluginMenuPlan {
        nodes,
        shortcut_conflicts,
    }
}

fn plan_plugin_menu_item(
    plugin: &ActivePluginMenuDefinition,
    routed_item: &RoutedPluginMenuItem,
    active_keys: &HashSet<String>,
    shortcut_conflicts: &mut Vec<(String, String)>,
) -> PlannedPluginMenuNode {
    let item = &routed_item.item;
    if item.separator {
        return PlannedPluginMenuNode::Separator;
    }
    let shortcut = item.key_binding.as_ref().and_then(|key| {
        let normalized = normalize_mpv_key(key);
        if active_keys.contains(&normalized) {
            shortcut_conflicts.push((plugin.name.clone(), key.clone()));
            return None;
        }
        native_key_equivalent(&normalized).map(|(key_equivalent, modifier_mask)| {
            PlannedPluginShortcut {
                key_equivalent,
                modifier_mask,
            }
        })
    });
    let children = item
        .items
        .iter()
        .cloned()
        .map(|child| RoutedPluginMenuItem {
            owner_label: routed_item.owner_label.clone(),
            role: routed_item.role.clone(),
            item: child,
        })
        .map(|child| plan_plugin_menu_item(plugin, &child, active_keys, shortcut_conflicts))
        .collect();
    planned_plugin_item(
        plugin_menu_item_id(
            &routed_item.owner_label,
            &routed_item.role,
            &plugin.identifier,
            &item.id,
        ),
        item.title.clone(),
        item.enabled,
        item.selected,
        shortcut,
        children,
    )
}

fn planned_plugin_item(
    id: impl Into<String>,
    title: impl Into<String>,
    enabled: bool,
    selected: bool,
    shortcut: Option<PlannedPluginShortcut>,
    children: Vec<PlannedPluginMenuNode>,
) -> PlannedPluginMenuNode {
    PlannedPluginMenuNode::Item {
        id: id.into(),
        title: title.into(),
        enabled,
        selected,
        submenu: false,
        shortcut,
        children,
    }
}

fn planned_plugin_submenu(
    id: impl Into<String>,
    title: impl Into<String>,
    enabled: bool,
    selected: bool,
    shortcut: Option<PlannedPluginShortcut>,
    children: Vec<PlannedPluginMenuNode>,
) -> PlannedPluginMenuNode {
    PlannedPluginMenuNode::Item {
        id: id.into(),
        title: title.into(),
        enabled,
        selected,
        submenu: true,
        shortcut,
        children,
    }
}

fn plugin_developer_tool_item_id(identifier: &str, role: &str) -> String {
    format!("{PLUGIN_DEVELOPER_TOOL_ID_PREFIX}{identifier}::{role}")
}

fn plugin_menu_item_id(owner_label: &str, role: &str, identifier: &str, item_id: &str) -> String {
    format!("{PLUGIN_MENU_ID_PREFIX}{owner_label}::{role}::{identifier}::{item_id}")
}

fn plugin_menu_request_from_id(id: &str) -> Option<PluginMenuRequest> {
    let id = id.strip_prefix(PLUGIN_MENU_ID_PREFIX)?;
    let mut fields = id.splitn(4, "::");
    let owner_label = fields.next()?;
    let role = fields.next()?;
    let identifier = fields.next()?;
    let item_id = fields.next()?;
    if owner_label.is_empty()
        || !matches!(role, "entry" | "global")
        || identifier.is_empty()
        || item_id.is_empty()
    {
        return None;
    }
    Some(PluginMenuRequest {
        owner_label: owner_label.to_string(),
        role: role.to_string(),
        identifier: identifier.to_string(),
        item_id: item_id.to_string(),
    })
}

fn plugin_developer_tool_request_from_id(id: &str) -> Option<PluginDeveloperToolRequest> {
    let id = id.strip_prefix(PLUGIN_DEVELOPER_TOOL_ID_PREFIX)?;
    let (identifier, role) = id.split_once("::")?;
    if identifier.is_empty() || !matches!(role, "entry" | "global") {
        return None;
    }
    Some(PluginDeveloperToolRequest {
        identifier: identifier.to_string(),
        role: role.to_string(),
    })
}

fn window_menu<R: Runtime>(app: &AppHandle<R>, player: &PlayerState) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(app, "iina.menu.window", "Window", true)?;
    submenu.append(&PredefinedMenuItem::minimize(
        app,
        Some(&localization::menu_title("Minimize")),
    )?)?;
    submenu.append(&PredefinedMenuItem::maximize(
        app,
        Some(&localization::menu_title("Zoom")),
    )?)?;
    add_item(
        &submenu,
        app,
        CUSTOM_TOUCH_BAR_MENU_ID,
        "Custom Touch Bar...",
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.inspector",
        "Inspector",
        Some("CmdOrCtrl+I"),
        player.current_url.is_some(),
    )?;
    add_item(
        &submenu,
        app,
        "iina.log-viewer",
        "Log Viewer",
        Some("CmdOrCtrl+Shift+L"),
        true,
    )?;
    add_separator(&submenu, app)?;
    submenu.append(&PredefinedMenuItem::bring_all_to_front(
        app,
        Some(&localization::menu_title("Bring All to Front")),
    )?)?;
    Ok(submenu)
}

fn help_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(app, "iina.menu.help", "Help", true)?;
    add_item(
        &submenu,
        app,
        "iina.help",
        "IINA Help",
        Some("CmdOrCtrl+?"),
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item(
        &submenu,
        app,
        "iina.release-highlights",
        "Release Highlights",
        None,
        true,
    )?;
    add_separator(&submenu, app)?;
    add_item(&submenu, app, "iina.github", "GitHub", None, true)?;
    add_item(&submenu, app, "iina.website", "Website", None, true)?;
    Ok(submenu)
}

fn playlist_submenu<R: Runtime>(
    app: &AppHandle<R>,
    player: &PlayerState,
) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu_key(
        app,
        "iina.menu.playlist",
        "MainMenu",
        "Gi5-1S-RQB.title",
        "Playlist",
        true,
    )?;
    for (index, item) in player.playlist.iter().enumerate() {
        add_literal_check_item(
            &submenu,
            app,
            &format!("iina.select-playlist-item.{index}"),
            &item.title,
            None,
            true,
            item.current,
        )?;
    }
    Ok(submenu)
}

fn chapters_submenu<R: Runtime>(
    app: &AppHandle<R>,
    player: &PlayerState,
) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(app, "iina.menu.chapters", "Chapters", true)?;
    let standard_time = player
        .chapters
        .last()
        .map(|chapter| format_chapter_time(chapter.time_seconds))
        .unwrap_or_default();
    for (index, chapter) in player.chapters.iter().enumerate() {
        let time = pad_chapter_time(&format_chapter_time(chapter.time_seconds), &standard_time);
        let next_time = player
            .chapters
            .get(index + 1)
            .map(|next| next.time_seconds)
            .unwrap_or(f64::INFINITY);
        let is_playing =
            player.position_seconds >= chapter.time_seconds && player.position_seconds < next_time;
        add_literal_check_item(
            &submenu,
            app,
            &format!("iina.select-chapter.{index}"),
            &format!("{time} \u{2013} {}", chapter.title),
            None,
            true,
            is_playing,
        )?;
    }
    Ok(submenu)
}

fn track_submenu<R: Runtime>(
    app: &AppHandle<R>,
    menu_id: &str,
    title: &str,
    item_prefix: &str,
    tracks: &[Track],
    selected_id: i64,
) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(app, menu_id, title, true)?;
    add_check_item(
        &submenu,
        app,
        &format!("{item_prefix}.0"),
        "<None>",
        None,
        true,
        selected_id == 0,
    )?;
    for track in tracks.iter().filter(|track| track.id != 0) {
        add_literal_check_item(
            &submenu,
            app,
            &format!("{item_prefix}.{}", track.id),
            &track.title,
            None,
            true,
            track.id == selected_id,
        )?;
    }
    Ok(submenu)
}

fn aspect_submenu<R: Runtime>(
    app: &AppHandle<R>,
    player: &PlayerState,
) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(app, "iina.menu.aspect-ratio", "Aspect Ratio", true)?;
    add_check_item(
        &submenu,
        app,
        "iina.set-aspect.0",
        "Default",
        None,
        true,
        player.quick_settings.video_aspect == "Default",
    )?;
    for (index, aspect) in IINA_ASPECTS.iter().enumerate() {
        add_check_item(
            &submenu,
            app,
            &format!("iina.set-aspect.{}", index + 1),
            aspect,
            None,
            true,
            player.quick_settings.video_aspect == *aspect,
        )?;
    }
    Ok(submenu)
}

fn crop_submenu<R: Runtime>(app: &AppHandle<R>, player: &PlayerState) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(app, "iina.menu.crop", "Crop", true)?;
    add_check_item(
        &submenu,
        app,
        "iina.set-crop.0",
        "None",
        None,
        true,
        player.quick_settings.video_crop == "None" && player.quick_settings.custom_crop.is_none(),
    )?;
    for (index, crop) in IINA_ASPECTS.iter().enumerate() {
        add_check_item(
            &submenu,
            app,
            &format!("iina.set-crop.{}", index + 1),
            crop,
            None,
            true,
            player.quick_settings.video_crop == *crop,
        )?;
    }
    add_separator(&submenu, app)?;
    add_check_item(
        &submenu,
        app,
        "iina.custom-crop",
        "Custom...",
        None,
        true,
        player.quick_settings.custom_crop.is_some(),
    )?;
    Ok(submenu)
}

fn rotation_submenu<R: Runtime>(
    app: &AppHandle<R>,
    player: &PlayerState,
) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(app, "iina.menu.rotation", "Rotation", true)?;
    for degrees in IINA_ROTATIONS {
        add_check_item(
            &submenu,
            app,
            &format!("iina.set-rotation.{degrees}"),
            &format!("{degrees}\u{00b0}"),
            None,
            true,
            player.quick_settings.video_rotate == degrees,
        )?;
    }
    Ok(submenu)
}

fn flip_submenu<R: Runtime>(app: &AppHandle<R>, player: &PlayerState) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu_key(
        app,
        "iina.menu.flip",
        "MainMenu",
        "mT1-1m-HwN.title",
        "Flip",
        true,
    )?;
    add_check_item(
        &submenu,
        app,
        "iina.toggle-mirror",
        "Horizontal (Mirror)",
        None,
        true,
        player.quick_settings.video_mirrored,
    )?;
    add_check_item(
        &submenu,
        app,
        "iina.toggle-flip",
        "Vertical (Flip)",
        None,
        true,
        player.quick_settings.video_flipped,
    )?;
    Ok(submenu)
}

fn audio_device_submenu<R: Runtime>(
    app: &AppHandle<R>,
    player: &PlayerState,
) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(app, "iina.menu.audio-device", "Audio Device", true)?;
    for (index, device) in player.audio_devices.iter().enumerate() {
        add_literal_check_item(
            &submenu,
            app,
            &format!("iina.select-audio-device.{index}"),
            &format!("[{}] {}", device.description, device.name),
            None,
            true,
            device.name == player.audio_device,
        )?;
    }
    Ok(submenu)
}

fn selected_track_id(tracks: &[Track]) -> i64 {
    tracks
        .iter()
        .find(|track| track.selected)
        .map(|track| track.id)
        .unwrap_or(0)
}

fn has_iina_delogo_filter(player: &PlayerState) -> bool {
    player.video_filters.iter().any(|filter| {
        filter.label.as_deref() == Some("iina_delogo") || filter.name == "iina_delogo"
    })
}

fn has_current_video(player: &PlayerState) -> bool {
    player.current_url.is_some()
        && player
            .tracks
            .video
            .iter()
            .any(|track| track.id != 0 && !track.metadata.albumart)
}

fn format_chapter_time(seconds: f64) -> String {
    let total_seconds = if seconds.is_finite() {
        seconds.max(0.0) as u64
    } else {
        0
    };
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn pad_chapter_time(time: &str, standard_time: &str) -> String {
    let missing = standard_time
        .chars()
        .count()
        .saturating_sub(time.chars().count());
    let prefix = standard_time
        .chars()
        .take(missing)
        .map(|character| if character == ':' { ':' } else { '0' })
        .collect::<String>();
    format!("{prefix}{time}")
}

fn add_item<R: Runtime>(
    submenu: &Submenu<R>,
    app: &AppHandle<R>,
    id: &str,
    text: &str,
    accelerator: Option<&str>,
    enabled: bool,
) -> tauri::Result<()> {
    let item = MenuItem::with_id(
        app,
        id,
        localization::menu_title(text),
        enabled,
        accelerator,
    )?;
    submenu.append(&item)
}

fn add_item_key<R: Runtime>(
    submenu: &Submenu<R>,
    app: &AppHandle<R>,
    id: &str,
    table: &str,
    key: &str,
    text: &str,
    accelerator: Option<&str>,
    enabled: bool,
) -> tauri::Result<()> {
    let item = MenuItem::with_id(
        app,
        id,
        localization::menu_title_key(table, key, text),
        enabled,
        accelerator,
    )?;
    submenu.append(&item)
}

fn add_literal_item<R: Runtime>(
    submenu: &Submenu<R>,
    app: &AppHandle<R>,
    id: &str,
    text: &str,
    accelerator: Option<&str>,
    enabled: bool,
) -> tauri::Result<()> {
    let item = MenuItem::with_id(app, id, text, enabled, accelerator)?;
    submenu.append(&item)
}

fn add_check_item<R: Runtime>(
    submenu: &Submenu<R>,
    app: &AppHandle<R>,
    id: &str,
    text: &str,
    accelerator: Option<&str>,
    enabled: bool,
    checked: bool,
) -> tauri::Result<()> {
    let item = CheckMenuItem::with_id(
        app,
        id,
        localization::menu_title(text),
        enabled,
        checked,
        accelerator,
    )?;
    submenu.append(&item)
}

#[allow(clippy::too_many_arguments)]
fn add_check_item_key<R: Runtime>(
    submenu: &Submenu<R>,
    app: &AppHandle<R>,
    id: &str,
    table: &str,
    key: &str,
    text: &str,
    accelerator: Option<&str>,
    enabled: bool,
    checked: bool,
) -> tauri::Result<()> {
    let item = CheckMenuItem::with_id(
        app,
        id,
        localization::menu_title_key(table, key, text),
        enabled,
        checked,
        accelerator,
    )?;
    submenu.append(&item)
}

fn add_literal_check_item<R: Runtime>(
    submenu: &Submenu<R>,
    app: &AppHandle<R>,
    id: &str,
    text: &str,
    accelerator: Option<&str>,
    enabled: bool,
    checked: bool,
) -> tauri::Result<()> {
    let item = CheckMenuItem::with_id(app, id, text, enabled, checked, accelerator)?;
    submenu.append(&item)
}

fn localized_submenu<R: Runtime>(
    app: &AppHandle<R>,
    id: impl Into<tauri::menu::MenuId>,
    title: &str,
    enabled: bool,
) -> tauri::Result<Submenu<R>> {
    Submenu::with_id(app, id, localization::menu_title(title), enabled)
}

fn localized_submenu_key<R: Runtime>(
    app: &AppHandle<R>,
    id: impl Into<tauri::menu::MenuId>,
    table: &str,
    key: &str,
    title: &str,
    enabled: bool,
) -> tauri::Result<Submenu<R>> {
    Submenu::with_id(
        app,
        id,
        localization::menu_title_key(table, key, title),
        enabled,
    )
}

fn add_separator<R: Runtime>(submenu: &Submenu<R>, app: &AppHandle<R>) -> tauri::Result<()> {
    submenu.append(&PredefinedMenuItem::separator(app)?)
}

fn add_saved_filters_submenu<R: Runtime>(
    parent: &Submenu<R>,
    app: &AppHandle<R>,
    id: &str,
    title: &str,
    item_prefix: &str,
    kind: FilterKind,
    filters: &[SavedFilter],
    player: &PlayerState,
) -> tauri::Result<()> {
    let submenu = localized_submenu(app, id, title, true)?;
    if filters.is_empty() {
        add_item(
            &submenu,
            app,
            &format!("{id}.empty"),
            "No saved filters",
            None,
            false,
        )?;
    } else {
        for (index, filter) in filters.iter().enumerate() {
            add_literal_check_item(
                &submenu,
                app,
                &format!("{item_prefix}.{index}"),
                &filter.name,
                None,
                true,
                player.has_filter(kind, &filter.filter_string),
            )?;
        }
    }
    parent.append(&submenu)
}

fn add_submenu_placeholder<R: Runtime>(
    parent: &Submenu<R>,
    app: &AppHandle<R>,
    id: &str,
    title: &str,
    entries: &[&str],
) -> tauri::Result<()> {
    let submenu = localized_submenu(app, id, title, true)?;
    for (index, entry) in entries.iter().enumerate() {
        add_item(&submenu, app, &format!("{id}.{index}"), entry, None, false)?;
    }
    parent.append(&submenu)
}

fn subtitle_encoding_submenu<R: Runtime>(
    app: &AppHandle<R>,
    current_encoding: &str,
) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(app, "iina.menu.subtitle-encoding", "Encoding", true)?;
    for (index, (reference_title, code)) in IINA_SUBTITLE_ENCODINGS.iter().enumerate() {
        let title = if index == 0 {
            localization::menu_title_key("Localizable", "subencoding.auto", reference_title)
        } else {
            (*reference_title).to_string()
        };
        add_literal_check_item(
            &submenu,
            app,
            &format!("iina.set-subtitle-encoding.{index}"),
            &title,
            None,
            true,
            current_encoding == *code,
        )?;
        if index == 0 {
            add_separator(&submenu, app)?;
        }
    }
    Ok(submenu)
}

fn online_subtitle_providers_for_menu<R: Runtime>(
    app: &AppHandle<R>,
) -> Vec<SubtitleProviderMenuItem> {
    let mut providers = BUILT_IN_SUBTITLE_PROVIDERS
        .iter()
        .map(|(id, name)| SubtitleProviderMenuItem {
            id: (*id).to_string(),
            name: (*name).to_string(),
            menu_title: (*name).to_string(),
        })
        .collect::<Vec<_>>();
    let plugins_enabled = app
        .try_state::<AppState>()
        .and_then(|state| {
            state.preferences.lock().ok().and_then(|preferences| {
                preferences
                    .values
                    .get("iinaEnablePluginSystem")
                    .and_then(serde_json::Value::as_bool)
            })
        })
        .unwrap_or(false);
    if !plugins_enabled {
        return providers;
    }
    let records = match crate::plugins::list(app) {
        Ok(records) => records,
        Err(error) => {
            eprintln!("failed to enumerate plugin subtitle providers: {error}");
            return providers;
        }
    };
    for plugin in records.into_iter().filter(|plugin| plugin.enabled) {
        for provider in plugin.subtitle_providers {
            providers.push(SubtitleProviderMenuItem {
                id: format!("plugin:{}:{}", plugin.identifier, provider.id),
                name: provider.name.clone(),
                menu_title: format!("{} — {}", provider.name, plugin.name),
            });
        }
    }
    providers
}

fn online_subtitle_provider_submenu<R: Runtime>(
    app: &AppHandle<R>,
    providers: &[SubtitleProviderMenuItem],
) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(
        app,
        "iina.menu.find-online-subtitles-from",
        "Find Online Subtitles from",
        true,
    )?;
    for (index, provider) in providers.iter().enumerate() {
        if index == BUILT_IN_SUBTITLE_PROVIDERS.len() {
            add_separator(&submenu, app)?;
        }
        add_literal_item(
            &submenu,
            app,
            &format!("{SUBTITLE_PROVIDER_MENU_ID_PREFIX}{}", provider.id),
            &provider.menu_title,
            None,
            true,
        )?;
    }
    Ok(submenu)
}

fn subtitle_provider_id_from_menu_item(id: &str) -> Option<&str> {
    id.strip_prefix(SUBTITLE_PROVIDER_MENU_ID_PREFIX)
        .filter(|provider| !provider.is_empty() && !provider.contains('\0'))
}

fn apply_player_menu_command<R: Runtime>(app: &AppHandle<R>, id: &str, target: &str) {
    let resolved_bindings = resolved_menu_bindings_for_app(app);
    let _ = apply_player_menu_command_with_binding(app, id, target, resolved_bindings.get(id));
}

fn apply_player_menu_command_with_binding<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    target: &str,
    binding: Option<&ResolvedMenuBinding>,
) -> bool {
    let state = app.state::<AppState>();
    let Ok(session) = state.player_session_for_window(target) else {
        return false;
    };
    let saved_filter_command = saved_filter_command_for_menu_item(app, id);
    let command = {
        let Ok(player) = session.player().lock() else {
            return false;
        };
        saved_filter_command.or_else(|| command_for_menu_item_with_binding(id, &player, binding))
    };
    let Some(command) = command else {
        return false;
    };
    if matches!(&command, PlayerCommand::Stop)
        && state.save_playback_position_for_window(target).is_err()
    {
        return false;
    }
    let snapshot = {
        let Ok(mut player) = session.player().lock() else {
            return false;
        };
        player.apply(command);
        player.clone()
    };
    let _ = session.sync_mpv_executor_from_player();
    let _ = app.emit_to(target, MENU_EVENT_PLAYER_STATE, &snapshot);
    let _ = refresh_iina_menu(app);
    true
}

fn saved_filter_command_for_menu_item<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
) -> Option<PlayerCommand> {
    let state = app.state::<AppState>();
    let preferences = state.preferences.lock().ok()?;
    saved_filter_command_from_lists(
        id,
        &preferences.saved_filters(SAVED_VIDEO_FILTERS_KEY),
        &preferences.saved_filters(SAVED_AUDIO_FILTERS_KEY),
    )
}

fn saved_filter_command_from_lists(
    id: &str,
    video_filters: &[SavedFilter],
    audio_filters: &[SavedFilter],
) -> Option<PlayerCommand> {
    let (kind, filter) =
        if let Some(index) = parse_indexed_menu_item(id, "iina.toggle-saved-video-filter.") {
            (FilterKind::Video, video_filters.get(index)?)
        } else if let Some(index) = parse_indexed_menu_item(id, "iina.toggle-saved-audio-filter.") {
            (FilterKind::Audio, audio_filters.get(index)?)
        } else {
            return None;
        };
    Some(PlayerCommand::ToggleSavedFilter {
        kind,
        name: filter.name.clone(),
        filter: filter.filter_string.clone(),
    })
}

#[cfg(test)]
fn command_for_menu_item(id: &str, player: &PlayerState) -> Option<PlayerCommand> {
    command_for_menu_item_with_binding(id, player, None)
}

fn command_for_menu_item_with_binding(
    id: &str,
    player: &PlayerState,
    binding: Option<&ResolvedMenuBinding>,
) -> Option<PlayerCommand> {
    if let Some(index) = parse_indexed_menu_item(id, "iina.set-subtitle-encoding.") {
        let (_, encoding) = IINA_SUBTITLE_ENCODINGS.get(index)?;
        return Some(PlayerCommand::SetSubEncoding {
            encoding: (*encoding).to_string(),
        });
    }
    if let Some(index) = parse_indexed_menu_item(id, "iina.select-playlist-item.") {
        return Some(PlayerCommand::SelectPlaylistItem { index });
    }
    if let Some(index) = parse_indexed_menu_item(id, "iina.select-chapter.") {
        return Some(PlayerCommand::SelectChapter { index });
    }
    if let Some(id) = parse_track_menu_item(id, "iina.select-video-track.") {
        return Some(PlayerCommand::SelectTrack {
            kind: TrackSelectionKind::Video,
            id,
        });
    }
    if let Some(id) = parse_track_menu_item(id, "iina.select-audio-track.") {
        return Some(PlayerCommand::SelectTrack {
            kind: TrackSelectionKind::Audio,
            id,
        });
    }
    if let Some(id) = parse_track_menu_item(id, "iina.select-subtitle-track.") {
        return Some(PlayerCommand::SelectTrack {
            kind: TrackSelectionKind::Subtitles,
            id,
        });
    }
    if let Some(id) = parse_track_menu_item(id, "iina.select-second-subtitle-track.") {
        return Some(PlayerCommand::SelectTrack {
            kind: TrackSelectionKind::SecondSubtitles,
            id,
        });
    }
    if let Some(index) = parse_indexed_menu_item(id, "iina.set-aspect.") {
        let aspect = if index == 0 {
            "Default"
        } else {
            *IINA_ASPECTS.get(index - 1)?
        };
        return Some(PlayerCommand::SetVideoAspect {
            aspect: aspect.to_string(),
        });
    }
    if let Some(index) = parse_indexed_menu_item(id, "iina.set-crop.") {
        let crop = if index == 0 {
            "None"
        } else {
            *IINA_ASPECTS.get(index - 1)?
        };
        return Some(PlayerCommand::SetVideoCrop {
            crop: crop.to_string(),
        });
    }
    if let Some(degrees) = parse_track_menu_item(id, "iina.set-rotation.") {
        if IINA_ROTATIONS.contains(&degrees) {
            return Some(PlayerCommand::SetVideoRotate { degrees });
        }
    }
    if let Some(index) = parse_indexed_menu_item(id, "iina.select-audio-device.") {
        let device = player.audio_devices.get(index)?;
        return Some(PlayerCommand::SelectAudioDevice {
            name: device.name.clone(),
        });
    }

    match id {
        "iina.toggle-pause" => Some(PlayerCommand::TogglePause),
        "iina.stop" => Some(PlayerCommand::Stop),
        "iina.seek-forward-5" | "iina.seek-backward-5" if binding.is_some() => {
            Some(PlayerCommand::SeekRelative {
                seconds: binding?.numeric_value?,
                option: if binding?.exact_seek {
                    RelativeSeekOption::Exact
                } else {
                    RelativeSeekOption::Auto
                },
            })
        }
        "iina.seek-forward-5" => Some(PlayerCommand::Seek {
            seconds: player.position_seconds + 5.0,
        }),
        "iina.seek-backward-5" => Some(PlayerCommand::Seek {
            seconds: player.position_seconds - 5.0,
        }),
        "iina.frame-step-forward" => Some(PlayerCommand::FrameStep { backwards: false }),
        "iina.frame-step-backward" => Some(PlayerCommand::FrameStep { backwards: true }),
        "iina.jump-beginning" => Some(PlayerCommand::Seek { seconds: 0.0 }),
        "iina.speed-2x" => Some(PlayerCommand::MultiplySpeed {
            factor: binding
                .and_then(|binding| binding.numeric_value)
                .unwrap_or(2.0),
        }),
        "iina.speed-1_1x" => Some(PlayerCommand::MultiplySpeed {
            factor: binding
                .and_then(|binding| binding.numeric_value)
                .unwrap_or(1.1),
        }),
        "iina.speed-0_5x" => Some(PlayerCommand::MultiplySpeed {
            factor: binding
                .and_then(|binding| binding.numeric_value)
                .unwrap_or(0.5),
        }),
        "iina.speed-0_9x" => Some(PlayerCommand::MultiplySpeed {
            factor: binding
                .and_then(|binding| binding.numeric_value)
                .unwrap_or(0.9),
        }),
        "iina.speed-reset" => Some(PlayerCommand::SetSpeed { speed: 1.0 }),
        "iina.ab-loop" => Some(PlayerCommand::CycleAbLoop),
        "iina.file-loop" => Some(PlayerCommand::ToggleFileLoop),
        "iina.playlist-loop" => Some(PlayerCommand::TogglePlaylistLoop),
        "iina.next-media" => Some(PlayerCommand::PlaylistNext),
        "iina.previous-media" => Some(PlayerCommand::PlaylistPrev),
        "iina.next-chapter" => {
            let next = active_chapter_index(player).saturating_add(1);
            (next < player.chapters.len()).then_some(PlayerCommand::SelectChapter { index: next })
        }
        "iina.previous-chapter" => {
            (!player.chapters.is_empty()).then_some(PlayerCommand::SelectChapter {
                index: active_chapter_index(player).saturating_sub(1),
            })
        }
        "iina.cycle-video-tracks" => Some(PlayerCommand::CycleTrack {
            kind: TrackSelectionKind::Video,
        }),
        "iina.cycle-audio-tracks" => Some(PlayerCommand::CycleTrack {
            kind: TrackSelectionKind::Audio,
        }),
        "iina.cycle-subtitles" => Some(PlayerCommand::CycleTrack {
            kind: TrackSelectionKind::Subtitles,
        }),
        "iina.show-playlist" => Some(toggle_sidebar_command(player, SidebarTab::Playlist)),
        "iina.show-chapters" => Some(toggle_sidebar_command(player, SidebarTab::Chapters)),
        "iina.show-video-panel" => Some(toggle_sidebar_command(player, SidebarTab::Video)),
        "iina.show-audio-panel" => Some(toggle_sidebar_command(player, SidebarTab::Audio)),
        "iina.show-subtitles-panel" => Some(toggle_sidebar_command(player, SidebarTab::Subtitles)),
        "iina.volume-up-5" => Some(PlayerCommand::SetVolume {
            volume: player.volume
                + binding
                    .and_then(|binding| binding.numeric_value)
                    .unwrap_or(5.0),
        }),
        "iina.volume-up-1" => Some(PlayerCommand::SetVolume {
            volume: player.volume
                + binding
                    .and_then(|binding| binding.numeric_value)
                    .unwrap_or(1.0),
        }),
        "iina.volume-down-5" => Some(PlayerCommand::SetVolume {
            volume: player.volume
                + binding
                    .and_then(|binding| binding.numeric_value)
                    .unwrap_or(-5.0),
        }),
        "iina.volume-down-1" => Some(PlayerCommand::SetVolume {
            volume: player.volume
                + binding
                    .and_then(|binding| binding.numeric_value)
                    .unwrap_or(-1.0),
        }),
        "iina.audio-delay-up-0_5" => Some(PlayerCommand::SetAudioDelay {
            seconds: player.quick_settings.audio_delay
                + binding
                    .and_then(|binding| binding.numeric_value)
                    .unwrap_or(0.5),
        }),
        "iina.audio-delay-up-0_1" => Some(PlayerCommand::SetAudioDelay {
            seconds: player.quick_settings.audio_delay
                + binding
                    .and_then(|binding| binding.numeric_value)
                    .unwrap_or(0.1),
        }),
        "iina.audio-delay-down-0_5" => Some(PlayerCommand::SetAudioDelay {
            seconds: player.quick_settings.audio_delay
                + binding
                    .and_then(|binding| binding.numeric_value)
                    .unwrap_or(-0.5),
        }),
        "iina.audio-delay-down-0_1" => Some(PlayerCommand::SetAudioDelay {
            seconds: player.quick_settings.audio_delay
                + binding
                    .and_then(|binding| binding.numeric_value)
                    .unwrap_or(-0.1),
        }),
        "iina.audio-delay-reset" => Some(PlayerCommand::SetAudioDelay { seconds: 0.0 }),
        "iina.subtitle-delay-up-0_5" => Some(PlayerCommand::SetSubDelay {
            seconds: player.quick_settings.sub_delay
                + binding
                    .and_then(|binding| binding.numeric_value)
                    .unwrap_or(0.5),
        }),
        "iina.subtitle-delay-up-0_1" => Some(PlayerCommand::SetSubDelay {
            seconds: player.quick_settings.sub_delay
                + binding
                    .and_then(|binding| binding.numeric_value)
                    .unwrap_or(0.1),
        }),
        "iina.subtitle-delay-down-0_5" => Some(PlayerCommand::SetSubDelay {
            seconds: player.quick_settings.sub_delay
                + binding
                    .and_then(|binding| binding.numeric_value)
                    .unwrap_or(-0.5),
        }),
        "iina.subtitle-delay-down-0_1" => Some(PlayerCommand::SetSubDelay {
            seconds: player.quick_settings.sub_delay
                + binding
                    .and_then(|binding| binding.numeric_value)
                    .unwrap_or(-0.1),
        }),
        "iina.subtitle-delay-reset" => Some(PlayerCommand::SetSubDelay { seconds: 0.0 }),
        "iina.subtitle-scale-up" => Some(PlayerCommand::SetSubScale {
            scale: player.quick_settings.sub_scale
                * binding
                    .and_then(|binding| binding.numeric_value)
                    .unwrap_or(1.1),
        }),
        "iina.subtitle-scale-down" => Some(PlayerCommand::SetSubScale {
            scale: player.quick_settings.sub_scale
                * binding
                    .and_then(|binding| binding.numeric_value)
                    .unwrap_or(0.9),
        }),
        "iina.subtitle-scale-reset" => Some(PlayerCommand::SetSubScale { scale: 1.0 }),
        "iina.mute" => Some(PlayerCommand::ToggleMute),
        "iina.deinterlace" => Some(PlayerCommand::SetDeinterlace {
            enabled: !player.quick_settings.deinterlace,
        }),
        "iina.toggle-mirror" => Some(PlayerCommand::SetVideoMirror {
            enabled: !player.quick_settings.video_mirrored,
        }),
        "iina.toggle-flip" => Some(PlayerCommand::SetVideoFlip {
            enabled: !player.quick_settings.video_flipped,
        }),
        _ => None,
    }
}

fn parse_indexed_menu_item(id: &str, prefix: &str) -> Option<usize> {
    id.strip_prefix(prefix)?.parse().ok()
}

fn parse_track_menu_item(id: &str, prefix: &str) -> Option<i64> {
    id.strip_prefix(prefix)?.parse().ok()
}

fn active_chapter_index(player: &PlayerState) -> usize {
    player
        .chapters
        .iter()
        .enumerate()
        .rev()
        .find(|(_, chapter)| chapter.time_seconds <= player.position_seconds)
        .map(|(index, _)| index)
        .unwrap_or_default()
}

fn toggle_sidebar_command(player: &PlayerState, tab: SidebarTab) -> PlayerCommand {
    let is_active = player.sidebar.visible
        && match &tab {
            SidebarTab::Playlist => matches!(player.sidebar.tab, SidebarTab::Playlist),
            SidebarTab::Chapters => matches!(player.sidebar.tab, SidebarTab::Chapters),
            SidebarTab::Video => matches!(player.sidebar.tab, SidebarTab::Video),
            SidebarTab::Audio => matches!(player.sidebar.tab, SidebarTab::Audio),
            SidebarTab::Subtitles => matches!(player.sidebar.tab, SidebarTab::Subtitles),
        };
    if is_active {
        PlayerCommand::HideSidebar
    } else {
        PlayerCommand::ShowSidebar { tab }
    }
}

fn emit_request<R: Runtime>(app: &AppHandle<R>, target: &str, action: &str) {
    let _ = app.emit_to(
        target,
        MENU_EVENT_REQUEST,
        MenuRequest {
            action: action.to_string(),
            path: None,
            provider_id: None,
        },
    );
}

fn emit_provider_request<R: Runtime>(
    app: &AppHandle<R>,
    target: &str,
    action: &str,
    provider_id: &str,
) {
    let _ = app.emit_to(
        target,
        MENU_EVENT_REQUEST,
        MenuRequest {
            action: action.to_string(),
            path: None,
            provider_id: Some(provider_id.to_string()),
        },
    );
}

fn emit_request_with_path<R: Runtime>(
    app: &AppHandle<R>,
    target: &str,
    action: &str,
    path: String,
) {
    let _ = app.emit_to(
        target,
        MENU_EVENT_REQUEST,
        MenuRequest {
            action: action.to_string(),
            path: Some(path),
            provider_id: None,
        },
    );
}

fn open_recent_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    let submenu = localized_submenu(app, "iina.menu.open-recent", "Open Recent", true)?;
    let recent_documents = app
        .try_state::<AppState>()
        .and_then(|state| state.recent_documents().ok())
        .unwrap_or_default();
    for (index, document) in recent_documents.iter().enumerate() {
        add_literal_item(
            &submenu,
            app,
            &format!("iina.open-recent.{index}"),
            &document.title,
            None,
            true,
        )?;
    }
    if !recent_documents.is_empty() {
        add_separator(&submenu, app)?;
    }
    add_item(
        &submenu,
        app,
        "iina.clear-recent",
        "Clear Menu",
        None,
        !recent_documents.is_empty(),
    )?;
    Ok(submenu)
}

fn recent_document_path_for_menu_item<R: Runtime>(app: &AppHandle<R>, id: &str) -> Option<String> {
    let index = id
        .strip_prefix("iina.open-recent.")?
        .parse::<usize>()
        .ok()?;
    app.try_state::<AppState>()
        .and_then(|state| state.recent_documents().ok())
        .and_then(|recent_documents| {
            recent_documents
                .get(index)
                .map(|document| document.path.clone())
        })
}

fn toggle_fullscreen<R: Runtime>(app: &AppHandle<R>, target: &str) {
    if let Some(window) = app.get_webview_window(player_main_window_label(target)) {
        let state = app.state::<AppState>();
        let is_fullscreen = crate::commands::player_window_is_fullscreen(&window).unwrap_or(false);
        let _ = crate::commands::set_player_window_fullscreen(
            app,
            state.inner(),
            &window,
            !is_fullscreen,
        );
    }
}

fn toggle_always_on_top<R: Runtime>(app: &AppHandle<R>, target: &str) {
    if let Some(window) = app.get_webview_window(player_main_window_label(target)) {
        let is_always_on_top = window.is_always_on_top().unwrap_or(false);
        let _ = window.set_always_on_top(!is_always_on_top);
        let _ = refresh_iina_menu(app);
    }
}

fn resize_player_window<R: Runtime>(app: &AppHandle<R>, target: &str, action: WindowSizeAction) {
    let Some(window) = app.get_webview_window(player_main_window_label(target)) else {
        return;
    };
    if let Err(error) = execute_window_size(app.state::<AppState>().inner(), &window, action) {
        eprintln!("Unable to apply IINA window-size action: {error}");
    }
}

fn saved_filters_for_menu<R: Runtime>(app: &AppHandle<R>) -> (Vec<SavedFilter>, Vec<SavedFilter>) {
    let Some(state) = app.try_state::<AppState>() else {
        return (Vec::new(), Vec::new());
    };
    let Ok(preferences) = state.preferences.lock() else {
        return (Vec::new(), Vec::new());
    };
    (
        preferences.saved_filters(SAVED_VIDEO_FILTERS_KEY),
        preferences.saved_filters(SAVED_AUDIO_FILTERS_KEY),
    )
}

fn active_player_snapshot<R: Runtime>(app: &AppHandle<R>) -> PlayerState {
    let target = active_player_window_label(app);
    let Some(state) = app.try_state::<AppState>() else {
        return PlayerState::default();
    };
    let Ok(session) = state.player_session_for_window(&target) else {
        return PlayerState::default();
    };
    session
        .player()
        .lock()
        .map(|player| player.clone())
        .unwrap_or_default()
}

fn active_player_window_is_fullscreen<R: Runtime>(app: &AppHandle<R>) -> bool {
    let target = active_player_window_label(app);
    app.get_webview_window(player_main_window_label(&target))
        .and_then(|window| crate::commands::player_window_is_fullscreen(&window).ok())
        .unwrap_or(false)
}

fn active_player_window_is_always_on_top<R: Runtime>(app: &AppHandle<R>) -> bool {
    let target = active_player_window_label(app);
    app.get_webview_window(player_main_window_label(&target))
        .and_then(|window| window.is_always_on_top().ok())
        .unwrap_or(false)
}

pub(crate) fn active_player_window_label<R: Runtime>(app: &AppHandle<R>) -> String {
    let focused = app
        .webview_windows()
        .into_iter()
        .find_map(|(label, window)| {
            window
                .is_focused()
                .ok()
                .filter(|focused| *focused)
                .map(|_| label)
        });
    let last_active = app
        .state::<AppState>()
        .last_active_player_session_label()
        .ok();
    resolve_active_player_window_label(focused.as_deref(), last_active.as_deref())
}

fn active_plugin_owner_window_label<R: Runtime>(app: &AppHandle<R>) -> String {
    let focused = app
        .webview_windows()
        .into_iter()
        .find_map(|(label, window)| {
            window
                .is_focused()
                .ok()
                .filter(|focused| *focused)
                .map(|_| label)
        });
    resolve_active_plugin_owner_window_label(focused.as_deref()).to_string()
}

fn resolve_active_plugin_owner_window_label(focused_window: Option<&str>) -> &str {
    focused_window
        .filter(|label| {
            *label == "main" || label.starts_with("mini-player") || label.starts_with("player-")
        })
        .map(player_session_label_for_window)
        .unwrap_or("main")
}

fn resolve_active_player_window_label(
    focused_window: Option<&str>,
    last_active_player: Option<&str>,
) -> String {
    focused_window
        .filter(|label| {
            *label == "main" || label.starts_with("mini-player") || label.starts_with("player-")
        })
        .or_else(|| {
            last_active_player.filter(|label| *label == "main" || label.starts_with("player-"))
        })
        .unwrap_or("main")
        .to_string()
}

/// Window-level menu actions always belong to the owning main player window. A Mini Player keeps
/// receiving menu requests for its playback session, but fullscreen, floating, and resize must be
/// applied to that session's main window, matching IINA's per-`PlayerCore` window ownership.
fn player_main_window_label(target: &str) -> &str {
    player_session_label_for_window(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_binding(key: &str, action: &[&str], is_iina_command: bool) -> ActiveKeyBinding {
        ActiveKeyBinding {
            normalized_mpv_key: key.to_string(),
            action: action.iter().map(|part| (*part).to_string()).collect(),
            is_iina_command,
        }
    }

    fn plugin_item(id: &str, title: &str) -> PluginMenuItemDefinition {
        PluginMenuItemDefinition {
            id: id.to_string(),
            title: title.to_string(),
            enabled: true,
            selected: false,
            key_binding: None,
            separator: false,
            items: Vec::new(),
        }
    }

    fn plugin_definition(
        order_index: usize,
        owner_label: &str,
        role: &str,
        identifier: &str,
        name: &str,
        has_global_instance: bool,
        items: Vec<PluginMenuItemDefinition>,
    ) -> PluginMenuDefinition {
        PluginMenuDefinition {
            order_index,
            owner_label: owner_label.to_string(),
            role: role.to_string(),
            identifier: identifier.to_string(),
            name: name.to_string(),
            has_global_instance,
            items,
        }
    }

    #[test]
    fn open_menu_titles_follow_playback_state_and_window_preference() {
        let idle = open_menu_titles(true, false);
        assert_eq!(idle.open, "Open...");
        assert_eq!(idle.open_alternative, "Open...");
        assert_eq!(idle.open_url, "Open URL...");
        assert_eq!(idle.open_url_alternative, "Open URL...");

        let new_window = open_menu_titles(true, true);
        assert_eq!(new_window.open, "Open in New Window...");
        assert_eq!(new_window.open_alternative, "Open...");
        assert_eq!(new_window.open_url, "Open URL in New Window...");
        assert_eq!(new_window.open_url_alternative, "Open URL...");

        let current_window = open_menu_titles(false, true);
        assert_eq!(current_window.open, "Open...");
        assert_eq!(current_window.open_alternative, "Open in New Window...");
        assert_eq!(current_window.open_url, "Open URL...");
        assert_eq!(
            current_window.open_url_alternative,
            "Open URL in New Window..."
        );
    }

    #[test]
    fn command_n_new_window_is_an_opt_in_native_file_menu_action() {
        assert!(!enable_cmd_n_value(None));
        assert!(!enable_cmd_n_value(Some(&serde_json::json!(false))));
        assert!(!enable_cmd_n_value(Some(&serde_json::json!(1))));
        assert!(enable_cmd_n_value(Some(&serde_json::json!(true))));
        assert_eq!(frontend_request_for_menu_item("iina.new-window"), None);
    }

    #[test]
    fn focused_non_player_window_falls_back_to_last_active_player() {
        assert_eq!(
            resolve_active_player_window_label(Some("preferences"), Some("player-2")),
            "player-2"
        );
        assert_eq!(
            resolve_active_player_window_label(Some("playback-history"), Some("main")),
            "main"
        );
        assert_eq!(
            resolve_active_player_window_label(Some("mini-player-player-2"), Some("main")),
            "mini-player-player-2"
        );
        assert_eq!(
            resolve_active_player_window_label(None, Some("invalid-window")),
            "main"
        );
    }

    #[test]
    fn plugin_owner_follows_the_focused_player_core_and_utility_windows_use_main() {
        assert_eq!(
            resolve_active_plugin_owner_window_label(Some("main")),
            "main"
        );
        assert_eq!(
            resolve_active_plugin_owner_window_label(Some("mini-player-player-1")),
            "player-1"
        );
        assert_eq!(
            resolve_active_plugin_owner_window_label(Some("player-2")),
            "player-2"
        );
        assert_eq!(
            resolve_active_plugin_owner_window_label(Some("preferences")),
            "main"
        );
        assert_eq!(resolve_active_plugin_owner_window_label(None), "main");
    }

    #[test]
    fn mini_player_window_actions_target_their_owning_main_player() {
        assert_eq!(player_main_window_label("main"), "main");
        assert_eq!(player_main_window_label("mini-player"), "main");
        assert_eq!(player_main_window_label("player-2"), "player-2");
        assert_eq!(player_main_window_label("mini-player-player-2"), "player-2");
    }

    #[test]
    fn all_six_iina_window_size_menu_items_share_the_native_router() {
        assert_eq!(
            [
                ("iina.half-size", WindowSizeAction::Half),
                ("iina.normal-size", WindowSizeAction::Normal),
                ("iina.double-size", WindowSizeAction::Double),
                ("iina.fit-screen", WindowSizeAction::FitToScreen),
                ("iina.bigger-size", WindowSizeAction::Bigger),
                ("iina.smaller-size", WindowSizeAction::Smaller),
            ]
            .into_iter()
            .filter(|(id, action)| window_size_action_for_menu_item(id) == Some(*action))
            .count(),
            6
        );
    }

    #[test]
    fn maps_menu_items_to_player_commands() {
        let mut player = PlayerState::default();
        player.duration_seconds = 12.0;
        player.position_seconds = 4.0;
        player.volume = 98.0;
        player.quick_settings.audio_delay = 0.25;
        player.quick_settings.sub_delay = -0.25;

        assert!(matches!(
            command_for_menu_item("iina.toggle-pause", &player),
            Some(PlayerCommand::TogglePause)
        ));
        assert!(matches!(
            command_for_menu_item("iina.seek-forward-5", &player),
            Some(PlayerCommand::Seek { seconds }) if (seconds - 9.0).abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item("iina.seek-backward-5", &player),
            Some(PlayerCommand::Seek { seconds }) if (seconds + 1.0).abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item("iina.frame-step-forward", &player),
            Some(PlayerCommand::FrameStep { backwards: false })
        ));
        assert!(matches!(
            command_for_menu_item("iina.frame-step-backward", &player),
            Some(PlayerCommand::FrameStep { backwards: true })
        ));
        assert!(matches!(
            command_for_menu_item("iina.jump-beginning", &player),
            Some(PlayerCommand::Seek { seconds }) if seconds.abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item("iina.volume-up-5", &player),
            Some(PlayerCommand::SetVolume { volume }) if (volume - 103.0).abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item("iina.volume-down-5", &player),
            Some(PlayerCommand::SetVolume { volume }) if (volume - 93.0).abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item("iina.volume-up-1", &player),
            Some(PlayerCommand::SetVolume { volume }) if (volume - 99.0).abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item("iina.volume-down-1", &player),
            Some(PlayerCommand::SetVolume { volume }) if (volume - 97.0).abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item("iina.audio-delay-up-0_1", &player),
            Some(PlayerCommand::SetAudioDelay { seconds }) if (seconds - 0.35).abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item("iina.audio-delay-down-0_5", &player),
            Some(PlayerCommand::SetAudioDelay { seconds }) if (seconds + 0.25).abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item("iina.audio-delay-reset", &player),
            Some(PlayerCommand::SetAudioDelay { seconds }) if seconds.abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item("iina.subtitle-delay-up-0_5", &player),
            Some(PlayerCommand::SetSubDelay { seconds }) if (seconds - 0.25).abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item("iina.subtitle-delay-down-0_1", &player),
            Some(PlayerCommand::SetSubDelay { seconds }) if (seconds + 0.35).abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item("iina.subtitle-delay-reset", &player),
            Some(PlayerCommand::SetSubDelay { seconds }) if seconds.abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item("iina.mute", &player),
            Some(PlayerCommand::ToggleMute)
        ));
        assert!(matches!(
            command_for_menu_item("iina.speed-1_1x", &player),
            Some(PlayerCommand::MultiplySpeed { factor }) if (factor - 1.1).abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item("iina.speed-reset", &player),
            Some(PlayerCommand::SetSpeed { speed }) if (speed - 1.0).abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item("iina.file-loop", &player),
            Some(PlayerCommand::ToggleFileLoop)
        ));
        assert!(matches!(
            command_for_menu_item("iina.ab-loop", &player),
            Some(PlayerCommand::CycleAbLoop)
        ));
        assert!(matches!(
            command_for_menu_item("iina.playlist-loop", &player),
            Some(PlayerCommand::TogglePlaylistLoop)
        ));
        assert!(matches!(
            command_for_menu_item("iina.toggle-mirror", &player),
            Some(PlayerCommand::SetVideoMirror { enabled: true })
        ));
        assert!(matches!(
            command_for_menu_item("iina.toggle-flip", &player),
            Some(PlayerCommand::SetVideoFlip { enabled: true })
        ));
    }

    #[test]
    fn routes_native_dialog_menu_items_to_frontend_requests() {
        assert_eq!(
            frontend_request_for_menu_item("iina.jump-to"),
            Some("jump-to")
        );
        assert_eq!(
            frontend_request_for_menu_item("iina.save-current-playlist"),
            Some("save-current-playlist")
        );
        assert_eq!(
            frontend_request_for_menu_item("iina.subtitle-font"),
            Some("subtitle-font")
        );
        assert_eq!(
            frontend_request_for_menu_item("iina.delogo"),
            Some("delogo")
        );
        assert_eq!(frontend_request_for_menu_item("iina.open"), None);
        assert_eq!(
            auxiliary_window_action_for_menu_item("iina.log-viewer"),
            Some(AuxiliaryWindowMenuAction::LogViewer)
        );
        assert_eq!(
            auxiliary_window_action_for_menu_item("iina.inspector"),
            Some(AuxiliaryWindowMenuAction::Inspector)
        );
        assert_eq!(
            external_page_for_menu_item("iina.help").map(IinaExternalPage::url),
            Some("https://github.com/iina/iina/wiki")
        );
        assert_eq!(
            external_page_for_menu_item("iina.github").map(IinaExternalPage::url),
            Some("https://github.com/iina/iina")
        );
        assert_eq!(
            external_page_for_menu_item("iina.website").map(IinaExternalPage::url),
            Some("https://iina.io")
        );
        assert_eq!(external_page_for_menu_item("iina.open"), None);
    }

    #[test]
    fn online_subtitle_source_menu_uses_request_scoped_provider_ids() {
        assert_eq!(
            BUILT_IN_SUBTITLE_PROVIDERS,
            [
                (":opensubtitles", "opensubtitles.com"),
                (":assrt", "assrt.net"),
                (":shooter", "shooter.cn"),
            ]
        );
        assert_eq!(
            subtitle_provider_id_from_menu_item(
                "iina.find-online-subtitle-provider.plugin:fixture:custom"
            ),
            Some("plugin:fixture:custom")
        );
        assert!(
            subtitle_provider_id_from_menu_item("iina.find-online-subtitle-provider.").is_none()
        );

        let payload = serde_json::to_value(MenuRequest {
            action: "find-online-subtitles".to_string(),
            path: None,
            provider_id: Some(":assrt".to_string()),
        })
        .unwrap();
        assert_eq!(payload["providerId"], ":assrt");
        assert!(payload.get("provider-id").is_none());

        let frontend = include_str!("../../src/main.js");
        for contract in [
            "showOnlineSubtitlePanel(event.payload?.providerId ?? null)",
            "async function showOnlineSubtitlePanel(providerId = null)",
            "async function searchActiveOnlineSubtitleProvider(providerOverride = null)",
            "invoke(\"search_online_subtitles\", { providerId })",
        ] {
            assert!(
                frontend.contains(contract),
                "missing provider override contract: {contract}"
            );
        }
    }

    #[test]
    fn edit_menu_actions_match_the_macos_first_responder_contract() {
        assert_eq!(
            EDIT_RESPONDER_ACTIONS,
            [
                ("iina.delete", "delete:", "\u{0008}"),
                ("iina.menu.transformations.0", "uppercaseWord:", ""),
                ("iina.menu.transformations.1", "lowercaseWord:", ""),
                ("iina.menu.transformations.2", "capitalizeWord:", ""),
                ("iina.menu.speech.0", "startSpeaking:", ""),
                ("iina.menu.speech.1", "stopSpeaking:", ""),
            ]
        );
    }

    #[test]
    fn delogo_check_state_accepts_the_iina_label_or_filter_name() {
        let mut player = PlayerState::default();
        assert!(!has_current_video(&player));
        player.tracks.video.clear();
        player.current_url = Some("/tmp/audio-only.m4a".to_string());
        assert!(!has_current_video(&player));
        let mut metadata = crate::player::TrackMetadata::default();
        metadata.albumart = true;
        player.tracks.video.push(Track {
            id: 1,
            title: "Video".to_string(),
            selected: true,
            metadata,
        });
        assert!(!has_current_video(&player));
        player.tracks.video[0].metadata.albumart = false;
        assert!(has_current_video(&player));
        assert!(!has_iina_delogo_filter(&player));

        player.video_filters =
            vec![
                crate::mpv::MpvFilter::from_raw("@iina_delogo:lavfi=[delogo=x=1:y=1:w=10:h=10]")
                    .expect("labeled delogo filter"),
            ];
        assert!(has_iina_delogo_filter(&player));

        player.video_filters = vec![
            crate::mpv::MpvFilter::from_raw("iina_delogo=active").expect("named delogo filter")
        ];
        assert!(has_iina_delogo_filter(&player));
    }

    #[test]
    fn resolves_native_menu_equivalents_and_dynamic_numeric_actions_from_active_bindings() {
        let bindings = vec![
            active_binding("Alt+Meta+k", &["seek", "17", "relative+exact"], false),
            active_binding("Meta+界", &["video-panel"], true),
            active_binding("Meta+d", &["add", "volume", "-7"], false),
        ];

        let resolved = resolve_menu_bindings(&bindings);
        let seek = resolved
            .get("iina.seek-forward-5")
            .expect("dynamic seek binding should resolve");
        assert_eq!(seek.key_equivalent, "k");
        assert_eq!(
            seek.modifier_mask,
            NATIVE_MODIFIER_OPTION | NATIVE_MODIFIER_COMMAND
        );
        assert_eq!(seek.numeric_value, Some(17.0));
        assert!(seek.exact_seek);
        assert!(seek
            .title
            .as_deref()
            .is_some_and(|title| title.contains("17")));
        assert_eq!(
            resolved.get("iina.show-video-panel"),
            Some(&ResolvedMenuBinding {
                key_equivalent: "界".to_string(),
                modifier_mask: NATIVE_MODIFIER_COMMAND,
                numeric_value: None,
                exact_seek: false,
                title: None,
            })
        );
        assert_eq!(
            resolved
                .get("iina.volume-down-5")
                .and_then(|binding| binding.numeric_value),
            Some(-7.0)
        );

        let mut player = PlayerState::default();
        player.volume = 98.0;
        assert!(matches!(
            command_for_menu_item_with_binding(
                "iina.seek-forward-5",
                &player,
                resolved.get("iina.seek-forward-5")
            ),
            Some(PlayerCommand::SeekRelative {
                seconds,
                option: RelativeSeekOption::Exact
            }) if (seconds - 17.0).abs() < f64::EPSILON
        ));
        assert!(matches!(
            command_for_menu_item_with_binding(
                "iina.volume-down-5",
                &player,
                resolved.get("iina.volume-down-5")
            ),
            Some(PlayerCommand::SetVolume { volume }) if (volume - 91.0).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn preserves_extra_action_bindings_as_hidden_executable_menu_rows() {
        let bindings = vec![
            active_binding("Meta+l", &["seek", "5"], false),
            active_binding("Alt+Meta+l", &["seek", "17", "relative+exact"], false),
        ];
        let groups = resolve_menu_binding_groups(&bindings);
        let seek = groups.get("iina.seek-forward-5").unwrap();
        assert_eq!(seek.len(), 2);
        assert_eq!(seek[0].numeric_value, Some(5.0));
        assert_eq!(seek[1].numeric_value, Some(17.0));
        assert!(seek[1].exact_seek);

        let spec_index = MENU_BINDING_SPECS
            .iter()
            .position(|spec| spec.id == "iina.seek-forward-5")
            .unwrap();
        let duplicate_id = duplicate_menu_binding_id(spec_index, 1);
        let (target_id, duplicate) =
            duplicate_menu_binding_from_id(&duplicate_id, &groups).unwrap();
        assert_eq!(target_id, "iina.seek-forward-5");
        assert_eq!(duplicate.key_equivalent, "l");
        assert_eq!(
            duplicate.modifier_mask,
            NATIVE_MODIFIER_OPTION | NATIVE_MODIFIER_COMMAND
        );

        let player = PlayerState::default();
        assert!(matches!(
            command_for_menu_item_with_binding(target_id, &player, Some(duplicate)),
            Some(PlayerCommand::SeekRelative {
                seconds,
                option: RelativeSeekOption::Exact,
            }) if (seconds - 17.0).abs() < f64::EPSILON
        ));
        assert_eq!(duplicate_menu_binding_entries(&groups).len(), 1);
    }

    #[test]
    fn native_key_equivalents_cover_reference_symbols_function_keys_and_unicode() {
        assert_eq!(
            native_key_equivalent("Ctrl+Meta+F12"),
            Some((
                String::from('\u{f70f}'),
                NATIVE_MODIFIER_CONTROL | NATIVE_MODIFIER_COMMAND
            ))
        );
        assert_eq!(
            native_key_equivalent("Meta+SHARP"),
            Some(("#".to_string(), NATIVE_MODIFIER_COMMAND))
        );
        assert_eq!(
            native_key_equivalent("Alt+界"),
            Some(("界".to_string(), NATIVE_MODIFIER_OPTION))
        );
        assert_eq!(native_key_equivalent("Meta+a-Meta+b-Meta+c"), None);
    }

    #[test]
    fn native_alternate_menu_contract_matches_iina_135_pairs() {
        let items = native_alternate_menu_items_for_titles(open_menu_titles(false, true));
        assert_eq!(items.len(), 12);
        assert_eq!(items[0], ("File", "Open in New Window...", true));
        assert_eq!(items[1], ("File", "Open URL in New Window...", true));
        for expected in [
            ("Playback", "Next Frame", false),
            ("Playback", "Previous Frame", false),
            ("Playback", "Speed Up to 1.1x", false),
            ("Playback", "Speed Down to 0.9x", false),
            ("Audio", "Volume + 1%", false),
            ("Audio", "Volume - 1%", false),
            ("Audio", "Audio Delay + 0.1s", false),
            ("Audio", "Audio Delay - 0.1s", false),
            ("Subtitles", "Subtitle Delay + 0.1s", false),
            ("Subtitles", "Subtitle Delay - 0.1s", false),
        ] {
            assert!(
                items.contains(&expected),
                "missing alternate item {expected:?}"
            );
        }
    }

    #[test]
    fn maps_dynamic_playlist_chapter_and_track_menu_items() {
        let player = PlayerState::default();

        assert!(matches!(
            command_for_menu_item("iina.set-subtitle-encoding.28", &player),
            Some(PlayerCommand::SetSubEncoding { encoding }) if encoding == "GB18030"
        ));
        assert!(command_for_menu_item("iina.set-subtitle-encoding.44", &player).is_none());

        assert!(matches!(
            command_for_menu_item("iina.select-playlist-item.3", &player),
            Some(PlayerCommand::SelectPlaylistItem { index: 3 })
        ));
        assert!(matches!(
            command_for_menu_item("iina.select-chapter.2", &player),
            Some(PlayerCommand::SelectChapter { index: 2 })
        ));
        assert!(matches!(
            command_for_menu_item("iina.select-video-track.0", &player),
            Some(PlayerCommand::SelectTrack {
                kind: TrackSelectionKind::Video,
                id: 0
            })
        ));
        assert!(matches!(
            command_for_menu_item("iina.select-second-subtitle-track.42", &player),
            Some(PlayerCommand::SelectTrack {
                kind: TrackSelectionKind::SecondSubtitles,
                id: 42
            })
        ));
        assert!(matches!(
            command_for_menu_item("iina.set-aspect.3", &player),
            Some(PlayerCommand::SetVideoAspect { aspect }) if aspect == "16:9"
        ));
        assert!(matches!(
            command_for_menu_item("iina.set-crop.0", &player),
            Some(PlayerCommand::SetVideoCrop { crop }) if crop == "None"
        ));
        assert!(matches!(
            command_for_menu_item("iina.set-rotation.270", &player),
            Some(PlayerCommand::SetVideoRotate { degrees: 270 })
        ));
        assert!(matches!(
            command_for_menu_item("iina.select-audio-device.0", &player),
            Some(PlayerCommand::SelectAudioDevice { name }) if name == "auto"
        ));
        assert!(command_for_menu_item("iina.select-chapter.invalid", &player).is_none());
        assert!(command_for_menu_item("iina.set-aspect.99", &player).is_none());
        assert!(command_for_menu_item("iina.set-rotation.45", &player).is_none());
    }

    #[test]
    fn chapter_menu_times_match_iina_padding() {
        assert_eq!(format_chapter_time(0.0), "00:00");
        assert_eq!(format_chapter_time(3_723.9), "1:02:03");
        assert_eq!(pad_chapter_time("00:10", "1:02:03"), "0:00:10");
        assert_eq!(pad_chapter_time("12:34", "12:34"), "12:34");
    }

    #[test]
    fn parses_plugin_menu_event_identifiers() {
        let request = plugin_menu_request_from_id(
            "iina.plugin-menu.player-2::entry::io.iina.fixture::item_1",
        )
        .expect("plugin menu request");
        assert_eq!(request.owner_label, "player-2");
        assert_eq!(request.role, "entry");
        assert_eq!(request.identifier, "io.iina.fixture");
        assert_eq!(request.item_id, "item_1");
        assert!(plugin_menu_request_from_id("iina.plugin-menu.malformed").is_none());
    }

    #[test]
    fn active_plugin_menu_order_is_install_order_and_global_items_precede_entry_items() {
        let a = plugin_definition(
            0,
            "player-2",
            "entry",
            "io.iina.a",
            "A",
            false,
            vec![plugin_item("a", "A Entry")],
        );
        let b_global = plugin_definition(
            1,
            "main",
            "global",
            "io.iina.b",
            "B",
            true,
            vec![plugin_item("bg", "B Global")],
        );
        let b_entry = plugin_definition(
            1,
            "player-2",
            "entry",
            "io.iina.b",
            "B",
            true,
            vec![plugin_item("be", "B Entry")],
        );
        // A forceUpdate uses retain+push, so A can be physically last without changing its menu
        // position. B also intentionally has a global definition while A does not.
        let active = active_plugin_menu_definitions(&[b_global, b_entry, a], "player-2");
        assert_eq!(
            active
                .iter()
                .map(|plugin| plugin.name.as_str())
                .collect::<Vec<_>>(),
            ["A", "B"]
        );
        assert_eq!(
            active[1]
                .items
                .iter()
                .map(|item| (item.role.as_str(), item.item.title.as_str()))
                .collect::<Vec<_>>(),
            [("global", "B Global"), ("entry", "B Entry")]
        );
    }

    #[test]
    fn plugin_menu_matches_reference_overflow_conflicts_and_empty_developer_submenu() {
        let empty = plugin_definition(
            0,
            "player-2",
            "entry",
            "io.iina.empty",
            "Empty",
            false,
            Vec::new(),
        );
        let mut items = (0..6)
            .map(|index| plugin_item(&format!("item-{index}"), &format!("Item {index}")))
            .collect::<Vec<_>>();
        items[0].key_binding = Some("Meta+x".to_string());
        items[1].selected = true;
        items[1].items.push(plugin_item("nested", "Nested"));
        let visible = plugin_definition(
            1,
            "player-2",
            "entry",
            "io.iina.visible",
            "Visible",
            true,
            items,
        );
        let active = active_plugin_menu_definitions(&[empty, visible], "player-2");
        let plan = plugin_menu_plan(&active, &[active_binding("Meta+x", &["quit"], false)], true);
        assert_eq!(
            plan.shortcut_conflicts,
            [("Visible".to_string(), "Meta+x".to_string())]
        );
        let titles = plan
            .nodes
            .iter()
            .map(|node| match node {
                PlannedPluginMenuNode::Separator => "<separator>",
                PlannedPluginMenuNode::Item { title, .. } => title.as_str(),
            })
            .collect::<Vec<_>>();
        assert_eq!(titles[0], "⚠︎ Conflicting key shortcuts…");
        assert_eq!(
            &titles[1..5],
            ["Manage Plugins…", "<separator>", "<separator>", "Visible"]
        );
        assert!(titles.contains(&"More…"));
        let more = plan.nodes.iter().find(
            |node| matches!(node, PlannedPluginMenuNode::Item { title, .. } if title == "More…"),
        );
        assert!(matches!(
            more,
            Some(PlannedPluginMenuNode::Item { children, .. }) if children.len() == 1
        ));
        let selected_parent = plan.nodes.iter().find(
            |node| matches!(node, PlannedPluginMenuNode::Item { title, .. } if title == "Item 1"),
        );
        assert!(matches!(
            selected_parent,
            Some(PlannedPluginMenuNode::Item { selected: true, children, .. }) if children.len() == 1
        ));

        let empty_plan = plugin_menu_plan(&[], &[], true);
        assert!(empty_plan.nodes.iter().any(|node| matches!(
            node,
            PlannedPluginMenuNode::Item { title, submenu: true, children, .. }
                if title == "Developer Tool" && children.is_empty()
        )));
    }

    #[test]
    fn maps_iina_saved_filter_menu_ids_and_shortcuts() {
        let video = vec![SavedFilter {
            name: "Cinema".to_string(),
            filter_string: "eq=gamma=0.8:contrast=1.2".to_string(),
            shortcut_key: "c".to_string(),
            shortcut_key_modifiers: "ms".to_string(),
        }];
        let audio = vec![SavedFilter {
            name: "Normalize".to_string(),
            filter_string: "lavfi=[loudnorm]".to_string(),
            shortcut_key: "n".to_string(),
            shortcut_key_modifiers: "o".to_string(),
        }];

        assert!(matches!(
            saved_filter_command_from_lists(
                "iina.toggle-saved-video-filter.0",
                &video,
                &audio
            ),
            Some(PlayerCommand::ToggleSavedFilter {
                kind: FilterKind::Video,
                name,
                filter,
            }) if name == "Cinema" && filter == "eq=gamma=0.8:contrast=1.2"
        ));
        assert!(matches!(
            saved_filter_command_from_lists(
                "iina.toggle-saved-audio-filter.0",
                &video,
                &audio
            ),
            Some(PlayerCommand::ToggleSavedFilter {
                kind: FilterKind::Audio,
                name,
                filter,
            }) if name == "Normalize" && filter == "lavfi=[loudnorm]"
        ));
        assert!(saved_filter_command_from_lists(
            "iina.toggle-saved-video-filter.4",
            &video,
            &audio
        )
        .is_none());
        assert_eq!(
            native_saved_filter_key_equivalent(&video[0]),
            Some((
                "c".to_string(),
                NATIVE_MODIFIER_COMMAND | NATIVE_MODIFIER_SHIFT
            ))
        );
        assert_eq!(
            native_saved_filter_key_equivalent(&audio[0]),
            Some(("n".to_string(), NATIVE_MODIFIER_OPTION))
        );

        let localized = SavedFilter {
            name: "本地化".to_string(),
            filter_string: "eq=brightness=0.1".to_string(),
            shortcut_key: "界".to_string(),
            shortcut_key_modifiers: "mcos".to_string(),
        };
        assert_eq!(
            native_saved_filter_key_equivalent(&localized),
            Some((
                "界".to_string(),
                NATIVE_MODIFIER_COMMAND
                    | NATIVE_MODIFIER_CONTROL
                    | NATIVE_MODIFIER_OPTION
                    | NATIVE_MODIFIER_SHIFT
            ))
        );
    }
}
