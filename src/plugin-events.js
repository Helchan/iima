export function normalizePluginEventName(name) {
  const normalized = String(name || "").trim();
  const parts = normalized.split(".");
  const validPrefix = parts[0] === "mpv" || parts[0] === "iina";
  const validShape = parts.length === 2 || (parts.length === 3 && parts[2] === "changed");
  if (!validPrefix || !validShape || !parts[1]) {
    throw new Error(`Incorrect event name syntax: \"${normalized}\"`);
  }
  return normalized;
}

export function pluginChangedMpvProperty(name) {
  const normalized = normalizePluginEventName(name);
  const parts = normalized.split(".");
  return parts[0] === "mpv" && parts.length === 3 ? parts[1] : null;
}

export function pluginMpvPropertyEventValue(property) {
  switch (property?.format) {
    case "flag":
      return property.value === true || property.value === "true" || property.value === "yes" || property.value === "1";
    case "int64": {
      const value = Number.parseInt(String(property.value ?? "0"), 10);
      return Number.isFinite(value) ? value : 0;
    }
    case "double": {
      const value = Number(property.value);
      return Number.isFinite(value) ? value : 0;
    }
    case "string":
      return property.value == null ? "" : String(property.value);
    default:
      // IINA passes 0 for MPV_FORMAT_NONE and formats it does not explicitly bridge.
      return 0;
  }
}

/**
 * Consumes one per-player native mpv event batch without collapsing repeated events.
 *
 * `emit` follows EventController's `(name, ...args)` shape. `currentUrl` is only a fallback for
 * `iina.file-loaded`; normally the preceding observed `path` event owns the exact URL.
 */
export function consumePluginMpvEventBatch(
  previous,
  batch,
  emit,
  currentUrl = () => null,
) {
  let cursor = Number.isSafeInteger(previous?.cursor) ? previous.cursor : 0;
  let path = typeof previous?.path === "string" ? previous.path : null;
  for (const record of Array.from(batch?.events || [])) {
    const nextCursor = Number(record?.cursor);
    if (!Number.isSafeInteger(nextCursor) || nextCursor <= cursor) continue;
    cursor = nextCursor;
    const event = record?.event;
    if (!event || typeof event.name !== "string" || event.name === "none") continue;

    const property = event.property;
    if (property?.name) {
      const value = pluginMpvPropertyEventValue(property);
      if (property.name === "path" && typeof value === "string" && value) path = value;
      emit(`mpv.${property.name}.changed`, value);
    }

    if (event.name === "start-file") {
      emit("iina.file-started");
    } else if (event.name === "file-loaded") {
      emit("iina.file-loaded", path || currentUrl() || "");
    }
    emit(`mpv.${event.name}`);
  }
  const batchCursor = Number(batch?.cursor);
  if (Number.isSafeInteger(batchCursor) && batchCursor > cursor && !(batch?.events?.length)) {
    cursor = batchCursor;
  }
  return { cursor, path };
}
