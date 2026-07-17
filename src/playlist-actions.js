export const PLAYLIST_PLUGIN_MENU_HOOK = Object.freeze({
  supported: true,
  reason: null,
});

export const PLAYLIST_DROP_REVEAL_DELAY_MS = 300;
const PLAYLIST_DROP_REVEAL_EDGE_FRACTION = 0.2;
const PLAYLIST_DROP_BLACKLIST_EXTENSIONS = new Set([
  "utf", "utf8", "utf-8", "idx", "sub", "srt", "smi", "rt", "ssa", "aqt", "jss", "js", "ass",
  "mks", "vtt", "sup", "scc", "m3u", "m3u8", "pls",
]);

/** Mirrors IINA's file-side `hasPlayableFiles` test for drag-hover purposes. */
export function playlistDropPathMayBePlayable(path) {
  const value = String(path || "").trim();
  if (!value) return false;
  if (isNetworkPlaylistPath(value)) return true;
  const leaf = value.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || "";
  const separator = leaf.lastIndexOf(".");
  const extension = separator >= 0 ? leaf.slice(separator + 1).toLowerCase() : "";
  return !PLAYLIST_DROP_BLACKLIST_EXTENSIONS.has(extension);
}

export function playlistDropShouldReveal({
  pointerX,
  viewportWidth,
  hasPlayableFiles,
  miniPlayer = false,
  playlistVisible = false,
} = {}) {
  const x = Number(pointerX);
  const width = Number(viewportWidth);
  return Boolean(
    hasPlayableFiles
      && !miniPlayer
      && !playlistVisible
      && Number.isFinite(x)
      && Number.isFinite(width)
      && width > 0
      && x > width * (1 - PLAYLIST_DROP_REVEAL_EDGE_FRACTION),
  );
}

export function normalizePlaylistIndexes(indexes, playlistLength) {
  return [...new Set(indexes)]
    .filter((index) => Number.isInteger(index) && index >= 0 && index < playlistLength)
    .sort((left, right) => left - right);
}

export function playlistContextTargetIndexes(selectedIndexes, clickedIndex, playlistLength) {
  const selected = normalizePlaylistIndexes(selectedIndexes, playlistLength);
  if (!Number.isInteger(clickedIndex) || clickedIndex < 0 || clickedIndex >= playlistLength) return [];
  return selected.includes(clickedIndex) ? selected : [clickedIndex];
}

export function isNetworkPlaylistPath(path) {
  return /^[^:/?#]+:(?:\/\/)?/.test(String(path || ""));
}

export function playlistTargetSubsets(items, indexes) {
  const selected = normalizePlaylistIndexes(indexes, items.length);
  const local = selected.filter((index) => !isNetworkPlaylistPath(items[index]?.path));
  const network = selected.filter((index) => isNetworkPlaylistPath(items[index]?.path));
  return { selected, local, network };
}

export function playlistContextMenuModel(items, indexes) {
  const targets = playlistTargetSubsets(items, indexes);
  const menu = [];
  if (targets.selected.length) {
    const first = items[targets.selected[0]] || {};
    menu.push({ kind: "header", label: targets.selected.length === 1
      ? String(first.title || first.path || "")
      : `${targets.selected.length} Items` });
    menu.push({ kind: "separator" });
    menu.push(
      { kind: "action", id: "play-next", label: "Play Next" },
      { kind: "action", id: "open-new-window", label: "Play in New Window" },
      { kind: "action", id: "remove", label: targets.selected.length === 1 ? "Remove" : "Remove Selected" },
      { kind: "separator" },
    );
    if (targets.network.length) {
      menu.push(
        { kind: "action", id: "open-browser", label: "Open in Browser" },
        { kind: "action", id: "copy-urls", label: targets.network.length === 1 ? "Copy URL" : "Copy URLs" },
        { kind: "separator" },
      );
    }
    if (targets.local.length) {
      menu.push(
        { kind: "action", id: "trash", label: targets.local.length === 1 ? "Move to Trash" : "Move Selected Files to Trash" },
        { kind: "action", id: "reveal", label: "Show in Finder" },
        { kind: "separator" },
      );
    }
  }
  menu.push(
    { kind: "action", id: "add-file", label: "Add File…" },
    { kind: "action", id: "add-url", label: "Add URL…" },
    { kind: "action", id: "clear", label: "Clear Playlist" },
  );
  return { targets, menu, pluginHook: PLAYLIST_PLUGIN_MENU_HOOK };
}

export function reorderedPlaylistItems(items, indexes, destination) {
  const selectedIndexes = normalizePlaylistIndexes(indexes, items.length);
  if (!selectedIndexes.length) return { moved: false, items };
  const selectedIndexSet = new Set(selectedIndexes);
  const selectedItems = selectedIndexes.map((index) => items[index]);
  const remainingItems = items.filter((_, index) => !selectedIndexSet.has(index));
  const boundedDestination = Math.max(0, Math.min(items.length, destination));
  const adjustedDestination = Math.min(
    remainingItems.length,
    boundedDestination - selectedIndexes.filter((index) => index < boundedDestination).length,
  );
  const reorderedItems = [
    ...remainingItems.slice(0, adjustedDestination),
    ...selectedItems,
    ...remainingItems.slice(adjustedDestination),
  ];
  return {
    moved: reorderedItems.some((item, index) => item !== items[index]),
    items: reorderedItems,
    selectedIndexes: selectedItems.map((_, index) => adjustedDestination + index),
  };
}

export function playlistSelectionAfterAction(selection, action) {
  if (["play-next", "remove", "trash", "reveal", "clear"].includes(action)) return [];
  return [...selection];
}

export function playlistPasteDestination(selection, playlistLength) {
  return normalizePlaylistIndexes(selection, playlistLength)[0] ?? 0;
}

export function playlistRowInsertionIndex(rowRects, pointerY, playlistLength) {
  for (let index = 0; index < rowRects.length; index += 1) {
    const rect = rowRects[index];
    if (pointerY < rect.top + rect.height / 2) return index;
  }
  return Math.max(0, playlistLength);
}

export function playlistDropTargets({
  filePaths = [],
  uriList = "",
  text = "",
  allowAbsolutePathText = false,
} = {}) {
  const files = filePaths.filter((path) => typeof path === "string" && path.length > 0);
  if (files.length) return files;

  const uris = String(uriList)
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("#") && isNetworkPlaylistPath(line));
  if (uris.length) return uris;

  const candidate = String(text)
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean);
  if (!candidate) return [];
  if (isNetworkPlaylistPath(candidate)) return [candidate];
  if (allowAbsolutePathText && /^\/(?:[^/]+(?:\/[^/]+)*)$/.test(candidate)) return [candidate];
  return [];
}
