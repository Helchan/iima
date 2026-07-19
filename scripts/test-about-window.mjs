import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const referenceRoot = join(root, "参考", "iina", "iina");
const read = (path) => readFileSync(join(root, path), "utf8");
const sha256 = (contents) => createHash("sha256").update(contents).digest("hex");
const rtfText = (path) => execFileSync(
  "/usr/bin/textutil",
  ["-convert", "txt", "-stdout", path],
  { encoding: "utf8", maxBuffer: 4 * 1024 * 1024 },
).replaceAll("\r\n", "\n");
const rtfHtml = (path) => {
  const html = execFileSync(
    "/usr/bin/textutil",
    ["-convert", "html", "-stdout", path],
    { encoding: "utf8", maxBuffer: 4 * 1024 * 1024 },
  ).replaceAll("\r\n", "\n");
  return {
    style: html.match(/<style type="text\/css">\s*([\s\S]*?)\s*<\/style>/)?.[1] ?? "",
    body: html.match(/<body>\s*([\s\S]*?)\s*<\/body>/)?.[1] ?? "",
  };
};

const swift = read("参考/iina/iina/AboutWindowController.swift");
const avatarSwift = read("参考/iina/iina/AboutWindowContributorAvatarItem.swift");
const xib = read("参考/iina/iina/Base.lproj/AboutWindowController.xib");
const avatarXib = read("参考/iina/iina/AboutWindowContributorAvatarItem.xib");
const backend = read("src-tauri/src/about_window.rs");
const mpvBackend = read("src-tauri/src/mpv.rs");
const lib = read("src-tauri/src/lib.rs");
const menu = read("src-tauri/src/menu.rs");
const html = read("src/about.html");
const css = read("src/about.css");
const runtime = read("src/about.js");
const generator = read("scripts/generate-localizations.mjs");
const documents = JSON.parse(read("src/assets/iina/about-documents.json"));

