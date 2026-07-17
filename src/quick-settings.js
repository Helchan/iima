function normalizedTrackId(value) {
  const id = Number(value);
  return Number.isSafeInteger(id) && id >= 0 ? id : 0;
}

export function selectedTrackId(tracks) {
  const selected = Array.from(tracks || []).find((track) => track?.selected);
  return selected ? normalizedTrackId(selected.id) : 0;
}

export function quickSettingsTrackRows(tracks, selectedId = null) {
  const source = Array.from(tracks || []).filter((track) => normalizedTrackId(track?.id) !== 0);
  const activeId = selectedId === null ? selectedTrackId(tracks) : normalizedTrackId(selectedId);
  return [
    {
      id: 0,
      title: "None",
      selected: activeId === 0,
      metadata: {},
      virtual: true,
    },
    ...source.map((track) => ({
      ...track,
      selected: normalizedTrackId(track.id) === activeId,
    })),
  ];
}

export function subtitleTrackSections(tracks, secondaryId) {
  const primaryId = selectedTrackId(tracks);
  const normalizedSecondaryId = normalizedTrackId(secondaryId);
  const availableIds = new Set(Array.from(tracks || []).map((track) => normalizedTrackId(track?.id)));
  availableIds.add(0);
  const safeSecondaryId = availableIds.has(normalizedSecondaryId) ? normalizedSecondaryId : 0;
  return {
    primaryId,
    secondaryId: safeSecondaryId,
    primary: quickSettingsTrackRows(tracks, primaryId),
    secondary: quickSettingsTrackRows(tracks, safeSecondaryId),
    canSwap: primaryId !== safeSecondaryId && (primaryId !== 0 || safeSecondaryId !== 0),
  };
}

export function trackStatusBadgesForQuickSettings(track) {
  const metadata = track?.metadata ?? {};
  const badges = [];
  if (metadata.default_track) badges.push("Default");
  if (metadata.forced) badges.push("Forced");
  if (metadata.external) badges.push("External");
  if (metadata.albumart) badges.push("Album Art");
  if (metadata.image) badges.push("Image");
  if (metadata.main_selection && !track?.selected) badges.push("Main");
  return badges;
}

export function subtitleTextStyleAvailable(track) {
  if (!track || normalizedTrackId(track.id) === 0) return false;
  const metadata = track.metadata ?? {};
  const codec = String(metadata.codec || "").toLowerCase();
  return !metadata.image && !["ass", "ssa", "hdmv_pgs_subtitle", "dvb_subtitle"].includes(codec);
}
