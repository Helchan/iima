import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const [html, css, runtime, backend, native, buildScript, mpv, library, menu, packageJson, zhHansSource] = await Promise.all([
  readFile(new URL("../src/inspector.html", import.meta.url), "utf8"),
  readFile(new URL("../src/inspector.css", import.meta.url), "utf8"),
  readFile(new URL("../src/inspector.js", import.meta.url), "utf8"),
  readFile(new URL("../src-tauri/src/inspector_window.rs", import.meta.url), "utf8"),
  readFile(new URL("../src-tauri/src/native_inspector.m", import.meta.url), "utf8"),
  readFile(new URL("../src-tauri/build.rs", import.meta.url), "utf8"),
  readFile(new URL("../src-tauri/src/mpv.rs", import.meta.url), "utf8"),
  readFile(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8"),
  readFile(new URL("../src-tauri/src/menu.rs", import.meta.url), "utf8"),
  readFile(new URL("../package.json", import.meta.url), "utf8"),
  readFile(new URL("../src/locales/zh-Hans.json", import.meta.url), "utf8"),
]);
const zhHans = JSON.parse(zhHansSource);

for (const [tab, key] of [
  ["General", "Fqo-1c-3L1.ibShadowedLabels[0]"],
  ["Tracks", "Fqo-1c-3L1.ibShadowedLabels[1]"],
  ["File", "Fqo-1c-3L1.ibShadowedLabels[2]"],
  ["Status", "Fqo-1c-3L1.ibShadowedLabels[3]"],
]) {
  assert.match(html, new RegExp(`data-i18n-key="${key.replaceAll("[", "\\[").replaceAll("]", "\\]")}"[^>]*>${tab}<`));
}
for (const field of [
  "video.format", "video.size", "video.codec", "video.colorspace", "video.pixelFormat",
  "audio.format", "audio.channels", "audio.codec", "audio.sampleRate",
]) assert.ok(html.includes(`data-general="${field}"`), field);
for (const field of ["id", "properties", "sourceId", "language", "filePath", "decoder", "sampleRate"]) {
  assert.ok(html.includes(`data-track="${field}"`), field);
}
for (const field of ["avSyncDifference", "totalAvSync", "droppedFrames", "mistimedFrames", "displayFps", "estimatedOutputFps", "estimatedDisplayFps"]) {
  assert.ok(html.includes(`data-status="${field}"`), field);
}

assert.match(css, /min-width:\s*350px/);
assert.match(css, /min-height:\s*430px/);
assert.match(runtime, /invoke\("get_inspector_snapshot"\)/);
assert.match(runtime, /invoke\("set_inspector_watch_properties", \{ properties \}\)/);
assert.match(runtime, /window\.setInterval\(refresh, 1000\)/);
assert.match(runtime, /trKey\("InspectorWindowController", "F0z-JX-Cv5\.title", "Inspector"\)/);
assert.doesNotMatch(runtime, /plugin_mpv_|execute_iina_command|player_command/);
assert.equal(zhHans.contexts["InspectorWindowController.strings:F0z-JX-Cv5.title"], "检查器");
assert.equal(zhHans.contexts["InspectorWindowController.strings:Fqo-1c-3L1.ibShadowedLabels[0]"], "通用");
assert.equal(zhHans.contexts["InspectorWindowController.strings:Fqo-1c-3L1.ibShadowedLabels[1]"], "轨道");
assert.equal(zhHans.contexts["InspectorWindowController.strings:Fqo-1c-3L1.ibShadowedLabels[2]"], "文件");
assert.equal(zhHans.contexts["InspectorWindowController.strings:Fqo-1c-3L1.ibShadowedLabels[3]"], "状态");

assert.match(backend, /INSPECTOR_WINDOW_LABEL: &str = "inspector"/);
assert.match(backend, /INSPECTOR_REFRESH_INTERVAL_MS: u64 = 1_000/);
assert.match(backend, /MAX_WATCH_PROPERTIES: usize = 32/);
assert.match(backend, /MAX_WATCH_PROPERTY_NAME_BYTES: usize = 96/);
assert.match(backend, /last_active_player_session_label\(\)/);
assert.match(backend, /native_video::status\(session\.label\(\)\)/);
assert.match(backend, /window\.label\(\) == INSPECTOR_WINDOW_LABEL/);
assert.match(backend, /preferences\.save_to_file/);
assert.match(backend, /read_string_properties\(requested_names\.iter\(\)\.copied\(\)\)/);
for (const contract of ["NSWindowStyleMaskUtilityWindow", "NSWindowStyleMaskHUDWindow", "window.hidesOnDeactivate = YES", "window.releasedWhenClosed = NO", "IINAInspectorPanel", "NSFloatingWindowLevel"]) {
  assert.ok(native.includes(contract), contract);
}
assert.match(buildScript, /\.file\("src\/native_inspector\.m"\)/);
assert.match(mpv, /pub\(crate\) fn read_string_properties/);
assert.match(mpv, /rather than a Tauri command/);
for (const command of ["show_inspector", "get_inspector_snapshot", "set_inspector_watch_properties"]) {
  assert.ok(library.includes(command), command);
}
assert.match(library, /label == inspector_window::INSPECTOR_WINDOW_LABEL[\s\S]*?api\.prevent_close\(\);[\s\S]*?window\.hide\(\)/);
assert.match(menu, /crate::inspector_window::show_inspector_window\(app\)/);
assert.match(packageJson, /"inspector:test":\s*"node scripts\/test-inspector\.mjs"/);

console.log("Inspector window contract checks passed");
