import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import {
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
import { createServer } from "node:net";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const defaultApp = join(root, "src-tauri", "target", "release", "bundle", "macos", "IINA.app");
const appPath = resolve(process.env.IIMA_WEBKIT_PROBE_APP || defaultApp);
const appBinary = join(appPath, "Contents", "MacOS", "iima");
const keepFixture = process.argv.includes("--keep") || process.env.IIMA_WEBKIT_PROBE_KEEP === "1";
const timeoutMs = Math.max(10_000, Number(process.env.IIMA_WEBKIT_PROBE_TIMEOUT_MS) || 45_000);
const identifier = "io.iina.sync-webkit-probe";

function requireFreshPackagedApp() {
  if (!existsSync(appBinary)) throw new Error(`Packaged IINA executable not found: ${appBinary}`);
  const sourcePaths = [
    join(root, "src", "main.js"),
    join(root, "src", "plugin-realm.js"),
    join(root, "src", "plugin-sync.js"),
    join(root, "src-tauri", "src", "commands.rs"),
    join(root, "src-tauri", "src", "native_text_encoding.m"),
    join(root, "src-tauri", "src", "plugin_sync.rs"),
    join(root, "src-tauri", "src", "plugin_websocket.rs"),
  ];
  const newestSource = Math.max(...sourcePaths.map((path) => statSync(path).mtimeMs));
  if (
    statSync(appBinary).mtimeMs < newestSource
    && process.env.IIMA_WEBKIT_PROBE_ALLOW_STALE !== "1"
  ) {
    throw new Error(
      "Packaged IINA.app is older than the synchronous plugin sources; run npm run package:mac -- --skip-dmg first",
    );
  }
}

function freeLoopbackPort() {
  return new Promise((resolvePort, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close((error) => {
        if (error) reject(error);
        else resolvePort(address.port);
      });
    });
  });
}

