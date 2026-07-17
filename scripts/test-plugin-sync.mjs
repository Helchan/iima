import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  createPluginSyncTransport,
  pluginFileHandleReadValue,
  pluginFileSystemPermissionError,
  pluginPathForApi,
  pluginSyncProtocolLimit,
  pluginWebSocketHandlerValue,
  pluginWebSocketPort,
  withPluginFileSystemPermission,
} from "../src/plugin-sync.js";

const TOKEN = "a".repeat(64);
const ENDPOINT = "iima-plugin-sync://localhost/invoke";

class FakeXMLHttpRequest {
  static instances = [];
  static next = { status: 200, responseText: '{"ok":true,"value":7}' };

  constructor() {
    this.headers = new Map();
    this.status = 0;
    this.responseText = "";
    FakeXMLHttpRequest.instances.push(this);
  }

  open(method, endpoint, async) {
    this.method = method;
    this.endpoint = endpoint;
    this.async = async;
  }

  setRequestHeader(name, value) {
    this.headers.set(name.toLocaleLowerCase(), value);
  }

  send(body) {
    this.body = body;
    if (FakeXMLHttpRequest.next.throw) throw FakeXMLHttpRequest.next.throw;
    this.status = FakeXMLHttpRequest.next.status;
    this.responseText = FakeXMLHttpRequest.next.responseText;
  }
}

const nativeCalls = [];
const invoke = async (command, args) => {
  nativeCalls.push({ command, args });
  if (command === "plugin_sync_prepare_grant") {
    return { token: TOKEN, endpoint: ENDPOINT, role: args.role };
  }
  if (command === "plugin_sync_revoke_grant") return null;
  throw new Error(`Unexpected command ${command}`);
};

const transport = await createPluginSyncTransport({
  identifier: "io.iina.sync-test",
  role: "entry",
  invoke,
  XMLHttpRequestClass: FakeXMLHttpRequest,
});
assert.deepEqual(Object.keys(transport), ["invokeSync", "revoke"]);
assert.equal(JSON.stringify(transport), "{}", "the opaque grant must not be an enumerable capability");
assert.ok(!transport.invokeSync.toString().includes(TOKEN), "function source must not reveal the grant");
assert.equal(nativeCalls[0].command, "plugin_sync_prepare_grant");
assert.deepEqual(nativeCalls[0].args, { identifier: "io.iina.sync-test", role: "entry" });

const value = transport.invokeSync("file.exists", { path: "@data/example.txt" });
assert.equal(value, 7);
assert.equal(value instanceof Promise, false, "reference synchronous methods must not return Promises");
const request = FakeXMLHttpRequest.instances.at(-1);
assert.equal(request.method, "POST");
assert.equal(request.endpoint, ENDPOINT);
assert.equal(request.async, false, "the custom-scheme request must be synchronous");
assert.equal(request.headers.get("content-type"), "application/json");
assert.deepEqual(JSON.parse(request.body), {
  grant: TOKEN,
  method: "file.exists",
  args: { path: "@data/example.txt" },
});

FakeXMLHttpRequest.next = {
  status: 200,
  responseText: '{"ok":false,"error":"Cannot read file: denied"}',
};
assert.throws(
  () => transport.invokeSync("file.read", { path: "/denied" }),
  /Cannot read file: denied/,
  "native operation failures must synchronously become JavaScript exceptions",
);

FakeXMLHttpRequest.next = {
  status: 403,
  responseText: '{"ok":false,"error":"Plugin synchronization origin is not allowed"}',
};
assert.throws(
  () => transport.invokeSync("file.exists", { path: "@data/example.txt" }),
  /origin is not allowed/,
);

await assert.rejects(
  createPluginSyncTransport({
    identifier: "io.iina.bad-grant",
    role: "global",
    invoke: async () => ({ token: "predictable", endpoint: ENDPOINT, role: "global" }),
    XMLHttpRequestClass: FakeXMLHttpRequest,
  }),
  /authorization is invalid/,
  "short or non-random-looking grants must fail closed in the parent realm",
);

await transport.revoke();
await transport.revoke();
assert.equal(
  nativeCalls.filter(({ command }) => command === "plugin_sync_revoke_grant").length,
  1,
  "grant revocation must be idempotent",
);
assert.deepEqual(nativeCalls.at(-1).args, {
  identifier: "io.iina.sync-test",
  grant: TOKEN,
});
assert.throws(
  () => transport.invokeSync("file.exists", { path: "@data/example.txt" }),
  /has been revoked/,
);

