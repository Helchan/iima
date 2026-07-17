import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");
const [
  referenceMenu,
  referenceDevTool,
  menu,
  commands,
  plugins,
  nativeMenu,
  nativeMenuBridge,
  developerBackend,
  library,
  runtime,
  realm,
  html,
  css,
  developerRuntime,
] = await Promise.all([
  read("参考/iina/iina/MenuController.swift"),
  read("参考/iina/iina/JavascriptDevTool.swift"),
  read("src-tauri/src/menu.rs"),
  read("src-tauri/src/commands.rs"),
  read("src-tauri/src/plugins.rs"),
  read("src-tauri/src/native_menu.rs"),
  read("src-tauri/src/native_menu.m"),
  read("src-tauri/src/plugin_developer_tool.rs"),
  read("src-tauri/src/lib.rs"),
  read("src/main.js"),
  read("src/plugin-realm.js"),
  read("src/plugin-devtool.html"),
  read("src/plugin-devtool.css"),
  read("src/plugin-devtool.js"),
]);

for (const contract of [
  'pluginMenu.addItem(withTitle: "Manage Plugins…")',
  "if counter == 5",
  'moreItem.title = "More…"',
  '"⚠︎ Conflicting key shortcuts…"',
  'developerTool.title = "Developer Tool"',
  'pluginMenu.addItem(withTitle: "Reload all plugins"',
]) assert.ok(referenceMenu.includes(contract), `reference menu contract missing: ${contract}`);

