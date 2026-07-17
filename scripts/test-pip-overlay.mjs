import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

const [html, css, frontend, nativeVideo, nativeVideoRust, commands, referenceXib, artwork] =
  await Promise.all([
    readFile(new URL("../src/index.html", import.meta.url), "utf8"),
    readFile(new URL("../src/styles.css", import.meta.url), "utf8"),
    readFile(new URL("../src/main.js", import.meta.url), "utf8"),
    readFile(new URL("../src-tauri/src/native_video.m", import.meta.url), "utf8"),
    readFile(new URL("../src-tauri/src/native_video.rs", import.meta.url), "utf8"),
    readFile(new URL("../src-tauri/src/commands.rs", import.meta.url), "utf8"),
    readFile(new URL("../参考/iina/iina/Base.lproj/MainWindowController.xib", import.meta.url), "utf8"),
    readFile(new URL("../src/assets/iina/icons/playing-in-pip.png", import.meta.url)),
  ]);

for (const contract of [
  'image="playing-in-pip"',
  'width="87" height="68"',
  'title="This video is playing in picture in picture"',
  'firstAttribute="centerY" secondItem="Nrz-jZ-Luf" secondAttribute="centerY"',
  'secondItem="YkI-ii-zaW" secondAttribute="bottom" constant="8"',
]) {
  assert.ok(referenceXib.includes(contract), `reference PIP overlay contract is missing: ${contract}`);
}

assert.ok(html.includes('id="pip-overlay" class="pip-overlay" hidden'));
assert.ok(html.includes('class="pip-overlay-icon" src="assets/iina/icons/playing-in-pip.png"'));
assert.ok(html.includes('class="pip-overlay-label">This video is playing in picture in picture</div>'));

for (const contract of [
  ".pip-overlay-icon {",
  "top: 50%;",
  "left: 50%;",
  "width: 87px;",
  "height: 68px;",
  "transform: translate(-50%, -50%);",
  ".pip-overlay-label {",
  "top: calc(50% + 42px);",
  "line-height: 16px;",
  "white-space: nowrap;",
  ".app-shell.theme-material--light .pip-overlay {",
  "background: rgba(242, 242, 247, 0.88);",
  ".app-shell.theme-material--light .pip-overlay-icon {",
  "filter: none;",
]) {
  assert.ok(css.includes(contract), `replicated PIP overlay CSS is missing: ${contract}`);
}

assert.equal(artwork.subarray(1, 4).toString("ascii"), "PNG");
assert.equal(artwork.readUInt32BE(16), 345);
assert.equal(artwork.readUInt32BE(20), 272);
assert.equal(
  createHash("sha256").update(artwork).digest("hex"),
  "ce9fa06eba86cbf3c910100a0102bb8434a9f9c9e3a22175ff0d36c2efff1246",
);

for (const contract of [
  "let pipOverlayClosing = false;",
  'tauriListen("iima-pip-will-close"',
  "pipOverlayClosing = exiting;",
  "function renderPipOverlay(nextState = state)",
  "Boolean(nextState?.pip_active) && !pipOverlayClosing",
  "if (!nextState.pip_active) pipOverlayClosing = false;",
]) {
  assert.ok(frontend.includes(contract), `PIP overlay state contract is missing: ${contract}`);
}

for (const contract of [
  "iima_native_video_pip_closing",
  "iima_native_video_pip_will_close_callback",
  "iima_native_video_set_pip_will_close_callback",
]) {
  assert.ok(nativeVideo.includes(contract), `native PIP closure contract is missing: ${contract}`);
}
const prepareStart = nativeVideo.indexOf("static void iima_native_video_prepare_for_pip_closure");
const replacementWindow = nativeVideo.indexOf('[pip setValue:parent forKey:@"replacementWindow"]', prepareStart);
const willCloseCallback = nativeVideo.indexOf("iima_native_video_pip_will_close_callback(", prepareStart);
assert.ok(prepareStart >= 0 && willCloseCallback > prepareStart && replacementWindow > willCloseCallback);
assert.ok(nativeVideoRust.includes('app.emit_to(session_label, "iima-pip-will-close", ())'));
assert.ok(commands.includes("native_video::register_pip_will_close_emitter(app);"));

console.log("PIP MainWindow overlay contracts pass");