function pluginEntry(fallbackPath, websocketPort) {
  return `"use strict";
const fallbackPath = ${JSON.stringify(fallbackPath)};
const websocketPort = ${JSON.stringify(websocketPort)};

function isPromiseLike(value) {
  return value !== null && (typeof value === "object" || typeof value === "function")
    && typeof value.then === "function";
}

function writeFallback(payload) {
  const raw = JSON.stringify(payload);
  const encoded = btoa(unescape(encodeURIComponent(raw)));
  const command = "umask 077; printf %s " + encoded + " | /usr/bin/base64 -D > " + fallbackPath;
  void iina.utils.exec("/bin/sh", ["-c", command]);
}

function publish(payload) {
  try {
    iina.file.write("@data/webkit-result.json", JSON.stringify(payload));
  } catch (error) {
    writeFallback({
      ok: false,
      phase: "publish",
      error: String(error && (error.stack || error.message) || error),
      partial: payload,
    });
  }
}

try {
  const startedAt = Date.now();
  const missing = iina.file.exists("@data/missing.txt");
  const existsElapsedMs = Date.now() - startedAt;
  const writeReturn = iina.file.write("@data/atomic.txt", "Hello 同步");
  const readValue = iina.file.read("@data/atomic.txt");
  const cp1252Value = iina.file.read("@data/cp1252.txt", { encoding: "windowsCP1252" });
  const listed = iina.file.list("@data/", { includeSubDir: true });

  let nestedWriteError = null;
  try {
    iina.file.write("@data/missing-parent/child.txt", "must fail");
  } catch (error) {
    nestedWriteError = String(error && error.message || error);
  }

  const readHandle = iina.file.handle("@data/handle.bin", "read");
  const handleBytes = readHandle.readToEnd();
  const handleOffset = readHandle.offset();
  const readCloseReturn = readHandle.close();

  const cleanupHandle = iina.file.handle("@data/cleanup.bin", "write");
  const writeHandleRead = cleanupHandle.read(1);
  const writeHandleReadToEnd = cleanupHandle.readToEnd();
  const cleanupWriteReturn = cleanupHandle.write(Uint8Array.from([9, 8, 7]));
  // Deliberately leave cleanupHandle open. Native owner/application teardown must close it.

  const fileInPath = iina.utils.fileInPath("/bin/sh");
  const resolvedPath = iina.utils.resolvePath("@data/atomic.txt");
  const execValue = iina.utils.exec("/bin/echo", ["sync-webkit-probe"]);

  const standaloneIsOpen = iina.standaloneWindow.isOpen();
  const standaloneSetStyleReturn = iina.standaloneWindow.setStyle("body { color: white; }");

  let websocketInvalidPortError = null;
  try {
    iina.ws.createServer({ port: 0 });
  } catch (error) {
    websocketInvalidPortError = String(error && error.message || error);
  }
  const websocketCreateReturn = iina.ws.createServer({ port: websocketPort });
  const websocketStartReturn = iina.ws.startServer();
  let websocketSecondStartError = null;
  try {
    iina.ws.startServer();
  } catch (error) {
    websocketSecondStartError = String(error && error.message || error);
  }
  const websocketObjectHandlerReturn = iina.ws.onMessage({ marker: true });
  const websocketRemoveHandlerReturn = iina.ws.onMessage(() => {});
  const websocketPrimitiveHandlerReturn = iina.ws.onMessage(7);
  const websocketSendValue = iina.ws.sendText("missing-connection", "ignored");

  publish({
    ok: true,
    realm: {
      XMLHttpRequest: typeof XMLHttpRequest,
      tauri: typeof __TAURI__,
      tauriInternals: typeof __TAURI_INTERNALS__,
      document: typeof document,
      window: typeof window,
      parent: typeof parent,
      globalXMLHttpRequest: typeof globalThis.XMLHttpRequest,
      globalTauri: typeof globalThis.__TAURI__,
    },
    file: {
      missing,
      existsType: typeof missing,
      existsIsPromise: isPromiseLike(missing),
      existsElapsedMs,
      writeReturnType: typeof writeReturn,
      readValue,
      readType: typeof readValue,
      cp1252Value,
      listIsArray: Array.isArray(listed),
      listIsPromise: isPromiseLike(listed),
      list: listed,
      nestedWriteError,
    },
    handle: {
      bytes: Array.from(handleBytes || []),
      bytesTag: Object.prototype.toString.call(handleBytes),
      bytesInPluginRealm: handleBytes instanceof Uint8Array,
      offset: handleOffset,
      readCloseReturnType: typeof readCloseReturn,
      writeHandleRead,
      writeHandleReadToEnd,
      cleanupWriteReturnType: typeof cleanupWriteReturn,
    },
    utils: {
      fileInPath,
      fileInPathType: typeof fileInPath,
      resolvedPath,
      resolvedPathType: typeof resolvedPath,
      execIsPromiseLike: isPromiseLike(execValue),
    },
    standalone: {
      isOpen: standaloneIsOpen,
      isOpenType: typeof standaloneIsOpen,
      isOpenIsPromise: isPromiseLike(standaloneIsOpen),
      setStyleReturnType: typeof standaloneSetStyleReturn,
    },
    websocket: {
      invalidPortError: websocketInvalidPortError,
      createReturnType: typeof websocketCreateReturn,
      startReturnType: typeof websocketStartReturn,
      secondStartError: websocketSecondStartError,
      objectHandlerReturnType: typeof websocketObjectHandlerReturn,
      removeHandlerReturnType: typeof websocketRemoveHandlerReturn,
      primitiveHandlerReturnType: typeof websocketPrimitiveHandlerReturn,
      sendIsPromiseLike: isPromiseLike(websocketSendValue),
    },
  });
} catch (error) {
  const failure = {
    ok: false,
    phase: "entry",
    error: String(error && (error.stack || error.message) || error),
  };
  try {
    iina.file.write("@data/webkit-result.json", JSON.stringify(failure));
  } catch {
    writeFallback(failure);
  }
}
`;
}

