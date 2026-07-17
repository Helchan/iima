/**
 * Models AppKit's `acceptsFirstMouse(for:)` boundary for the WebView-owned
 * video surface. A focus event caused by an activation click is committed on
 * the next task, so the pointer sequence from that same native event remains
 * distinguishable from a later click in an already-active window.
 */
export class FirstMouseGate {
  constructor({
    active = true,
    acceptsFirstMouse = () => false,
    doubleClickInterval = 500,
    now = () => performance.now(),
  } = {}) {
    this.ready = Boolean(active);
    this.acceptsFirstMouse = acceptsFirstMouse;
    this.doubleClickInterval = doubleClickInterval;
    this.now = now;
    this.focusEpoch = 0;
    this.suppressedPointers = new Map();
    this.suppressedClickButtons = new Set();
    this.suppressDoubleClickUntil = 0;
  }

  blur() {
    this.focusEpoch += 1;
    this.ready = false;
    this.suppressedPointers.clear();
    this.suppressedClickButtons.clear();
    this.suppressDoubleClickUntil = 0;
  }

  beginFocus() {
    this.focusEpoch += 1;
    this.ready = false;
    return this.focusEpoch;
  }

  commitFocus(epoch) {
    if (epoch !== this.focusEpoch) return false;
    this.ready = true;
    return true;
  }

  shouldSuppressPointer(event, phase) {
    const pointerId = Number(event?.pointerId);
    const button = Number(event?.button);
    if (this.acceptsFirstMouse()) {
      if (phase === "up" || phase === "cancel") this.suppressedPointers.delete(pointerId);
      return false;
    }
    if (phase === "down") {
      if (this.ready) return false;
      this.suppressedPointers.set(pointerId, Number.isInteger(button) ? button : 0);
      this.suppressedClickButtons.add(Number.isInteger(button) ? button : 0);
      this.suppressDoubleClickUntil = this.now() + this.doubleClickInterval;
      return true;
    }
    if (!this.suppressedPointers.has(pointerId)) return false;
    if (phase === "up" || phase === "cancel") this.suppressedPointers.delete(pointerId);
    return true;
  }

  shouldSuppressAction(event) {
    if (this.acceptsFirstMouse()) return false;
    const type = String(event?.type || "");
    const button = Number.isInteger(Number(event?.button)) ? Number(event.button) : 0;
    if (type === "dblclick" && this.now() <= this.suppressDoubleClickUntil) return true;
    if (this.suppressedClickButtons.delete(button)) return true;
    return !this.ready;
  }
}
