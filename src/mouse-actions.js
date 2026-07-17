export const MouseClickAction = Object.freeze({
  none: 0,
  fullscreen: 1,
  pause: 2,
  hideOsc: 3,
  togglePip: 4,
});

const prefControlContext = (key) => Object.freeze({
  table: "PrefControlViewController",
  key,
});

// These are deliberately per-control. IINA 1.3.5 does not expose every
// MouseClickAction in every popup in PrefControlViewController.xib.
export const MOUSE_CLICK_ACTION_OPTIONS = Object.freeze({
  singleClickAction: Object.freeze([
    Object.freeze([MouseClickAction.hideOsc, "Hide OSC"]),
    Object.freeze([MouseClickAction.pause, "Pause / Resume", prefControlContext("A8c-wa-IFR.title")]),
    Object.freeze([MouseClickAction.none, "None", prefControlContext("w54-7I-QfW.title")]),
  ]),
  doubleClickAction: Object.freeze([
    Object.freeze([MouseClickAction.fullscreen, "Toggle fullscreen", prefControlContext("6F4-gm-oBg.title")]),
    Object.freeze([MouseClickAction.pause, "Pause / Resume", prefControlContext("Dm7-JG-fLd.title")]),
    Object.freeze([MouseClickAction.togglePip, "Toggle Picture-in-Picture", prefControlContext("rvS-xA-7bv.title")]),
    Object.freeze([MouseClickAction.none, "None", prefControlContext("wkV-Gq-B0T.title")]),
  ]),
  rightClickAction: Object.freeze([
    Object.freeze([MouseClickAction.hideOsc, "Hide OSC"]),
    Object.freeze([MouseClickAction.pause, "Pause / Resume", prefControlContext("mOX-Fi-oTv.title")]),
    Object.freeze([MouseClickAction.togglePip, "Toggle Picture-in-Picture", prefControlContext("ibM-4r-SQA.title")]),
    Object.freeze([MouseClickAction.none, "None", prefControlContext("oOk-y6-lek.title")]),
  ]),
  middleClickAction: Object.freeze([
    Object.freeze([MouseClickAction.hideOsc, "Hide OSC"]),
    Object.freeze([MouseClickAction.pause, "Pause / Resume", prefControlContext("uqM-UM-Xvb.title")]),
    Object.freeze([MouseClickAction.fullscreen, "Toggle fullscreen", prefControlContext("7wy-xt-c8Z.title")]),
    Object.freeze([MouseClickAction.togglePip, "Toggle Picture-in-Picture", prefControlContext("3Pm-nn-HpB.title")]),
    Object.freeze([MouseClickAction.none, "None", prefControlContext("unj-zt-RyU.title")]),
  ]),
  forceTouchAction: Object.freeze([
    Object.freeze([MouseClickAction.hideOsc, "Hide OSC"]),
    Object.freeze([MouseClickAction.pause, "Pause / Resume", prefControlContext("Ba4-eT-rTv.title")]),
    Object.freeze([MouseClickAction.fullscreen, "Toggle fullscreen", prefControlContext("hDh-Lk-FJe.title")]),
    Object.freeze([MouseClickAction.none, "None", prefControlContext("jnR-2I-4x4.title")]),
  ]),
});

// NSEvent.doubleClickInterval is 0.5s by default. The WebView does not expose
// that AppKit value, so this keeps the first click pending for the same default
// interval while the browser decides whether it belongs to a double click.
export const DEFAULT_DOUBLE_CLICK_INTERVAL_MS = 500;

export function normalizeMouseClickAction(value) {
  const action = Number(value);
  return Number.isInteger(action) && action >= MouseClickAction.none && action <= MouseClickAction.togglePip
    ? action
    : MouseClickAction.none;
}

export async function dispatchMouseClickAction(action, handlers) {
  switch (normalizeMouseClickAction(action)) {
    case MouseClickAction.fullscreen:
      await handlers.fullscreen?.();
      return true;
    case MouseClickAction.pause:
      await handlers.pause?.();
      return true;
    case MouseClickAction.hideOsc:
      await handlers.hideOsc?.();
      return true;
    case MouseClickAction.togglePip:
      await handlers.togglePip?.();
      return true;
    default:
      return false;
  }
}
