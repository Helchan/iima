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
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { dirname, extname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const defaultAppPath = resolve(
  join(root, "src-tauri", "target", "release", "bundle", "macos", "IINA.app"),
);
const appPath = resolve(
  process.env.IIMA_GUI_COLD_LAUNCH_APP
    || defaultAppPath,
);
const appBinary = join(appPath, "Contents", "MacOS", "iima");
const cliBinary = join(appPath, "Contents", "MacOS", "iina-cli");
const releaseBinary = join(root, "src-tauri", "target", "release", "iima");
const cliReleaseBinary = join(root, "src-tauri", "target", "release", "iina-cli");
const fixture = resolve(
  process.env.IIMA_GUI_MEDIA_FIXTURE
    || "/private/tmp/iima-gui-playback-fixture.mp4",
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

const rounds = positiveIntegerEnvironment("IIMA_GUI_COLD_LAUNCH_ROUNDS", 3, 1);
const timeoutMs = positiveIntegerEnvironment("IIMA_GUI_COLD_LAUNCH_TIMEOUT_MS", 25_000, 10_000);
const keep = process.argv.includes("--keep") || process.env.IIMA_GUI_COLD_LAUNCH_KEEP === "1";

function requireExecutable(path, label) {
  if (!existsSync(path)) throw new Error(`${label} is missing: ${path}`);
  const stats = statSync(path);
  if (!stats.isFile() || stats.size === 0) throw new Error(`${label} is not a non-empty file: ${path}`);
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
  requireExecutable(appBinary, "Packaged IINA executable");
  requireExecutable(cliBinary, "Packaged iina-cli executable");
  if (!existsSync(fixture)) throw new Error(`GUI cold-launch fixture is missing: ${fixture}`);
  const fixtureStats = statSync(fixture);
  if (!fixtureStats.isFile() || fixtureStats.size === 0) {
    throw new Error(`GUI cold-launch fixture is not a non-empty file: ${fixture}`);
  }
  if (process.env.IIMA_GUI_COLD_LAUNCH_ALLOW_STALE === "1") return;

  const sharedBuildSources = [
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock",
    "src-tauri/build.rs",
  ].map((path) => join(root, path));
  const playbackSources = [
    "src-tauri/src/commands.rs",
    "src-tauri/src/launch.rs",
    "src-tauri/src/mpv.rs",
    "src-tauri/src/native_window.m",
    "src-tauri/src/native_window_behavior.rs",
    "src-tauri/src/native_video.m",
    "src-tauri/src/native_video.rs",
  ].map((path) => join(root, path)).concat(sharedBuildSources);
  const cliSources = [
    join(root, "src-tauri", "src", "bin", "iina-cli.rs"),
    ...sharedBuildSources,
  ];

  requireArtifactNewerThanSources(appBinary, playbackSources, "Packaged IINA executable");
  requireArtifactNewerThanSources(cliBinary, cliSources, "Packaged iina-cli executable");

  // The packager copies target/release binaries before signing. Checking only the
  // copied files would let a --skip-build package look fresh even when Cargo's
  // executable predates the renderer fix.
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
  const rootPath = join(home, "Library", "Logs", "io.iima.player");
  if (!existsSync(rootPath)) return null;
  const candidates = readdirSync(rootPath, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => join(rootPath, entry.name, "mpv.log"))
    .filter(existsSync)
    .sort();
  return candidates.at(-1) || null;
}

function prepareRound(index) {
  const home = mkdtempSync(join(tmpdir(), `iima-gui-cold-launch-${index}-`));
  const config = join(home, "Library", "Application Support", "io.iima.player");
  const mediaDirectory = join(home, "media");
  const temporary = join(home, "tmp");
  mkdirSync(config, { recursive: true });
  mkdirSync(mediaDirectory, { recursive: true });
  mkdirSync(temporary, { recursive: true });
  const media = join(mediaDirectory, `cold-launch${extname(fixture) || ".mp4"}`);
  symlinkSync(fixture, media);
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
  writeFileSync(join(config, ".firstLaunchAfter1.3.5"), "packaged GUI acceptance\n");
  return { home, media, temporary };
}

async function stopChild(child, state) {
  if (state.exited) return;
  child.kill("SIGTERM");
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

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

async function runRound(index) {
  const fixtureState = prepareRound(index);
  let stdout = "";
  let stderr = "";
  const childState = { exited: false, code: null, signal: null, spawnError: null };
  const child = spawn(cliBinary, ["--keep-running", fixtureState.media], {
    cwd: dirname(cliBinary),
    env: {
      ...process.env,
      HOME: fixtureState.home,
      CFFIXED_USER_HOME: fixtureState.home,
      TMPDIR: fixtureState.temporary,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => { stdout = `${stdout}${chunk}`.slice(-64_000); });
  child.stderr.on("data", (chunk) => { stderr = `${stderr}${chunk}`.slice(-64_000); });
  child.once("exit", (code, signal) => {
    childState.exited = true;
    childState.code = code;
    childState.signal = signal;
  });
  child.once("error", (error) => {
    childState.exited = true;
    childState.spawnError = error;
  });

  let logPath = null;
  let log = "";
  const startedAt = Date.now();
  let succeeded = false;
  let caughtError = null;
  try {
    const deadline = startedAt + timeoutMs;
    let playbackReady = false;
    while (Date.now() < deadline) {
      logPath = latestMpvLog(fixtureState.home);
      if (logPath) log = readFileSync(logPath, "utf8");
      assert.doesNotMatch(log, /No render context set/);
      const loadedRequestedMedia = log.includes(`url="${fixtureState.media}"`);
      const rendererReady = /VO: \[libmpv\]/.test(log);
      const playbackStarted = /playback restart complete/.test(log);
      if (loadedRequestedMedia && rendererReady && playbackStarted) {
        playbackReady = true;
        break;
      }
      if (childState.exited) {
        throw new Error(
          `Packaged IINA exited before playback became ready (code=${childState.code}, signal=${childState.signal}, spawnError=${childState.spawnError?.message ?? "none"})\n${stdout}\n${stderr}`,
        );
      }
      await delay(100);
    }
    assert.ok(
      playbackReady,
      `packaged IINA did not become playback-ready within ${timeoutMs} ms `
        + `(log=${logPath ?? "none"}, loadfile=${log.includes(`url="${fixtureState.media}"`)}, `
        + `renderer=${/VO: \[libmpv\]/.test(log)}, restart=${/playback restart complete/.test(log)})`,
    );
    assert.ok(logPath, "packaged IINA did not create an mpv log");
    const logLines = log.split("\n");
    const loadfileLines = logLines.filter((line) => line.includes("Run command: loadfile"));
    const requestedMedia = new RegExp(`url="${escapeRegExp(fixtureState.media)}"`);
    assert.equal(
      loadfileLines.filter((line) => requestedMedia.test(line)).length,
      1,
      "isolated cold launch must submit the requested media exactly once",
    );
    assert.equal(loadfileLines.length, 1, "isolated cold launch must submit exactly one loadfile");
    assert.match(log, /VO: \[libmpv\]/);
    assert.match(log, /playback restart complete/);
    assert.doesNotMatch(log, /No render context set/);
    const loadfileIndex = logLines.findIndex((line) => (
      line.includes("Run command: loadfile") && requestedMedia.test(line)
    ));
    const rendererIndex = logLines.findIndex((line) => line.includes("VO: [libmpv]"));
    const playbackIndex = logLines.findIndex((line) => line.includes("playback restart complete"));
    assert.ok(
      loadfileIndex < rendererIndex && rendererIndex < playbackIndex,
      "cold-launch evidence must progress from loadfile to libmpv VO to playback restart",
    );
    await delay(250);
    assert.equal(childState.exited, false, "--keep-running IINA exited immediately after playback started");
    succeeded = true;
    return {
      round: index,
      elapsedMs: Date.now() - startedAt,
      appPid: child.pid,
      media: fixtureState.media,
      logPath,
      evidence: log.split("\n").filter((line) => (
        line.includes("Run command: loadfile")
        || line.includes("VO: [libmpv]")
        || line.includes("playback restart complete")
      )),
      ...(keep ? { fixtureHome: fixtureState.home } : {}),
    };
  } catch (error) {
    caughtError = error instanceof Error ? error : new Error(String(error));
    caughtError.message = `${caughtError.message}\nGUI cold-launch fixture preserved at ${fixtureState.home}`;
    throw caughtError;
  } finally {
    let stopError = null;
    try {
      await stopChild(child, childState);
    } catch (error) {
      stopError = error instanceof Error ? error : new Error(String(error));
    }
    if (succeeded && !keep && childState.exited) {
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
const results = [];
for (let index = 1; index <= rounds; index += 1) results.push(await runRound(index));
console.log(JSON.stringify({ schema: 1, appPath, fixture, rounds: results }, null, 2));
