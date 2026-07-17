import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  buildPreferenceSearchEntries,
  filterPreferenceSearchEntries,
  formPreferenceSearchTerm,
  nextPreferenceSearchIndex,
  normalizePreferenceSearchQuery,
  normalizePreferenceSearchTerm,
  preferenceSearchTargetKeys,
  preferenceSearchLabelsForControl,
  preferenceSearchTokens,
  preferenceSearchTokensMatch,
} from "../src/preference-search.js";
import { PREFERENCE_PANES } from "../src/preference-panes.js";
import {
  appendUniqueLanguageTokens,
  iinaLanguageTokenCompletions,
  languageTokenFromEditingString,
  languageTokensFromCsv,
  nextLanguageCompletionIndex,
  serializeLanguageTokens,
} from "../src/language-token-field.js";
import { parseIinaIso639Strings } from "./iso639-catalog.mjs";

assert.equal(formPreferenceSearchTerm("  On Screen Display:…  "), "On Screen Display");
assert.equal(normalizePreferenceSearchTerm("ＨＤＲ（Video）"), "ｈｄｒ（video）");
assert.equal(normalizePreferenceSearchQuery("  SCREEN display:   "), "  screen display");
assert.deepEqual(preferenceSearchTokens("  screen　display "), ["screen", "display"]);
assert.equal(
  preferenceSearchTokensMatch(["Video/Audio", "On Screen Display", "Enable OSD"], "scr dis"),
  true,
);
assert.equal(
  preferenceSearchTokensMatch(["字幕", "自动载入", "优先语言"], "自动　语言"),
  true,
);
assert.equal(
  preferenceSearchTokensMatch(["General", "Playlist", "Only in music mode"], "music missing"),
  false,
);
assert.equal(
  preferenceSearchTokensMatch(["On Screen Display"], "display:"),
  true,
  "the reference removes one trailing query colon",
);

const panes = [{
  id: "general",
  title: "General",
  sections: [{
    title: "Behavior:",
    controls: [
      { key: "pauseWhenOpen", label: "Pause" },
      {
        key: "spdifOutput",
        label: "S/PDIF output:",
        items: [{ key: "spdifAC3", label: "AC3" }, { key: "spdifDTS", label: "DTS" }],
      },
    ],
  }],
}];
const entries = buildPreferenceSearchEntries(panes);
assert.deepEqual(
  entries.map(({ label, key, sourceOrder }) => [label, key, sourceOrder]),
  [
    [null, null, 0],
    ["Pause", "pauseWhenOpen", 1],
    ["S/PDIF output:", "spdifOutput", 2],
    ["AC3", "spdifAC3", 3],
    ["DTS", "spdifDTS", 4],
  ],
  "the completion table keeps section-then-label source order",
);
assert.deepEqual(
  filterPreferenceSearchEntries(entries, "beh", (entry) => [
    entry.pane.title,
    entry.section.title,
    entry.label,
  ]).map(({ sourceOrder }) => sourceOrder),
  [0, 1, 2, 3, 4],
);
assert.deepEqual(
  filterPreferenceSearchEntries(entries, "beh dt", (entry) => [
    entry.pane.title,
    entry.section.title,
    entry.label,
  ]).map(({ label }) => label),
  ["DTS"],
);
assert.equal(nextPreferenceSearchIndex(-1, 3, 1), 0);
assert.equal(nextPreferenceSearchIndex(-1, 3, -1), 2);
assert.equal(nextPreferenceSearchIndex(0, 3, -1), 0);
assert.equal(nextPreferenceSearchIndex(2, 3, 1), 2);
assert.deepEqual(
  preferenceSearchLabelsForControl({
    label: "Synthetic label",
    searchLabels: [
      "Visible button",
      { label: "Visible toggle", targetKey: "visible:toggle" },
    ],
  }),
  [
    { label: "Visible button" },
    { label: "Visible toggle", l10n: undefined, targetKey: "visible:toggle" },
  ],
);
assert.deepEqual(preferenceSearchTargetKeys({
  key: "assrtToken",
  control: {
    key: "assrtToken",
    visibleWhen: ["onlineSubProvider", ":assrt"],
    dependsOn: { key: "enableOnlineSubtitles", equals: true },
  },
}), ["assrtToken", "onlineSubProvider", "enableOnlineSubtitles"]);

