import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import {
  accessSync,
  constants,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const defaultAppPath = resolve(
  join(root, "src-tauri", "target", "release", "bundle", "macos", "IINA.app"),
);
const appPath = resolve(process.env.IIMA_GUI_STDIN_APP || defaultAppPath);
const appBinary = join(appPath, "Contents", "MacOS", "iima");
const cliBinary = join(appPath, "Contents", "MacOS", "iina-cli");
const releaseBinary = join(root, "src-tauri", "target", "release", "iima");
const cliReleaseBinary = join(root, "src-tauri", "target", "release", "iina-cli");
const fixture = resolve(
  process.env.IIMA_GUI_STDIN_FIXTURE || "/private/tmp/iima-stdin-fixture.ts",
);

function positiveIntegerEnvironment(name, fallback, minimum) {
  const raw = process.env[name];
  if (raw === undefined) return fallback;
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value < minimum) {
    throw new Error(`${name} must be a safe integer greater than or equal to ${minimum}`);
  }
  return value;
}

const timeoutMs = positiveIntegerEnvironment("IIMA_GUI_STDIN_TIMEOUT_MS", 25_000, 10_000);
const soakMs = positiveIntegerEnvironment("IIMA_GUI_STDIN_SOAK_MS", 5_000, 3_000);
const keep = process.argv.includes("--keep") || process.env.IIMA_GUI_STDIN_KEEP === "1";

function requireExecutable(path, label) {
  if (!existsSync(path)) throw new Error(`${label} is missing: ${path}`);
  const stats = statSync(path);
  if (!stats.isFile() || stats.size === 0) {
    throw new Error(`${label} is not a non-empty file: ${path}`);
  }
  try {
    accessSync(path, constants.X_OK);
  } catch {
    throw new Error(`${label} is not executable: ${path}`);
  }
}

function requireArtifactNewerThanSources(artifact, sources, label) {
  for (const source of sources) {
    if (!existsSync(source)) throw new Error(`Freshness input for ${label} is missing: ${source}`);
  }
  const newestSource = sources
    .map((path) => ({ path, mtimeMs: statSync(path).mtimeMs }))
    .reduce((latest, candidate) => (candidate.mtimeMs > latest.mtimeMs ? candidate : latest));
  if (statSync(artifact).mtimeMs < newestSource.mtimeMs) {
    throw new Error(
      `${label} is older than ${newestSource.path}; rebuild and repackage IINA.app first`,
    );
  }
}

