import { cp, mkdir, rm } from "node:fs/promises";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { spawnSync } from "node:child_process";

const REQUIRED_SOURCE_FILES = [
  "SafariExtensionHandler.swift",
  "open-in-iina.js",
  "ToolbarItemIcon.pdf",
  "Info.plist",
  "OpenInIINA.entitlements",
];

function run(command, arguments_, cwd, capture = false) {
  const result = spawnSync(command, arguments_, {
    cwd,
    env: process.env,
    encoding: capture ? "utf8" : undefined,
    stdio: capture ? ["ignore", "pipe", "pipe"] : "inherit",
  });
  if (result.status !== 0) {
    throw new Error(
      `${command} exited with ${result.status}${
        capture ? `\n${result.stdout ?? ""}${result.stderr ?? ""}` : ""
      }`,
    );
  }
  return capture ? result.stdout : undefined;
}

function requireSourceFiles(sourceDirectory) {
  for (const file of REQUIRED_SOURCE_FILES) {
    const path = join(sourceDirectory, file);
    if (!existsSync(path)) {
      throw new Error(`Safari extension source is missing ${file}: ${path}`);
    }
  }
}

function setPlistString(path, key, value, cwd) {
  const replacement = spawnSync(
    "plutil",
    ["-replace", key, "-string", value, path],
    {
      cwd,
      env: process.env,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  if (replacement.status !== 0) {
    run("plutil", ["-insert", key, "-string", value, path], cwd);
  }
  const actual = run("plutil", ["-extract", key, "raw", path], cwd, true).trim();
  if (actual !== value) {
    throw new Error(`Expected Safari ${key}=${value}, found ${actual}`);
  }
}

/**
 * Build the IINA 1.3.5 Safari App Extension without Xcode project resolution or
 * network access. Signing deliberately remains the caller's responsibility so
 * local ad-hoc packaging and a real Developer ID/App Store pipeline cannot be
 * confused with one another.
 */
export async function buildSafariExtensionBundle({
  sourceDirectory,
  extensionPath,
  bundleIdentifier,
  shortVersion,
  bundleVersion,
  workDirectory = join(dirname(extensionPath), ".safari-extension-build"),
  architectures = ["arm64", "x86_64"],
}) {
  if (!sourceDirectory || !extensionPath || !bundleIdentifier || !shortVersion || !bundleVersion) {
    throw new Error("Safari extension build requires source, output, identifier, and versions");
  }
  if (!Array.isArray(architectures) || architectures.length === 0) {
    throw new Error("Safari extension build requires at least one architecture");
  }
  requireSourceFiles(sourceDirectory);

  const contentsDirectory = join(extensionPath, "Contents");
  const binaryDirectory = join(contentsDirectory, "MacOS");
  const resourcesDirectory = join(contentsDirectory, "Resources");
  const binaryPath = join(binaryDirectory, "OpenInIINA");
  const infoPath = join(contentsDirectory, "Info.plist");

  await rm(workDirectory, { recursive: true, force: true });
  await rm(extensionPath, { recursive: true, force: true });
  await mkdir(binaryDirectory, { recursive: true });
  await mkdir(resourcesDirectory, { recursive: true });
  await mkdir(join(workDirectory, "module-cache"), { recursive: true });

  let completed = false;
  try {
    const cwd = dirname(sourceDirectory);
    const sdkPath = run("xcrun", ["--sdk", "macosx", "--show-sdk-path"], cwd, true).trim();
    const architectureBinaries = [];
    for (const architecture of architectures) {
      if (!/^[A-Za-z0-9_]+$/.test(architecture)) {
        throw new Error(`Invalid Safari extension architecture: ${architecture}`);
      }
      const architectureBinary = join(workDirectory, `OpenInIINA-${architecture}`);
      run(
        "xcrun",
        [
          "swiftc",
          "-module-name",
          "OpenInIINA",
          "-O",
          "-whole-module-optimization",
          "-application-extension",
          "-parse-as-library",
          "-sdk",
          sdkPath,
          "-target",
          `${architecture}-apple-macos10.13`,
          "-module-cache-path",
          join(workDirectory, "module-cache"),
          "-emit-executable",
          join(sourceDirectory, "SafariExtensionHandler.swift"),
          "-Xlinker",
          "-e",
          "-Xlinker",
          "_NSExtensionMain",
          "-framework",
          "Cocoa",
          "-framework",
          "SafariServices",
          "-o",
          architectureBinary,
        ],
        cwd,
      );
      architectureBinaries.push(architectureBinary);
    }

    if (architectureBinaries.length === 1) {
      await cp(architectureBinaries[0], binaryPath);
    } else {
      run("lipo", ["-create", ...architectureBinaries, "-output", binaryPath], cwd);
    }

    await cp(join(sourceDirectory, "Info.plist"), infoPath);
    await cp(
      join(sourceDirectory, "open-in-iina.js"),
      join(resourcesDirectory, "open-in-iina.js"),
    );
    await cp(
      join(sourceDirectory, "ToolbarItemIcon.pdf"),
      join(resourcesDirectory, "ToolbarItemIcon.pdf"),
    );

    setPlistString(infoPath, "CFBundleIdentifier", bundleIdentifier, cwd);
    setPlistString(infoPath, "CFBundleShortVersionString", shortVersion, cwd);
    setPlistString(infoPath, "CFBundleVersion", bundleVersion, cwd);
    run("plutil", ["-lint", infoPath], cwd, true);
    completed = true;
  } finally {
    await rm(workDirectory, { recursive: true, force: true });
    if (!completed) await rm(extensionPath, { recursive: true, force: true });
  }

  return {
    extensionPath,
    binaryPath,
    infoPath,
    entitlementsPath: join(sourceDirectory, "OpenInIINA.entitlements"),
  };
}
