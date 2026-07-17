import assert from "node:assert/strict";
import {
  consumePluginMpvEventBatch,
  normalizePluginEventName,
  pluginChangedMpvProperty,
  pluginMpvPropertyEventValue,
} from "../src/plugin-events.js";

assert.equal(normalizePluginEventName("mpv.seek"), "mpv.seek");
assert.equal(normalizePluginEventName("iina.window-fs.changed"), "iina.window-fs.changed");
assert.equal(
  normalizePluginEventName("mpv.video-params/primaries.changed"),
  "mpv.video-params/primaries.changed",
);
for (const invalid of ["seek", "other.seek", "mpv.seek.extra", "iina..changed", "mpv.seek.changed.more"]) {
  assert.throws(() => normalizePluginEventName(invalid), /Incorrect event name syntax/);
}
assert.equal(pluginChangedMpvProperty("mpv.playlist-pos.changed"), "playlist-pos");
assert.equal(pluginChangedMpvProperty("iina.window-main.changed"), null);
assert.equal(pluginChangedMpvProperty("mpv.seek"), null);

assert.equal(pluginMpvPropertyEventValue({ format: "flag", value: "true" }), true);
assert.equal(pluginMpvPropertyEventValue({ format: "flag", value: "false" }), false);
assert.equal(pluginMpvPropertyEventValue({ format: "int64", value: "7" }), 7);
assert.equal(pluginMpvPropertyEventValue({ format: "double", value: "1.25" }), 1.25);
assert.equal(pluginMpvPropertyEventValue({ format: "string", value: "auto" }), "auto");
assert.equal(pluginMpvPropertyEventValue({ format: "none", value: null }), 0);

const record = (cursor, name, property = null) => ({
  cursor,
  event: {
    event_id: 0,
    name,
    error: 0,
    reply_userdata: 0,
    property,
    start_file: null,
    end_file: null,
    hook: null,
  },
});

const calls = [];
const next = consumePluginMpvEventBatch(
  { cursor: 0, path: null },
  {
    cursor: 7,
    events: [
      record(1, "property-change", { name: "path", format: "string", value: "file:///tmp/a.mp4" }),
      record(2, "start-file"),
      record(3, "seek"),
      record(4, "seek"),
      record(5, "property-change", { name: "pause", format: "flag", value: "true" }),
      record(6, "playback-restart"),
      record(7, "file-loaded"),
    ],
  },
  (name, ...args) => calls.push([name, ...args]),
  () => "file:///tmp/wrong.mp4",
);

assert.deepEqual(next, { cursor: 7, path: "file:///tmp/a.mp4" });
assert.deepEqual(calls, [
  ["mpv.path.changed", "file:///tmp/a.mp4"],
  ["mpv.property-change"],
  ["iina.file-started"],
  ["mpv.start-file"],
  ["mpv.seek"],
  ["mpv.seek"],
  ["mpv.pause.changed", true],
  ["mpv.property-change"],
  ["mpv.playback-restart"],
  ["iina.file-loaded", "file:///tmp/a.mp4"],
  ["mpv.file-loaded"],
]);

const replayCalls = [];
const replay = consumePluginMpvEventBatch(
  next,
  { cursor: 7, events: [record(6, "seek"), record(7, "file-loaded")] },
  (name, ...args) => replayCalls.push([name, ...args]),
);
assert.deepEqual(replay, next);
assert.deepEqual(replayCalls, []);

console.log("Plugin Event API compatibility checks passed");
