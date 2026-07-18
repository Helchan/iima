import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { isMacOSHost } from "../src/platform.js";
import { htmlForBuildPlatform } from "./platform-html.mjs";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const index = readFileSync(join(root, "src", "index.html"), "utf8");
const frontend = readFileSync(join(root, "src", "main.js"), "utf8");
const styles = readFileSync(join(root, "src", "styles.css"), "utf8");
const commands = readFileSync(join(root, "src-tauri", "src", "commands.rs"), "utf8");
const nativeWindow = readFileSync(join(root, "src-tauri", "src", "native_window.m"), "utf8");
const capability = JSON.parse(readFileSync(
  join(root, "src-tauri", "capabilities", "default.json"),
  "utf8",
));

assert.match(index, /<div id="media-title" class="media-title">IINA<\/div>/);
assert.match(frontend, /classList\.add\("platform-macos"\)/);
assert.match(styles, /\.platform-macos \.media-title\s*\{[^}]*display:\s*none;/s);
assert.doesNotMatch(styles, /(?<!platform-macos )\.media-title\s*\{[^}]*display:\s*none;/s);

assert.equal(isMacOSHost({ userAgentDataPlatform: "Unknown", platform: "MacIntel" }), true);
assert.equal(isMacOSHost({ userAgentDataPlatform: "Unknown", userAgent: "Mozilla/5.0 (Macintosh)" }), true);
assert.equal(isMacOSHost({ userAgentDataPlatform: "Windows", platform: "Win32" }), false);
assert.match(htmlForBuildPlatform(index, "darwin"), /<html lang="en" class="platform-macos">/);
assert.doesNotMatch(htmlForBuildPlatform(index, "win32"), /platform-macos/);

assert.match(index, /<div class="top-bar" data-tauri-drag-region="deep">/);
assert.ok(capability.windows.includes("player-*"));
assert.ok(capability.windows.includes("mini-player-*"));
assert.ok(capability.permissions.includes("core:window:allow-start-dragging"));
assert.match(styles, /\.player-window\s*\{[^}]*cursor:\s*default;[^}]*user-select:\s*none;/s);
assert.match(styles, /\.player-window input\[type="text"\][\s\S]*?cursor:\s*text;[\s\S]*?user-select:\s*text;/);
assert.match(styles, /\.top-bar\s*\{[^}]*pointer-events:\s*auto;/s);
assert.match(
  styles,
  /--player-titlebar-height:\s*22px/,
  "non-macOS titlebar height must retain IINA's 22 pt baseline",
);
assert.match(
  styles,
  /html\.platform-macos\s*\{[^}]*--player-titlebar-height:\s*28px/s,
  "modern macOS titlebar height must match IINA's 28 pt runtime layout",
);
assert.match(
  styles,
  /\.top-bar\s*\{[^}]*height:\s*var\(--player-titlebar-height\)/s,
  "titlebar material must use the platform titlebar height",
);
assert.match(
  styles,
  /\.player-window--osc-top \.osc\s*\{[^}]*top:\s*var\(--player-titlebar-height\)/s,
  "top OSC must remain below the platform titlebar height",
);

assert.match(commands, /player_window_chrome_visible\(snapshot, fullscreen\)/);
assert.match(commands, /player_state_uses_media_window\(snapshot\) && snapshot\.osc_visible && !fullscreen/);
assert.match(commands, /sync_player_window_title\(window, snapshot\)\?;\s*sync_player_window_chrome\(window, snapshot\)\?;/s);
assert.match(commands, /player_window_is_fullscreen\(window\)\?/);
assert.match(commands, /sync_player_window_chrome_for_fullscreen\(window, &snapshot, fullscreen\)\?;/);
const lifecycle = commands
  .split("pub(crate) fn observe_player_window_lifecycle")[1]
  ?.split("pub(crate) fn remove_player_window_lifecycle")[0];
assert.ok(lifecycle, "missing player-window lifecycle observer");
assert.match(
  lifecycle,
  /sync_player_window_chrome_for_fullscreen\(&window, &chrome_snapshot, fullscreen\)\?;/,
  "native green-button fullscreen must synchronize titlebar chrome",
);

for (const contract of [
  "NSWindowCloseButton",
  "NSWindowMiniaturizeButton",
  "NSWindowZoomButton",
  "NSWindowDocumentIconButton",
  "IIMAPlayerWindowTitleTextField",
  "IIMAPlayerTitleContainsEvent",
  "NSEventMaskLeftMouseDown",
  "NSEventMaskLeftMouseDragged",
  "NSEventMaskLeftMouseUp",
  "IIMAPlayerMinimumInitialDragDistance = 3.0",
  "IIMAPlayerTitleDragStarts",
  "distance <= IIMAPlayerMinimumInitialDragDistance",
  "[event.window performWindowDragWithEvent:event]",
  "titleTextField.editable = NO;",
  "titleTextField.selectable = NO;",
  "titleTextField.refusesFirstResponder = YES;",
  "IIMAPreparePlayerWindowChromeForAnimation",
  "iima_native_set_player_window_chrome_visible",
  "context.duration = 0.25;",
]) {
  assert.ok(nativeWindow.includes(contract), `missing native titlebar contract: ${contract}`);
}

const titleDragMonitor = nativeWindow
  .split("if (event.type == NSEventTypeLeftMouseDown)")[1]
  ?.split("IIMAEmitPlayerInput(event, targetLabel)")[0];
assert.ok(titleDragMonitor, "missing native title drag event monitor");
assert.match(
  titleDragMonitor,
  /IIMAPlayerTitleDragStarts[\s\S]*NSEventTypeLeftMouseDragged[\s\S]*distance <= IIMAPlayerMinimumInitialDragDistance[\s\S]*performWindowDragWithEvent[\s\S]*NSEventTypeLeftMouseUp/,
  "native filename drag must follow IINA's mouseDown/threshold/mouseDragged/mouseUp flow",
);
assert.doesNotMatch(
  titleDragMonitor.split("NSEventTypeLeftMouseDragged")[0],
  /performWindowDragWithEvent/,
  "mouseDown must not immediately start or consume a window drag",
);

assert.doesNotMatch(
  nativeWindow,
  /IIMAApplyPlayerWindowChromeAlpha\(window, shouldShow \? 0\.0 : 1\.0\)/,
  "animation reversals must continue from the current presentation alpha",
);

console.log("Player titlebar has one macOS title owner and follows OSC visibility");
