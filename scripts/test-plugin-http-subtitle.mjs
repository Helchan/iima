import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

import { xmlRpcDecodeResponse, xmlRpcEncodeCall } from "../src/plugin-xmlrpc.js";
import { pluginPathForApi } from "../src/plugin-sync.js";

const frontend = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
const backend = readFileSync(new URL("../src-tauri/src/commands.rs", import.meta.url), "utf8");

function sourceSection(start, end) {
  const startIndex = frontend.indexOf(start);
  const endIndex = frontend.indexOf(end, startIndex);
  assert.ok(startIndex >= 0 && endIndex > startIndex, `Unable to find production section ${start}`);
  return frontend.slice(startIndex, endIndex);
}

function plain(value) {
  return value === undefined ? undefined : JSON.parse(JSON.stringify(value));
}

const calls = [];
const results = [];
const syncCalls = [];
const invokeSync = (method, args) => {
  syncCalls.push({ method, args });
  if (args.path === "@sub/not-a-track") throw new Error("The path @sub/not-a-track is invalid");
  if (args.path === "@current/output.srt") {
    throw new Error("@current is unavailable without a local media file");
  }
  return args.path;
};
const invoke = (command, args) => {
  calls.push({ command, args });
  assert.ok(results.length, `No queued result for ${command}`);
  const value = results.shift();
  return value instanceof Error ? Promise.reject(value) : Promise.resolve(value);
};
const httpContext = vm.createContext({
  URL,
  console,
  invoke,
  pluginPathForApi,
  xmlRpcDecodeResponse,
  xmlRpcEncodeCall,
});
vm.runInContext(`${sourceSection("function createPluginHttpApi", "function createPluginWebSocketApi")}
globalThis.contract = {
  createPluginHttpApi,
  pluginHttpResponse,
  pluginHttpResponseIsOk,
  pluginHttpValidatedUrl,
  pluginHttpDownloadMethod,
};`, httpContext);

const runtime = {
  spec: {
    identifier: "io.iina.http-test",
    allowed_domains: ["api.example.com", "*.cdn.example.com"],
  },
};
let permitted = false;
let fileSystemPermitted = false;
const hasPermission = (permission) => permission === "network-request" ? permitted : fileSystemPermitted;
const http = httpContext.contract.createPluginHttpApi(runtime, hasPermission, true, invokeSync);

assert.throws(
  () => http.get("https://api.example.com/value"),
  /must declare permission "network-request"/,
  "network permission failures must be synchronous and must not manufacture a Promise",
);
assert.equal(calls.length, 0);
assert.doesNotThrow(() => http.xmlrpc("https://api.example.com/RPC2"), "xmlrpc creation has no permission gate in IINA 1.3.5");

permitted = true;
assert.throws(() => http.get("not a URL"), /URL not a URL is invalid\./);
assert.throws(() => http.get("https://denied.example/value"), /is not allowed\./);
assert.throws(() => http.download("https://api.example.com/file", "@tmp/file", { method: "get" }), /method is invalid\./);
assert.equal(calls.length, 0);
assert.equal(syncCalls.length, 0, "method validation precedes destination parsing");

assert.throws(
  () => http.download("https://api.example.com/file", "/tmp/external.bin"),
  /must declare permission "file-system"/,
  "an external destination fails synchronously before Promise creation",
);
assert.equal(calls.length, 0);
assert.equal(syncCalls.length, 0);
assert.throws(
  () => http.download("https://api.example.com/file", "@sub/not-a-track"),
  /The path @sub\/not-a-track is invalid/,
  "a player-track destination is permission-exempt but synchronously parsed",
);
assert.equal(calls.length, 0);
fileSystemPermitted = true;
assert.throws(
  () => http.download("https://api.example.com/file", "@current/output.srt"),
  /Not allowed to write to the destination\./,
  "an unavailable @current destination fails before Promise creation",
);
assert.equal(calls.length, 0);

const globalHttp = httpContext.contract.createPluginHttpApi(runtime, hasPermission, false, invokeSync);
assert.throws(
  () => globalHttp.download("https://api.example.com/file", "@sub/7"),
  /The path should be an absolute path: @sub\/7/,
  "the global controller cannot borrow the player realm's track resolver",
);
assert.equal(calls.length, 0);
fileSystemPermitted = false;

