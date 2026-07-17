import { readFileSync } from "node:fs";
import { join } from "node:path";

function xcconfigValue(source, key) {
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = source.match(new RegExp(`^\\s*${escapedKey}\\s*=\\s*(.+?)\\s*$`, "m"));
  if (!match) throw new Error(`Reference Deployment.xcconfig is missing ${key}`);
  return match[1];
}

export function readReferencePackageIdentity(root) {
  const deploymentConfig = readFileSync(
    join(root, "参考", "iina", "Configs", "Deployment.xcconfig"),
    "utf8",
  );
  return {
    marketingVersion: xcconfigValue(deploymentConfig, "MARKETING_VERSION"),
    buildVersion: xcconfigValue(deploymentConfig, "CURRENT_PROJECT_VERSION"),
  };
}

export function cargoPackageVersion(source) {
  const packageStart = source.indexOf("[package]");
  const nextSection = source.indexOf("\n[", packageStart + "[package]".length);
  const packageSection = packageStart < 0
    ? ""
    : source.slice(packageStart + "[package]".length, nextSection < 0 ? undefined : nextSection);
  const match = packageSection.match(/^version\s*=\s*"([^"]+)"\s*$/m);
  if (!match) throw new Error("Cargo.toml [package] is missing version");
  return match[1];
}

export function cargoLockPackageVersion(source, packageName) {
  for (const section of source.split("[[package]]").slice(1)) {
    const name = section.match(/^\s*name\s*=\s*"([^"]+)"\s*$/m)?.[1];
    if (name !== packageName) continue;
    const version = section.match(/^\s*version\s*=\s*"([^"]+)"\s*$/m)?.[1];
    if (!version) throw new Error(`Cargo.lock package ${packageName} is missing version`);
    return version;
  }
  throw new Error(`Cargo.lock is missing package ${packageName}`);
}
