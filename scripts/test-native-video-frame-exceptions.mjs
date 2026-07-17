import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));

if (process.platform !== "darwin") {
  console.log("Native video frame exception-boundary checks skipped outside macOS");
} else {
  const temporaryRoot = mkdtempSync(join(tmpdir(), "iima-native-video-frame-test-"));
  const executable = join(temporaryRoot, "native-video-frame-exception-harness");
  const mainThreadExecutable = join(temporaryRoot, "native-video-main-thread-harness");
  try {
    const compile = spawnSync(
      "xcrun",
      [
        "--sdk", "macosx", "clang",
        "-x", "objective-c",
        "-fobjc-arc",
        "-fblocks",
        "-DGL_SILENCE_DEPRECATION",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-Wno-deprecated-declarations",
        "-I", "参考/iina/deps/include",
        "scripts/native-video-frame-exception-harness.m",
        "-framework", "AppKit",
        "-framework", "ColorSync",
        "-framework", "CoreVideo",
        "-framework", "OpenGL",
        "-framework", "QuartzCore",
        "-o", executable,
      ],
      { cwd: root, encoding: "utf8" },
    );
    assert.equal(
      compile.status,
      0,
      `Native video Objective-C harness compilation failed:\n${compile.stdout}${compile.stderr}`,
    );

    const run = spawnSync(executable, [], {
      cwd: root,
      encoding: "utf8",
      timeout: 30_000,
    });
    assert.equal(
      run.status,
      0,
      `Native video Objective-C harness failed:\n${run.stdout}${run.stderr}`,
    );
    process.stdout.write(run.stdout);

    const mainThreadCompile = spawnSync(
      "xcrun",
      [
        "--sdk", "macosx", "clang",
        "-x", "objective-c",
        "-fobjc-arc",
        "-fblocks",
        "-DGL_SILENCE_DEPRECATION",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-Wno-deprecated-declarations",
        "-I", "参考/iina/deps/include",
        "src-tauri/src/native_video.m",
        "scripts/native-video-main-thread-harness.m",
        "-framework", "AppKit",
        "-framework", "ColorSync",
        "-framework", "CoreVideo",
        "-framework", "OpenGL",
        "-framework", "QuartzCore",
        "-o", mainThreadExecutable,
      ],
      { cwd: root, encoding: "utf8" },
    );
    assert.equal(
      mainThreadCompile.status,
      0,
      `Native video main-thread harness compilation failed:\n${mainThreadCompile.stdout}${mainThreadCompile.stderr}`,
    );

    const mainThreadRun = spawnSync(mainThreadExecutable, [], {
      cwd: root,
      encoding: "utf8",
      timeout: 30_000,
    });
    assert.equal(
      mainThreadRun.status,
      0,
      `Native video main-thread harness failed:\n${mainThreadRun.stdout}${mainThreadRun.stderr}`,
    );
    process.stdout.write(mainThreadRun.stdout);
  } finally {
    rmSync(temporaryRoot, { recursive: true, force: true });
  }
}
