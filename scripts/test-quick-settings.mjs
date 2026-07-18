import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  quickSettingsTrackRows,
  selectedTrackId,
  subtitleTextStyleAvailable,
  subtitleTrackSections,
  trackStatusBadgesForQuickSettings,
} from "../src/quick-settings.js";

const videoTracks = [
  { id: 7, title: "#7 Main h264", selected: true, metadata: { source_title: "Main", codec: "h264" } },
  { id: 8, title: "Album Art", selected: false, metadata: { albumart: true } },
];
assert.equal(selectedTrackId(videoTracks), 7);
assert.deepEqual(
  quickSettingsTrackRows(videoTracks).map((track) => [track.id, track.selected]),
  [[0, false], [7, true], [8, false]],
);
assert.deepEqual(
  quickSettingsTrackRows(videoTracks.map((track) => ({ ...track, selected: false })))
    .map((track) => [track.id, track.selected]),
  [[0, true], [7, false], [8, false]],
);

const subtitles = [
  { id: 0, title: "None", selected: false, metadata: {} },
  { id: 11, title: "English", selected: true, metadata: { language: "eng", codec: "srt" } },
  { id: 12, title: "Signs", selected: false, metadata: { language: "jpn", codec: "ass" } },
];
const sections = subtitleTrackSections(subtitles, 12);
assert.equal(sections.primaryId, 11);
assert.equal(sections.secondaryId, 12);
assert.equal(sections.canSwap, true);
assert.deepEqual(sections.primary.map((track) => [track.id, track.selected]), [[0, false], [11, true], [12, false]]);
assert.deepEqual(sections.secondary.map((track) => [track.id, track.selected]), [[0, false], [11, false], [12, true]]);
assert.equal(subtitleTrackSections(subtitles, 404).secondaryId, 0);
assert.equal(subtitleTrackSections([{ ...subtitles[0], selected: true }], 0).canSwap, false);

assert.deepEqual(trackStatusBadgesForQuickSettings({
  selected: false,
  metadata: {
    default_track: true,
    forced: true,
    external: true,
    albumart: true,
    image: true,
    main_selection: true,
  },
}), ["Default", "Forced", "External", "Album Art", "Image", "Main"]);
assert.equal(subtitleTextStyleAvailable(subtitles[1]), true);
assert.equal(subtitleTextStyleAvailable(subtitles[2]), false);
assert.equal(subtitleTextStyleAvailable({ id: 13, metadata: { codec: "hdmv_pgs_subtitle" } }), false);
assert.equal(subtitleTextStyleAvailable({ id: 14, metadata: { codec: "subrip", image: true } }), false);
assert.equal(subtitleTextStyleAvailable({ id: 0, metadata: {} }), false);

const frontend = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
const css = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
const player = readFileSync(new URL("../src-tauri/src/player.rs", import.meta.url), "utf8");
for (const contract of [
  'heading: "Subtitle:"',
  'heading: "Secondary subtitle:"',
  'command({ type: "swap-subtitle-tracks" })',
  'renderTrackList(sections.secondary, "second-subtitles"',
  'quickSettingsTrackRows(nextState.tracks.video)',
  'quickSettingsTrackRows(nextState.tracks.audio)',
  'parts.push(`Source #${metadata.source_id}`)',
  "item.metadata?.source_title",
  "pluginRuntimeOrder = specs.map((spec) => spec.identifier)",
  "updatePluginSidebarTabVisibility(state)",
  "ensurePluginSidebar(runtime)",
  'action === "delogo"',
  'startCustomDelogoEditor()',
  'type: "remove-delogo"',
  'editor.mode === "delogo" ? "set-delogo-region" : "set-custom-video-crop"',
  'trKey("FreeSelectingViewController", "mCM-Di-cvS.title", "Select Region")',
]) {
  assert.ok(frontend.includes(contract), `Missing Quick Settings frontend contract: ${contract}`);
}

assert.match(
  css,
  /\.sidebar \{[\s\S]*?top: 0;[\s\S]*?width: 270px;/,
  "the player sidebar must extend under the title bar with the playlist default width",
);
assert.match(
  css,
  /\.sidebar-tabs \{[\s\S]*?min-height: 76px;[\s\S]*?padding-top: 28px;/,
  "sidebar tabs must preserve IINA's 28 px title-bar downshift and 48 px tab row",
);
assert.doesNotMatch(
  css,
  /@media \(max-width: 760px\) \{[\s\S]*?\.sidebar \{[\s\S]*?width: 100vw;/,
  "Retina player windows below 760 CSS px must not turn the sidebar into a full-window sheet",
);
for (const contract of [
  'function sidebarWidthForTab(tab)',
  'tab === "playlist" || tab === "chapters"',
  'Number(getPreferenceValue("playlistWidth")) || 270',
  'return Math.min(400, Math.max(240, preferred));',
  'els.sidebar.style.width = `${sidebarWidthForTab(nextState.sidebar.tab)}px`;',
]) {
  assert.ok(frontend.includes(contract), `Missing IINA sidebar sizing contract: ${contract}`);
}
for (const contract of [
  "SwapSubtitleTracks",
  'self.record_mpv_int("sid", secondary_id)',
  'self.record_mpv_int("secondary-sid", primary_id)',
  "primary_and_secondary_subtitle_tracks_swap_as_one_player_command",
  "SetDelogoRegion",
  "RemoveDelogo",
  '"@iina_delogo:lavfi=[delogo=x={x}:y={y}:w={width}:h={height}]"',
  "delogo_replaces_and_removes_the_single_iina_labeled_filter",
]) {
  assert.ok(player.includes(contract), `Missing Quick Settings player contract: ${contract}`);
}

console.log("Quick Settings track and secondary-subtitle contracts pass");
