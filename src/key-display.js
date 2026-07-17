// IINA's KeyCodeHelper has two related display paths:
// 1. input.conf keys are rendered with macOS modifier/key glyphs;
// 2. a newly recorded NSEvent uses the active keyboard layout for its printable label.
// KeyboardEvent.key already contains the WebKit/macOS layout-resolved character, while
// KeyboardEvent.code lets us retain IINA/mpv's physical-key fallback for non-ASCII input.

const PRETTY_MPV_KEYS = new Map([
  ["META", "⌘"],
  ["SHIFT", "⇧"],
  ["ALT", "⌥"],
  ["CTRL", "⌃"],
  ["SHARP", "#"],
  ["ENTER", "↩︎"],
  ["KP_ENTER", "↩︎"],
  ["SPACE", "␣"],
  ["IDEOGRAPHIC_SPACE", "␣"],
  ["BS", "⌫"],
  ["DEL", "⌦"],
  ["KP_DEL", "⌦"],
  ["INS", "Ins"],
  ["KP_INS", "Ins"],
  ["TAB", "⇥"],
  ["ESC", "⎋"],
  ["UP", "↑"],
  ["DOWN", "↓"],
  ["LEFT", "←"],
  ["RIGHT", "→"],
  ["PGUP", "⇞"],
  ["PGDWN", "⇟"],
  ["HOME", "↖︎"],
  ["END", "↘︎"],
  ["PLAY", "▶︎ ❙ ❙"],
  ["PLAYPAUSE", "▶︎ ❙ ❙"],
  ["PREV", "◀︎◀︎"],
  ["REWIND", "◀︎◀︎"],
  ["NEXT", "▶︎▶︎"],
  ["FORWARD", "▶︎▶︎"],
  ["STOP", "■︎"],
  ["PLUS", "+"],
  ["KP_DEC", "."],
  ["KP0", "0"],
  ["KP1", "1"],
  ["KP2", "2"],
  ["KP3", "3"],
  ["KP4", "4"],
  ["KP5", "5"],
  ["KP6", "6"],
  ["KP7", "7"],
  ["KP8", "8"],
  ["KP9", "9"],
]);

const DOM_KEY_TO_MPV_KEY = new Map([
  [" ", "SPACE"],
  ["Spacebar", "SPACE"],
  ["Escape", "ESC"],
  ["Esc", "ESC"],
  ["Enter", "ENTER"],
  ["Tab", "TAB"],
  ["Backspace", "BS"],
  ["Delete", "DEL"],
  ["Del", "DEL"],
  ["Insert", "INS"],
  ["Home", "HOME"],
  ["End", "END"],
  ["PageUp", "PGUP"],
  ["PageDown", "PGDWN"],
  ["ArrowLeft", "LEFT"],
  ["ArrowRight", "RIGHT"],
  ["ArrowUp", "UP"],
  ["ArrowDown", "DOWN"],
  ["PrintScreen", "PRINT"],
  ["Pause", "PAUSE"],
  ["MediaPlayPause", "PLAYPAUSE"],
  ["MediaPlay", "PLAY"],
  ["MediaPause", "PAUSE"],
  ["MediaStop", "STOP"],
  ["MediaTrackNext", "NEXT"],
  ["MediaTrackPrevious", "PREV"],
  ["AudioVolumeUp", "VOLUME_UP"],
  ["AudioVolumeDown", "VOLUME_DOWN"],
  ["AudioVolumeMute", "MUTE"],
]);

const NUMPAD_CODE_TO_MPV_KEY = new Map([
  ["NumpadDecimal", "KP_DEC"],
  ["NumpadEnter", "KP_ENTER"],
  ["NumpadAdd", "PLUS"],
  ["NumpadSubtract", "-"],
  ["NumpadMultiply", "*"],
  ["NumpadDivide", "/"],
  ["NumpadEqual", "="],
]);

// Carbon keyMap's printable US fallback, expressed with DOM physical-key codes.
const PHYSICAL_CODE_TO_MPV_KEY = new Map([
  ..."ABCDEFGHIJKLMNOPQRSTUVWXYZ".split("").map((letter) => [`Key${letter}`, letter.toLowerCase()]),
  ..."0123456789".split("").map((digit) => [`Digit${digit}`, digit]),
  ["Equal", "="],
  ["Minus", "-"],
  ["BracketRight", "]"],
  ["BracketLeft", "["],
  ["Quote", "'"],
  ["Semicolon", ";"],
  ["Backslash", "\\"],
  ["Comma", ","],
  ["Slash", "/"],
  ["Period", "."],
  ["Backquote", "`"],
]);

