import { chmod, cp, mkdir, readdir, rename, rm, symlink } from "node:fs/promises";
import { existsSync, readFileSync, readlinkSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { buildSafariExtensionBundle } from "./build-safari-extension.mjs";
import { readReferencePackageIdentity } from "./package-identity.mjs";
import { resolveSparkleChannel } from "./sparkle-channel.mjs";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const srcTauri = join(root, "src-tauri");
const releaseDir = join(srcTauri, "target", "release");
const bundleDir = resolve(
  process.env.IIMA_PACKAGE_BUNDLE_DIR || join(releaseDir, "bundle"),
);
const appPath = join(bundleDir, "macos", "IINA.app");
const appContentsDir = join(appPath, "Contents");
const appBinary = join(appPath, "Contents", "MacOS", "iima");
const releaseBinary = join(releaseDir, "iima");
const cliAppBinary = join(appPath, "Contents", "MacOS", "iina-cli");
const cliReleaseBinary = join(releaseDir, "iina-cli");
const frameworksDir = join(appPath, "Contents", "Frameworks");
const resourcesDir = join(appPath, "Contents", "Resources");
const appInfoPlist = join(appPath, "Contents", "Info.plist");
const appPkgInfo = join(appPath, "Contents", "PkgInfo");
const appPluginsDir = join(appPath, "Contents", "PlugIns");
const tauriConfigPath = join(srcTauri, "tauri.conf.json");
const supplementalInfoPlist = join(srcTauri, "IINA-Info.plist");
const appIconSource = join(srcTauri, "icons", "icon.icns");
const documentIconSourceDir = join(srcTauri, "icons", "doc");
const safariExtensionSource = join(root, "browser", "Safari_Open_In_IINA");
const safariExtensionEntitlements = join(
  safariExtensionSource,
  "OpenInIINA.entitlements",
);
const safariExtensionPath = join(appPluginsDir, "OpenInIINA.appex");
const safariExtensionBinary = join(
  safariExtensionPath,
  "Contents",
  "MacOS",
  "OpenInIINA",
);
const safariExtensionBuildDir = join(bundleDir, ".safari-extension-build");
const tauriConfig = JSON.parse(readFileSync(tauriConfigPath, "utf8"));
const referencePackageIdentity = readReferencePackageIdentity(root);
const appName = tauriConfig.productName || "IINA";
const appVersion = tauriConfig.version || referencePackageIdentity.marketingVersion;
const appBuildVersion = "93";
const appIdentifier = tauriConfig.identifier || "io.iima.player";
if (
  referencePackageIdentity.marketingVersion !== "1.3.5" ||
  referencePackageIdentity.buildVersion !== "141"
) {
  throw new Error("The pinned IINA reference identity must remain 1.3.5 build 141");
}
const iinaDepsLib = join(root, "参考", "iina", "deps", "lib");
const iinaLocalizationRoot = join(root, "参考", "iina", "iina");
const sparkleFrameworkSource = join(
  root,
  "参考",
  "iina",
  "build",
  "DerivedData",
  "SourcePackages",
  "artifacts",
  "sparkle",
  "Sparkle",
  "Sparkle.xcframework",
  "macos-arm64_x86_64",
  "Sparkle.framework",
);
const sparkleFramework = join(frameworksDir, "Sparkle.framework");
const sparkleBinary = join(sparkleFramework, "Versions", "B", "Sparkle");
const referenceDsaPublicKeySource = join(root, "参考", "iina", "iina", "dsa_pub.pem");
const dsaPublicKey = join(resourcesDir, "dsa_pub.pem");
const sparkleChannel = resolveSparkleChannel();
const stableAppcastUrl = sparkleChannel.stableAppcastUrl;
const betaAppcastUrl = sparkleChannel.betaAppcastUrl;
const publicEdKey = sparkleChannel.publicEdKey;
const dsaPublicKeySource = sparkleChannel.mode === "reference"
  ? referenceDsaPublicKeySource
  : sparkleChannel.dsaPublicKeyPath;
const dmgDir = join(bundleDir, "dmg");
const dmgVariant = process.env.IIMA_DMG_VARIANT || "bundled_libmpv";
const dmgArchitecture =
  process.env.IIMA_DMG_ARCH || (process.arch === "arm64" ? "aarch64" : "x64");
const dmgPath = join(
  dmgDir,
  `IINA_${appVersion}_${dmgArchitecture}_${dmgVariant}.dmg`,
);
const stagingDir = join(bundleDir, "dmg-staging-iina");
const mountDir = join(bundleDir, "dmg-mount-iina");
const finderDiskName = basename(mountDir);
const writableDmgPath = join(dmgDir, ".IINA-package-read-write.dmg");
const pendingDmgPath = join(dmgDir, ".IINA-package-pending.dmg");

const skipBuild = process.argv.includes("--skip-build");
const skipDmg = process.argv.includes("--skip-dmg");

function run(cmd, args, options = {}) {
  console.log(`$ ${[cmd, ...args].join(" ")}`);
  const result = spawnSync(cmd, args, {
    cwd: root,
    env: process.env,
    stdio: "inherit",
    ...options,
  });

  if (result.status !== 0) {
    throw new Error(`${cmd} exited with ${result.status}`);
  }
}

function capture(cmd, args) {
  const result = spawnSync(cmd, args, {
    cwd: root,
    env: process.env,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });

  if (result.status !== 0) {
    throw new Error(
      `${cmd} exited with ${result.status}\n${result.stdout}${result.stderr}`,
    );
  }

  return result.stdout;
}

function requireFile(path, label) {
  if (!existsSync(path)) {
    throw new Error(`${label} not found: ${path}`);
  }
}

function setPlistString(path, key, value) {
  const replace = spawnSync("plutil", ["-replace", key, "-string", value, path], {
    cwd: root,
    env: process.env,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (replace.status !== 0) {
    run("plutil", ["-insert", key, "-string", value, path]);
  }
  const actual = capture("plutil", ["-extract", key, "raw", path]).trim();
  if (actual !== value) {
    throw new Error(`Expected ${key}=${value}, found ${actual}`);
  }
}

function setPlistBoolean(path, key, value) {
  const stringValue = value ? "true" : "false";
  const replace = spawnSync("plutil", ["-replace", key, "-bool", stringValue, path], {
    cwd: root,
    env: process.env,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (replace.status !== 0) {
    run("plutil", ["-insert", key, "-bool", stringValue, path]);
  }
  const actual = capture("plutil", ["-extract", key, "raw", path]).trim();
  if (actual !== stringValue) {
    throw new Error(`Expected ${key}=${stringValue}, found ${actual}`);
  }
}

function removePlistKey(path, key) {
  spawnSync("plutil", ["-remove", key, path], {
    cwd: root,
    env: process.env,
    stdio: "ignore",
  });
}

async function ensureAppBundleSkeleton() {
  requireFile(tauriConfigPath, "Tauri config");
  requireFile(supplementalInfoPlist, "Supplemental IINA Info.plist");
  requireFile(appIconSource, "IINA app icon");
  requireFile(documentIconSourceDir, "IINA document icon folder");

  const created = !existsSync(appPath);
  await mkdir(join(appContentsDir, "MacOS"), { recursive: true });
  await mkdir(frameworksDir, { recursive: true });
  await mkdir(resourcesDir, { recursive: true });

  if (!existsSync(appInfoPlist)) {
    await cp(supplementalInfoPlist, appInfoPlist, { preserveTimestamps: true });
  }

  for (const [key, value] of [
    ["CFBundleExecutable", "iima"],
    ["CFBundleIconFile", "IINA.icns"],
    ["CFBundleIdentifier", appIdentifier],
    ["CFBundleInfoDictionaryVersion", "6.0"],
    ["CFBundleName", appName],
    ["CFBundlePackageType", "APPL"],
    ["CFBundleShortVersionString", appVersion],
    ["CFBundleSignature", "????"],
    ["CFBundleVersion", appBuildVersion],
    ["LSMinimumSystemVersion", "10.13"],
    ["NSPrincipalClass", "NSApplication"],
  ]) {
    setPlistString(appInfoPlist, key, value);
  }
  setPlistBoolean(appInfoPlist, "NSHighResolutionCapable", true);
  setPlistBoolean(appInfoPlist, "NSPrefersDisplaySafeAreaCompatibilityMode", false);
  capture("plutil", ["-lint", appInfoPlist]);

  writeFileSync(appPkgInfo, "APPL????", { mode: 0o644 });
  await cp(appIconSource, join(resourcesDir, "IINA.icns"), {
    preserveTimestamps: true,
  });

  const documentIcons = (await readdir(documentIconSourceDir))
    .filter((entry) => entry.endsWith(".icns"))
    .sort();
  if (documentIcons.length !== 22) {
    throw new Error(`Expected 22 IINA document icons, found ${documentIcons.length}`);
  }
  for (const icon of documentIcons) {
    await cp(join(documentIconSourceDir, icon), join(resourcesDir, icon), {
      preserveTimestamps: true,
    });
  }

  console.log(
    `${created ? "Created" : "Prepared"} app bundle skeleton from repository inputs: ${appPath}`,
  );
}

async function copyIinaFrameworks() {
  requireFile(iinaDepsLib, "IINA deps lib folder");

  await rm(frameworksDir, { recursive: true, force: true });
  await mkdir(frameworksDir, { recursive: true });

  const entries = await readdir(iinaDepsLib);
  const dylibs = entries.filter((entry) => entry.endsWith(".dylib")).sort();

  for (const dylib of dylibs) {
    await cp(join(iinaDepsLib, dylib), join(frameworksDir, dylib), {
      preserveTimestamps: true,
    });
  }

  console.log(`Copied ${dylibs.length} IINA dylibs into ${frameworksDir}`);
  if (dylibs.length !== 71) {
    throw new Error(`Expected 71 IINA dylibs, copied ${dylibs.length}`);
  }
}

async function copySparkleUpdater() {
  requireFile(sparkleFrameworkSource, "Reference Sparkle.framework");
  requireFile(appInfoPlist, "App Info.plist");
  if (dsaPublicKeySource) requireFile(dsaPublicKeySource, "Sparkle DSA public key");
  if (
    sparkleChannel.mode === "owned" &&
    dsaPublicKeySource &&
    readFileSync(dsaPublicKeySource).equals(readFileSync(referenceDsaPublicKeySource))
  ) {
    throw new Error("A Tauri-owned update channel must not reuse IINA's DSA public key");
  }

  await cp(sparkleFrameworkSource, sparkleFramework, {
    recursive: true,
    preserveTimestamps: true,
    verbatimSymlinks: true,
  });
  await mkdir(resourcesDir, { recursive: true });
  if (dsaPublicKeySource) {
    await cp(dsaPublicKeySource, dsaPublicKey, { preserveTimestamps: true });
    setPlistString(appInfoPlist, "SUPublicDSAKeyFile", "dsa_pub.pem");
  } else {
    await rm(dsaPublicKey, { force: true });
    removePlistKey(appInfoPlist, "SUPublicDSAKeyFile");
  }

  setPlistString(appInfoPlist, "SUFeedURL", stableAppcastUrl);
  setPlistString(appInfoPlist, "SUPublicEDKey", publicEdKey);
  if (sparkleChannel.mode === "owned") {
    setPlistString(appInfoPlist, "IIMAUpdateChannelMode", "owned");
    setPlistString(appInfoPlist, "IIMABetaFeedURL", betaAppcastUrl);
  } else {
    // Keep the reference-mode updater keys byte-for-byte aligned with IINA
    // 1.3.5. Its beta URL is a source constant rather than an Info.plist key.
    removePlistKey(appInfoPlist, "IIMAUpdateChannelMode");
    removePlistKey(appInfoPlist, "IIMABetaFeedURL");
  }
  console.log(`Copied Sparkle.framework and ${sparkleChannel.mode} update-channel metadata into ${appPath}`);
}

async function copyInfoPlistLocalizations() {
  const sourceDirectories = (await readdir(iinaLocalizationRoot, { withFileTypes: true }))
    .filter((entry) => entry.isDirectory() && entry.name.endsWith(".lproj"))
    .map((entry) => entry.name)
    .sort();
  const localizations = [];
  for (const directory of sourceDirectories) {
    const source = join(iinaLocalizationRoot, directory, "InfoPlist.strings");
    if (!existsSync(source)) continue;
    const destinationDirectory = join(resourcesDir, directory);
    await mkdir(destinationDirectory, { recursive: true });
    await cp(source, join(destinationDirectory, "InfoPlist.strings"), {
      preserveTimestamps: true,
    });
    localizations.push(directory);
  }
  if (localizations.length !== 57) {
    throw new Error(`Expected 57 IINA InfoPlist localizations, copied ${localizations.length}`);
  }
  console.log(`Copied ${localizations.length} IINA InfoPlist localizations`);
}

async function buildSafariExtension() {
  await buildSafariExtensionBundle({
    sourceDirectory: safariExtensionSource,
    extensionPath: safariExtensionPath,
    bundleIdentifier: `${appIdentifier}.OpenInIINA`,
    shortVersion: appVersion,
    bundleVersion: appBuildVersion,
    workDirectory: safariExtensionBuildDir,
  });
  console.log(`Built universal Safari extension from repository source: ${safariExtensionPath}`);
}

async function verifyInfoPlistLocalizations() {
  const packaged = (await readdir(resourcesDir, { withFileTypes: true }))
    .filter((entry) => entry.isDirectory() && entry.name.endsWith(".lproj"))
    .map((entry) => entry.name)
    .filter((directory) => existsSync(join(resourcesDir, directory, "InfoPlist.strings")))
    .sort();
  if (packaged.length !== 57) {
    throw new Error(`Expected 57 packaged InfoPlist localizations, found ${packaged.length}`);
  }
  const emptyLocalizations = [];
  for (const directory of packaged) {
    const source = join(iinaLocalizationRoot, directory, "InfoPlist.strings");
    const destination = join(resourcesDir, directory, "InfoPlist.strings");
    const sourceContents = readFileSync(source);
    if (!sourceContents.equals(readFileSync(destination))) {
      throw new Error(`Packaged ${directory}/InfoPlist.strings differs from IINA 1.3.5`);
    }
    if (sourceContents.toString("utf8").trim()) capture("plutil", ["-lint", destination]);
    else emptyLocalizations.push(directory);
  }
  const expectedEmpty = ["Base.lproj", "sr-CS.lproj", "sr-Cyrl.lproj"];
  if (JSON.stringify(emptyLocalizations) !== JSON.stringify(expectedEmpty)) {
    throw new Error(`Unexpected empty InfoPlist localizations: ${emptyLocalizations.join(", ")}`);
  }
  const simplifiedChinese = readFileSync(
    join(resourcesDir, "zh-Hans.lproj", "InfoPlist.strings"),
    "utf8",
  );
  if (!simplifiedChinese.includes("基于 GPLv3 授权发布")) {
    throw new Error("Packaged Simplified Chinese InfoPlist localization is incomplete");
  }
  console.log(`Verified ${packaged.length} byte-identical InfoPlist localizations`);
}

function verifySparkleUpdater() {
  const sparkleInfo = join(
    sparkleFramework,
    "Versions",
    "B",
    "Resources",
    "Info.plist",
  );
  const requiredUpdaterFiles = [
    [sparkleBinary, "Sparkle framework binary"],
    [join(sparkleFramework, "Versions", "B", "Autoupdate"), "Sparkle Autoupdate helper"],
    [join(sparkleFramework, "Versions", "B", "Updater.app"), "Sparkle Updater app"],
    [join(sparkleFramework, "Versions", "B", "XPCServices", "Downloader.xpc"), "Sparkle Downloader XPC"],
    [join(sparkleFramework, "Versions", "B", "XPCServices", "Installer.xpc"), "Sparkle Installer XPC"],
  ];
  if (dsaPublicKeySource) {
    requiredUpdaterFiles.push([dsaPublicKey, "Sparkle DSA public key"]);
  }
  for (const [path, label] of requiredUpdaterFiles) {
    requireFile(path, label);
  }
  const frameworkBinaryLink = readlinkSync(join(sparkleFramework, "Sparkle"));
  if (frameworkBinaryLink !== "Versions/Current/Sparkle") {
    throw new Error(`Sparkle framework symlink escaped the bundle: ${frameworkBinaryLink}`);
  }

  const architectures = new Set(capture("lipo", ["-archs", sparkleBinary]).trim().split(/\s+/));
  for (const architecture of ["x86_64", "arm64"]) {
    if (!architectures.has(architecture)) {
      throw new Error(`Sparkle.framework is missing ${architecture}`);
    }
  }
  const frameworkVersion = capture("plutil", [
    "-extract",
    "CFBundleShortVersionString",
    "raw",
    sparkleInfo,
  ]).trim();
  if (frameworkVersion !== "2.9.4") {
    throw new Error(`Expected Sparkle 2.9.4, found ${frameworkVersion}`);
  }
  if (dsaPublicKeySource && !readFileSync(dsaPublicKey).equals(readFileSync(dsaPublicKeySource))) {
    throw new Error("Packaged Sparkle DSA public key differs from its configured source");
  }
  const updaterMetadata = [
    ["SUFeedURL", stableAppcastUrl],
    ["SUPublicEDKey", publicEdKey],
  ];
  if (sparkleChannel.mode === "owned") {
    updaterMetadata.push(["IIMAUpdateChannelMode", "owned"]);
    updaterMetadata.push(["IIMABetaFeedURL", betaAppcastUrl]);
  }
  if (dsaPublicKeySource) updaterMetadata.push(["SUPublicDSAKeyFile", "dsa_pub.pem"]);
  for (const [key, expected] of updaterMetadata) {
    const actual = capture("plutil", ["-extract", key, "raw", appInfoPlist]).trim();
    if (actual !== expected) {
      throw new Error(`Expected ${key}=${expected}, found ${actual}`);
    }
  }
  if (!dsaPublicKeySource) {
    const staleDsaKey = spawnSync("plutil", ["-extract", "SUPublicDSAKeyFile", "raw", appInfoPlist]);
    if (staleDsaKey.status === 0 || existsSync(dsaPublicKey)) {
      throw new Error("Owned Ed25519-only update channel retained stale DSA metadata");
    }
  }
  if (sparkleChannel.mode === "reference") {
    for (const staleOwnedKey of ["IIMAUpdateChannelMode", "IIMABetaFeedURL"]) {
      const stale = spawnSync("plutil", ["-extract", staleOwnedKey, "raw", appInfoPlist]);
      if (stale.status === 0) {
        throw new Error(`Reference update channel retained stale ${staleOwnedKey} metadata`);
      }
    }
  }
  console.log(`Verified Sparkle ${frameworkVersion} (${[...architectures].join(", ")})`);
}

function verifySafariExtension() {
  const info = join(safariExtensionPath, "Contents", "Info.plist");
  const script = join(safariExtensionPath, "Contents", "Resources", "open-in-iina.js");
  const icon = join(safariExtensionPath, "Contents", "Resources", "ToolbarItemIcon.pdf");
  if (dirname(safariExtensionPath) !== appPluginsDir) {
    throw new Error(`Safari extension escaped the app PlugIns directory: ${safariExtensionPath}`);
  }
  for (const [path, label] of [
    [safariExtensionBinary, "Safari extension binary"],
    [info, "Safari extension Info.plist"],
    [script, "Safari extension content script"],
    [icon, "Safari extension toolbar icon"],
  ]) {
    requireFile(path, label);
  }

  const architectures = new Set(
    capture("lipo", ["-archs", safariExtensionBinary]).trim().split(/\s+/),
  );
  for (const architecture of ["arm64", "x86_64"]) {
    if (!architectures.has(architecture)) {
      throw new Error(`Safari extension is missing ${architecture}`);
    }
  }
  for (const [key, expected] of [
    ["CFBundleIdentifier", `${appIdentifier}.OpenInIINA`],
    ["CFBundleShortVersionString", appVersion],
    ["CFBundleVersion", appBuildVersion],
    ["CFBundlePackageType", "XPC!"],
    ["NSExtension.NSExtensionPointIdentifier", "com.apple.Safari.extension"],
    ["NSExtension.NSExtensionPrincipalClass", "OpenInIINA.SafariExtensionHandler"],
  ]) {
    const actual = capture("plutil", ["-extract", key, "raw", info]).trim();
    if (actual !== expected) {
      throw new Error(`Expected Safari ${key}=${expected}, found ${actual}`);
    }
  }
  if (!readFileSync(script).equals(readFileSync(join(safariExtensionSource, "open-in-iina.js")))) {
    throw new Error("Packaged Safari content script differs from repository source");
  }
  if (!readFileSync(icon).equals(readFileSync(join(safariExtensionSource, "ToolbarItemIcon.pdf")))) {
    throw new Error("Packaged Safari toolbar icon differs from repository source");
  }
  const binary = readFileSync(safariExtensionBinary);
  if (!binary.includes(Buffer.from("iina://weblink?url="))) {
    throw new Error("Safari extension binary is missing the IINA URL-scheme route");
  }
  if (!capture("otool", ["-L", safariExtensionBinary]).includes("SafariServices.framework")) {
    throw new Error("Safari extension binary is not linked to SafariServices.framework");
  }
  const registeredScheme = capture("plutil", [
    "-extract",
    "CFBundleURLTypes.0.CFBundleURLSchemes.0",
    "raw",
    appInfoPlist,
  ]).trim();
  if (registeredScheme !== "iina") {
    throw new Error(`Expected host app to register iina URL scheme, found ${registeredScheme}`);
  }
  const entitlements = capture("codesign", ["-d", "--entitlements", ":-", safariExtensionPath]);
  if (!entitlements.includes("com.apple.security.app-sandbox")) {
    throw new Error("Safari extension signature is missing its sandbox entitlement");
  }
  console.log(`Verified universal Safari extension (${[...architectures].join(", ")})`);
}

function patchMpvPlaceboSymbol() {
  const libmpv = join(frameworksDir, "libmpv.2.dylib");
  const from = Buffer.from("_pl_log_create_349");
  const to = Buffer.from("_pl_log_create_338");

  requireFile(libmpv, "Bundled libmpv");
  if (from.length !== to.length) {
    throw new Error("libmpv symbol patch requires equal-length symbols");
  }

  const buffer = readFileSync(libmpv);
  let offset = 0;
  let count = 0;

  while ((offset = buffer.indexOf(from, offset)) !== -1) {
    to.copy(buffer, offset);
    offset += to.length;
    count += 1;
  }

  if (count > 0) {
    writeFileSync(libmpv, buffer);
  }

  const imports = capture("nm", ["-u", libmpv]);
  if (imports.includes("_pl_log_create_349")) {
    throw new Error("Bundled libmpv still imports _pl_log_create_349");
  }
  if (!imports.includes("_pl_log_create_338")) {
    throw new Error("Bundled libmpv does not import _pl_log_create_338");
  }

  console.log(`Patched libmpv/libplacebo ABI symbol occurrences: ${count}`);
}

function ensureRpath(path, rpath) {
  const loadCommands = capture("otool", ["-l", path]);
  if (loadCommands.includes(`path ${rpath} `)) {
    return;
  }
  run("install_name_tool", ["-add_rpath", rpath, path]);
}

function smokeLoadLibmpv() {
  const libmpv = resolve(join(frameworksDir, "libmpv.2.dylib"));
  const script = [
    "import ctypes, pathlib, sys",
    `p = pathlib.Path(${JSON.stringify(libmpv)})`,
    "lib = ctypes.CDLL(str(p))",
    "required = ['mpv_create', 'mpv_initialize', 'mpv_command', 'mpv_set_property', 'mpv_wait_event', 'mpv_render_context_create', 'mpv_render_context_render']",
    "missing = [name for name in required if not hasattr(lib, name)]",
    "print('loaded', p)",
    "print('missing', missing)",
    "sys.exit(1 if missing else 0)",
  ].join("; ");

  run("python3", ["-c", script]);
}

function verifyBundledMediaRuntime() {
  const libmpv = join(frameworksDir, "libmpv.2.dylib");
  const requiredFfmpegLibraries = [
    "libavcodec.61.dylib",
    "libavdevice.61.dylib",
    "libavfilter.10.dylib",
    "libavformat.61.dylib",
    "libavutil.59.dylib",
    "libswresample.5.dylib",
    "libswscale.8.dylib",
  ];
  for (const library of requiredFfmpegLibraries) {
    requireFile(join(frameworksDir, library), `Bundled FFmpeg library ${library}`);
  }
  for (const executable of ["ffmpeg", "ffprobe"]) {
    if (existsSync(join(appContentsDir, "MacOS", executable))) {
      throw new Error(`The app must not bundle a host ${executable} command-line executable`);
    }
  }

  const dependencies = capture("otool", ["-L", libmpv]);
  for (const library of requiredFfmpegLibraries) {
    if (!dependencies.includes(`@rpath/${library}`)) {
      throw new Error(`Bundled libmpv does not use the packaged ${library}`);
    }
  }
  if (!readFileSync(appBinary).includes(Buffer.from("bundled libmpv returned no playable streams or duration"))) {
    throw new Error("Packaged app is missing the self-contained libmpv media-probe backend");
  }
  console.log("Verified self-contained libmpv/FFmpeg media probe and thumbnail runtime");
}

function verifyCliCompanion() {
  for (const option of ["--help", "-h"]) {
    const help = spawnSync(cliAppBinary, [option], {
      cwd: root,
      env: process.env,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
    if (help.status !== 0 || help.stderr !== "" || !help.stdout.startsWith("Usage: iina-cli")) {
      throw new Error(`Packaged iina-cli ${option} contract failed`);
    }
    for (const marker of ["--separate-windows | -w", "--keep-running", "--music-mode", "--pip"]) {
      if (!help.stdout.includes(marker)) {
        throw new Error(`Packaged iina-cli help is missing ${marker}`);
      }
    }
  }

  const conflict = spawnSync(cliAppBinary, ["--music-mode", "--pip"], {
    cwd: root,
    env: process.env,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (
    conflict.status !== 64 ||
    conflict.stdout !== "Cannot specify both --music-mode and --pip\n" ||
    conflict.stderr !== ""
  ) {
    throw new Error(
      `Packaged iina-cli incompatible-mode contract failed: status=${conflict.status} stdout=${JSON.stringify(conflict.stdout)} stderr=${JSON.stringify(conflict.stderr)}`,
    );
  }

  const mainArchitectures = new Set(capture("lipo", ["-archs", appBinary]).trim().split(/\s+/));
  const cliArchitectures = new Set(capture("lipo", ["-archs", cliAppBinary]).trim().split(/\s+/));
  if (!mainArchitectures.has(process.arch) || !cliArchitectures.has(process.arch)) {
    throw new Error(
      `Packaged executable architecture mismatch: main=${[...mainArchitectures].join(",")} cli=${[...cliArchitectures].join(",")}`,
    );
  }
  console.log("Verified packaged iina-cli help, usage-error, and architecture contracts");
}

function verifyLocalizationRuntime() {
  const exports = capture("nm", ["-gU", appBinary]);
  for (const symbol of [
    "_iima_native_preferred_languages_json",
    "_iima_native_free_localization_string",
    "_iima_native_font_picker_choose",
    "_iima_keychain_read_opensubtitles",
    "_iima_keychain_write_opensubtitles",
  ]) {
    if (!exports.includes(symbol)) {
      throw new Error(`Packaged app is missing localization export ${symbol}`);
    }
  }

  const binary = readFileSync(appBinary);
  for (const marker of [
    "IINA release/1.3.5 native menu localization tables",
    "zh-Hans.json",
    "选择字体",
    "IINA OpenSubtitles Account",
    "ratelimit-remaining",
    "OPEN_SUBTITLES_INVALID_TOKEN:",
  ]) {
    if (!binary.includes(Buffer.from(marker))) {
      throw new Error(`Packaged app is missing localization marker ${marker}`);
    }
  }
  console.log("Verified packaged frontend and native localization runtimes");
}

async function createDmg() {
  await mkdir(dmgDir, { recursive: true });
  await rm(stagingDir, { recursive: true, force: true });
  await rm(mountDir, { recursive: true, force: true });
  await rm(writableDmgPath, { force: true });
  await rm(pendingDmgPath, { force: true });
  await mkdir(stagingDir, { recursive: true });
  await mkdir(mountDir, { recursive: true });
  await cp(appPath, join(stagingDir, "IINA.app"), {
    recursive: true,
    preserveTimestamps: true,
    verbatimSymlinks: true,
  });
  await symlink("/Applications", join(stagingDir, "Applications"));

  run("hdiutil", [
    "create",
    "-ov",
    "-srcfolder",
    stagingDir,
    "-volname",
    "IINA",
    "-fs",
    "HFS+",
    "-format",
    "UDRW",
    writableDmgPath,
  ]);

  let mounted = false;
  try {
    run("hdiutil", [
      "attach",
      "-readwrite",
      "-noverify",
      "-noautoopen",
      "-mountpoint",
      mountDir,
      writableDmgPath,
    ]);
    mounted = true;

    const finderLayoutScript = `
tell application "Finder"
  tell disk "${finderDiskName}"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set bounds of container window to {100, 100, 600, 450}
    set viewOptions to the icon view options of container window
    set arrangement of viewOptions to not arranged
    set icon size of viewOptions to 128
    set position of item "IINA.app" of container window to {128, 170}
    set position of item "Applications" of container window to {372, 170}
    set extension hidden of item "IINA.app" to true
    update without registering applications
    delay 2
    close
  end tell
end tell
`;
    run("osascript", ["-e", finderLayoutScript]);
  } finally {
    if (mounted) {
      try {
        run("hdiutil", ["detach", mountDir]);
      } catch (error) {
        console.warn(`Normal DMG detach failed; retrying with force: ${error.message}`);
        run("hdiutil", ["detach", "-force", mountDir]);
      }
    }
  }

  run("hdiutil", [
    "convert",
    writableDmgPath,
    "-format",
    "UDZO",
    "-imagekey",
    "zlib-level=9",
    "-o",
    pendingDmgPath,
  ]);
  run("hdiutil", ["verify", pendingDmgPath]);

  // Keep any previous release artifact intact until its replacement is complete
  // and verified, then swap the new image into the established output path.
  await rename(pendingDmgPath, dmgPath);

  await rm(writableDmgPath, { force: true });
  await rm(stagingDir, { recursive: true, force: true });
  await rm(mountDir, { recursive: true, force: true });
  console.log("Created 500x350 IINA DMG with Applications drop link");
}

if (!skipBuild) {
  run("npm", ["run", "locales:check"]);
  run("npm", ["run", "build"]);
  run("cargo", [
    "build",
    "--release",
    "--features",
    "custom-protocol,macos-cli",
    "--offline",
    "--manifest-path",
    "src-tauri/Cargo.toml",
  ]);
  run("cargo", [
    "build",
    "--release",
    "--features",
    "custom-protocol,macos-cli",
    "--offline",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--bin",
    "iina-cli",
  ]);
}

requireFile(releaseBinary, "Release binary");
requireFile(cliReleaseBinary, "iina-cli binary");

await ensureAppBundleSkeleton();
await cp(releaseBinary, appBinary);
await cp(cliReleaseBinary, cliAppBinary);
await chmod(appBinary, 0o755);
await chmod(cliAppBinary, 0o755);
await copyIinaFrameworks();
await copySparkleUpdater();
await copyInfoPlistLocalizations();
await buildSafariExtension();
patchMpvPlaceboSymbol();
ensureRpath(appBinary, "@executable_path/../Frameworks");
ensureRpath(join(frameworksDir, "libmpv.2.dylib"), "@loader_path");
run("codesign", ["--force", "--deep", "--sign", "-", appPath]);
run("codesign", [
  "--force",
  "--sign",
  "-",
  "--entitlements",
  safariExtensionEntitlements,
  safariExtensionPath,
]);
run("codesign", ["--force", "--sign", "-", appPath]);
run("codesign", ["--verify", "--deep", "--strict", "--verbose=2", appPath]);
verifySparkleUpdater();
verifySafariExtension();
await verifyInfoPlistLocalizations();
smokeLoadLibmpv();
verifyBundledMediaRuntime();
verifyCliCompanion();
verifyLocalizationRuntime();

if (!skipDmg) {
  await createDmg();
}

console.log(`Packaged app: ${appPath}`);
if (!skipDmg) {
  console.log(`Packaged DMG: ${dmgPath}`);
}
