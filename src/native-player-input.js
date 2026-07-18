const NATIVE_ESCAPE_KEY_CODE = 53;

const NSEVENT_MODIFIER_FLAGS = Object.freeze({
  shift: 1 << 17,
  control: 1 << 18,
  option: 1 << 19,
  command: 1 << 20,
});

function hasModifier(flags, mask) {
  return (flags & mask) !== 0;
}

/**
 * Converts the AppKit escape bridge into the same KeyboardEvent shape that the
 * WebView normally receives. Other native key-down events deliberately return
 * null: AppKit continues delivering those to WebKit, so synthesizing them here
 * would execute a shortcut twice.
 */
export function nativeEscapeKeyboardEventInit(payload) {
  if (payload?.kind !== "key-down" || Number(payload.key_code) !== NATIVE_ESCAPE_KEY_CODE) {
    return null;
  }
  const modifiers = Number(payload.modifiers) || 0;
  return {
    key: "Escape",
    code: "Escape",
    bubbles: true,
    cancelable: true,
    composed: true,
    repeat: Boolean(payload.repeat),
    shiftKey: hasModifier(modifiers, NSEVENT_MODIFIER_FLAGS.shift),
    ctrlKey: hasModifier(modifiers, NSEVENT_MODIFIER_FLAGS.control),
    altKey: hasModifier(modifiers, NSEVENT_MODIFIER_FLAGS.option),
    metaKey: hasModifier(modifiers, NSEVENT_MODIFIER_FLAGS.command),
  };
}

export function nativePlayerMousePoint(payload) {
  if (payload?.kind !== "mouse-move") return null;
  const x = Number(payload.x);
  const y = Number(payload.y);
  if (!Number.isFinite(x) || !Number.isFinite(y)) return null;
  return { x, y };
}

/**
 * AppKit normally treats an otherwise-unhandled Escape as the standard way to
 * leave native fullscreen. The native bridge swallows the physical event to
 * guarantee one configured key-binding dispatch, so restore that AppKit
 * fallback only when neither a plugin nor the active key map handled Escape.
 */
export function shouldExitFullscreenForUnboundEscape(event, handled, fullscreen) {
  return !handled && Boolean(fullscreen) && event?.key === "Escape";
}
