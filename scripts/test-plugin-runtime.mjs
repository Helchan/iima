import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const frontend = read("../src/main.js");
const commands = read("../src-tauri/src/commands.rs");
const library = read("../src-tauri/src/lib.rs");
const keychain = read("../src-tauri/src/native_keychain.m");
const websocket = read("../src-tauri/src/plugin_websocket.rs");
const utilities = read("../src-tauri/src/plugin_utils.rs");
const globalApi = read("../src-tauri/src/plugin_global.rs");
const pluginWebview = read("../src-tauri/src/plugin_webview.rs");
const plugins = read("../src-tauri/src/plugins.rs");
const player = read("../src-tauri/src/player.rs");
const mpv = read("../src-tauri/src/mpv.rs");
const nativePrompt = read("../src-tauri/src/native_prompt.m");
const nativeWindow = read("../src-tauri/src/native_window.m");

for (const contract of [
  "playlist: createPluginPlaylistApi(runtime)",
  "function createPluginPlaylistApi(runtime)",
  'if (value === null) return "<null>";',
  'if (value === undefined) return "<undefined>";',
  "registerMenuItemBuilder: (builder)",
  "keychainWrite: (service, name, password)",
  "keychainRead: (service, name)",
  'invokeSync("file.trash"',
  "const handle = (path, mode)",
  '"file.handle.readtoend"',
  'pluginFileHandleReadValue(call("file.handle.readtoend"), createUint8Array)',
  "ws: createPluginWebSocketApi(runtime, invokeSync, role)",
  "function createPluginWebSocketApi(runtime, invokeSync, role)",
  "createServer: (options = {})",
  "startServer: ()",
  "onStateUpdate: (handler)",
  "onMessage: (handler)",
  "onNewConnection: (handler)",
  "onConnectionStateUpdate: (handler)",
  "sendText: (connection, text)",
  "iima-plugin-websocket-message",
  "text: () =>",
  "data: () => Uint8Array.from(data)",
  'utils: createPluginUtilsApi(runtime, hasPermission, invokeSync, globalRole !== "controller", role)',
  'function createPluginUtilsApi(runtime, hasPermission, invokeSync, playerAvailable = true, role = "entry")',
  "fileInPath: (file)",
  "resolvePath: (path)",
  "exec: (file, args = [], cwd = null, stdoutHook = null, stderrHook = null)",
  'if (message.includes("Cannot find the binary")) throw -1;',
  'if (message.includes("not executable, and execute permission cannot be added")) throw -2;',
  "chooseFile: (title, options = {})",
  "path ?? new Promise(() => {})",
  "To call this API, the plugin must declare permission",
  "iima-plugin-utils-exec-output",
  "runtime.utilsExecHooks.set(requestId",
  'hooks.role !== String(payload?.role || "")',
  "runtime.websocket[role]",
  'const role = String(payload?.role || "")',
  "function createPluginGlobalControllerApi(runtime)",
  "function createPluginGlobalChildApi(runtime)",
  'invoke("plugin_global_create_player_instance"',
  'invoke("plugin_global_post_to_child"',
  'invoke("plugin_global_post_to_controller"',
  "getLabel: () => managedPluginUserLabel",
  'await invoke("plugin_global_register_controller"',
  'await invoke("plugin_global_unregister_controller"',
  'iima-plugin-global-controller-message',
  'iima-plugin-global-child-message',
  "runtime.globalControllerListeners.set",
  "runtime.globalChildListeners.set",
  "isMiniPlayerWindow",
  "managedPluginEnablesAll",
  "xmlrpc: (location)",
  "xmlRpcEncodeCall(methodName, args)",
  "xmlRpcDecodeResponse(response.text)",
  "function pluginHttpUrlAllowed(runtime, rawUrl)",
  "standaloneWindow: createPluginStandaloneWindowApi(runtime, invokeSync, role)",
  "function createPluginStandaloneWindowApi(runtime, invokeSync, role)",
  'runVoid(queuePluginPageLoad(runtime, "standalone", String(path), false, "file", true, role)',
  "if (!page || page.generation === 0) return;",
  "plugin_standalone_window_set_property",
  "plugin_standalone_window_set_frame",
  "function createPluginOverlayApi(runtime, hasPermission)",
  "queuePluginPageLoad(runtime, \"overlay\"",
  "function createPluginSidebarApi(runtime)",
  "queuePluginPageLoad(runtime, \"sidebar\"",
  "iima-plugin-webview-message",
  "__iimaPluginPagePost",
  "registerPluginPageListener(page, name, callback)",
  "page.listeners.clear()",
  "plugin_webview_cleanup",
  "plugin_webview_cleanup_role",
  "function queryPluginOverlayHitTests(event)",
  "page.clickableEnabled = Boolean(clickable)",
  "__iimaPluginPageHitTest",
  "consumePluginMpvEventBatch",
  'invoke("plugin_mpv_observe_property"',
  "iima-plugin-mpv-events",
  "iima-plugin-host-event",
  'emitPluginEvent("iina.window-loaded")',
  'emitPluginEvent("iina.thumbnails-ready")',
  'emitPluginEvent("iina.plugin-overlay-loaded")',
]) {
  assert.ok(frontend.includes(contract), `Missing frontend plugin API contract: ${contract}`);
}