function prepareFixture(websocketPort) {
  const home = mkdtempSync("/tmp/iima-plugin-sync-webkit-");
  const config = join(home, "Library", "Application Support", "io.iima.player");
  const plugins = join(config, "plugins");
  const pluginRoot = join(plugins, `${identifier}.iinaplugin`);
  const data = join(plugins, ".data", identifier);
  const temporary = join(home, "tmp");
  const fallbackPath = join(home, "webkit-fallback.json");
  mkdirSync(pluginRoot, { recursive: true });
  mkdirSync(data, { recursive: true });
  mkdirSync(temporary, { recursive: true });
  mkdirSync(join(data, "real-dir"));
  symlinkSync(join(data, "real-dir"), join(data, "directory-link"));
  symlinkSync(join(data, "missing-target"), join(data, "dangling-link"));
  writeFileSync(join(data, "cp1252.txt"), Buffer.from([0x63, 0x61, 0x66, 0xe9, 0x20, 0x80]));
  writeFileSync(join(data, "handle.bin"), Buffer.from([0, 127, 255, 1]));
  writeFileSync(join(data, "cleanup.bin"), Buffer.alloc(0));
  writeFileSync(
    join(config, "preferences.json"),
    `${JSON.stringify({
      values: {
        actionAfterLaunch: 0,
        enableAdvancedSettings: true,
        enableLogging: true,
        iinaEnablePluginSystem: true,
        logLevel: 0,
      },
    }, null, 2)}\n`,
  );
  writeFileSync(
    join(plugins, "plugins.json"),
    `${JSON.stringify({ enabled: { [identifier]: true }, order: [identifier] }, null, 2)}\n`,
  );
  writeFileSync(
    join(pluginRoot, "Info.json"),
    `${JSON.stringify({
      name: "Synchronous WebKit Probe",
      author: { name: "IIMA Tauri verification" },
      identifier,
      version: "1.0.0",
      entry: "entry.js",
      permissions: ["file-system"],
    }, null, 2)}\n`,
  );
  writeFileSync(join(pluginRoot, "entry.js"), pluginEntry(fallbackPath, websocketPort));
  return {
    home,
    config,
    data,
    temporary,
    resultPath: join(data, "webkit-result.json"),
    fallbackPath,
  };
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

async function waitForResult(fixture, child, output) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    for (const path of [fixture.resultPath, fixture.fallbackPath]) {
      if (!existsSync(path)) continue;
      const raw = readFileSync(path, "utf8");
      if (!raw.trim()) continue;
      return JSON.parse(raw);
    }
    if (child.exitCode !== null) {
      throw new Error(`Packaged IINA exited before publishing the probe result\n${output()}`);
    }
    await delay(100);
  }
  throw new Error(`Timed out waiting for packaged WebKit probe\n${output()}`);
}

function quitPackagedApp(child) {
  const quit = spawnSync("/usr/bin/osascript", [
    "-e",
    'tell application id "io.iima.player" to quit',
  ], { encoding: "utf8" });
  if (quit.status !== 0 && child.exitCode === null) child.kill("SIGTERM");
}

async function waitForExit(child) {
  const deadline = Date.now() + 15_000;
  while (child.exitCode === null && Date.now() < deadline) await delay(100);
  if (child.exitCode === null) {
    child.kill("SIGTERM");
    await delay(500);
  }
}