assert.equal(pluginSyncProtocolLimit, 64 * 1024 * 1024);
assert.equal(pluginFileHandleReadValue(null), null, "reading a write handle must return nil");
assert.deepEqual(
  Array.from(pluginFileHandleReadValue([0, 127, 255])),
  [0, 127, 255],
  "reading a read handle must return a Uint8Array",
);
let realmArrayFactoryCalls = 0;
const realmArray = pluginFileHandleReadValue([1, 2], (bytes) => {
  realmArrayFactoryCalls += 1;
  return { realmBytes: Array.from(bytes) };
});
assert.deepEqual(realmArray, { realmBytes: [1, 2] });
assert.equal(realmArrayFactoryCalls, 1, "file handles must construct typed arrays in the plugin realm");

let permissionOperationCalls = 0;
const deniedPermission = () => false;
const deniedResolve = withPluginFileSystemPermission(deniedPermission, null, () => {
  permissionOperationCalls += 1;
  return "/must-not-run";
});
const deniedExec = withPluginFileSystemPermission(deniedPermission, null, () => {
  permissionOperationCalls += 1;
  return Promise.resolve({ status: 0 });
});
assert.equal(deniedResolve, null, "resolvePath must return nil without file-system permission");
assert.equal(deniedExec, null, "exec must return nil, not a Promise, without file-system permission");
assert.equal(permissionOperationCalls, 0, "denied utilities must not invoke the native backend");
const permittedExec = Promise.resolve({ status: 0 });
assert.equal(
  withPluginFileSystemPermission(() => true, null, () => permittedExec),
  permittedExec,
  "permitted exec must retain its reference Promise surface",
);

assert.equal(
  pluginPathForApi("@data/private.json"),
  "@data/private.json",
  "private paths bypass file-system permission in every realm",
);
assert.equal(pluginPathForApi("@data/"), "@data/");
assert.equal(pluginPathForApi("@tmp/"), "@tmp/");
assert.throws(
  () => pluginPathForApi("@data"),
  (error) => error.message === pluginFileSystemPermissionError,
  "IINA only recognizes private magic paths with the trailing slash prefix",
);
assert.throws(
  () => pluginPathForApi("@data", { hasFileSystemPermission: true }),
  /The path should be an absolute path: @data/,
  "an exact private-root token remains an ordinary relative path after permission",
);
for (const path of ["@tmp", "@datax/file", "@tmpx/file"]) {
  assert.throws(
    () => pluginPathForApi(path),
    (error) => error.message === pluginFileSystemPermissionError,
    `${path} must not be confused with an IINA private-path prefix`,
  );
}
assert.throws(
  () => pluginPathForApi("@tmp", { hasFileSystemPermission: true }),
  /The path should be an absolute path: @tmp/,
);
assert.equal(
  pluginPathForApi("@subtitle/7", { playerAvailable: true }),
  "@subtitle/7",
  "the player realm preserves IINA's broad @sub track prefix",
);
assert.throws(
  () => pluginPathForApi("/Users/example/movie.mp4"),
  (error) => error.message === pluginFileSystemPermissionError,
  "File API external paths must synchronously throw IINA's exact permission exception",
);
assert.throws(
  () => pluginPathForApi("@sub/7", { playerAvailable: false }),
  (error) => error.message === pluginFileSystemPermissionError,
  "a global controller has no player-track permission exemption",
);
assert.throws(
  () => pluginPathForApi("@sub/7", {
    hasFileSystemPermission: true,
    playerAvailable: false,
  }),
  /The path should be an absolute path: @sub\/7/,
  "a permitted global controller still treats track magic as an ordinary relative path",
);
assert.throws(
  () => pluginPathForApi("@current/sidecar.srt", {
    hasFileSystemPermission: true,
    playerAvailable: false,
  }),
  /The path should be an absolute path: @current\/sidecar\.srt/,
  "@current expansion is player-only",
);
assert.equal(
  pluginPathForApi("@current/sidecar.srt", {
    hasFileSystemPermission: true,
    playerAvailable: true,
  }),
  "@current/sidecar.srt",
);
assert.throws(
  () => pluginPathForApi("https://example.com/media.mp4", { hasFileSystemPermission: true }),
  /The path should be an absolute path/,
  "normal File/Utils path parsing does not accept URLs",
);
assert.equal(
  pluginPathForApi("https://example.com/media.mp4", {
    hasFileSystemPermission: true,
    forceLocalPath: false,
  }),
  "https://example.com/media.mp4",
  "core.open keeps forceLocalPath=false after the same permission gate",
);