const renderedEntries = buildPreferenceSearchEntries(PREFERENCE_PANES);
const renderedLabels = renderedEntries.map((entry) => entry.label).filter(Boolean);
for (const invisibleSyntheticLabel of [
  "Initial window size and position:",
  "Clear playback data and thumbnail cache",
  "Chrome Firefox",
  "Key mappings",
]) {
  assert.equal(
    renderedLabels.includes(invisibleSyntheticLabel),
    false,
    `search must not index non-rendered metadata: ${invisibleSyntheticLabel}`,
  );
}
for (const visibleLabel of [
  "Initial window size:",
  "Initial window position:",
  "Clear Saved Playback Progress…",
  "Clear Playback History…",
  "Clear Thumbnail Cache…",
  "Chrome",
  "Firefox",
  "Use system media control",
]) {
  assert.ok(renderedLabels.includes(visibleLabel), `missing rendered search label: ${visibleLabel}`);
}
const searchRendered = (query) => filterPreferenceSearchEntries(
  renderedEntries,
  query,
  (entry) => [entry.pane.title, entry.section.title, entry.label],
);
assert.ok(searchRendered("saved playback progress").some((entry) => entry.label === "Clear Saved Playback Progress…"));
assert.ok(searchRendered("initial window size").some((entry) => entry.targetKey === "initialWindowSizePosition:size"));
assert.equal(
  searchRendered("initial size position").some((entry) => entry.key === "initialWindowSizePosition"),
  false,
  "the former combined synthetic geometry label must no longer match",
);
assert.equal(
  searchRendered("playback data thumbnail").some((entry) => entry.key === "utilityCache"),
  false,
  "the former synthetic cache label must no longer match",
);

const referenceController = readFileSync(
  new URL("../参考/iina/iina/PreferenceWindowController.swift", import.meta.url),
  "utf8",
);
const referenceXib = readFileSync(
  new URL("../参考/iina/iina/Base.lproj/PreferenceWindowController.xib", import.meta.url),
  "utf8",
);
for (const contract of [
  "while t.count != 0",
  'if c == " " || c == "\u3000"',
  "trimWhitespaceSuffix().removedLastSemicolon()",
  "currentCompletionResults = tries.filter { $0.active }.map { $0.returnValue }",
  "maskView.perform(#selector(maskView.highlight(_:)), with: label, afterDelay: 0.25)",
  "view.scrollToVisible(view.bounds.insetBy(dx: 0, dy: -20))",
]) {
  assert.ok(referenceController.includes(contract), `missing reference search contract: ${contract}`);
}
assert.match(referenceXib, /userLabel="Completion Popover"/u);
assert.match(referenceXib, /title="No Result"/u);
assert.match(referenceXib, /horizontalLineScroll="42"/u);

const isoSource = readFileSync(
  new URL("../参考/iina/iina/ISO639.strings", import.meta.url),
  "utf8",
);
const languages = parseIinaIso639Strings(isoSource);
const generatedLanguages = JSON.parse(readFileSync(
  new URL("../src/assets/iina/iso639.json", import.meta.url),
  "utf8",
));
assert.ok(languages.length > 680, "the full IINA ISO 639 catalog must be indexed");
assert.deepEqual(generatedLanguages, languages);
assert.deepEqual(languageTokenFromEditingString("English (eng)", languages), {
  code: "eng",
  identifier: "eng",
  editingString: "English (eng)",
});
assert.deepEqual(languageTokenFromEditingString("Custom, Language", languages), {
  code: null,
  identifier: "custom; language",
  editingString: "Custom, Language",
});
const initialTokens = languageTokensFromCsv("eng,jpn,custom", languages);
assert.equal(serializeLanguageTokens(initialTokens), "eng,jpn,custom");
assert.equal(
  serializeLanguageTokens(appendUniqueLanguageTokens(initialTokens, [initialTokens[0]])),
  "eng,jpn,custom",
);
assert.equal(
  iinaLanguageTokenCompletions(languages, "val", initialTokens)[0].editingString,
  "Catalan (ca)",
  "completion matches every semicolon-separated language name by prefix",
);
assert.equal(
  iinaLanguageTokenCompletions(languages, "eng", initialTokens)
    .some((token) => token.code === "eng"),
  false,
  "saved language codes are excluded from completion results",
);
assert.equal(nextLanguageCompletionIndex(-1, 4, 1), 0);
assert.equal(nextLanguageCompletionIndex(0, 4, -1), 0);

const frontend = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
for (const contract of [
  "normalizePreferenceSearchQuery(els.preferencesSearch.value)",
  "nextPreferenceSearchIndex(",
  'empty.textContent = tr("No Result")',
  "preferenceSearchTargetKeys(entry)",
  "}, 250);",
  "renderPreferenceLanguageTokenField(control, value, disabled)",
  "iinaLanguageTokenCompletions(languages, input.value, tokens)",
]) {
  assert.ok(frontend.includes(contract), `missing preference interaction contract: ${contract}`);
}

console.log("AppKit-style preference search and language-token checks passed");
