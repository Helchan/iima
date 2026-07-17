import assert from "node:assert/strict";

import {
  IinaScrollGestureState,
  NSEventPhase,
  PinchAction,
  ScrollAction,
  exceedsWindowDragThreshold,
  iinaScrollAmount,
  iinaScrollDelta,
  iinaScrollDirection,
  normalizePinchAction,
} from "../src/input-gestures.js";

const mouseVertical = { delta_x: 0, delta_y: -8, precise: false, natural: false, phase: 0 };
assert.equal(iinaScrollDirection(mouseVertical), "vertical");
assert.equal(iinaScrollDelta(mouseVertical, "vertical"), -2);
assert.equal(iinaScrollAmount(ScrollAction.seek, mouseVertical, "vertical", 3), -4);
assert.equal(iinaScrollAmount(ScrollAction.volume, mouseVertical, "vertical", 3), -2);

const mouseHorizontal = { delta_x: 8, delta_y: 20, precise: false, natural: false, phase: 0 };
assert.equal(iinaScrollDirection(mouseHorizontal), "horizontal");
assert.equal(iinaScrollDelta(mouseHorizontal, "horizontal"), -1);

const trackpadBegin = {
  delta_x: 0,
  delta_y: 1.5,
  precise: true,
  natural: true,
  phase: NSEventPhase.began,
};
assert.equal(iinaScrollDelta(trackpadBegin, "vertical"), -1.5);
assert.equal(iinaScrollAmount(ScrollAction.seek, trackpadBegin, "vertical", 3), -0.375);
assert.equal(iinaScrollAmount(ScrollAction.volume, trackpadBegin, "vertical", 3), -1.125);

const gesture = new IinaScrollGestureState();
let plan = gesture.advance(trackpadBegin, () => ScrollAction.seek, true);
assert.deepEqual(
  [plan.direction, plan.pause, plan.resume, plan.isMouse],
  ["vertical", true, false, false],
);
plan = gesture.advance(
  { ...trackpadBegin, delta_x: 12, delta_y: 0, phase: NSEventPhase.changed },
  () => ScrollAction.volume,
  false,
);
assert.equal(plan.direction, "vertical", "trackpad direction remains locked for the gesture");
assert.equal(plan.action, ScrollAction.seek, "trackpad action remains locked for the gesture");
plan = gesture.advance(
  { ...trackpadBegin, delta_y: 0, phase: NSEventPhase.ended },
  () => ScrollAction.seek,
  false,
);
assert.equal(plan.resume, true);
assert.equal(gesture.direction, null);

assert.equal(normalizePinchAction(0), PinchAction.windowSize);
assert.equal(normalizePinchAction("1"), PinchAction.fullscreen);
assert.equal(normalizePinchAction(99), PinchAction.none);
assert.equal(exceedsWindowDragThreshold({ x: 10, y: 10 }, { x: 13, y: 10 }), false);
assert.equal(exceedsWindowDragThreshold({ x: 10, y: 10 }, { x: 13.01, y: 10 }), true);
assert.equal(exceedsWindowDragThreshold({ x: 10, y: 10 }, { x: 12.2, y: 12.2 }), true);

console.log("Native input gesture contracts pass");
