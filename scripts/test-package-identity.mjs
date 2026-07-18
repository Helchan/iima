import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  cargoLockPackageVersion,
  cargoPackageVersion,
  readReferencePackageIdentity,
} from "./package-identity.mjs";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const reference = readReferencePackageIdentity(root);
const packageJson = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const tauriConfig = JSON.parse(
  readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"),
);
const cargoToml = readFileSync(join(root, "src-tauri", "Cargo.toml"), "utf8");
const cargoLock = readFileSync(join(root, "src-tauri", "Cargo.lock"), "utf8");
const safariInfo = readFileSync(
  join(root, "browser", "Safari_Open_In_IINA", "Info.plist"),
  "utf8",
);
const packageSource = readFileSync(join(root, "scripts", "package-macos.mjs"), "utf8");
const referenceShared = readFileSync(
  join(root, "参考", "iina", "Configs", "Shared.xcconfig"),
  "utf8",
);

assert.deepEqual(reference, { marketingVersion: "1.3.5", buildVersion: "141" });
assert.match(referenceShared, /#include "Deployment\.xcconfig"/);
assert.equal(packageJson.version, "0.9.2");
assert.equal(tauriConfig.version, packageJson.version);
assert.equal(cargoPackageVersion(cargoToml), packageJson.version);
assert.equal(cargoLockPackageVersion(cargoLock, "iima"), packageJson.version);
assert.match(safariInfo, /<key>CFBundleShortVersionString<\/key>\s*<string>0\.9\.2<\/string>/);
assert.match(safariInfo, /<key>CFBundleVersion<\/key>\s*<string>92<\/string>/);
assert.match(packageSource, /readReferencePackageIdentity\(root\)/);
assert.match(packageSource, /const appBuildVersion = "92"/);
assert.match(packageSource, /referencePackageIdentity\.marketingVersion !== "1\.3\.5"/);
assert.match(packageSource, /\["CFBundleShortVersionString", appVersion\]/);
assert.match(packageSource, /\["CFBundleVersion", appBuildVersion\]/);
assert.match(packageSource, /shortVersion: appVersion/);
assert.match(packageSource, /bundleVersion: appBuildVersion/);
assert.match(packageSource, /`IINA_\$\{appVersion\}_\$\{dmgArchitecture\}_\$\{dmgVariant\}\.dmg`/);
assert.equal(tauriConfig.identifier, "io.iima.player", "Tauri bundle isolation must remain owned");

console.log("Project identity is 0.9.2 build 92 with IINA 1.3.5 build 141 retained as reference");
