import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import {
  ONLINE_SUBTITLE_ACCESSORY_GEOMETRY,
  cancelOnlineSubtitleSelection,
  planOnlineSubtitleSearchResult,
  selectOnlineSubtitleCandidate,
} from "../src/online-subtitle-flow.js";

const candidates = [
  { id: "one", name: "One" },
  { id: "two", name: "Two" },
];

assert.deepEqual(planOnlineSubtitleSearchResult([]), {
  phase: "idle",
  effect: "empty",
  selectedId: null,
});
assert.deepEqual(planOnlineSubtitleSearchResult(candidates.slice(0, 1)), {
  phase: "downloading",
  effect: "download",
  selectedId: "one",
});
assert.deepEqual(planOnlineSubtitleSearchResult(candidates), {
  phase: "choosing",
  effect: "choose",
  selectedId: null,
});
assert.equal(selectOnlineSubtitleCandidate(candidates, "one"), "one");
assert.equal(selectOnlineSubtitleCandidate(candidates, "two"), "two");
assert.equal(selectOnlineSubtitleCandidate(candidates, "missing"), null);
assert.deepEqual(cancelOnlineSubtitleSelection("choosing"), {
  phase: "idle",
  effect: "canceled",
  selectedId: null,
});
assert.deepEqual(cancelOnlineSubtitleSelection("searching"), {
  phase: "idle",
  effect: "dismissed",
  selectedId: null,
});
assert.deepEqual(ONLINE_SUBTITLE_ACCESSORY_GEOMETRY, {
  width: 480,
  height: 272,
  rowHeight: 35,
});

const root = join(import.meta.dirname, "..");
const [mainSource, html, css, referenceXib, referenceController] = await Promise.all([
  readFile(join(root, "src", "main.js"), "utf8"),
  readFile(join(root, "src", "index.html"), "utf8"),
  readFile(join(root, "src", "styles.css"), "utf8"),
  readFile(join(root, "参考", "iina", "iina", "Base.lproj", "SubChooseViewController.xib"), "utf8"),
  readFile(join(root, "参考", "iina", "iina", "SubChooseViewController.swift"), "utf8"),
]);

assert.match(referenceXib, /width="480" height="272"/);
assert.match(referenceXib, /rowHeight="35"/);
assert.doesNotMatch(referenceXib, /allowsMultipleSelection="YES"/);
assert.match(referenceController, /tableView\.doubleAction = #selector\(downloadBtnAction/);
assert.match(referenceController, /downloadBtn\.isEnabled = tableView\.selectedRow != -1/);

assert.match(html, /id="online-subtitles-accessory" class="online-subtitles-accessory"/);
assert.match(html, /id="online-subtitles-list"[^>]+aria-multiselectable="false"/);
assert.doesNotMatch(html, /online-subtitles-modal|online-subtitles-close-button|aria-multiselectable="true"/);
assert.match(css, /\.online-subtitles-accessory\s*\{[^}]*width: 480px;[^}]*height: 272px;/s);
assert.match(css, /\.online-subtitle-row\s*\{[^}]*height: 35px;[^}]*min-height: 35px;/s);
assert.doesNotMatch(css, /\.online-subtitles-modal\s*\{/);

assert.match(mainSource, /showOsd\("Finding online subtitles…", \{\s*autoHide: false,/s);
assert.match(mainSource, /planOnlineSubtitleSearchResult\(onlineSubtitleCandidates\)/);
assert.match(mainSource, /plan\.effect === "empty"[\s\S]*showOsd\("No subtitles found"\)/);
assert.match(mainSource, /plan\.effect === "download"[\s\S]*await downloadSelectedOnlineSubtitles\(\)/);
assert.match(mainSource, /accessory: els\.onlineSubtitlesAccessory,[\s\S]*persistent: true,/);
assert.match(mainSource, /row\.addEventListener\("dblclick",/);
assert.match(mainSource, /selectedOnlineSubtitleId = selectOnlineSubtitleCandidate/);
assert.match(mainSource, /onlineSubtitleFlowPhase === "choosing" && event\.key === "Escape"[\s\S]*cancelOnlineSubtitleChooser\(\)/);
assert.match(mainSource, /function cancelOnlineSubtitleChooser\(\)[\s\S]*showOsd\("Canceled"\)/);
assert.match(mainSource, /target\.closest\("\.sidebar,\.top-bar,\.subtitle-popover,\.url-window,\.online-subtitles-accessory"\)/);
assert.match(mainSource, /\[role='button'\][^\n]+\.online-subtitles-accessory/);
assert.doesNotMatch(mainSource, /selectedOnlineSubtitleIds|onlineSubtitlesModal/);
assert.doesNotMatch(mainSource, /\.online-subtitles-window|online-subtitles-close/);
assert.match(mainSource, /if \(result === null\) \{[\s\S]*resetOnlineSubtitleFlow\(\);[\s\S]*return;/);
const noAutoHideIndex = mainSource.indexOf("if (options.persistent || options.autoHide === false) return;");
const timerIndex = mainSource.indexOf("osdTimer = setTimeout(() => hideOsd(), timeout);");
assert.ok(noAutoHideIndex >= 0 && timerIndex > noAutoHideIndex, "persistent OSD must bypass timer creation");

console.log("Online subtitle OSD flow, single selection, and reference geometry contracts pass");