assert.match(swift, /window\?\.titlebarAppearsTransparent = true/);
assert.match(swift, /window\?\.titleVisibility = \.hidden/);
assert.match(xib, /contentRect" x="196" y="240" width="640" height="400"/);
assert.match(xib, /frame" x="0\.0" y="0\.0" width="220" height="400"/);
assert.match(xib, /frame" x="70" y="280" width="80" height="80"/);
assert.match(xib, /frame" x="220" y="20" width="400" height="344"/);
assert.match(xib, /itemSize" width="32" height="32"/);
assert.match(avatarXib, /frame" x="0\.0" y="0\.0" width="50" height="50"/);
assert.match(avatarSwift, /cornerRadius = imageView\.frame\.width \/ 2/);

assert.match(backend, /const IINA_VERSION: &str = "0\.9\.4"/);
assert.match(backend, /const IINA_BUILD: &str = "94"/);
assert.match(backend, /\.inner_size\(640\.0, 400\.0\)/);
assert.match(backend, /\.min_inner_size\(640\.0, 400\.0\)/);
assert.match(backend, /\.max_inner_size\(640\.0, 400\.0\)/);
assert.match(backend, /\.title_bar_style\(tauri::TitleBarStyle::Overlay\)/);
assert.match(backend, /\.hidden_title\(true\)/);
assert.match(backend, /if let Some\(window\) = app\.get_webview_window\(ABOUT_WINDOW_LABEL\)/);
assert.match(backend, /Command::new\("\/usr\/bin\/open"\)/);
assert.match(backend, /AboutLink::from_id/);
assert.match(backend, /mpv::libmpv_runtime_versions\(\)/);
assert.doesNotMatch(backend, /media::media_runtime\(\)/);
assert.match(mpvBackend, /client\.get_string_property\("mpv-version"\)/);
assert.match(mpvBackend, /client\.get_string_property\("ffmpeg-version"\)/);
assert.match(mpvBackend, /\("vo", "null"\)/);
assert.match(mpvBackend, /\("ao", "null"\)/);

assert.doesNotMatch(menu, /PredefinedMenuItem::about\(/);
assert.match(menu, /"iina\.about"/);
assert.match(menu, /crate::about_window::show_about_window\(app\)/);
for (const command of ["show_about", "get_about_runtime", "open_about_link"]) {
  assert.match(lib, new RegExp(`\\b${command}\\b`));
}
assert.match(lib, /label == about_window::ABOUT_WINDOW_LABEL/);
assert.match(lib, /api\.prevent_close\(\)/);

assert.match(html, /assets\/iina\/app-icon\.png/);
for (const tab of ["license", "contributors", "credits"]) {
  assert.match(html, new RegExp(`data-tab="${tab}"`));
}
assert.match(css, /grid-template-columns: 220px 420px/);
assert.match(css, /width: 640px/);
assert.match(css, /height: 400px/);
assert.match(css, /\.about-icon \{[\s\S]*?width: 80px;[\s\S]*?height: 80px;/);
assert.match(css, /grid-template-columns: repeat\(auto-fill, 32px\)/);
assert.match(css, /grid-auto-rows: 32px/);
assert.match(css, /padding-block: 36px 20px/);
assert.match(css, /padding-inline: 0 20px/);

assert.match(runtime, /https:\/\/api\.github\.com\/repos\/iina\/iina\/contributors/);
assert.match(runtime, /response\.headers\.get\("link"\)/);
assert.match(runtime, /rel="next"/);
assert.match(runtime, /safeAvatarUrl/);
assert.match(runtime, /visited\.size < 32/);
assert.match(runtime, /Promise\.allSettled/);
assert.match(runtime, /ABOUT_DOCUMENT_LINKS/);
assert.match(runtime, /allowedElements = new Set\(\["A", "B", "BR", "I", "P", "SPAN"\]\)/);
assert.match(runtime, /attachShadow\(\{ mode: "open" \}\)/);
assert.doesNotMatch(runtime, /mpvVersion:\s*"mpv [0-9]/);
assert.doesNotMatch(runtime, /ffmpegVersion:\s*"FFmpeg [0-9]/);

const contributionDirectories = readdirSync(referenceRoot, { withFileTypes: true })
  .filter((entry) => entry.isDirectory() && entry.name.endsWith(".lproj"))
  .filter((entry) => existsSync(join(referenceRoot, entry.name, "Contribution.rtf")))
  .map((entry) => entry.name)
  .sort((left, right) => left.localeCompare(right, "en"));
const expectedLocales = contributionDirectories
  .map((directory) => directory.slice(0, -".lproj".length))
  .sort((left, right) => left.localeCompare(right, "en"));
assert.equal(contributionDirectories.length, 29);
assert.deepEqual(Object.keys(documents.licenses), expectedLocales);
assert.deepEqual(Object.keys(documents.licenseHtml), expectedLocales);
for (const directory of contributionDirectories) {
  const locale = directory.slice(0, -".lproj".length);
  const path = join(referenceRoot, directory, "Contribution.rtf");
  const source = readFileSync(path);
  assert.equal(documents.sources[`licenses/${locale}`], sha256(source));
  assert.equal(documents.licenses[locale], rtfText(path));
  assert.deepEqual(documents.licenseHtml[locale], rtfHtml(path));
}
const creditsPath = join(referenceRoot, "Credits.rtf");
const creditsSource = readFileSync(creditsPath);
assert.equal(documents.sources.credits, sha256(creditsSource));
assert.equal(documents.credits, rtfText(creditsPath));
assert.deepEqual(documents.creditsHtml, rtfHtml(creditsPath));
assert.ok(documents.credits.length > 6_000);
assert.match(generator, /Contribution\.rtf and Credits\.rtf/);

const iconPath = join(root, "src", "assets", "iina", "app-icon.png");
assert.ok(existsSync(iconPath));
assert.ok(statSync(iconPath).size > 1_000);

console.log("About window matches the IINA 1.3.5 geometry, documents, routing, and offline-safe fallback contracts");
