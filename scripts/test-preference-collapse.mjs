import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  expandPreferenceCollapseForSearch,
  setPreferenceCollapseOpen,
  togglePreferenceCollapse,
} from "../src/preference-collapse.js";
import {
  PREFERENCE_PANES,
  preferenceControlByKey,
  preferenceDisclosureChildren,
} from "../src/preference-panes.js";

function fakeCollapse({ checkbox = false, disableContents = false } = {}) {
  const attributes = new Map();
  const classes = new Set();
  const fields = [{ disabled: false }, { disabled: false }];
  const collapse = {
    classList: {
      toggle(name, force) {
        if (force) classes.add(name);
        else classes.delete(name);
      },
    },
    querySelector(selector) {
      if (selector === "[data-pref-collapse-trigger]") return trigger;
      if (selector === "[data-pref-collapse-content]") return content;
      return null;
    },
  };
  const trigger = {
    type: checkbox ? "checkbox" : "button",
    checked: false,
    setAttribute(name, value) { attributes.set(name, value); },
    getAttribute(name) { return attributes.get(name) ?? null; },
    closest(selector) { return selector === "[data-pref-collapse]" ? collapse : null; },
  };
  const content = {
    hidden: false,
    dataset: disableContents ? { prefCollapseDisableContents: "true" } : {},
    querySelectorAll() { return fields; },
    closest(selector) { return selector === "[data-pref-collapse]" ? collapse : null; },
  };
  const target = {
    closest(selector) { return selector === "[data-pref-collapse]" ? collapse : null; },
  };
  return { attributes, classes, collapse, content, fields, target, trigger };
}

const disclosure = fakeCollapse();
assert.equal(setPreferenceCollapseOpen(disclosure.trigger, disclosure.content, false), false);
assert.equal(disclosure.content.hidden, true, "reference disclosure groups start folded");
assert.equal(disclosure.attributes.get("aria-expanded"), "false");
assert.equal(disclosure.classes.has("is-open"), false);
assert.equal(togglePreferenceCollapse(disclosure.trigger, disclosure.content), true);
assert.equal(disclosure.content.hidden, false, "toggle reveals disclosure children");
assert.equal(disclosure.attributes.get("aria-expanded"), "true");
assert.equal(disclosure.classes.has("is-open"), true);

const geometry = fakeCollapse({ checkbox: true, disableContents: true });
setPreferenceCollapseOpen(geometry.trigger, geometry.content, false);
assert.equal(geometry.trigger.checked, false);
assert.equal(geometry.content.hidden, true);
assert.ok(geometry.fields.every((field) => field.disabled), "folded geometry fields are hidden and disabled");
let persistedGeometry = "1280+20-20";
assert.equal(expandPreferenceCollapseForSearch(geometry.target), true);
assert.equal(geometry.trigger.checked, true);
assert.equal(geometry.content.hidden, false, "search navigation reveals the owning geometry region");
assert.ok(geometry.fields.every((field) => !field.disabled));
assert.equal(
  persistedGeometry,
  "1280+20-20",
  "search expansion does not dispatch the preference action or rewrite the persisted geometry",
);
assert.equal(expandPreferenceCollapseForSearch({ closest: () => null }), false);

const generalMedia = preferenceControlByKey("generalMediaOpenedHint");
assert.equal(generalMedia.type, "disclosure");
assert.equal(generalMedia.defaultOpen, false);
assert.deepEqual(
  preferenceDisclosureChildren(
    PREFERENCE_PANES.find((pane) => pane.id === "general").sections[0].controls,
    generalMedia.disclosureId,
  ).map((control) => control.key),
  ["pauseWhenOpen", "fullScreenWhenOpen"],
);
const generalPause = preferenceControlByKey("generalPauseResumeHint");
assert.equal(generalPause.defaultOpen, false);
assert.equal(generalPause.disclosureId, "general-pause-resume");
const subAuto = preferenceControlByKey("subAutoLoadAdvancedDisclosure");
const subText = preferenceControlByKey("subTextAdvancedDisclosure");
assert.equal(subAuto.defaultOpen, false);
assert.equal(subText.defaultOpen, false);
assert.deepEqual(
  [subAuto.disclosureId, subText.disclosureId],
  ["subtitle-auto-load-advanced", "subtitle-text-advanced"],
);

const generalXib = readFileSync(
  new URL("../参考/iina/iina/Base.lproj/PrefGeneralViewController.xib", import.meta.url),
  "utf8",
);
const uiXib = readFileSync(
  new URL("../参考/iina/iina/Base.lproj/PrefUIViewController.xib", import.meta.url),
  "utf8",
);
const subXib = readFileSync(
  new URL("../参考/iina/iina/Base.lproj/PrefSubViewController.xib", import.meta.url),
  "utf8",
);
assert.equal((generalXib.match(/customClass="CollapseView"/gu) || []).length, 2);
assert.equal((uiXib.match(/customClass="CollapseView"/gu) || []).length, 2);
assert.equal((subXib.match(/customClass="CollapseView"/gu) || []).length, 2);
for (const title of ["When media is opened:", "Pause/resume when:"]) {
  assert.ok(generalXib.includes(`title="${title}"`));
}
for (const title of ["Initial window size:", "Initial window position:"]) {
  assert.match(uiXib, new RegExp(`type="check" title="${title}"[^>]*state="on"`, "u"));
}
assert.ok(subXib.includes('title="Advanced" id="DUA-7H-Yje"'));
assert.ok(subXib.includes('title="Advanced" id="X7X-18-FJg"'));

const mainSource = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
for (const contract of [
  "renderPreferenceDisclosure(",
  "setPreferenceCollapseOpen(trigger, content, control.defaultOpen === true)",
  "expandPreferenceCollapseForSearch(target)",
  "sizeFields.dataset.prefCollapseDisableContents = \"true\"",
  "positionFields.dataset.prefCollapseDisableContents = \"true\"",
]) {
  assert.ok(mainSource.includes(contract), `missing rendered collapse contract: ${contract}`);
}

console.log("Reference CollapseView visibility, accessibility, and search-expansion checks passed");