function requireFreshPackage() {
  if (process.platform !== "darwin") {
    throw new Error("Packaged stdin playback acceptance requires macOS");
  }
  requireExecutable(appBinary, "Packaged IINA executable");
  requireExecutable(cliBinary, "Packaged iina-cli executable");
  if (!existsSync(fixture)) {
    throw new Error(
      `Packaged stdin playback fixture is missing: ${fixture}. `
        + "Create the streamable MPEG-TS fixture before running this test; the test does not invoke ffmpeg.",
    );
  }
  const fixtureStats = statSync(fixture);
  if (!fixtureStats.isFile() || fixtureStats.size === 0) {
    throw new Error(`Packaged stdin playback fixture is not a non-empty file: ${fixture}`);
  }
  if (process.env.IIMA_GUI_STDIN_ALLOW_STALE === "1") return;

  const sharedBuildSources = [
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock",
    "src-tauri/build.rs",
  ].map((path) => join(root, path));
  const playbackSources = [
    "src-tauri/src/commands.rs",
    "src-tauri/src/launch.rs",
    "src-tauri/src/mpv.rs",
    "src-tauri/src/native_video.m",
    "src-tauri/src/native_video.rs",
    "src-tauri/src/native_window.m",
    "src-tauri/src/window_size.rs",
  ].map((path) => join(root, path)).concat(sharedBuildSources);
  const cliSources = [
    join(root, "src-tauri", "src", "bin", "iina-cli.rs"),
    ...sharedBuildSources,
  ];

  requireArtifactNewerThanSources(appBinary, playbackSources, "Packaged IINA executable");
  requireArtifactNewerThanSources(cliBinary, cliSources, "Packaged iina-cli executable");

  if (appPath === defaultAppPath) {
    requireExecutable(releaseBinary, "Cargo release IINA executable");
    requireExecutable(cliReleaseBinary, "Cargo release iina-cli executable");
    requireArtifactNewerThanSources(releaseBinary, playbackSources, "Cargo release IINA executable");
    requireArtifactNewerThanSources(cliReleaseBinary, cliSources, "Cargo release iina-cli executable");
    if (statSync(appBinary).mtimeMs < statSync(releaseBinary).mtimeMs) {
      throw new Error("Packaged IINA executable predates the Cargo release executable; repackage it first");
    }
    if (statSync(cliBinary).mtimeMs < statSync(cliReleaseBinary).mtimeMs) {
      throw new Error("Packaged iina-cli executable predates the Cargo release executable; repackage it first");
    }
  }
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

function latestMpvLog(home) {
  const logRoot = join(home, "Library", "Logs", "io.iima.player");
  if (!existsSync(logRoot)) return null;
  const candidates = readdirSync(logRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => join(logRoot, entry.name, "mpv.log"))
    .filter(existsSync)
    .sort();
  return candidates.at(-1) || null;
}

function prepareFixtureHome() {
  const home = mkdtempSync(join(tmpdir(), "iima-gui-stdin-playback-"));
  const config = join(home, "Library", "Application Support", "io.iima.player");
  const temporary = join(home, "tmp");
  mkdirSync(config, { recursive: true });
  mkdirSync(temporary, { recursive: true });
  writeFileSync(
    join(config, "preferences.json"),
    `${JSON.stringify({
      values: {
        actionAfterLaunch: 2,
        alwaysOpenInNewWindow: true,
        enableAdvancedSettings: true,
        enableLogging: true,
        keepOpenOnFileEnd: true,
        pauseWhenOpen: false,
        playlistAutoAdd: false,
        recordPlaybackHistory: false,
        recordRecentFiles: false,
        trackAllFilesInRecentOpenMenu: false,
      },
    }, null, 2)}\n`,
  );
  writeFileSync(join(config, ".firstLaunchAfter1.3.5"), "packaged stdin acceptance\n");
  return { home, temporary };
}

function crashEvidence(output) {
  return [
    /uncaught exception/i,
    /CGSSetSurfaceColorSpace/i,
    /NSInternalInconsistencyException/i,
    /abort/i,
  ].find((pattern) => pattern.test(output));
}

function assertNoCrashOutput(output) {
  const evidence = crashEvidence(output);
  assert.equal(
    evidence,
    undefined,
    `packaged stdin playback emitted crash evidence matching ${evidence}:\n${output}`,
  );
}

async function stopChild(child, state) {
  if (!state.exited) child.kill("SIGTERM");
  const deadline = Date.now() + 5_000;
  while (!state.exited && Date.now() < deadline) await delay(50);
  if (!state.exited) {
    child.kill("SIGKILL");
    const killDeadline = Date.now() + 2_000;
    while (!state.exited && Date.now() < killDeadline) await delay(50);
  }
  if (!state.exited) {
    throw new Error(`Packaged IINA process ${child.pid ?? "<unknown>"} did not exit after SIGKILL`);
  }
}

async function runAcceptance() {
  const fixtureState = prepareFixtureHome();
  let stdout = "";
  let stderr = "";
  let log = "";
  let logPath = null;
  const childState = {
    exited: false,
    code: null,
    signal: null,
    spawnError: null,
    stdinError: null,
  };
  const child = spawn(
    cliBinary,
    ["--stdin", "--keep-running", "--mpv-demuxer-lavf-format=mpegts"],
    {
      cwd: dirname(cliBinary),
      env: {
        ...process.env,
        HOME: fixtureState.home,
        CFFIXED_USER_HOME: fixtureState.home,
        TMPDIR: fixtureState.temporary,
      },
      stdio: ["pipe", "pipe", "pipe"],
    },
  );
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => { stdout = `${stdout}${chunk}`.slice(-64_000); });
  child.stderr.on("data", (chunk) => { stderr = `${stderr}${chunk}`.slice(-64_000); });
  child.stdin.once("error", (error) => { childState.stdinError = error; });
  child.once("exit", (code, signal) => {
    childState.exited = true;
    childState.code = code;
    childState.signal = signal;
  });
  child.once("error", (error) => {
    childState.exited = true;
    childState.spawnError = error;
  });

  const startedAt = Date.now();
  let playbackReadyAt = null;
  let succeeded = false;
  let caughtError = null;
  try {
    child.stdin.end(readFileSync(fixture));
    const deadline = startedAt + timeoutMs;
    while (Date.now() < deadline) {
      logPath = latestMpvLog(fixtureState.home);
      if (logPath) log = readFileSync(logPath, "utf8");
      assertNoCrashOutput(`${stdout}\n${stderr}\n${log}`);
      assert.doesNotMatch(log, /No render context set/);
      if (childState.exited) {
        throw new Error(
          `Packaged IINA exited before stdin playback became ready `
            + `(code=${childState.code}, signal=${childState.signal}, `
            + `spawnError=${childState.spawnError?.message ?? "none"}, `
            + `stdinError=${childState.stdinError?.message ?? "none"})`,
        );
      }

      const logLines = log.split("\n");
      const loadfileIndex = logLines.findIndex((line) => (
        line.includes("Run command: loadfile") && line.includes('url="-"')
      ));
      const rendererIndex = logLines.findIndex((line) => line.includes("VO: [libmpv]"));
      const playbackIndex = logLines.findIndex((line) => line.includes("playback restart complete"));
      if (
        loadfileIndex >= 0
        && rendererIndex > loadfileIndex
        && playbackIndex > rendererIndex
      ) {
        playbackReadyAt = Date.now();
        break;
      }
      await delay(100);
    }

    assert.ok(
      playbackReadyAt,
      `packaged IINA did not reach ordered stdin playback readiness within ${timeoutMs} ms `
        + `(log=${logPath ?? "none"})`,
    );
    assert.ok(logPath, "packaged stdin playback did not create an mpv log");
    const logLines = log.split("\n");
    const loadfileLines = logLines.filter((line) => line.includes("Run command: loadfile"));
    const stdinLoadfileLines = loadfileLines.filter((line) => line.includes('url="-"'));
    assert.equal(stdinLoadfileLines.length, 1, "stdin playback must submit loadfile '-' exactly once");
    assert.equal(loadfileLines.length, 1, "isolated stdin playback must submit exactly one loadfile");

    const soakDeadline = playbackReadyAt + soakMs;
    while (Date.now() < soakDeadline) {
      log = readFileSync(logPath, "utf8");
      assertNoCrashOutput(`${stdout}\n${stderr}\n${log}`);
      assert.doesNotMatch(log, /No render context set/);
      if (childState.exited) {
        throw new Error(
          `Packaged IINA exited during the ${soakMs} ms post-start playback window `
            + `(code=${childState.code}, signal=${childState.signal}, `
            + `spawnError=${childState.spawnError?.message ?? "none"}, `
            + `stdinError=${childState.stdinError?.message ?? "none"})`,
        );
      }
      await delay(Math.min(100, soakDeadline - Date.now()));
    }
    assertNoCrashOutput(`${stdout}\n${stderr}\n${log}`);
    assert.equal(childState.exited, false, "--keep-running IINA was not alive after the soak window");

    log = readFileSync(logPath, "utf8");
    const finalLines = log.split("\n");
    const loadfileIndex = finalLines.findIndex((line) => (
      line.includes("Run command: loadfile") && line.includes('url="-"')
    ));
    const rendererIndex = finalLines.findIndex((line) => line.includes("VO: [libmpv]"));
    const playbackIndex = finalLines.findIndex((line) => line.includes("playback restart complete"));
    assert.ok(
      loadfileIndex >= 0 && loadfileIndex < rendererIndex && rendererIndex < playbackIndex,
      "stdin evidence must progress from loadfile '-' to libmpv VO to playback restart",
    );
    const finalLoadfileLines = finalLines.filter((line) => line.includes("Run command: loadfile"));
    assert.equal(
      finalLoadfileLines.filter((line) => line.includes('url="-"')).length,
      1,
      "final stdin log must contain loadfile '-' exactly once",
    );
    assert.equal(finalLoadfileLines.length, 1, "final isolated stdin log must contain one loadfile");
    assert.doesNotMatch(log, /No render context set/);
    succeeded = true;
    return {
      elapsedMs: Date.now() - startedAt,
      soakMs,
      appPid: child.pid,
      logPath,
      evidence: finalLines.filter((line) => (
        (line.includes("Run command: loadfile") && line.includes('url="-"'))
        || line.includes("VO: [libmpv]")
        || line.includes("playback restart complete")
      )),
      stderrBytes: Buffer.byteLength(stderr),
      ...(keep ? { fixtureHome: fixtureState.home } : {}),
    };
  } catch (error) {
    const originalError = error instanceof Error ? error : new Error(String(error));
    const stdoutPath = join(fixtureState.home, "packaged-stdin-stdout.log");
    const stderrPath = join(fixtureState.home, "packaged-stdin-stderr.log");
    writeFileSync(stdoutPath, stdout);
    writeFileSync(stderrPath, stderr);
    caughtError = new Error(`${originalError.message}\nPackaged stdin fixture preserved at ${fixtureState.home}`
      + `\nmpv log: ${logPath ?? "none"}`
      + `\nstdout log: ${stdoutPath}`
      + `\nstderr log: ${stderrPath}`
      + `\nstdout tail:\n${stdout}`
      + `\nstderr tail:\n${stderr}`, { cause: originalError });
    throw caughtError;
  } finally {
    let stopError = null;
    const wasAliveBeforeCleanup = !childState.exited;
    try {
      await stopChild(child, childState);
    } catch (error) {
      stopError = error instanceof Error ? error : new Error(String(error));
    }
    if (succeeded && !stopError) {
      try {
        if (logPath && existsSync(logPath)) log = readFileSync(logPath, "utf8");
        assert.equal(
          wasAliveBeforeCleanup,
          true,
          `Packaged IINA exited between the final acceptance check and cleanup `
            + `(code=${childState.code}, signal=${childState.signal})`,
        );
        assert.notEqual(childState.code, 134, "Packaged IINA exited with abort status 134");
        assert.notEqual(childState.signal, "SIGABRT", "Packaged IINA exited via SIGABRT");
        assertNoCrashOutput(`${stdout}\n${stderr}\n${log}`);
        assert.doesNotMatch(log, /No render context set/);
      } catch (error) {
        const originalError = error instanceof Error ? error : new Error(String(error));
        const stdoutPath = join(fixtureState.home, "packaged-stdin-stdout.log");
        const stderrPath = join(fixtureState.home, "packaged-stdin-stderr.log");
        writeFileSync(stdoutPath, stdout);
        writeFileSync(stderrPath, stderr);
        stopError = new Error(
          `${originalError.message}\nPackaged stdin fixture preserved at ${fixtureState.home}`
            + `\nmpv log: ${logPath ?? "none"}`
            + `\nstdout log: ${stdoutPath}`
            + `\nstderr log: ${stderrPath}`,
          { cause: originalError },
        );
      }
    }
    if (succeeded && !stopError && !keep && childState.exited) {
      rmSync(fixtureState.home, { recursive: true, force: true });
    }
    if (stopError) {
      if (caughtError) {
        caughtError.message = `${caughtError.message}\nAdditionally, cleanup failed: ${stopError.message}`;
      } else {
        throw stopError;
      }
    }
  }
}

requireFreshPackage();
const result = await runAcceptance();
console.log(JSON.stringify({ schema: 1, appPath, fixture, result }, null, 2));