const MODIFIER_ONLY_KEYS = new Set([
  "Alt", "AltGraph", "CapsLock", "Control", "Fn", "FnLock", "Meta", "NumLock", "ScrollLock", "Shift", "Symbol", "SymbolLock",
]);

function isSingleCharacter(value) {
  return [...String(value ?? "")].length === 1;
}

function isAsciiPrintable(value) {
  return /^[!-~]$/u.test(String(value ?? ""));
}

export function isModifierOnlyKeyboardEvent(event) {
  return MODIFIER_ONLY_KEYS.has(String(event?.key ?? ""));
}

export function mpvKeyTokenFromKeyboardEvent(event) {
  const code = String(event?.code ?? "");
  const numpadDigit = code.match(/^Numpad([0-9])$/u);
  if (numpadDigit) return `KP${numpadDigit[1]}`;
  if (NUMPAD_CODE_TO_MPV_KEY.has(code)) return NUMPAD_CODE_TO_MPV_KEY.get(code);

  const rawKey = String(event?.key ?? "");
  if (DOM_KEY_TO_MPV_KEY.has(rawKey)) return DOM_KEY_TO_MPV_KEY.get(rawKey);
  if (/^F(?:[1-9]|1[0-9]|2[0-4])$/iu.test(rawKey)) return rawKey.toUpperCase();
  if (isAsciiPrintable(rawKey)) return rawKey;

  // IINA falls back to its physical Carbon key map when charactersIgnoringModifiers
  // is not a classic ASCII printable character. This keeps mpv keys stable while the
  // separately returned display string remains keyboard-layout aware.
  return PHYSICAL_CODE_TO_MPV_KEY.get(code) ?? rawKey;
}

export function macOSReadableKey(key) {
  const raw = String(key ?? "");
  const normalized = raw.toUpperCase();
  const pretty = PRETTY_MPV_KEYS.get(normalized);
  if (pretty !== undefined) return pretty;
  if (/^F(?:[1-9]|1[0-9]|2[0-4])$/u.test(normalized)) return normalized;
  return isSingleCharacter(raw) ? raw.toLocaleUpperCase() : raw;
}

export function macOSModifierSymbols(modifiers, key = "", inferShiftFromUppercase = true) {
  const flags = {
    ctrl: Boolean(modifiers?.ctrl),
    alt: Boolean(modifiers?.alt),
    shift: Boolean(modifiers?.shift)
      || (inferShiftFromUppercase && /^\p{Lu}$/u.test(String(key))),
    meta: Boolean(modifiers?.meta),
  };
  return [
    ["ctrl", "⌃"],
    ["alt", "⌥"],
    ["shift", "⇧"],
    ["meta", "⌘"],
  ].filter(([name]) => flags[name]).map(([, symbol]) => symbol).join("");
}

export function macOSReadableMpvKey(key, modifiers) {
  return `${macOSModifierSymbols(modifiers, key)}${macOSReadableKey(key)}`;
}

export function macOSReadableKeyboardEvent(event) {
  const rawKey = String(event?.key ?? "");
  const token = mpvKeyTokenFromKeyboardEvent(event);
  const displayKey = isSingleCharacter(rawKey) && rawKey !== " "
    ? rawKey.toLocaleUpperCase()
    : macOSReadableKey(token);
  const modifiers = {
    ctrl: Boolean(event?.ctrlKey),
    alt: Boolean(event?.altKey),
    shift: Boolean(event?.shiftKey),
    meta: Boolean(event?.metaKey),
  };
  return `${macOSModifierSymbols(modifiers, "", false)}${displayKey}`;
}

export function macOSReadableSavedFilterShortcut(key, modifiers = "") {
  const modifierSet = String(modifiers ?? "");
  return `${macOSModifierSymbols({
    ctrl: modifierSet.includes("c"),
    alt: modifierSet.includes("o"),
    shift: modifierSet.includes("s"),
    meta: modifierSet.includes("m"),
  }, key)}${macOSReadableKey(key)}`;
}