assert.ok(frontend.includes("options?.enabled === undefined ? true : Boolean(options.enabled)"));
assert.ok(frontend.includes("options?.keyBinding === undefined ? null : String(options.keyBinding)"));
assert.ok(frontend.includes("function queuePluginMenuSync(runtime, role)"));
assert.ok(frontend.includes("function syncPluginMenu(runtime, role)"));
assert.ok(frontend.includes('menuItems: { entry: [], global: [] }'));
assert.ok(frontend.includes('pluginMenuItemKey(role, itemId)'));
assert.ok(frontend.includes("if (!paths || !Number.isInteger(index) || index >= count)"));
assert.ok(frontend.includes("if (typeof listener.callback === \"function\" && Boolean(listener.callback(args)))"));

assert.ok(frontend.includes("function createPluginOverlayApi(runtime, hasPermission)"));
assert.ok(frontend.includes("if (!permitted()) return;"));
assert.ok(frontend.includes("const overlay = ensurePluginOverlay(runtime);"));
assert.ok(frontend.includes('observe(queuePluginPageLoad(runtime, "overlay", String(path), false), "loadFile")'));
assert.ok(frontend.includes('observe(queuePluginPageLoad(runtime, "sidebar", String(path), false), "loadFile")'));
assert.ok(!frontend.includes('return queuePluginPageLoad(runtime, "overlay"'));
assert.ok(!frontend.includes('return queuePluginPageLoad(runtime, "sidebar"'));

assert.ok(commands.includes("pub fn plugin_mpv_observe_property("));
assert.ok(library.includes("plugin_mpv_observe_property,"));
assert.ok(library.includes("emit_all_player_mpv_event_batches"));
assert.ok(player.includes("pub mpv_event_cursor: u64"));
assert.ok(player.includes("pub fn plugin_mpv_events_after("));
assert.ok(mpv.includes("pub new_events: Vec<MpvClientEvent>"));

for (const command of [
  "plugin_keychain_read",
  "plugin_keychain_write",
  "plugin_file_trash",
  "plugin_file_handle_open",
  "plugin_file_handle_offset",
  "plugin_file_handle_seek",
  "plugin_file_handle_seek_to_end",
  "plugin_file_handle_read",
  "plugin_file_handle_read_to_end",
  "plugin_file_handle_write",
  "plugin_file_handle_close",
]) {
  assert.ok(commands.includes(`pub ${command.startsWith("plugin_keychain") ? "async " : ""}fn ${command}(`), `Missing backend command: ${command}`);
  assert.ok(library.includes(`${command},`), `Backend command is not registered: ${command}`);
}

