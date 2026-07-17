const IINA_COMMAND_PREFIX = "#@iina";
const MODIFIERS_IN_ORDER = ["Ctrl", "Alt", "Shift", "Meta"];

const SHIFTED_KEY_MAP = new Map([
  ["a", "A"], ["b", "B"], ["c", "C"], ["d", "D"], ["e", "E"], ["f", "F"],
  ["g", "G"], ["h", "H"], ["i", "I"], ["j", "J"], ["k", "K"], ["l", "L"],
  ["m", "M"], ["n", "N"], ["o", "O"], ["p", "P"], ["q", "Q"], ["r", "R"],
  ["s", "S"], ["t", "T"], ["u", "U"], ["v", "V"], ["w", "W"], ["x", "X"],
  ["y", "Y"], ["z", "Z"],
  ["1", "!"], ["2", "@"], ["3", "SHARP"], ["4", "$"], ["5", "%"],
  ["6", "^"], ["7", "&"], ["8", "*"], ["9", "("], ["0", ")"],
  ["=", "PLUS"], ["-", "_"], ["]", "}"], ["[", "{"], ["'", "\""],
  [";", ":"], ["\\", "|"], [",", "<"], ["/", "?"], [".", ">"], ["`", "~"],
]);
const SHIFTED_KEYS = new Set(SHIFTED_KEY_MAP.values());

export const IINA_DEFAULT_INPUT_CONF = `# default input config for IINA
# customized a little bit from the mpv one

#@iina Shift+Meta+v video-panel
#@iina Shift+Meta+a audio-panel
#@iina Shift+Meta+s sub-panel

Ctrl+Meta+v cycle video
Ctrl+Meta+s cycle sub
Ctrl+Meta+a cycle audio

SPACE cycle pause
Meta+. stop

RIGHT seek  5
LEFT  seek -5
Alt+RIGHT frame-step
Alt+LEFT frame-back-step

Shift+LEFT   sub-seek -1
Shift+RIGHT  sub-seek  1

Meta+s screenshot

Meta+l ab-loop
Meta+L cycle-values loop "inf" "no"

#@iina Shift+Meta+p playlist-panel
Meta+RIGHT playlist-next
Meta+LEFT playlist-prev

#@iina Shift+Meta+c chapter-panel
Shift+Meta+> add chapter 1
Shift+Meta+< add chapter -1

Meta+[ multiply speed 0.5
Meta+] multiply speed 2.0
Alt+Meta+[ multiply speed 0.9091
Alt+Meta+] multiply speed 1.1
Meta+\\ set speed 1.0

Meta+0 set window-scale 0.5
Meta+1 set window-scale 1
Meta+2 set window-scale 2
#@iina Meta+3 fit-to-screen

#@iina Meta+- smaller-window
#@iina Meta+= bigger-window

#@iina Ctrl+Meta+p toggle-pip
Ctrl+Meta+f cycle fullscreen
Ctrl+Meta+t cycle ontop

#@iina Alt+Meta+m toggle-music-mode

UP    add volume 5
DOWN  add volume -5
Alt+UP add volume 1
Alt+DOWN add volume -1

Meta+/ cycle mute

Shift+( add audio-delay 0.5
Shift+) add audio-delay -0.5
Alt+Shift+( add audio-delay 0.1
Alt+Shift+) add audio-delay -0.1
Shift+_ set audio-delay 0

#@iina Meta+D find-online-subs

Z add sub-delay -0.5
X add sub-delay 0.5
Alt+Z add sub-delay -0.1
Alt+X add sub-delay 0.1
C set sub-delay 0

ESC set fullscreen no
ENTER set fullscreen yes


# Alternative mpv key bindings

q quit

p cycle pause                          # toggle pause/playback mode

. frame-step                           # advance one frame and pause
, frame-back-step                      # go back by one frame and pause

m cycle mute

Shift+PGUP seek 600
Shift+PGDWN seek -600

G add sub-scale +0.1                   # increase the subtitle font size
F add sub-scale -0.1                   # decrease the subtitle font size

r add sub-pos -1
R add sub-pos +1
t add sub-pos +1

f cycle fullscreen                     # toggle fullscreen

E cycle edition                        # next edition

POWER quit
PLAY cycle pause
PAUSE cycle pause
PLAYPAUSE cycle pause
STOP quit
FORWARD seek 60
REWIND seek -60
NEXT playlist-next
PREV playlist-prev
VOLUME_UP add volume 2
VOLUME_DOWN add volume -2
MUTE cycle mute
CLOSE_WIN quit`;

