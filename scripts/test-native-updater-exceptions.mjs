import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));

if (process.platform !== "darwin") {
  console.log("Native updater Objective-C exception-boundary checks skipped outside macOS");
} else {
  const temporaryRoot = mkdtempSync(join(tmpdir(), "iima-native-updater-test-"));
  const executable = join(temporaryRoot, "native-updater-exception-harness");
  try {
    const compile = spawnSync(
      "xcrun",
      [
        "--sdk", "macosx", "clang",
        "-x", "objective-c",
        "-fobjc-arc",
        "-fblocks",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-Wno-deprecated-declarations",
        "scripts/native-updater-exception-harness.m",
        "-framework", "Foundation",
        "-framework", "AppKit",
        "-o", executable,
      ],
      { cwd: root, encoding: "utf8" },
    );
    assert.equal(
      compile.status,
      0,
      `Native updater Objective-C harness compilation failed:\n${compile.stdout}${compile.stderr}`,
    );

    const run = spawnSync(executable, [], {
      cwd: root,
      encoding: "utf8",
      timeout: 30_000,
    });
    assert.equal(
      run.status,
      0,
      `Native updater Objective-C harness failed:\n${run.stdout}${run.stderr}`,
    );
    process.stdout.write(run.stdout);
  } finally {
    rmSync(temporaryRoot, { recursive: true, force: true });
  }
}