for (const contract of [
  '"Manage Plugins…"',
  "PLUGIN_MENU_FIRST_LEVEL_LIMIT: usize = 5",
  '"More…"',
  '"⚠︎ Conflicting key shortcuts…"',
  '"Developer Tool"',
  '"Reload all plugins"',
  "active_plugin_menu_definitions",
  'for role in ["global", "entry"]',
  "order_index",
  "active.sort_by",
  "resolve_active_plugin_owner_window_label",
  'player_session_label_for_window(&active_plugin_target)',
]) {
  // The active target is now resolved by a dedicated helper; accept either the helper body or its
  // direct mapping expression to keep this check about behavior rather than spelling.
  if (contract === 'player_session_label_for_window(&active_plugin_target)') {
    assert.match(menu, /resolve_active_plugin_owner_window_label[\s\S]*?map\(player_session_label_for_window\)/);
  } else {
    assert.ok(menu.includes(contract), `Tauri plugin menu contract missing: ${contract}`);
  }
}
assert.match(menu, /active_keys\.contains\(&normalized\)[\s\S]*?shortcut_conflicts\.push[\s\S]*?return None/);
assert.match(menu, /plugin_menu_item_id\([\s\S]*?owner_label[\s\S]*?role[\s\S]*?identifier/);
assert.match(commands, /pub fn set_plugin_menu_items\([\s\S]*?window: WebviewWindow[\s\S]*?role: String/);
assert.match(commands, /retain\(\|menu\|[\s\S]*?menu\.owner_label != owner_label[\s\S]*?menu\.role != role/);
for (const field of ["order_index", "owner_label", "role", "has_global_instance"]) {
  assert.ok(plugins.includes(`pub ${field}:`), `PluginMenuDefinition missing ${field}`);
}

for (const contract of [
  "NativeMenuKeyEquivalent::path",
  "NativeMenuItemState::path",
  "configure_item_states",
]) assert.ok(menu.includes(contract) || nativeMenu.includes(contract), contract);
for (const contract of [
  "iima_native_set_menu_item_key_equivalent_at_path",
  "iima_native_set_menu_item_state_at_path",
  "iima_native_plugin_developer_tool_available",
  "@available(macOS 12.0, *)",
]) assert.ok(nativeMenuBridge.includes(contract), contract);

assert.match(referenceDevTool, /CGSize\(width: 500, height: 400\)/);
assert.match(referenceDevTool, /styleMask: \[\.titled, \.closable, \.miniaturizable, \.resizable\]/);
assert.match(referenceDevTool, /window\.title = "DevTool: " \+ title/);
assert.match(referenceDevTool, /jsContext\.evaluateScript\(source\)/);
assert.match(referenceDevTool, /inst\.logHandler/);
assert.match(referenceDevTool, /compactMap\(\{ \$0\.prompt \}\)\.reversed\(\)/);
assert.match(referenceDevTool, /windows: \[JSContext: JSDevToolWindow\]/);

for (const contract of [
  'WebviewUrl::App("plugin-devtool.html".into())',
  '.title(format!("DevTool: {display_title}"))',
  ".inner_size(500.0, 400.0)",
  ".resizable(true)",
  ".maximizable(true)",
  "get_webview_window(&label)",
  "window.show()",
  "window.set_focus()",
  "current_contexts: HashMap<PluginRealmKey, String>",
  "window_contexts: HashMap<String, PluginDeveloperToolContext>",
  "context_id: String",
  "set_plugin_developer_tool_realm_context",
  "current_context_id(&realm_key)",
  "retain_window_context(context.clone())",
]) assert.ok(developerBackend.includes(contract), contract);
assert.match(developerBackend, /fn window_label\([\s\S]*?context_id: &str[\s\S]*?owner_label[\s\S]*?context_id/);
assert.match(developerBackend, /set_current_context\([\s\S]*?current_contexts\.insert/);
assert.match(library, /is_plugin_developer_tool_label\(&label\)[\s\S]*?api\.prevent_close\(\)[\s\S]*?window\.hide\(\)/);
assert.ok(library.includes("get_plugin_developer_tool_context"));
assert.ok(library.includes("set_plugin_developer_tool_realm_context"));

for (const id of ["developer-run", "developer-global", "developer-clear", "developer-splitter"]) {
  assert.ok(html.includes(`id="${id}"`), `Developer Tool HTML missing ${id}`);
}
assert.equal((html.match(/<svg /g) || []).length, 3, "three toolbar actions must use visual icons");
assert.ok(html.includes("Cmd+Return to run code"));
assert.match(css, /grid-template-rows: minmax\(0, 1fr\) 5px var\(--developer-console-height/);
assert.match(css, /\.developer-input[\s\S]*?max-height: 100px/);
assert.match(css, /developer-splitter[\s\S]*?cursor: ns-resize/);

for (const contract of [
  'execute("$global"',
  'event.key === "Enter" && event.metaKey',
  'event.key === "ArrowUp" && !input.value',
  "pendingIndexes",
  'appendRow(`[${index}]:`',
  'appendRow("→"',
  'iima-plugin-developer-tool-result',
  'iima-plugin-developer-tool-log',
  'iima-plugin-developer-tool-evaluate',
  "contextId: context.contextId",
  "event.payload?.contextId !== context.contextId",
  'splitter.addEventListener("pointerdown"',
]) assert.ok(developerRuntime.includes(contract), contract);
for (const kind of ["number", "string", "boolean", "null", "undefined", "array", "object", "opaque"]) {
  assert.ok(runtime.includes(`kind: "${kind}"`) || developerRuntime.includes(`kind === "${kind}"`), kind);
}
assert.match(developerRuntime, /result\.entries[\s\S]*?result\.remaining/);
assert.match(developerRuntime, /developer-message--\$\{level\}/);

for (const contract of [
  "evaluateDeveloper(source)",
  "developerGlobal()",
  "capabilityGlobal.iina",
  "realm has been destroyed",
]) assert.ok(realm.includes(contract), contract);
assert.match(runtime, /refreshPluginRuntimes\(\{ force = false \} = \{\}\)/);
assert.match(runtime, /if \(force && spec\)[\s\S]*?await reloadPluginEntryRuntime\(runtime, spec\)/);
assert.match(runtime, /if \(spec && pluginRuntimeFingerprint\(spec\) === runtime\.fingerprint\) continue/);
assert.match(runtime, /iima-plugin-runtime-reload-all[\s\S]*?queuePluginRuntimeRefresh\(\{ force: true \}\)/);
assert.match(runtime, /menuItems: \{ entry: \[\], global: \[\] \}/);
assert.match(runtime, /syncPluginMenus[\s\S]*?syncPluginMenu\(runtime, "global"\)[\s\S]*?syncPluginMenu\(runtime, "entry"\)/);
for (const contract of [
  "realmContextIds: { entry: null, global: null }",
  "retiredRealmContexts: new Map()",
  'invoke("set_plugin_developer_tool_realm_context"',
  "pluginDeveloperToolTargets.set(contextId",
  "pluginDeveloperToolRealmContext(runtime, role, contextId)",
  "bindPluginApiToRealmLease(api, realmLease)",
  "createPluginConsole(runtime, realm.role, realm.contextId)",
  "return executePluginScript(runtime, nextPath, api, true, moduleExports, realm, scripts)",
]) assert.ok(runtime.includes(contract), contract);
assert.match(runtime, /iima-plugin-developer-tool-result[\s\S]*?contextId,[\s\S]*?requestId/);

const entryReloadStart = runtime.indexOf("async function unloadPluginEntryRuntime(runtime)");
const fullUnloadStart = runtime.indexOf("async function unloadPluginRuntime(runtime)");
assert.ok(entryReloadStart >= 0 && fullUnloadStart > entryReloadStart, "entry-only reload lifecycle is missing");
const entryReloadLifecycle = runtime.slice(entryReloadStart, fullUnloadStart);
for (const preservedGlobalResource of [
  "globalRealm",
  "globalModuleExports",
  "globalControllerRegistered",
  "globalControllerListeners",
  "globalControllerTask",
  "nextGlobalInstanceId",
]) {
  assert.ok(
    !entryReloadLifecycle.includes(`runtime.${preservedGlobalResource}`),
    `entry-only reload must preserve ${preservedGlobalResource}`,
  );
}
for (const preservedGlobalResource of [
  "runtime.syncTransports.global",
  "runtime.websocket.global",
  'pluginPageState(runtime, "standalone", "global")',
  'hooks.role === "global"',
]) {
  assert.ok(
    !entryReloadLifecycle.includes(preservedGlobalResource),
    `entry-only reload must preserve ${preservedGlobalResource}`,
  );
}
for (const replacedEntryResource of [
  "runtime.realmLeases.entry.active = false",
  "runtime.realmContextIds.entry = null",
  "runtime.moduleExports = new Map()",
  "runtime.syncTransports.entry",
  "await entrySyncTransport.revoke()",
  "runtime.websocket.entry",
  'role: "entry"',
  'pluginPageState(runtime, surface, "entry")',
  'await invoke("plugin_webview_cleanup_role"',
  'if (hooks.role === "entry") runtime.utilsExecHooks.delete(requestId)',
  "pluginDeveloperToolTargets.has(entryContextId)",
  "runtime.retiredRealmContexts.set(entryContextId",
  "entryRealm.destroy()",
  "runtime.eventListeners.clear()",
  "runtime.inputListeners.clear()",
  "runtime.globalChildListeners.clear()",
  "runtime.mpvHookCallbacks.clear()",
  'runtime.menuItems.entry.splice(0, runtime.menuItems.entry.length)',
  'await runPluginEntryRuntime(runtime)',
  'await syncPluginMenu(runtime, "entry")',
]) assert.ok(entryReloadLifecycle.includes(replacedEntryResource), replacedEntryResource);
assert.match(entryReloadLifecycle, /catch \(error\) \{[\s\S]*?await unloadPluginEntryRuntime\(runtime\);[\s\S]*?throw error/);

console.log("Plugin native menu, entry-only forced reload, and context-identity Developer Tool contracts passed");
