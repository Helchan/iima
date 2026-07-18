import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { FirstMouseGate } from "../src/first-mouse.js";

const tauriConfig = JSON.parse(readFileSync(
  new URL("../src-tauri/tauri.conf.json", import.meta.url),
  "utf8",
));
const playerWindow = tauriConfig.app.windows.find((window) => window.label === "main");
assert.equal(
  playerWindow?.acceptFirstMouse,
  true,
  "the WKWebView must receive an inactive-window click so controls can act on the first click",
);

const frontend = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
const commands = readFileSync(new URL("../src-tauri/src/commands.rs", import.meta.url), "utf8");
const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
for (const contract of [
  'els.videoStage.addEventListener("pointerdown", suppressFirstMouseSurfacePointer, { capture: true });',
  'els.videoStage.addEventListener("click", suppressFirstMouseSurfaceAction, { capture: true });',
  'els.playlistButton.addEventListener("click", () => toggleSidebar("playlist"));',
  'els.settingsButton.addEventListener("click", () => toggleSidebar("video"));',
]) {
  assert.ok(frontend.includes(contract), `Missing first-mouse control contract: ${contract}`);
}
for (const functionName of ["open_player_window_for_session", "enter_music_mode_window_for_session"]) {
  const body = commands.split(`fn ${functionName}`)[1]?.split("\nfn ")[0];
  assert.ok(body, `Missing dynamic window builder: ${functionName}`);
  assert.match(
    body,
    /\.accept_first_mouse\(true\)/,
    `${functionName} must deliver an inactive-window click to its controls`,
  );
}
assert.ok(
  frontend.includes('els.player.addEventListener("mousedown", preventPlayerChromeButtonMouseFocus, { capture: true });'),
  "player chrome must prevent WebKit buttons from becoming first responder",
);
assert.match(frontend, /function preventPlayerChromeButtonMouseFocus\([\s\S]*?event\.preventDefault\(\);\n\}/);
assert.match(styles, /\.osc button:focus,[\s\S]*?outline:\s*none;[\s\S]*?box-shadow:\s*none;/);
assert.ok(
  !frontend.includes('els.player.addEventListener("pointerdown", suppressFirstMouseSurfacePointer'),
  "first-mouse preference gating belongs to the video surface, not OSC controls",
);

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
