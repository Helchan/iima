import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  nativeEscapeKeyboardEventInit,
  nativePlayerMousePoint,
  shouldExitFullscreenForUnboundEscape,
} from "../src/native-player-input.js";

assert.deepEqual(nativePlayerMousePoint({ kind: "mouse-move", x: 123.5, y: 44 }), {
  x: 123.5,
  y: 44,
});
assert.equal(nativePlayerMousePoint({ kind: "scroll", x: 1, y: 2 }), null);
assert.equal(nativePlayerMousePoint({ kind: "mouse-move", x: "not-a-point", y: 2 }), null);

assert.deepEqual(
  nativeEscapeKeyboardEventInit({
    kind: "key-down",
    key_code: 53,
    modifiers: (1 << 17) | (1 << 19) | (1 << 20),
    repeat: true,
  }),
  {
    key: "Escape",
    code: "Escape",
    bubbles: true,
    cancelable: true,
    composed: true,
    repeat: true,
    shiftKey: true,
    ctrlKey: false,
    altKey: true,
    metaKey: true,
  },
);
assert.equal(
  nativeEscapeKeyboardEventInit({ kind: "key-down", key_code: 124, modifiers: 0 }),
  null,
  "ordinary keys must keep their single normal WebKit delivery",
);
assert.equal(nativeEscapeKeyboardEventInit({ kind: "mouse-move", key_code: 53 }), null);

assert.equal(shouldExitFullscreenForUnboundEscape({ key: "Escape" }, false, true), true);
assert.equal(shouldExitFullscreenForUnboundEscape({ key: "Escape" }, true, true), false);
assert.equal(shouldExitFullscreenForUnboundEscape({ key: "Escape" }, false, false), false);
assert.equal(shouldExitFullscreenForUnboundEscape({ key: "Enter" }, false, true), false);

const frontendSource = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
for (const contract of [
  "window.addEventListener(\"keydown\", handleWindowKeyDown);",
  "function dispatchNativePlayerKeyDown(payload)",
  "shouldExitFullscreenForUnboundEscape(event, handled, windowFullscreenActive)",
  "new KeyboardEvent(\"keydown\", init)",
  "function scheduleNativePlayerMouseMove(payload)",
  "requestAnimationFrame(() =>",
  "handlePlayerPointerMovementForTarget(target);",
]) {
  assert.ok(frontendSource.includes(contract), `missing native player input contract: ${contract}`);
}

console.log("native player input bridge: ok");
