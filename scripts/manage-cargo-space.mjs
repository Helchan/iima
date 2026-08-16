import { access, readdir, rm } from "node:fs/promises";
import { constants } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const target = join(root, "src-tauri", "target");
const debug = join(target, "debug");
const release = join(target, "release");
const obsoleteLibraryOutputs = [
  join(release, "libiima_lib.a"),
  join(release, "libiima_lib.dylib"),
  join(release, "deps", "libiima_lib.a"),
  join(release, "deps", "libiima_lib.dylib"),
];
const warningBytes = 8 * 1024 ** 3;
const cleanupBytes = 12 * 1024 ** 3;

function formatBytes(bytes) {
  if (bytes === 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  const unit = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const digits = unit >= 3 ? 1 : 0;
  return `${(bytes / 1024 ** unit).toFixed(digits)} ${units[unit]}`;
}

async function exists(path) {
  try {
    await access(path, constants.F_OK);
    return true;
  } catch {
    return false;
  }
}

function diskUsage(path) {
  if (!path) return 0;
  try {
    const output = execFileSync("du", ["-sk", path], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    return Number.parseInt(output.trim().split(/\s+/, 1)[0], 10) * 1024;
  } catch {
    return 0;
  }
}

function totalDiskUsage(paths) {
  return paths.reduce((total, path) => total + diskUsage(path), 0);
}

async function report() {
  const rows = [
    ["target total", target],
    ["debug/test", debug],
    ["release (preserved)", release],
    ["obsolete static/dynamic iima libraries", null],
  ];

  if (await exists(target)) {
    for (const entry of await readdir(target, { withFileTypes: true })) {
      if (!entry.isDirectory() || entry.name === "debug" || entry.name === "release") continue;
      rows.push([`other target: ${entry.name}`, join(target, entry.name)]);
    }
  }

  console.log("Cargo build-space report");
  for (const [label, path] of rows) {
    const bytes = path ? diskUsage(path) : totalDiskUsage(obsoleteLibraryOutputs);
    const displayPath = path ? relative(root, path) : "four exact pre-rlib outputs";
    console.log(`${label.padEnd(42)} ${formatBytes(bytes).padStart(10)}  ${displayPath}`);
  }

  const debugBytes = diskUsage(debug);
  if (debugBytes >= cleanupBytes) {
    console.log("Action: debug/test artifacts exceed 12 GiB; run npm run cargo:space:clean.");
  } else if (debugBytes >= warningBytes) {
    console.log("Warning: debug/test artifacts exceed 8 GiB; consider a dry run with npm run cargo:space:clean:dry-run.");
  } else {
    console.log("Status: debug/test artifacts are below the 8 GiB warning threshold.");
  }
}

async function cleanDisposable(apply) {
  const debugBytes = diskUsage(debug);
  const obsoleteBytes = totalDiskUsage(obsoleteLibraryOutputs);
  console.log(`Selected path: ${relative(root, debug)} (${formatBytes(debugBytes)})`);
  console.log(`Selected legacy outputs: four exact static/dynamic library paths (${formatBytes(obsoleteBytes)})`);
  console.log(`Preserved path: ${relative(root, release)} (${formatBytes(diskUsage(release))})`);
  console.log(`Preserved path: ${relative(root, join(release, "bundle"))} (${formatBytes(diskUsage(join(release, "bundle")))})`);

  if (!apply) {
    console.log("Dry run only. Add --apply, or run npm run cargo:space:clean, to remove the selected path.");
    return;
  }

  await rm(debug, { recursive: true, force: true });
  for (const path of obsoleteLibraryOutputs) await rm(path, { force: true });
  console.log(`Removed disposable Cargo outputs; reclaimed approximately ${formatBytes(debugBytes + obsoleteBytes)}.`);
}

const [command = "report", ...args] = process.argv.slice(2);
if (command === "report") {
  await report();
} else if (command === "clean") {
  await cleanDisposable(args.includes("--apply"));
} else {
  console.error("Usage: node scripts/manage-cargo-space.mjs <report|clean> [--apply]");
  process.exitCode = 2;
}