results.push({ statusCode: 200, reason: "ok", data: { value: 1 }, text: "{\"value\":1}" });
assert.deepEqual(
  plain(await http.get("https://api.example.com/value")),
  { statusCode: 200, reason: "ok", data: { value: 1 }, text: "{\"value\":1}" },
);
assert.equal(calls.at(-1).args.permissionRequired, true);
assert.deepEqual(Object.keys(await Promise.resolve(httpContext.contract.pluginHttpResponse({}))), [
  "statusCode",
  "reason",
  "data",
  "text",
]);

results.push({ status_code: 404, reason: "not found", data: { error: true }, text: "missing" });
await assert.rejects(
  http.get("https://api.example.com/missing"),
  (error) => {
    assert.deepEqual(plain(error), {
      statusCode: 404,
      reason: "not found",
      data: { error: true },
      text: "missing",
    });
    return true;
  },
);

results.push({ statusCode: 302, reason: "found", data: null, text: "" });
assert.equal((await http.get("https://a.cdn.example.com/redirect")).statusCode, 302);

results.push({
  destination: "/tmp/plugin.bin",
  response: { statusCode: 200, reason: "ok", data: null, text: null },
});
assert.equal(
  await http.download("https://api.example.com/file", "@tmp/file", { method: "GET" }),
  undefined,
  "download success must resolve with JavaScript undefined",
);
assert.deepEqual(plain(syncCalls.at(-1)), {
  method: "core.resolveopen",
  args: { path: "@tmp/file" },
}, "@tmp destinations are synchronously accepted without file-system permission");

results.push({
  destination: "/tmp/plugin-data.bin",
  response: { statusCode: 200, reason: "ok", data: null, text: null },
});
assert.equal(
  await http.download("https://api.example.com/file", "@data/file", { method: "GET" }),
  undefined,
  "@data destinations also bypass file-system permission",
);
assert.equal(syncCalls.at(-1).args.path, "@data/file");

results.push({
  destination: null,
  response: { statusCode: 503, reason: "service unavailable", data: null, text: "retry" },
});
await assert.rejects(
  http.download("https://api.example.com/file", "@tmp/file"),
  (error) => error.statusCode === 503 && error.reason === "service unavailable",
);

const xmlrpc = http.xmlrpc("https://api.example.com/RPC2");
permitted = false;
results.push({
  statusCode: 200,
  reason: "ok",
  data: null,
  text: "<methodResponse><params><param><value><string>IINA</string></value></param></params></methodResponse>",
});
assert.equal(await xmlrpc.call("fixture.echo", ["IINA"]), "IINA");
assert.equal(calls.at(-1).command, "plugin_http_request");
assert.equal(calls.at(-1).args.permissionRequired, false, "XML-RPC bypasses the network-request permission gate");
assert.match(calls.at(-1).args.options.data, /<methodName>fixture\.echo<\/methodName>/);

results.push({
  statusCode: 200,
  reason: "ok",
  data: null,
  text: "<methodResponse><fault><value><string>denied</string></value></fault></methodResponse>",
});
try {
  await xmlrpc.call("fixture.fault", []);
  assert.fail("XML-RPC fault should reject");
} catch (error) {
  assert.equal(error, undefined, "XML-RPC fault rejection has no argument in the reference API");
}

results.push({ statusCode: 200, reason: "ok", data: null, text: "not XML" });
await assert.rejects(
  xmlrpc.call("fixture.bad", []),
  (error) => {
    assert.deepEqual(plain(error), {
      httpCode: 200,
      reason: "Bad response",
      description: "fixture.bad: [200] Bad response",
    });
    return true;
  },
);

results.push({ statusCode: 503, reason: "service unavailable", data: null, text: "" });
await assert.rejects(
  xmlrpc.call("fixture.down", []),
  (error) => {
    assert.deepEqual(plain(error), {
      httpCode: 503,
      reason: "service unavailable",
      description: "fixture.down: [503] service unavailable",
    });
    return true;
  },
);

const subtitleContext = vm.createContext({ console });
vm.runInContext(`${sourceSection("const pluginSubtitleItemStates", "function readPluginPreferenceValues")}
globalThis.contract = {
  createPluginSubtitleApi,
  createPluginSubtitleItem,
  downloadPluginSubtitleItem,
  pluginSubtitleDownloadUrlsForNativeBoundary,
};`, subtitleContext);
subtitleContext.createPluginConsole = () => ({ log: () => {} });
const subtitleRuntime = {
  spec: { identifier: "io.iina.subtitle-test" },
  subtitleProviders: new Map(),
};
const subtitle = subtitleContext.contract.createPluginSubtitleApi(subtitleRuntime);