function normalizeSingleMpvKeystroke(rawKeystroke) {
  if (rawKeystroke === "+") return "PLUS";

  const parts = rawKeystroke.replaceAll("++", "+PLUS").split("+");
  let key = parts.at(-1) ?? "";
  if (key === "#") key = "SHARP";
  else if (key === "+") key = "PLUS";
  else if (key.length > 1) key = key.toUpperCase();

  const modifiers = new Set();
  for (const rawModifier of parts.slice(0, -1)) {
    if (rawModifier.toLowerCase() === "shift") {
      if (SHIFTED_KEY_MAP.has(key)) key = SHIFTED_KEY_MAP.get(key);
      else if (!SHIFTED_KEYS.has(key)) modifiers.add("Shift");
    } else if (rawModifier.toLowerCase() === "meta") {
      modifiers.add("Meta");
    } else if (rawModifier.toLowerCase() === "ctrl") {
      modifiers.add("Ctrl");
    } else if (rawModifier.toLowerCase() === "alt") {
      modifiers.add("Alt");
    }
  }

  return [...MODIFIERS_IN_ORDER.filter((modifier) => modifiers.has(modifier)), key].join("+");
}

export function normalizeMpvKey(rawKey) {
  const key = String(rawKey ?? "");
  if (key === "default-bindings") return key;
  if ([...key].filter((character) => character === "-").length > 1) return key;
  return normalizeSingleMpvKeystroke(key);
}

export function normalizeModifiers(modifiers = {}) {
  return {
    alt: Boolean(modifiers.alt),
    ctrl: Boolean(modifiers.ctrl),
    meta: Boolean(modifiers.meta),
    shift: Boolean(modifiers.shift),
  };
}

export function mpvKeyFromParts(key, modifiers = {}) {
  const flags = normalizeModifiers(modifiers);
  const parts = [];
  if (flags.ctrl) parts.push("Ctrl");
  if (flags.alt) parts.push("Alt");
  if (flags.shift) parts.push("Shift");
  if (flags.meta) parts.push("Meta");
  parts.push(String(key ?? ""));
  return normalizeMpvKey(parts.join("+"));
}

export function mpvKeyToParts(rawKey) {
  const normalizedMpvKey = normalizeMpvKey(rawKey);
  const tokens = normalizedMpvKey.split("+");
  const modifiers = normalizeModifiers();
  while (tokens.length > 1 && MODIFIERS_IN_ORDER.includes(tokens[0])) {
    const modifier = tokens.shift();
    modifiers[modifier.toLowerCase()] = true;
  }
  return {
    key: tokens.join("+") || normalizedMpvKey,
    modifiers,
    normalizedMpvKey,
  };
}

export function normalizedAction(rawAction) {
  return String(rawAction ?? "")
    .split(/\s+/)
    .filter(Boolean)
    .join(" ");
}

export function keyMappingSection(rawAction) {
  const parts = normalizedAction(rawAction).split(" ").filter(Boolean);
  if (parts.length <= 1 || !parts[0].startsWith("{")) return null;
  const end = parts[0].indexOf("}");
  return end < 0 ? null : parts[0].slice(1, end).trim();
}

function rawActionFromSource(source) {
  if (typeof source.rawAction === "string") return source.rawAction;
  if (typeof source.rawCommand !== "string") return "";
  if (source.isIINACommand && source.rawCommand.startsWith(`${IINA_COMMAND_PREFIX} `)) {
    return source.rawCommand.slice(IINA_COMMAND_PREFIX.length).trim();
  }
  return source.rawCommand;
}

export function normalizeKeyMapping(source, index = 0) {
  if (!source || typeof source !== "object") return null;
  const fallbackRawKey = source.key ? mpvKeyFromParts(source.key, source.modifiers) : "";
  const rawKey = typeof source.rawKey === "string" ? source.rawKey : fallbackRawKey;
  const rawAction = rawActionFromSource(source);
  if (!rawKey.trim() || !normalizedAction(rawAction)) return null;

  const isIINACommand = Boolean(source.isIINACommand);
  const keyParts = mpvKeyToParts(rawKey);
  const action = normalizedAction(rawAction);
  const section = keyMappingSection(rawAction);
  let effectiveRawAction = rawAction;
  let inactiveReason = null;
  if (section === "default") {
    effectiveRawAction = action.split(" ").slice(1).join(" ");
  } else if (section !== null) {
    effectiveRawAction = null;
    inactiveReason = `section:${section}`;
  } else if (rawKey === "default-bindings" && action === "start") {
    effectiveRawAction = null;
    inactiveReason = "default-bindings";
  }

  return {
    id: typeof source.id === "string" && source.id ? source.id : `binding-${index}`,
    rawKey,
    rawAction,
    isIINACommand,
    comment: typeof source.comment === "string" ? source.comment : undefined,
    normalizedMpvKey: keyParts.normalizedMpvKey,
    key: keyParts.key,
    modifiers: keyParts.modifiers,
    normalizedRawAction: action,
    section,
    effectiveRawAction,
    runtimeEligible: effectiveRawAction !== null,
    inactiveReason,
    action: effectiveRawAction === null
      ? { type: "inactive", action, reason: inactiveReason }
      : isIINACommand
        ? { type: "iina-command", action: effectiveRawAction }
        : { type: "mpv-command", action: effectiveRawAction },
  };
}

