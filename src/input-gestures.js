export const NSEventPhase = Object.freeze({
  none: 0,
  began: 1,
  stationary: 2,
  changed: 4,
  ended: 8,
  cancelled: 16,
  mayBegin: 32,
});

export const ScrollAction = Object.freeze({
  volume: 0,
  seek: 1,
  none: 2,
  passToMpv: 3,
});

export const PinchAction = Object.freeze({
  windowSize: 0,
  fullscreen: 1,
  none: 2,
});

export const MINIMUM_INITIAL_DRAG_DISTANCE = 3;

const SEEK_AMOUNT_PRECISE = Object.freeze([0, 0.05, 0.1, 0.25, 0.5]);
const SEEK_AMOUNT_MOUSE = Object.freeze([0, 0.5, 1, 2, 4]);
const VOLUME_AMOUNT_PRECISE = Object.freeze([0, 0.25, 0.5, 0.75, 1]);

export function phaseContains(phase, flag) {
  return (Number(phase) & flag) !== 0;
}

export function iinaScrollDirection(event, previousDirection = null) {
  const phase = Number(event.phase) || 0;
  const isMouse = phase === NSEventPhase.none;
  if (isMouse || phaseContains(phase, NSEventPhase.began)) {
    if (Number(event.delta_x) !== 0) return "horizontal";
    if (Number(event.delta_y) !== 0) return "vertical";
  }
  if (!previousDirection && !phaseContains(phase, NSEventPhase.ended | NSEventPhase.cancelled)) {
    if (Number(event.delta_x) !== 0) return "horizontal";
    if (Number(event.delta_y) !== 0) return "vertical";
  }
  return previousDirection;
}

export function iinaScrollDelta(event, direction) {
  if (direction !== "horizontal" && direction !== "vertical") return 0;
  const precise = Boolean(event.precise);
  let deltaX = precise ? Number(event.delta_x) || 0 : Math.sign(Number(event.delta_x) || 0);
  let deltaY = precise ? Number(event.delta_y) || 0 : Math.sign(Number(event.delta_y) || 0) * 2;
  if (event.natural) deltaY = -deltaY;
  else deltaX = -deltaX;
  return direction === "horizontal" ? deltaX : deltaY;
}

export function iinaScrollAmount(action, event, direction, preferenceIndex) {
  const delta = iinaScrollDelta(event, direction);
  const index = Math.max(1, Math.min(4, Math.round(Number(preferenceIndex) || 1)));
  const isMouse = (Number(event.phase) || 0) === NSEventPhase.none;
  if (action === ScrollAction.seek) {
    return (isMouse ? SEEK_AMOUNT_MOUSE[index] : SEEK_AMOUNT_PRECISE[index]) * delta;
  }
  if (action === ScrollAction.volume) {
    return isMouse ? delta : VOLUME_AMOUNT_PRECISE[index] * delta;
  }
  return 0;
}

export class IinaScrollGestureState {
  direction = null;
  action = ScrollAction.none;
  wasPlayingBeforeSeeking = false;

  advance(event, resolveAction, isPlaying) {
    const phase = Number(event.phase) || 0;
    const isMouse = phase === NSEventPhase.none;
    const began = phaseContains(phase, NSEventPhase.began);
    const ended = phaseContains(phase, NSEventPhase.ended | NSEventPhase.cancelled);
    this.direction = iinaScrollDirection(event, this.direction);
    if (isMouse || began || this.action === ScrollAction.none) {
      this.action = normalizeScrollAction(resolveAction(this.direction));
    }
    const pause = !isMouse && began && this.action === ScrollAction.seek && Boolean(isPlaying);
    if (pause) this.wasPlayingBeforeSeeking = true;
    const resume = !isMouse && ended && this.wasPlayingBeforeSeeking;
    const result = {
      action: this.action,
      direction: this.direction,
      isMouse,
      began,
      ended,
      pause,
      resume,
    };
    if (ended) {
      this.direction = null;
      this.action = ScrollAction.none;
      this.wasPlayingBeforeSeeking = false;
    }
    return result;
  }
}

export function normalizeScrollAction(value) {
  const action = Number(value);
  return Number.isInteger(action) && action >= ScrollAction.volume && action <= ScrollAction.passToMpv
    ? action
    : ScrollAction.none;
}

export function normalizePinchAction(value) {
  const action = Number(value);
  return Number.isInteger(action) && action >= PinchAction.windowSize && action <= PinchAction.none
    ? action
    : PinchAction.none;
}

export function exceedsWindowDragThreshold(start, current) {
  const deltaX = Number(current?.x) - Number(start?.x);
  const deltaY = Number(current?.y) - Number(start?.y);
  return Number.isFinite(deltaX)
    && Number.isFinite(deltaY)
    && Math.hypot(deltaX, deltaY) > MINIMUM_INITIAL_DRAG_DISTANCE;
}