assert.equal(subtitle.CUSTOM_IMPLEMENTATION, "custom-implementation");
assert.throws(() => subtitle.registerProvider(7, {}), /A subtitle provider should have an id\./);
subtitle.registerProvider("", { search: async () => [], download: async () => [] });
assert.ok(Object.prototype.hasOwnProperty.call(subtitle.__providers, ""), "the reference accepts an empty string id without coercion");

function invokeSearch(id) {
  return new Promise((resolve, reject) => {
    subtitle.__invokeSearch(id, resolve, (error) => reject(new Error(String(error))));
  });
}

await assert.rejects(invokeSearch("missing"), /The provider with id "missing" is not registered\./);
subtitle.registerProvider("incomplete", {});
await assert.rejects(invokeSearch("incomplete"), /provider\.search doesn't exist or is not an async function\./);

const item = subtitle.item({ id: 1 });
assert.deepEqual(plain(item.data), { id: 1 });
assert.equal(item.desc, undefined);
item.desc = { name: "Managed" };
assert.deepEqual(plain(item.desc), { name: "Managed" });
assert.equal(Reflect.set(item, "data", { id: 2 }), false, "subtitle item data remains read-only");

subtitle.registerProvider("managed", {
  search: async () => [item],
  description: () => ({ name: "Ignored because desc is already set" }),
  download: async (subtitleItem) => [subtitleItem.data.id, "/tmp/managed.srt"],
});
const managed = await invokeSearch("managed");
assert.equal(managed[0], item);
assert.equal(item.desc.name, "Managed");
assert.deepEqual(
  plain(await subtitleContext.contract.downloadPluginSubtitleItem(item)),
  [1, "/tmp/managed.srt"],
  "the JavaScript callback checks Array.isArray only; string coercion belongs to the native URL boundary",
);
assert.throws(
  () => subtitleContext.contract.pluginSubtitleDownloadUrlsForNativeBoundary([1, "/tmp/managed.srt"]),
  /provider\.download should return an array of strings\./,
  "the native integration boundary rejects non-string array members with IINA's exact message",
);
assert.deepEqual(
  plain(subtitleContext.contract.pluginSubtitleDownloadUrlsForNativeBoundary(["@tmp/managed.srt"])),
  ["@tmp/managed.srt"],
);

const described = subtitle.item({ id: 2 });
subtitle.registerProvider("description", {
  search: async () => [described],
  description: (subtitleItem) => ({ name: `Subtitle ${subtitleItem.data.id}`, left: "provider" }),
  download: async () => ["/tmp/described.srt"],
});
await invokeSearch("description");
assert.deepEqual(plain(described.desc), { name: "Subtitle 2", left: "provider" });

subtitle.registerProvider("custom", {
  search: async () => subtitle.CUSTOM_IMPLEMENTATION,
  download: async () => [],
});
assert.equal(await invokeSearch("custom"), null, "CUSTOM_IMPLEMENTATION completes with null instead of failing");

subtitle.registerProvider("bad-search", {
  search: async () => "not-an-array",
  download: async () => [],
});
await assert.rejects(invokeSearch("bad-search"), /provider\.search should return an array of subtitle items\./);

const badDownload = subtitle.item({ id: 3 });
subtitle.registerProvider("bad-download", {
  search: async () => [badDownload],
  download: async () => "not-an-array",
});
await invokeSearch("bad-download");
await assert.rejects(
  subtitleContext.contract.downloadPluginSubtitleItem(badDownload),
  /provider\.download should return an array of strings\./,
);

for (const contract of [
  "if (result === null)",
  "api.__invokeSearch(",
  "pluginSubtitleItemStates.has(item)",
  "pluginSubtitleDownloadUrlsForNativeBoundary(downloaded)",
]) {
  assert.ok(frontend.includes(contract), `Missing plugin subtitle integration contract: ${contract}`);
}
for (const contract of [
  '#[serde(rename_all = "camelCase")]',
  "plugin_http_response_is_ok",
  "split_plugin_http_write_out",
  'Some("service unavailable")',
]) {
  assert.ok(backend.includes(contract), `Missing native HTTP result contract: ${contract}`);
}

console.log("Plugin HTTP, XML-RPC, and subtitle-provider compatibility checks passed");
