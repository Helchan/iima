import assert from "node:assert/strict";

import { FirstMouseGate } from "../src/first-mouse.js";

let clock = 100;
let acceptsFirstMouse = false;
const gate = new FirstMouseGate({
  active: true,
  acceptsFirstMouse: () => acceptsFirstMouse,
  doubleClickInterval: 500,
  now: () => clock,
});

assert.equal(gate.shouldSuppressPointer({ pointerId: 1, button: 0 }, "down"), false);
gate.blur();
const activationEpoch = gate.beginFocus();
assert.equal(gate.shouldSuppressPointer({ pointerId: 2, button: 0 }, "down"), true);
assert.equal(gate.shouldSuppressPointer({ pointerId: 2, button: 0 }, "move"), true);
assert.equal(gate.shouldSuppressPointer({ pointerId: 2, button: 0 }, "up"), true);
assert.equal(gate.commitFocus(activationEpoch), true);
assert.equal(gate.shouldSuppressAction({ type: "click", button: 0 }), true);
assert.equal(gate.shouldSuppressAction({ type: "click", button: 0 }), false);
assert.equal(gate.shouldSuppressAction({ type: "dblclick", button: 0 }), true);
clock += 501;
assert.equal(gate.shouldSuppressAction({ type: "dblclick", button: 0 }), false);

gate.blur();
const staleEpoch = gate.beginFocus();
gate.blur();
assert.equal(gate.commitFocus(staleEpoch), false, "a stale focus task must not re-arm the surface");

acceptsFirstMouse = true;
assert.equal(gate.shouldSuppressPointer({ pointerId: 3, button: 2 }, "down"), false);
assert.equal(gate.shouldSuppressAction({ type: "contextmenu", button: 2 }), false);

acceptsFirstMouse = false;
const rightEpoch = gate.beginFocus();
assert.equal(gate.shouldSuppressPointer({ pointerId: 4, button: 2 }, "down"), true);
assert.equal(gate.shouldSuppressAction({ type: "contextmenu", button: 2 }), true);
assert.equal(gate.shouldSuppressPointer({ pointerId: 4, button: 2 }, "cancel"), true);
assert.equal(gate.commitFocus(rightEpoch), true);

console.log("First-mouse activation contracts pass");