for (const command of ["confirm_plugin_permissions", "cancel_plugin_permissions"]) {
  assert.ok(commands.includes(`pub fn ${command}(`), `Missing plugin permission command: ${command}`);
  assert.ok(library.includes(`${command},`), `Plugin permission command is not registered: ${command}`);
}
for (const contract of [
  "PluginInstallResult::PermissionConfirmation",
  "PreparedPluginPermissionInstall",
  "plugin-permission-",
  "PLUGIN_PERMISSION_TOKEN_LIFETIME",
  "commit_new_staged_install_enabled",
  "confirm_plugin_permissions_in_root",
  "cancel_plugin_permissions_in_root",
  "only_added: true",
]) {
  assert.ok(plugins.includes(contract), `Missing staged plugin permission contract: ${contract}`);
}
for (const contract of [
  "function showPluginPermissionConfirmation(confirmation)",
  'result.status === "permission-confirmation"',
  'invoke("confirm_plugin_permissions"',
  'invoke("cancel_plugin_permissions"',
  "This update requires additional permissions. Please review them before proceeding.",
  "This plugin requires the following permissions. Please review them before proceeding.",
]) {
  assert.ok(frontend.includes(contract), `Missing plugin permission UI contract: ${contract}`);
}

assert.ok(commands.includes('format!("{identifier} - {service}")'));
assert.ok(keychain.includes("iima_keychain_read_generic"));
assert.ok(keychain.includes("iima_keychain_write_generic"));
assert.ok(keychain.includes("kSecClassGenericPassword"));
assert.ok(commands.includes("fn plugin_track_file_path("));
assert.ok(commands.includes("read_plugin_file_handle_to_end(handle, PLUGIN_FILE_HANDLE_MAX_IO_BYTES)"));

for (const command of [
  "plugin_utils_file_in_path",
  "plugin_utils_resolve_path",
  "plugin_utils_exec",
  "plugin_utils_ask",
  "plugin_utils_prompt",
  "plugin_utils_choose_file",
  "plugin_utils_open",
]) {
  const declaration = command === "plugin_utils_exec" ? `pub async fn ${command}(` : `pub fn ${command}(`;
  assert.ok(utilities.includes(declaration), `Missing utils backend command: ${command}`);
  assert.ok(library.includes(`${command},`), `Utils backend command is not registered: ${command}`);
}
assert.ok(utilities.includes("plugins::require_plugin_permission(app, identifier, \"file-system\")"));
assert.ok(utilities.includes('arg("exec \\"$0\\" \\"$@\\"")'));
assert.ok(utilities.includes('"iima-plugin-utils-exec-output"'));
assert.ok(utilities.includes('#[serde(rename_all = "camelCase")]'));
assert.ok(nativePrompt.includes("iima_native_confirm"));
assert.ok(nativePrompt.includes("iima_native_prompt_multiline_text"));
assert.ok(nativePrompt.includes("multiline ? 60 : 24"));
assert.ok(nativePrompt.includes("NSLineBreakByWordWrapping"));
assert.ok(utilities.includes("native_prompt::prompt_multiline_text("));

for (const command of [
  "plugin_webview_prepare_page",
  "plugin_webview_cleanup",
  "plugin_webview_cleanup_role",
  "plugin_standalone_window_load",
  "plugin_standalone_window_open",
  "plugin_standalone_window_close",
  "plugin_standalone_window_is_open",
  "plugin_standalone_window_set_property",
  "plugin_standalone_window_set_frame",
  "plugin_standalone_window_post_message",
  "plugin_standalone_window_set_simple_value",
]) {
  assert.ok(pluginWebview.includes(`pub fn ${command}(`), `Missing plugin WebView command: ${command}`);
  assert.ok(library.includes(`${command},`), `Plugin WebView command is not registered: ${command}`);
}
for (const contract of [
  'pub const PLUGIN_WEBVIEW_SCHEME: &str = "iima-plugin"',
  "const MAX_RESOURCE_COUNT: usize = 256",
  "const MAX_RESOURCE_BYTES: u64 = 8 * 1024 * 1024",
  "const MAX_PAGE_BYTES: u64 = 512 * 1024",
  "const MAX_TOTAL_RESOURCE_BYTES: u64 = 16 * 1024 * 1024",
  "const MAX_BRIDGE_MESSAGE_BYTES: usize = 256 * 1024",
  "Plugin resource escapes its package",
  "canonical_resource_check_rejects_parent_and_nested_symlink_escape",
  "allow-scripts allow-same-origin allow-forms allow-modals allow-popups allow-downloads",
  'Reflect.deleteProperty(window, "__TAURI__")',
  "Object.prototype.hasOwnProperty.call(element.dataset",
  'message.control === "hit-test"',
  "active_standalone_url(",
  "cleanup_owner",
  "cleanup_all",
]) {
  assert.ok(
    pluginWebview.includes(contract) || frontend.includes(contract),
    `Missing plugin WebView safety/compatibility contract: ${contract}`,
  );
}
assert.ok(library.includes(".register_uri_scheme_protocol(plugin_webview::PLUGIN_WEBVIEW_SCHEME"));
assert.ok(library.includes("plugin_webview::is_standalone_window_label(&label)"));
assert.ok(nativeWindow.includes("iima_native_configure_plugin_window"));
assert.ok(nativeWindow.includes("NSWindowStyleMaskFullSizeContentView"));
assert.ok(nativeWindow.includes("iima_native_set_plugin_window_frame"));

