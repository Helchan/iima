function fallbackAction(rawAction) {
  return { type: "mpv-command", action: rawAction };
}

function finiteNumber(value) {
  if (value === undefined || value === "") return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function booleanMpvValue(value) {
  const normalized = String(value ?? "").toLowerCase();
  if (["yes", "true", "1", "on"].includes(normalized)) return true;
  if (["no", "false", "0", "off"].includes(normalized)) return false;
  return null;
}

function relativeSeekOption(value) {
  // Keep an ordinary `seek ±N` binding on mpv's default relative/keyframe
  // path. IINA forwards these bindings verbatim; its adaptive exact-seek
  // policy is used by the scroll gesture path, not by keyboard mappings.
  if (value === undefined) return "relative";
  const normalized = String(value).toLowerCase();
  if (normalized === "auto") return "auto";
  if (["relative", "keyframes", "relative+keyframes"].includes(normalized)) return "relative";
  if (["exact", "relative+exact"].includes(normalized)) return "exact";
  return null;
}

/**
 * Classifies the ordinary mpv actions that iima must execute through its typed
 * player/window paths. Keeping this pure makes production and the browser mock
 * agree while preserving every unrecognised action verbatim for mpv.
 */
export function classifyMpvKeyAction(rawAction) {
  const action = String(rawAction ?? "").trim();
  const fallback = fallbackAction(action);
  const parts = action.split(/\s+/).filter(Boolean);
  if (parts[0] === "{default}") parts.shift();
  const [verb, property, value, ...rest] = parts;

  if (verb === "seek" && rest.length === 0) {
    const seconds = finiteNumber(property);
    const option = relativeSeekOption(value);
    if (seconds !== null && option !== null) {
      return { type: "seek-relative", seconds, option };
    }
  }

  if (verb === "add" && property === "volume" && rest.length === 0) {
    const amount = finiteNumber(value);
    if (amount !== null) return { type: "volume-relative", amount };
  }

  if (verb === "cycle" && value === undefined) {
    if (property === "fullscreen") return { type: "fullscreen-toggle" };
    if (property === "pause") return { type: "player", command: { type: "toggle-pause" } };
    if (property === "mute") return { type: "player", command: { type: "toggle-mute" } };
    const trackKind = {
      video: "video",
      audio: "audio",
      sub: "subtitles",
      sid: "subtitles",
      "secondary-sid": "second-subtitles",
    }[property];
    if (trackKind) return { type: "player", command: { type: "cycle-track", kind: trackKind } };
  }

  if (verb === "set" && rest.length === 0) {
    if (property === "fullscreen") {
      const fullscreen = booleanMpvValue(value);
      if (fullscreen !== null) return { type: "fullscreen-set", fullscreen };
    }
    if (property === "pause") {
      const paused = booleanMpvValue(value);
      if (paused !== null) {
        return { type: "player", command: { type: paused ? "pause" : "resume" } };
      }
    }
    if (property === "speed") {
      const speed = finiteNumber(value);
      if (speed !== null) return { type: "player", command: { type: "set-speed", speed } };
    }
  }

  if (verb === "multiply" && property === "speed" && rest.length === 0) {
    const factor = finiteNumber(value);
    if (factor !== null) return { type: "player", command: { type: "multiply-speed", factor } };
  }

  if (property === undefined) {
    const playerCommand = {
      "frame-step": { type: "frame-step", backwards: false },
      "frame-back-step": { type: "frame-step", backwards: true },
      "playlist-next": { type: "playlist-next" },
      "playlist-prev": { type: "playlist-prev" },
      "ab-loop": { type: "cycle-ab-loop" },
      stop: { type: "stop" },
      quit: { type: "stop" },
    }[verb];
    if (playerCommand) return { type: "player", command: playerCommand };
    if (verb === "screenshot") return { type: "screenshot" };
  }

  return fallback;
}