export function serializeKeyMapping(mapping) {
  return {
    id: mapping.id,
    rawKey: mapping.rawKey,
    rawAction: mapping.rawAction,
    comment: mapping.comment,
    isIINACommand: Boolean(mapping.isIINACommand),
  };
}

export function keyMappingPreferenceRows(configured, defaultMappings = []) {
  if (configured === null || configured === undefined) return Array.from(defaultMappings ?? []);
  return Array.isArray(configured) ? Array.from(configured) : [];
}

export function parseInputConfLine(line, index = 0) {
  let working = String(line ?? "");
  let isIINACommand = false;
  if (!working.trim()) return null;
  if (working.startsWith("#")) {
    if (!working.startsWith(IINA_COMMAND_PREFIX)) return null;
    isIINACommand = true;
    working = working.slice(IINA_COMMAND_PREFIX.length);
  }

  let comment;
  const commentIndex = working.indexOf("#");
  if (commentIndex >= 0) {
    comment = working.slice(commentIndex + 1);
    working = working.slice(0, commentIndex);
  }

  const match = working.trim().match(/^(\S+)\s+(.+)$/);
  if (!match) return null;
  return normalizeKeyMapping({
    id: `input-conf-${index}`,
    rawKey: match[1].trim(),
    rawAction: match[2].trim(),
    isIINACommand,
    comment,
  }, index);
}

export function parseInputConf(source) {
  return String(source ?? "")
    .split(/\r?\n/)
    .map(parseInputConfLine)
    .filter(Boolean);
}

export function keyMappingConfFileFormat(source, index = 0) {
  const mapping = normalizeKeyMapping(source, index);
  if (!mapping) return null;
  const prefix = mapping.isIINACommand ? `${IINA_COMMAND_PREFIX} ` : "";
  const comment = mapping.comment === undefined || mapping.comment === "" ? "" : `   #${mapping.comment}`;
  return `${prefix}${mapping.rawKey} ${mapping.normalizedRawAction}${comment}`;
}

export function generateInputConf(mappings) {
  return Array.from(mappings ?? []).reduce((contents, mapping, index) => {
    const line = keyMappingConfFileFormat(mapping, index);
    return line === null ? contents : `${contents}${line}\n`;
  }, "# Generated by IINA\n\n");
}

export function keyMappingSignature(mapping) {
  return mapping.normalizedMpvKey || normalizeMpvKey(mapping.rawKey);
}

export function activeKeyMappingsLastWins(mappings) {
  const order = [];
  const active = new Map();
  Array.from(mappings ?? []).forEach((source, index) => {
    const mapping = normalizeKeyMapping(source, index);
    if (!mapping?.runtimeEligible) return;
    const signature = keyMappingSignature(mapping);
    if (!active.has(signature)) order.push(signature);
    active.set(signature, mapping);
  });
  return order.map((signature) => active.get(signature));
}

export function keyMappingConflictState(mappings) {
  const indexesBySignature = new Map();
  Array.from(mappings ?? []).forEach((source, index) => {
    const mapping = normalizeKeyMapping(source, index);
    if (!mapping?.runtimeEligible) return;
    const signature = keyMappingSignature(mapping);
    const indexes = indexesBySignature.get(signature) ?? [];
    indexes.push(index);
    indexesBySignature.set(signature, indexes);
  });

  const duplicateSignatures = new Set();
  const activeIndexes = new Set();
  const shadowedIndexes = new Set();
  for (const [signature, indexes] of indexesBySignature) {
    if (indexes.length < 2) continue;
    duplicateSignatures.add(signature);
    activeIndexes.add(indexes.at(-1));
    indexes.slice(0, -1).forEach((index) => shadowedIndexes.add(index));
  }
  return { duplicateSignatures, activeIndexes, shadowedIndexes };
}

export function removeShadowedKeyMappings(mappings) {
  const rows = Array.from(mappings ?? []);
  const { shadowedIndexes } = keyMappingConflictState(rows);
  return rows.filter((_, index) => !shadowedIndexes.has(index));
}
