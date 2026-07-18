import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { classifyMpvKeyAction } from "../src/mpv-key-action.js";

assert.deepEqual(classifyMpvKeyAction("seek 12 exact"), {
  type: "seek-relative",
  seconds: 12,
  option: "exact",
});
assert.deepEqual(classifyMpvKeyAction("seek -7"), {
  type: "seek-relative",
  seconds: -7,
  option: "relative",
});
assert.deepEqual(classifyMpvKeyAction("seek 3 relative"), {
  type: "seek-relative",
  seconds: 3,
  option: "relative",
});
assert.deepEqual(classifyMpvKeyAction("seek 50 absolute-percent"), {
  type: "mpv-command",
  action: "seek 50 absolute-percent",
});

assert.deepEqual(classifyMpvKeyAction("add volume -5"), {
  type: "volume-relative",
  amount: -5,
});
assert.deepEqual(classifyMpvKeyAction("cycle pause"), {
  type: "player",
  command: { type: "toggle-pause" },
});
assert.deepEqual(classifyMpvKeyAction("cycle mute"), {
  type: "player",
  command: { type: "toggle-mute" },
});
assert.deepEqual(classifyMpvKeyAction("set pause no"), {
  type: "player",
  command: { type: "resume" },
});
assert.deepEqual(classifyMpvKeyAction("multiply speed 1.1"), {
  type: "player",
  command: { type: "multiply-speed", factor: 1.1 },
});
assert.deepEqual(classifyMpvKeyAction("set speed 1.25"), {
  type: "player",
  command: { type: "set-speed", speed: 1.25 },
});

for (const [rawAction, expected] of [
  ["frame-step", { type: "player", command: { type: "frame-step", backwards: false } }],
  ["frame-back-step", { type: "player", command: { type: "frame-step", backwards: true } }],
  ["playlist-next", { type: "player", command: { type: "playlist-next" } }],
  ["playlist-prev", { type: "player", command: { type: "playlist-prev" } }],
  ["ab-loop", { type: "player", command: { type: "cycle-ab-loop" } }],
  ["stop", { type: "player", command: { type: "stop" } }],
  ["quit", { type: "player", command: { type: "stop" } }],
  ["cycle video", { type: "player", command: { type: "cycle-track", kind: "video" } }],
  ["cycle audio", { type: "player", command: { type: "cycle-track", kind: "audio" } }],
  ["cycle sub", { type: "player", command: { type: "cycle-track", kind: "subtitles" } }],
  ["screenshot", { type: "screenshot" }],
  ["cycle fullscreen", { type: "fullscreen-toggle" }],
  ["set fullscreen no", { type: "fullscreen-set", fullscreen: false }],
]) {
  assert.deepEqual(classifyMpvKeyAction(rawAction), expected, rawAction);
}

assert.deepEqual(classifyMpvKeyAction("{default} seek 19 relative+exact"), {
  type: "seek-relative",
  seconds: 19,
  option: "exact",
});
assert.deepEqual(classifyMpvKeyAction("script-message fixture payload"), {
  type: "mpv-command",
  action: "script-message fixture payload",
});
assert.deepEqual(classifyMpvKeyAction("cycle   ontop"), {
  type: "mpv-command",
  action: "cycle   ontop",
});

const frontend = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
for (const contract of [
  "if (!isIINACommand) return classifyMpvKeyAction(action);",
  "applyMockMpvKeyAction(command.action);",
  ".then(() => executeKeyBinding(binding))",
  'type: "seek-relative", seconds: action.seconds, option: action.option',
  'type: "set-volume", volume: (Number(state.volume) || 0) + action.amount',
]) {
  assert.ok(frontend.includes(contract), `missing key-action integration contract: ${contract}`);
}

console.log("mpv key action classifier: ok");
