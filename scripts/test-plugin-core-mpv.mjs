import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { decodePluginMpvValue, encodePluginMpvValue } from "../src/plugin-mpv-values.js";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const frontend = read("../src/main.js");
const mpv = read("../src-tauri/src/mpv.rs");
const player = read("../src-tauri/src/player.rs");
const sync = read("../src-tauri/src/plugin_sync.rs");
const about = read("../src-tauri/src/about_window.rs");

assert.deepEqual(encodePluginMpvValue(true), { type: "flag", value: true });
assert.deepEqual(encodePluginMpvValue(Number.POSITIVE_INFINITY), { type: "double", value: "Infinity" });
assert.deepEqual(encodePluginMpvValue({ nested: [1, null, "x"] }), {
  type: "map",
  value: {
    nested: {
      type: "array",
      value: [
        { type: "double", value: "1" },
        { type: "null" },
        { type: "string", value: "x" },
      ],
    },
  },
});
assert.equal(decodePluginMpvValue({ type: "double", value: "-Infinity" }), Number.NEGATIVE_INFINITY);
assert.deepEqual(decodePluginMpvValue({ type: "byte-array", value: [0, 127, 255] }), [0, 127, 255]);
assert.throws(
  () => encodePluginMpvValue(undefined),
  /mpv\.set only supports numbers, strings, booleans and objects\./,
);
const cyclic = {};
cyclic.self = cyclic;
assert.throws(() => encodePluginMpvValue(cyclic), /cyclic object/);

for (const contract of [
  "function createPluginCoreApi(runtime, requirePermission, invokeSync, applyPlayerCommand)",
  'type: "seek-relative"',
  'type: "seek-absolute"',
  'type: "select-chapter"',
  "formattedTitie",
  'invokeSync("core.history"',
  'invokeSync("core.resolveopen"',
  'includes("@current is unavailable")',
  'invokeSync("core.version"',
  'invokeSync("core.window.snapshot"',
  'invokeSync("core.window.setframe"',
  'invokeSync("mpv.get"',
  'invokeSync("mpv.set"',
  'invokeSync("mpv.command"',
  'if (globalRole !== "controller")',
  "Trying to get preference value for undefined key ${key}",
  'throw new Error(`To call this API, the plugin must declare permission "${permission}" in its Info.json.`)',
  'detail: `From plugin ${runtime.spec.name}`',
]) {
  assert.ok(frontend.includes(contract), `Missing Core/mpv frontend contract: ${contract}`);
}

const apiFactory = frontend.slice(
  frontend.indexOf("function createIinaPluginApi("),
  frontend.indexOf("function createPluginCoreApi("),
);
const commonSurface = apiFactory.slice(0, apiFactory.indexOf('if (globalRole !== "controller")'));
for (const playerOnly of ["core:", "mpv:", "event:", "input:", "overlay:", "sidebar:", "playlist:", "subtitle:"]) {
  assert.ok(!commonSurface.includes(playerOnly), `Global controller leaked player-only API: ${playerOnly}`);
}
for (const common of ["console:", "menu:", "standaloneWindow:", "preferences:", "utils:", "http:", "file:", "ws:"]) {
  assert.ok(commonSurface.includes(common), `Global controller is missing common API: ${common}`);
}

for (const contract of [
  "pub enum MpvPluginValue",
  "ByteArray(Vec<u8>)",
  "SetPropertyNode",
  "decode_plugin_mpv_node",
  "plugin_property",
  "set_property_node",
]) {
  assert.ok(mpv.includes(contract), `Missing typed libmpv contract: ${contract}`);
}
assert.ok(player.includes("PluginMpvSetNative"));
assert.ok(player.includes("SeekAbsolute"));
assert.ok(player.includes("ExternalTrackKind::Video"));
for (const route of ["core.resolveopen", "core.version", "core.history", "core.window.snapshot", "core.window.setframe", "mpv.get", "mpv.set", "mpv.command"]) {
  assert.ok(sync.includes(`"${route}"`), `Missing synchronous plugin route: ${route}`);
}
assert.ok(about.includes('pub(crate) const IINA_VERSION: &str = "0.9.0";'));
assert.ok(about.includes('pub(crate) const IINA_BUILD: &str = "90";'));
assert.ok(sync.includes('"iina": crate::about_window::IINA_VERSION'));
assert.ok(sync.includes('"build": crate::about_window::IINA_BUILD'));

console.log("Plugin Core/mpv fidelity checks passed");
