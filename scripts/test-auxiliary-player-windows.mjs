import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");
const [
  openSwift,
  openXib,
  filterSwift,
  filterXib,
  preferencesSwift,
  preferencesXib,
  historyXib,
  logXib,
  backend,
  auxiliaryWindows,
  commands,
  native,
  library,
  menu,
  html,
  css,
  runtime,
] = await Promise.all([
  read("参考/iina/iina/OpenURLWindowController.swift"),
  read("参考/iina/iina/Base.lproj/OpenURLWindowController.xib"),
  read("参考/iina/iina/FilterWindowController.swift"),
  read("参考/iina/iina/Base.lproj/FilterWindowController.xib"),
  read("参考/iina/iina/PreferenceWindowController.swift"),
  read("参考/iina/iina/Base.lproj/PreferenceWindowController.xib"),
  read("参考/iina/iina/Base.lproj/HistoryWindowController.xib"),
  read("参考/iina/iina/Base.lproj/LogWindowController.xib"),
  read("src-tauri/src/auxiliary_player_windows.rs"),
  read("src-tauri/src/auxiliary_windows.rs"),
  read("src-tauri/src/commands.rs"),
  read("src-tauri/src/native_window.m"),
  read("src-tauri/src/lib.rs"),
  read("src-tauri/src/menu.rs"),
  read("src/index.html"),
  read("src/styles.css"),
  read("src/main.js"),
]);

assert.match(openXib, /releasedWhenClosed="NO"/);
assert.match(openXib, /width="576" height="270"/);
assert.match(openXib, /fullSizeContentView="YES"/);
assert.match(openSwift, /window\?\.isMovableByWindowBackground = true/);
assert.match(openSwift, /window\?\.titlebarAppearsTransparent = true/);
assert.match(openSwift, /window\?\.titleVisibility = \.hidden/);
assert.match(openSwift, /\[\.closeButton, \.miniaturizeButton, \.zoomButton\]/);
assert.match(openSwift, /PlayerCore\.activeOrNewForMenuAction\(isAlternative: isAlternativeAction\)/);

assert.match(filterXib, /title="Filters"[\s\S]*?releasedWhenClosed="NO"/);
assert.match(filterXib, /width="480" height="382"/);
assert.match(filterSwift, /PlayerCore\.lastActive/);
assert.match(preferencesXib, /frameAutosaveName="IINAPreferenceWindow"/);
assert.match(preferencesXib, /width="820" height="480"/);
assert.match(preferencesXib, /key="minSize" type="size" width="820" height="320"/);
assert.match(preferencesXib, /fullSizeContentView="YES"/);
assert.match(preferencesSwift, /window\?\.isMovableByWindowBackground = true/);

for (const contract of [
  "OPEN_URL_WINDOW_LABEL",
  "VIDEO_FILTER_WINDOW_LABEL",
  "AUDIO_FILTER_WINDOW_LABEL",
  "PREFERENCES_WINDOW_LABEL",
  "iima-auxiliary-window-context",
  "index.html?window-role=",
  "get_webview_window(spec.label)",
]) assert.ok(backend.includes(contract), contract);
assert.match(backend, /width: 576\.0,[\s\S]*?height: 270\.0/);
assert.match(backend, /width: 480\.0,[\s\S]*?height: 382\.0/);
assert.match(backend, /width: 820\.0,[\s\S]*?height: 480\.0,[\s\S]*?min_width: Some\(820\.0\),[\s\S]*?min_height: Some\(320\.0\)/);
assert.match(backend, /show_open_url_for_owner/);
assert.match(backend, /show_filter_for_owner/);
assert.match(backend, /show_preferences_for_pane/);