assert.equal(pluginWebSocketPort(65_535), 65_535);
assert.throws(() => pluginWebSocketPort(0), /port not specified/);
assert.throws(() => pluginWebSocketPort(12.5), /port not specified/);
const websocketObjectHandler = { call: "accepted like JavaScriptCore JSValue.isObject" };
assert.equal(pluginWebSocketHandlerValue(null, websocketObjectHandler), websocketObjectHandler);
assert.equal(pluginWebSocketHandlerValue(null, () => {}).__proto__, Function.prototype);
assert.equal(pluginWebSocketHandlerValue(null, 7), null, "primitive handlers are ignored");
assert.equal(
  pluginWebSocketHandlerValue(websocketObjectHandler, () => {}),
  null,
  "installing a second handler removes the managed reference",
);

const backend = readFileSync(new URL("../src-tauri/src/plugin_sync.rs", import.meta.url), "utf8");
const pluginPathsBackend = readFileSync(new URL("../src-tauri/src/plugins.rs", import.meta.url), "utf8");
for (const contract of [
  'pub const PLUGIN_SYNC_SCHEME: &str = "iima-plugin-sync"',
  'File::open("/dev/urandom")',
  "let mut bytes = [0_u8; 32]",
  "owner_webview_label",
  "GRANT_IDLE_LIFETIME",
  "MAX_PROTOCOL_BYTES: usize = 64 * 1024 * 1024",
  "MAX_JSON_STRING_BYTES: usize = 8 * 1024 * 1024",
  "request.method() == Method::OPTIONS",
  'header("Access-Control-Allow-Methods", "POST")',
  'header("Access-Control-Allow-Headers", "Content-Type")',
  'header(header::VARY, "Origin")',
  'header(header::CACHE_CONTROL, "no-store")',
  'header("X-Content-Type-Options", "nosniff")',
  "cfg!(debug_assertions) && origin == DEVELOPMENT_ORIGIN",
  "cleanup_plugin_file_handles_for_identifier",
  "cleanup_plugin_file_handle_tokens",
  "file_handle_tokens: BTreeSet<String>",
  "role: String",
  "require_file_handle(grant_token, &token)",
  "cleanup_grant_file_handles(&expired)",
  '"ws.createserver"',
  '"ws.startserver"',
]) {
  assert.ok(backend.includes(contract), `Missing synchronous bridge safety contract: ${contract}`);
}

const frontend = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
for (const contract of [
  "createPluginSyncTransport,",
  '} from "./plugin-sync.js"',
  'syncTransports: { entry: null, global: null }',
  'runtime.syncTransports.entry = await createPluginRoleSyncTransport(runtime, "entry")',
  'runtime.syncTransports.global = await createPluginRoleSyncTransport(runtime, "global")',
  "const invokeSync = syncTransport.invokeSync",
  'invokeSync("file.handle.open"',
  "pluginRealm.createUint8Array(bytes)",
  'call("file.handle.readtoend")',
  'invokeSync("utils.fileinpath"',
  'invokeSync("utils.keychainread"',
  "pluginPathForApi(value, {",
  'globalRole !== "controller"',
  'invokeSync("standalone.isopen")',
  "ws: createPluginWebSocketApi(runtime, invokeSync, role)",
  'invokeSync("ws.createserver", { port })',
  'invokeSync("ws.startserver")',
  "await entrySyncTransport.revoke()",
]) {
  assert.ok(frontend.includes(contract), `Missing synchronous frontend API contract: ${contract}`);
}
assert.ok(pluginPathsBackend.includes('"The path should be an absolute path: {raw_path}"'));
assert.ok(backend.includes('error == "@current is unavailable without a local media file"'));
const fileApiSource = frontend.slice(
  frontend.indexOf("function createPluginFileApi("),
  frontend.indexOf("const pluginSubtitleItemStates"),
);
assert.ok(!fileApiSource.includes("move:"), "JavascriptAPIFileExportable does not export move");
assert.ok(!backend.includes('"file.move"'), "the private sync grant must not expose an unexported route");

console.log("Plugin synchronous transport and API-shape checks passed");