for (const command of [
  "plugin_websocket_create_server",
  "plugin_websocket_start_server",
  "plugin_websocket_send_text",
  "plugin_websocket_stop",
]) {
  assert.ok(websocket.includes(`pub fn ${command}(`), `Missing WebSocket backend command: ${command}`);
  assert.ok(library.includes(`${command},`), `WebSocket backend command is not registered: ${command}`);
}

for (const contract of [
  "SocketAddrV4::new(Ipv4Addr::LOCALHOST, entry.port)",
  "const MAX_SERVERS: usize = 16",
  "const MAX_CONNECTIONS_PER_SERVER: usize = 32",
  "const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024",
  "const MAX_MESSAGE_BYTES: usize = 1024 * 1024",
  "Client WebSocket frames must be masked",
  "Only WebSocket version 13 is supported",
  "WebSocket handshake has no Host header",
  "fn validate_close_payload(payload: &[u8])",
  "sockets: Arc<Mutex<HashMap<String, TcpStream>>>",
  "connection_threads: Arc<Mutex<Vec<JoinHandle<()>>>>",
  "ws.sendText: connection is not ready",
  "write_server_frame(&mut *writer, 0x2, text.as_bytes())",
  "pub fn stop_identifier(app: &AppHandle, identifier: &str)",
  "pub fn stop_window(app: &AppHandle, window_label: &str)",
  "pub fn stop_all(app: &AppHandle)",
]) {
  assert.ok(websocket.includes(contract), `Missing WebSocket safety/compatibility contract: ${contract}`);
}
assert.ok(commands.includes("plugin_websocket::stop_identifier(&app, &identifier)"));
assert.ok(commands.includes("plugin_websocket::stop_all(&app)"));
assert.ok(library.includes("plugin_websocket::stop_window(app, &label)"));

for (const command of [
  "plugin_global_register_controller",
  "plugin_global_unregister_controller",
  "plugin_global_create_player_instance",
  "plugin_global_get_label",
  "plugin_global_post_to_controller",
  "plugin_global_post_to_child",
]) {
  assert.ok(globalApi.includes(`pub fn ${command}(`), `Missing global backend command: ${command}`);
  assert.ok(library.includes(`${command},`), `Global backend command is not registered: ${command}`);
}
for (const contract of [
  "const MAX_MANAGED_INSTANCES_PER_PLUGIN: usize = 32",
  "const MAX_MESSAGE_DATA_BYTES: usize = 1024 * 1024",
  "Plugin global controllers belong to the main window",
  "commands::open_plugin_managed_player_window(",
  "pub fn remove_window(window_label: &str)",
  "pub fn stop_identifier<R: Runtime>",
  "pub fn stop_all<R: Runtime>",
]) {
  assert.ok(globalApi.includes(contract), `Missing global lifecycle/safety contract: ${contract}`);
}
assert.ok(commands.includes("pub(crate) fn open_plugin_managed_player_window<R: Runtime>("));
assert.ok(commands.includes("plugin-managed={identifier}"));
assert.ok(commands.includes("plugin_global::stop_identifier(&app, &identifier)"));
assert.ok(commands.includes("plugin_global::stop_all(&app)"));
assert.ok(library.includes("plugin_global::remove_window(&label)"));
assert.ok(library.includes("plugin_global::stop_all(app)"));

console.log("Plugin runtime compatibility checks passed");
