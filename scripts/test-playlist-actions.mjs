import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  PLAYLIST_DROP_REVEAL_DELAY_MS,
  PLAYLIST_PLUGIN_MENU_HOOK,
  playlistContextMenuModel,
  playlistContextTargetIndexes,
  playlistDropTargets,
  playlistDropPathMayBePlayable,
  playlistDropShouldReveal,
  playlistPasteDestination,
  playlistRowInsertionIndex,
  playlistSelectionAfterAction,
} from "../src/playlist-actions.js";
import {
  playlistDurationSummary,
  playlistMetadata,
  playlistProgressFraction,
} from "../src/playlist-presentation.js";

const items = [
  { title: "Local One", path: "/tmp/one.mp4" },
  { title: "Remote One", path: "https://example.com/one" },
  { title: "Local Two", path: "/tmp/two.mkv" },
  { title: "Remote Two", path: "rtsp://example.com/two" },
];

assert.deepEqual(playlistContextTargetIndexes([0, 2], -1, items.length), []);
assert.deepEqual(playlistContextTargetIndexes([0, 2], 1, items.length), [1]);
assert.deepEqual(playlistContextTargetIndexes([0, 2], 2, items.length), [0, 2]);

const empty = playlistContextMenuModel(items, []);
assert.deepEqual(empty.targets, { selected: [], local: [], network: [] });
assert.deepEqual(empty.menu.map((item) => item.id).filter(Boolean), ["add-file", "add-url", "clear"]);

const mixed = playlistContextMenuModel(items, [0, 1, 3]);
assert.deepEqual(mixed.targets, { selected: [0, 1, 3], local: [0], network: [1, 3] });
assert.equal(mixed.menu.find((item) => item.id === "remove")?.label, "Remove Selected");
assert.equal(mixed.menu.find((item) => item.id === "trash")?.label, "Move to Trash");
const localMulti = playlistContextMenuModel(items, [0, 2]);
assert.equal(localMulti.menu.find((item) => item.id === "trash")?.label, "Move Selected Files to Trash");
assert.deepEqual(mixed.menu.map((item) => item.id).filter(Boolean), [
  "play-next",
  "open-new-window",
  "remove",
  "open-browser",
  "copy-urls",
  "trash",
  "reveal",
  "add-file",
  "add-url",
  "clear",
]);

assert.deepEqual(playlistSelectionAfterAction([0, 1], "copy-urls"), [0, 1]);
assert.deepEqual(playlistSelectionAfterAction([0, 1], "play-next"), []);
assert.deepEqual(playlistSelectionAfterAction([0, 1], "trash"), []);
assert.equal(PLAYLIST_PLUGIN_MENU_HOOK.supported, true);
assert.equal(PLAYLIST_PLUGIN_MENU_HOOK.reason, null);

const frontend = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
for (const contract of [
  "function createPluginPlaylistApi(runtime)",
  "playlist: createPluginPlaylistApi(runtime)",
  "registerMenuItemBuilder: (builder)",
  "appendPluginPlaylistContextItems(menu, model.targets.selected)",
  "runtime.playlistMenuItemBuilder(Array.from(indexes))",
]) {
  assert.ok(frontend.includes(contract), `Missing plugin playlist contract: ${contract}`);
}

assert.equal(playlistPasteDestination([], items.length), 0);
assert.equal(playlistPasteDestination([3, 1], items.length), 1);
assert.equal(
  playlistRowInsertionIndex(
    [
      { top: 100, height: 20 },
      { top: 120, height: 20 },
    ],
    109,
    2,
  ),
  0,
);
assert.equal(playlistRowInsertionIndex([{ top: 100, height: 20 }], 116, 2), 2);
assert.deepEqual(playlistDropTargets({ filePaths: ["/tmp/a.mp4"] }), ["/tmp/a.mp4"]);
assert.deepEqual(
  playlistDropTargets({ uriList: "# browser comment\nhttps://example.com/a\nfile:///tmp/b.mp4" }),
  ["https://example.com/a", "file:///tmp/b.mp4"],
);
assert.deepEqual(playlistDropTargets({ text: "rtsp://example.com/live" }), ["rtsp://example.com/live"]);
assert.deepEqual(playlistDropTargets({ text: "not a URL" }), []);
assert.deepEqual(playlistDropTargets({ text: "/tmp/a.mp4" }), []);
assert.deepEqual(
  playlistDropTargets({ text: "/tmp/a.mp4", allowAbsolutePathText: true }),
  ["/tmp/a.mp4"],
);
assert.equal(PLAYLIST_DROP_REVEAL_DELAY_MS, 300);
assert.equal(playlistDropPathMayBePlayable("/tmp/movie.mp4"), true);
assert.equal(playlistDropPathMayBePlayable("/tmp/unknown.custom-media"), true);
assert.equal(playlistDropPathMayBePlayable("/tmp/subtitle.SRT"), false);
assert.equal(playlistDropPathMayBePlayable("/tmp/list.m3u8"), false);
assert.equal(playlistDropPathMayBePlayable("https://example.com/watch"), true);
assert.equal(playlistDropShouldReveal({
  pointerX: 801,
  viewportWidth: 1000,
  hasPlayableFiles: true,
}), true);
assert.equal(playlistDropShouldReveal({
  pointerX: 800,
  viewportWidth: 1000,
  hasPlayableFiles: true,
}), false);
assert.equal(playlistDropShouldReveal({
  pointerX: 900,
  viewportWidth: 1000,
  hasPlayableFiles: true,
  miniPlayer: true,
}), false);
assert.equal(playlistDropShouldReveal({
  pointerX: 900,
  viewportWidth: 1000,
  hasPlayableFiles: true,
  playlistVisible: true,
}), false);

const details = [
  {
    ready: true,
    duration_seconds: 100,
    playback_progress_seconds: 25,
    metadata_title: "Track One",
    metadata_artist: "Artist One",
  },
  { ready: true, duration_seconds: -1, playback_progress_seconds: 10 },
  { ready: true, duration_seconds: 50, playback_progress_seconds: 70 },
];
assert.deepEqual(playlistDurationSummary(details, [0, 2]), {
  totalSeconds: 150,
  selectedSeconds: 150,
});
assert.equal(playlistDurationSummary([{ ...details[0], ready: false }]), null);
assert.equal(playlistDurationSummary([{ ...details[0], duration_seconds: null }]), null);
assert.deepEqual(
  playlistMetadata(details[0], { playlistShowMetadata: true, playlistShowMetadataInMusicMode: true }, true),
  { title: "Track One", artist: "Artist One" },
);
assert.equal(
  playlistMetadata(details[0], { playlistShowMetadata: true, playlistShowMetadataInMusicMode: true }, false),
  null,
);
assert.equal(
  playlistMetadata({ ...details[0], metadata_artist: "" }, { playlistShowMetadata: true }, true),
  null,
);
assert.equal(playlistProgressFraction(details[0]), 0.25);
assert.equal(playlistProgressFraction(details[2]), 1);

console.log("Playlist action planner checks passed");