assert.match(commands, /pub fn submit_open_url\(/);
assert.match(commands, /fn open_url_submission_route\([\s\S]*?last_active_player_session_label\(\)/);
assert.match(commands, /fn open_url_submission_uses_the_latest_active_session_not_the_window_owner\(\)/);
assert.match(commands, /VIDEO_FILTER_WINDOW_LABEL,[\s\S]*?AUDIO_FILTER_WINDOW_LABEL/);
for (const action of ["iina.open-url", "iina.video-filters", "iina.audio-filters", "iina.preferences"]) {
  assert.ok(menu.includes(`"${action}"`), action);
}
for (const command of [
  "show_open_url_window",
  "show_filter_window",
  "show_preferences_window",
  "get_auxiliary_window_context",
  "hide_auxiliary_window",
  "request_player_plugin_runtime_refresh",
  "submit_open_url",
  "get_preference_snapshot",
]) assert.ok(library.includes(command), command);
assert.match(library, /is_reusable_auxiliary_window_label\(&label\)[\s\S]*?api\.prevent_close\(\)[\s\S]*?window\.hide\(\)/);

for (const contract of [
  "iima_native_configure_auxiliary_window",
  "window.releasedWhenClosed = NO",
  "NSWindowStyleMaskFullSizeContentView",
  "NSWindowTitleHidden",
  "window.movableByWindowBackground = YES",
  "NSWindowCloseButton",
  "IINAPreferenceWindow",
  "IIMAConfigureFrameAutosave",
  "setFrameUsingName:autosaveName force:NO",
  "NSWindow Frame %@",
]) assert.ok(native.includes(contract), contract);

assert.match(html, /get\("window-role"\)/);
assert.match(html, /Please enter the URL here……/);
assert.match(html, /data-i18n-table="OpenURLWindowController"/);
assert.match(html, /data-i18n-table="FilterWindowController"/);
assert.match(css, /data-window-role="open-url"/);
assert.match(css, /data-window-role="video-filter"/);
assert.match(css, /data-window-role="audio-filter"/);
assert.match(css, /data-window-role="preferences"/);
assert.match(css, /grid-template-rows: minmax\(120px, 1fr\) 1px 140px/);
assert.match(css, /grid-template-columns: 200px minmax\(620px, 1fr\)/);
assert.match(css, /\.preferences-sidebar \{[\s\S]*?min-height: 0/);
assert.match(css, /\.preferences-detail \{[\s\S]*?min-height: 0/);
assert.match(runtime, /renderPreferences\(\);\s*els\.preferencesContent\.scrollTop = 0/);
assert.match(runtime, /async function activateAuxiliaryWindowSurface\(/);
assert.match(runtime, /invoke\("get_auxiliary_window_context"\)/);
assert.match(runtime, /invoke\("show_open_url_window"/);
assert.match(runtime, /invoke\("show_filter_window"/);
assert.match(runtime, /invoke\("show_preferences_window"/);
assert.match(runtime, /invoke\("submit_open_url"/);
assert.match(runtime, /async function refreshPlayerPluginRuntimes\(\)/);
assert.match(runtime, /iima-plugin-runtime-refresh/);
assert.match(runtime, /iima-preference-changed/);
assert.match(runtime, /revision <= lastPreferenceChangeRevision/);
assert.match(commands, /pub fn get_preference_snapshot\(/);
assert.match(commands, /PREFERENCE_CHANGE_SEQUENCE[\s\S]*?load\(Ordering::Acquire\)[\s\S]*?saturating_sub\(1\)/);
assert.match(runtime, /async function reconcilePreferencesAfterListenerInstall\(\)/);
assert.match(runtime, /invoke\("get_preference_snapshot"\)/);
assert.match(runtime, /refreshPluginRuntime: false/);
assert.match(runtime, /await installTauriMenuListeners\(\);\s*await reconcilePreferencesAfterListenerInstall\(\);/);
assert.match(commands, /app\.emit\(PREFERENCE_CHANGED_EVENT, event\)/);
assert.equal((runtime.match(/tauriListen\("iima-preference-changed"/g) || []).length, 1);
assert.equal((commands.match(/app\.emit\(PREFERENCE_CHANGED_EVENT, event\)/g) || []).length, 1);
assert.match(commands, /origin_label: window\.label\(\)\.to_string\(\)/);
for (const host of ["main", "player-2"]) assert.ok(backend.includes(`is_player_plugin_runtime_host("${host}")`));
for (const utility of ["mini-player", "preferences", "video-filter"]) {
  assert.ok(backend.includes(`!is_player_plugin_runtime_host("${utility}")`));
}
assert.match(backend, /selected_plugin_identifier: Option<String>/);
assert.match(backend, /drain_pending_plugin_installs: bool/);
assert.match(runtime, /applyPluginPreferenceWindowContext\(resolvedContext\.selectedPluginIdentifier\)/);
assert.match(runtime, /async function requestPendingPluginInstallDrain\(/);
assert.match(runtime, /invoke\("has_pending_plugin_installs"\)/);
assert.match(runtime, /drainPendingPluginInstalls: true/);
assert.match(runtime, /if \(!isPreferencesAuxiliaryWindow\) return;/);
assert.match(commands, /claim_pending_plugin_install\([\s\S]*?PREFERENCES_WINDOW_LABEL/);
assert.match(commands, /pub fn has_pending_plugin_installs\(/);

assert.match(historyXib, /releasedWhenClosed="NO" frameAutosaveName="PlaybackHistoryWindow"/);
assert.match(logXib, /releasedWhenClosed="NO" frameAutosaveName="IINALogViewer"/);
assert.match(native, /iima_native_configure_retained_window/);
assert.match(native, /if \(hasSavedFrame\) \{\s*\[window setFrameUsingName:autosaveName force:NO\];\s*\}\s*\[window setFrameAutosaveName:autosaveName\]/);
assert.match(native, /IIMAConfigureFrameAutosave\(window, @"IINAPreferenceWindow"\)/);
assert.match(native, /iima_native_configure_retained_window[\s\S]*?IIMAConfigureFrameAutosave\(window, autosaveName\)/);
assert.match(commands, /inner_size\(600\.0, 400\.0\)/);
assert.match(commands, /min_inner_size\(400\.0, 200\.0\)/);
assert.match(commands, /configure_retained_window\([\s\S]*?&window,[\s\S]*?"PlaybackHistoryWindow"[\s\S]*?\)/);
assert.match(auxiliaryWindows, /inner_size\(600\.0, 335\.0\)/);
assert.match(auxiliaryWindows, /configure_retained_window\(&window, "IINALogViewer"\)/);
assert.match(backend, /PLAYBACK_HISTORY_WINDOW_LABEL[\s\S]*?LOG_VIEWER_WINDOW_LABEL/);

console.log("Reusable Open URL, filter, Preferences, History, and Log window contracts passed");
