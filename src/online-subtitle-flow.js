export const ONLINE_SUBTITLE_ACCESSORY_GEOMETRY = Object.freeze({
  width: 480,
  height: 272,
  rowHeight: 35,
});

export function planOnlineSubtitleSearchResult(candidates) {
  const normalized = Array.from(candidates || []);
  if (normalized.length === 0) {
    return Object.freeze({ phase: "idle", effect: "empty", selectedId: null });
  }
  if (normalized.length === 1) {
    return Object.freeze({
      phase: "downloading",
      effect: "download",
      selectedId: normalized[0]?.id ?? null,
    });
  }
  return Object.freeze({ phase: "choosing", effect: "choose", selectedId: null });
}

export function selectOnlineSubtitleCandidate(candidates, candidateId) {
  const id = String(candidateId ?? "");
  return Array.from(candidates || []).some((candidate) => String(candidate?.id ?? "") === id)
    ? id
    : null;
}

export function cancelOnlineSubtitleSelection(phase) {
  return Object.freeze({
    phase: "idle",
    effect: phase === "choosing" ? "canceled" : "dismissed",
    selectedId: null,
  });
}
