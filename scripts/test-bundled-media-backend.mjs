import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const media = readFileSync(join(root, "src-tauri", "src", "media.rs"), "utf8");
const mpv = readFileSync(join(root, "src-tauri", "src", "mpv.rs"), "utf8");
const packager = readFileSync(join(root, "scripts", "package-macos.mjs"), "utf8");
const reference = readFileSync(
  join(root, "参考", "iina", "iina", "FFmpegController.m"),
  "utf8",
);

function section(source, start, end) {
  const startIndex = source.indexOf(start);
  if (startIndex < 0) throw new Error(`Missing source marker: ${start}`);
  const endIndex = source.indexOf(end, startIndex + start.length);
  if (endIndex < 0) throw new Error(`Missing source marker: ${end}`);
  return source.slice(startIndex, endIndex);
}

const probe = section(media, "pub fn probe_media", "pub fn generate_cached_thumbnails");
if (!probe.includes("inspect_media(path)?")) {
  throw new Error("Media probing must use the isolated bundled-libmpv helper");
}
if (probe.includes("Command::new") || probe.includes("discover_executable")) {
  throw new Error("Production media probing must not invoke a host command-line tool");
}

const thumbnails = section(media, "fn generate_thumbnail_frames", "fn validate_thumbnail_source");
if (!thumbnails.includes("capture_video_frame")) {
  throw new Error("Thumbnail generation must use the isolated bundled-libmpv session");
}
if (thumbnails.includes("Command::new") || thumbnails.includes("discover_executable")) {
  throw new Error("Production thumbnail generation must not invoke host ffmpeg");
}

for (const marker of [
  "pub(crate) struct MpvHeadlessMediaSession",
  "pub(crate) fn inspect_media",
  "screenshot-to-file",
  "absolute+exact",
]) {
  if (!mpv.includes(marker)) throw new Error(`Bundled media helper is missing ${marker}`);
}

for (const library of [
  "libavcodec.61.dylib",
  "libavformat.61.dylib",
  "libavutil.59.dylib",
  "libswscale.8.dylib",
]) {
  if (!packager.includes(library)) {
    throw new Error(`Package gate is missing ${library}`);
  }
}
if (!packager.includes("must not bundle a host ${executable} command-line executable")) {
  throw new Error("Package gate does not reject host ffmpeg/ffprobe executable copies");
}

if (!reference.includes("avformat_open_input") || !reference.includes("sws_scale")) {
  throw new Error("Reference IINA FFmpeg in-process contract is unavailable");
}

console.log("Bundled libmpv/FFmpeg media backend contract passed");
