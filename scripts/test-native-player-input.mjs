import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  nativeEscapeKeyboardEventInit,
  nativePlayerMousePoint,
  shouldExitFullscreenForUnboundEscape,
} from "../src/native-player-input.js";

assert.deepEqual(nativePlayerMousePoint({ kind: "mouse-move", x: 123.5, y: 44 }), {
  x: 123.5,
  y: 44,
});
assert.equal(nativePlayerMousePoint({ kind: "scroll", x: 1, y: 2 }), null);
assert.equal(nativePlayerMousePoint({ kind: "mouse-move", x: "not-a-point", y: 2 }), null);

assert.deepEqual(
  nativeEscapeKeyboardEventInit({
    kind: "key-down",
    key_code: 53,
    modifiers: (1 << 17) | (1 << 19) | (1 << 20),
    repeat: true,
  }),
  {
    key: "Escape",
    code: "Escape",
    bubbles: true,
    cancelable: true,
    composed: true,
    repeat: true,
    shiftKey: true,
    ctrlKey: false,
    altKey: true,
    metaKey: true,
  },
);
assert.equal(
  nativeEscapeKeyboardEventInit({ kind: "key-down", key_code: 124, modifiers: 0 }),
  null,
  "ordinary keys must keep their single normal WebKit delivery",
);
assert.equal(nativeEscapeKeyboardEventInit({ kind: "mouse-move", key_code: 53 }), null);

assert.equal(shouldExitFullscreenForUnboundEscape({ key: "Escape" }, false, true), true);
assert.equal(shouldExitFullscreenForUnboundEscape({ key: "Escape" }, true, true), false);
assert.equal(shouldExitFullscreenForUnboundEscape({ key: "Escape" }, false, false), false);
assert.equal(shouldExitFullscreenForUnboundEscape({ key: "Enter" }, false, true), false);

const frontendSource = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
for (const contract of [
  "window.addEventListener(\"keydown\", handleWindowKeyDown);",
  "function dispatchNativePlayerKeyDown(payload)",
  "shouldExitFullscreenForUnboundEscape(event, handled, windowFullscreenActive)",
  "new KeyboardEvent(\"keydown\", init)",
  "function scheduleNativePlayerMouseMove(payload)",
  "requestAnimationFrame(() =>",
  "handlePlayerPointerMovementForTarget(target);",
]) {
  assert.ok(frontendSource.includes(contract), `missing native player input contract: ${contract}`);
}

const nativeSource = readFileSync(
  new URL("../src-tauri/src/native_window.m", import.meta.url),
  "utf8",
);
for (const contract of [
  "unsigned long long eventPhase = 0;",
  "unsigned long long momentumPhase = 0;",
  "momentumPhase = (unsigned long long)event.momentumPhase;",
  "           momentumPhase,",
]) {
  assert.ok(nativeSource.includes(contract), `missing safe native phase contract: ${contract}`);
}
assert.ok(
  !nativeSource.includes(": (unsigned long long)event.phase;"),
  "mouse and pressure events must not read gesture-only phase metadata",
);
assert.ok(
  !nativeSource.includes("           (unsigned long long)event.momentumPhase,"),
  "non-scroll events must not read scroll-only momentum metadata",
);

if (process.platform === "darwin") {
  const temporary = mkdtempSync(join(tmpdir(), "iima-native-player-input-"));
  try {
    const sourcePath = new URL("../src-tauri/src/native_window.m", import.meta.url).pathname;
    const harnessPath = join(temporary, "native-player-input-harness.m");
    const executablePath = join(temporary, "native-player-input-harness");
    writeFileSync(harnessPath, `
#import <Cocoa/Cocoa.h>
#include ${JSON.stringify(sourcePath)}

static int capturedKind = 0;
static unsigned long long capturedPhase = 99;
static unsigned long long capturedMomentumPhase = 99;

static void capturePlayerInput(const char *windowLabel,
                               int kind,
                               double x,
                               double y,
                               double deltaX,
                               double deltaY,
                               int precise,
                               int natural,
                               unsigned long long phase,
                               unsigned long long momentumPhase,
                               int stage,
                               double magnification,
                               void *context) {
  (void)windowLabel; (void)x; (void)y; (void)deltaX; (void)deltaY;
  (void)precise; (void)natural; (void)stage; (void)magnification; (void)context;
  capturedKind = kind;
  capturedPhase = phase;
  capturedMomentumPhase = momentumPhase;
}

int main(void) {
  @autoreleasepool {
    IIMAPlayerInputHandler = capturePlayerInput;
    IIMAPlayerInputContext = NULL;
    NSEvent *mouseMove = [NSEvent mouseEventWithType:NSEventTypeMouseMoved
                                             location:NSMakePoint(12, 34)
                                        modifierFlags:0
                                            timestamp:1
                                         windowNumber:0
                                              context:nil
                                          eventNumber:1
                                           clickCount:0
                                             pressure:0];
    IIMAEmitPlayerInput(mouseMove, @"main");
    if (capturedKind != 4 || capturedPhase != 0 || capturedMomentumPhase != 0) return 10;

    capturedKind = 0;
    capturedPhase = 99;
    capturedMomentumPhase = 99;
    NSEvent *keyDown = [NSEvent keyEventWithType:NSEventTypeKeyDown
                                        location:NSZeroPoint
                                   modifierFlags:NSEventModifierFlagCommand
                                       timestamp:2
                                    windowNumber:0
                                         context:nil
                                      characters:@"x"
                     charactersIgnoringModifiers:@"x"
                                        isARepeat:NO
                                          keyCode:53];
    IIMAEmitPlayerInput(keyDown, @"main");
    if (capturedKind != 5 || capturedPhase == 0 || capturedMomentumPhase != 0) return 11;
  }
  return 0;
}
`);
    const compile = spawnSync(
      "xcrun",
      [
        "--sdk", "macosx", "clang", "-fobjc-arc", "-fblocks",
        harnessPath, "-framework", "Cocoa", "-framework", "IOKit", "-o", executablePath,
      ],
      { encoding: "utf8" },
    );
    assert.equal(
      compile.status,
      0,
      `native player input harness failed to compile:\n${compile.stdout}\n${compile.stderr}`,
    );
    const run = spawnSync(executablePath, [], { encoding: "utf8" });
    assert.equal(
      run.status,
      0,
      `MouseMoved/KeyDown metadata caused an AppKit exception or wrong ABI output:\n${run.stdout}\n${run.stderr}`,
    );
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

console.log("native player input bridge: ok");
