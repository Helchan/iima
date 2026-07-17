import assert from "node:assert/strict";
import {
  DEFAULT_DOUBLE_CLICK_INTERVAL_MS,
  MOUSE_CLICK_ACTION_OPTIONS,
  MouseClickAction,
  dispatchMouseClickAction,
  normalizeMouseClickAction,
} from "../src/mouse-actions.js";

assert.equal(DEFAULT_DOUBLE_CLICK_INTERVAL_MS, 500);
assert.equal(normalizeMouseClickAction("4"), MouseClickAction.togglePip);
assert.equal(normalizeMouseClickAction(2), MouseClickAction.pause);
assert.equal(normalizeMouseClickAction(-1), MouseClickAction.none);
assert.equal(normalizeMouseClickAction(5), MouseClickAction.none);
assert.equal(normalizeMouseClickAction(1.5), MouseClickAction.none);

const optionValues = (options) => options.map(([value, title]) => [value, title]);
const optionContext = (group, title) => MOUSE_CLICK_ACTION_OPTIONS[group]
  .find(([, candidate]) => candidate === title)?.[2];

assert.deepEqual(optionValues(MOUSE_CLICK_ACTION_OPTIONS.singleClickAction), [
  [3, "Hide OSC"],
  [2, "Pause / Resume"],
  [0, "None"],
]);
assert.deepEqual(optionValues(MOUSE_CLICK_ACTION_OPTIONS.doubleClickAction), [
  [1, "Toggle fullscreen"],
  [2, "Pause / Resume"],
  [4, "Toggle Picture-in-Picture"],
  [0, "None"],
]);
assert.deepEqual(optionValues(MOUSE_CLICK_ACTION_OPTIONS.rightClickAction), [
  [3, "Hide OSC"],
  [2, "Pause / Resume"],
  [4, "Toggle Picture-in-Picture"],
  [0, "None"],
]);
assert.deepEqual(optionValues(MOUSE_CLICK_ACTION_OPTIONS.middleClickAction), [
  [3, "Hide OSC"],
  [2, "Pause / Resume"],
  [1, "Toggle fullscreen"],
  [4, "Toggle Picture-in-Picture"],
  [0, "None"],
]);
assert.deepEqual(optionValues(MOUSE_CLICK_ACTION_OPTIONS.forceTouchAction), [
  [3, "Hide OSC"],
  [2, "Pause / Resume"],
  [1, "Toggle fullscreen"],
  [0, "None"],
]);

assert.deepEqual(optionContext("singleClickAction", "Pause / Resume"), {
  table: "PrefControlViewController",
  key: "A8c-wa-IFR.title",
});
assert.deepEqual(optionContext("doubleClickAction", "Pause / Resume"), {
  table: "PrefControlViewController",
  key: "Dm7-JG-fLd.title",
});
assert.deepEqual(optionContext("rightClickAction", "Toggle Picture-in-Picture"), {
  table: "PrefControlViewController",
  key: "ibM-4r-SQA.title",
});
assert.deepEqual(optionContext("middleClickAction", "None"), {
  table: "PrefControlViewController",
  key: "unj-zt-RyU.title",
});
assert.deepEqual(optionContext("forceTouchAction", "Toggle fullscreen"), {
  table: "PrefControlViewController",
  key: "hDh-Lk-FJe.title",
});

const calls = [];
const handlers = {
  fullscreen: () => calls.push("fullscreen"),
  pause: () => calls.push("pause"),
  hideOsc: () => calls.push("hideOsc"),
  togglePip: () => calls.push("togglePip"),
};

assert.equal(await dispatchMouseClickAction(MouseClickAction.none, handlers), false);
assert.equal(await dispatchMouseClickAction(MouseClickAction.fullscreen, handlers), true);
assert.equal(await dispatchMouseClickAction(MouseClickAction.pause, handlers), true);
assert.equal(await dispatchMouseClickAction(MouseClickAction.hideOsc, handlers), true);
assert.equal(await dispatchMouseClickAction(MouseClickAction.togglePip, handlers), true);
assert.deepEqual(calls, ["fullscreen", "pause", "hideOsc", "togglePip"]);

console.log("Mouse action contracts pass");
