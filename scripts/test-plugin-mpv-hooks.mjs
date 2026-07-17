import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { invokePluginMpvHook } from "../src/plugin-mpv-hooks.js";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const frontend = read("../src/main.js");
const hooks = read("../src-tauri/src/plugin_mpv_hooks.rs");
const mpv = read("../src-tauri/src/mpv.rs");
const library = read("../src-tauri/src/lib.rs");

for (const command of ["plugin_mpv_add_hook", "plugin_mpv_continue_hook", "plugin_mpv_remove_hooks"]) {
  assert.ok(hooks.includes(`pub fn ${command}(`), `Missing backend command: ${command}`);
  assert.ok(library.includes(`${command},`), `Backend command is not registered: ${command}`);
}
for (const contract of [
  '"mpv_hook_add"',
  '"mpv_hook_continue"',
  "MpvEventHook",
  "take_pending_hook_events",
]) {
  assert.ok(mpv.includes(contract), `Missing libmpv hook contract: ${contract}`);
}
for (const contract of [
  "addHook: (name, priority, callback)",
  'tauriListen("iima-plugin-mpv-hook"',
  'invoke("plugin_mpv_remove_hooks"',
]) {
  assert.ok(frontend.includes(contract), `Missing frontend hook contract: ${contract}`);
}
assert.ok(hooks.includes("executor.continue_hook(hook_id)"));
assert.ok(hooks.includes("pub fn stop_identifier(state: &AppState, identifier: &str)"));
assert.ok(hooks.includes("pub fn stop_window(state: &AppState, window_label: &str)"));
assert.ok(hooks.includes("pub fn stop_all(state: &AppState)"));

const payload = { identifier: "io.iina.test", callbackId: 1, hookId: 9, name: "on_load" };

{
  let continuations = 0;
  invokePluginMpvHook(() => {}, payload, () => { continuations += 1; });
  assert.equal(continuations, 1, "a synchronous hook auto-continues after returning");
}

{
  let continuations = 0;
  invokePluginMpvHook((next) => {
    next();
    next();
  }, payload, () => { continuations += 1; });
  assert.equal(continuations, 1, "next and synchronous auto-continue share a once-only gate");
}

{
  let continuations = 0;
  invokePluginMpvHook(async () => {}, payload, () => { continuations += 1; });
  await Promise.resolve();
  assert.equal(continuations, 0, "an async hook is not continued implicitly");
}

{
  let continuations = 0;
  invokePluginMpvHook(async (next) => {
    await Promise.resolve();
    next();
    next();
  }, payload, () => { continuations += 1; });
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(continuations, 1, "an async hook continues exactly once when it calls next");
}

{
  let continuations = 0;
  let callbackErrors = 0;
  invokePluginMpvHook(
    () => { throw new Error("boom"); },
    payload,
    () => { continuations += 1; },
    (phase) => { if (phase === "callback") callbackErrors += 1; },
  );
  assert.equal(callbackErrors, 1);
  assert.equal(continuations, 1, "a throwing synchronous hook still releases libmpv");
}

console.log("Plugin mpv hook callback checks passed");