function advancedLog(fixture) {
  const logRoot = join(fixture.home, "Library", "Logs", "io.iima.player");
  if (!existsSync(logRoot)) return "";
  return readdirSync(logRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .flatMap((entry) => {
      const path = join(logRoot, entry.name, "iina.log");
      return existsSync(path) ? [readFileSync(path, "utf8")] : [];
    })
    .join("\n");
}

function verifyResult(result, log) {
  assert.equal(result.ok, true, result.error || "plugin entry did not complete");
  for (const value of Object.values(result.realm)) assert.equal(value, "undefined");
  assert.equal(result.file.missing, false);
  assert.equal(result.file.existsType, "boolean");
  assert.equal(result.file.existsIsPromise, false);
  assert.equal(result.file.writeReturnType, "undefined");
  assert.equal(result.file.readValue, "Hello 同步");
  assert.equal(result.file.cp1252Value, "café €");
  assert.equal(result.file.listIsArray, true);
  assert.equal(result.file.listIsPromise, false);
  assert.match(result.file.nestedWriteError, /Cannot write file|No such file|directory/i);
  const directoryLink = result.file.list.find((entry) => entry.filename === "directory-link");
  const danglingLink = result.file.list.find((entry) => entry.filename === "dangling-link");
  assert.equal(directoryLink?.isDir, true);
  assert.equal(danglingLink?.isDir, false);
  assert.deepEqual(result.handle.bytes, [0, 127, 255, 1]);
  assert.equal(result.handle.bytesTag, "[object Uint8Array]");
  assert.equal(result.handle.bytesInPluginRealm, true);
  assert.equal(result.handle.offset, 4);
  assert.equal(result.handle.readCloseReturnType, "undefined");
  assert.equal(result.handle.writeHandleRead, null);
  assert.equal(result.handle.writeHandleReadToEnd, null);
  assert.equal(result.handle.cleanupWriteReturnType, "undefined");
  assert.equal(result.utils.fileInPath, true);
  assert.equal(result.utils.fileInPathType, "boolean");
  assert.equal(result.utils.resolvedPathType, "string");
  assert.equal(result.utils.execIsPromiseLike, true);
  assert.equal(result.standalone.isOpenType, "boolean");
  assert.equal(result.standalone.isOpenIsPromise, false);
  assert.equal(result.standalone.setStyleReturnType, "undefined");
  assert.match(result.websocket.invalidPortError, /port not specified/);
  assert.equal(result.websocket.createReturnType, "undefined");
  assert.equal(result.websocket.startReturnType, "undefined");
  assert.match(result.websocket.secondStartError, /not in ready state/);
  assert.equal(result.websocket.objectHandlerReturnType, "undefined");
  assert.equal(result.websocket.removeHandlerReturnType, "undefined");
  assert.equal(result.websocket.primitiveHandlerReturnType, "undefined");
  assert.equal(result.websocket.sendIsPromiseLike, true);
  assert.match(
    log,
    /synchronous invocation completed for io\.iina\.sync-webkit-probe entry \(file\.exists\) in WebView main: ok/,
  );
  assert.match(
    log,
    /synchronous invocation completed for io\.iina\.sync-webkit-probe entry \(ws\.startserver\) in WebView main: ok/,
  );
  assert.match(
    log,
    /application-exit cleanup completed: [1-9][0-9]* grant\(s\), 1 tracked file handle\(s\)/,
  );
}

requireFreshPackagedApp();
const websocketPort = await freeLoopbackPort();
const fixture = prepareFixture(websocketPort);
let stdout = "";
let stderr = "";
let child;
let succeeded = false;

try {
  child = spawn(appBinary, [], {
    cwd: dirname(appBinary),
    env: {
      ...process.env,
      CFFIXED_USER_HOME: fixture.home,
      HOME: fixture.home,
      TMPDIR: fixture.temporary,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => { stdout = `${stdout}${chunk}`.slice(-128_000); });
  child.stderr.on("data", (chunk) => { stderr = `${stderr}${chunk}`.slice(-128_000); });
  const output = () => [stdout, stderr].filter(Boolean).join("\n");
  const result = await waitForResult(fixture, child, output);
  quitPackagedApp(child);
  await waitForExit(child);
  const log = advancedLog(fixture);
  verifyResult(result, log);
  const evidence = {
    schema: 1,
    appPath,
    result,
    logEvidence: log
      .split("\n")
      .filter((line) => line.includes("[plugin-sync]")),
    ...(keepFixture ? { fixtureHome: fixture.home } : {}),
  };
  console.log(JSON.stringify(evidence, null, 2));
  succeeded = true;
} catch (error) {
  if (child?.exitCode === null) {
    quitPackagedApp(child);
    await waitForExit(child);
  }
  console.error(`WebKit probe fixture preserved at ${fixture.home}`);
  throw error;
} finally {
  if (succeeded && !keepFixture && child?.exitCode !== null) {
    rmSync(fixture.home, { recursive: true, force: true });
  }
}
