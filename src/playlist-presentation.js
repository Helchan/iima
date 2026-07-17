export function playlistMetadata(detail, preferences, isMusicMode) {
  if (preferences?.playlistShowMetadata === false) return null;
  if (preferences?.playlistShowMetadataInMusicMode !== false && !isMusicMode) return null;
  const title = String(detail?.metadata_title || "").trim();
  const artist = String(detail?.metadata_artist || "").trim();
  if (!title || !artist) return null;
  return { title, artist };
}

export function playlistDurationSummary(details, selectedIndexes = []) {
  if (
    !Array.isArray(details)
    || details.length === 0
    || details.some((item) => {
      const duration = item?.duration_seconds;
      return !item?.ready || duration === null || duration === undefined || !Number.isFinite(Number(duration));
    })
  ) {
    return null;
  }
  const duration = (item) => {
    const value = Number(item?.duration_seconds);
    return Number.isFinite(value) && value > 0 ? value : 0;
  };
  return {
    totalSeconds: details.reduce((total, item) => total + duration(item), 0),
    selectedSeconds: selectedIndexes.reduce(
      (total, index) => total + duration(details[index]),
      0,
    ),
  };
}

export function playlistProgressFraction(detail) {
  const duration = Number(detail?.duration_seconds);
  const progress = Number(detail?.playback_progress_seconds);
  if (!Number.isFinite(duration) || duration <= 0 || !Number.isFinite(progress) || progress < 0) {
    return 0;
  }
  return Math.max(0, Math.min(1, progress / duration));
}
