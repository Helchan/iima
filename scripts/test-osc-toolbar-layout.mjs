import assert from "node:assert/strict";

import { reconcileOscToolbarLayout } from "../src/osc-toolbar-layout.js";

function button(name) {
  return { hidden: false, name };
}

const settings = button("settings");
const playlist = button("playlist");
const pip = button("pip");
const buttons = new Map([
  [0, settings],
  [1, playlist],
  [2, pip],
]);
const container = {
  children: [],
  style: {},
  append(child) {
    const previousIndex = this.children.indexOf(child);
    if (previousIndex >= 0) this.children.splice(previousIndex, 1);
    this.children.push(child);
    this.appendCount = (this.appendCount ?? 0) + 1;
  },
};

let fingerprint = reconcileOscToolbarLayout({
  container,
  buttons,
  configured: [2, 1, 0],
});
assert.equal(fingerprint, "2,1,0");
assert.deepEqual(container.children, [pip, playlist, settings]);
assert.equal(container.style.width, "72px");
assert.equal(container.appendCount, 3);
assert.equal(pip.hidden, false);
assert.equal(playlist.hidden, false);
assert.equal(settings.hidden, false);

const stableChildren = [...container.children];
fingerprint = reconcileOscToolbarLayout({
  container,
  buttons,
  configured: [2, 1, 0],
  previousFingerprint: fingerprint,
});
assert.equal(container.appendCount, 3, "a state poll must not replace or move pressed toolbar buttons");
assert.deepEqual(container.children, stableChildren);

fingerprint = reconcileOscToolbarLayout({
  container,
  buttons,
  configured: [0, 1],
  previousFingerprint: fingerprint,
});
assert.equal(fingerprint, "0,1");
assert.deepEqual(container.children, [pip, settings, playlist]);
assert.equal(container.style.width, "48px");
assert.equal(container.appendCount, 5);
assert.equal(pip.hidden, true);
assert.equal(settings.hidden, false);
assert.equal(playlist.hidden, false);

console.log("OSC toolbar layout remains stable between preference changes");
