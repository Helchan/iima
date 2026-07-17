import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

import { createPluginRealmHost } from "../src/plugin-realm.js";

const realmSource = readFileSync(new URL("../src/plugin-realm.js", import.meta.url), "utf8");
const frontendSource = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
for (const contract of [
  'frame.setAttribute("sandbox", "allow-scripts allow-same-origin")',
  "default-src 'none'",
  "createCapabilityGlobal(realmWindow, RealmObject)",
  "frame.remove()",
  "PLUGIN_REALM_TAURI_GLOBALS",
]) {
  assert.ok(realmSource.includes(contract), `Missing production plugin realm contract: ${contract}`);
}
for (const contract of [
  'import { createWebKitPluginRealm } from "./plugin-realm.js"',
  'role: "entry"',
  'role: "global"',
  "runtime.entryRealm?.destroy()",
  "runtime.globalRealm?.destroy()",
]) {
  assert.ok(frontendSource.includes(contract), `Missing plugin realm integration contract: ${contract}`);
}
assert.ok(!frontendSource.includes("const evaluator = new Function("), "plugin code must not execute in the player realm");

const leaseBindingStart = frontendSource.indexOf("function bindPluginApiToRealmLease(api, lease)");
const leaseBindingEnd = frontendSource.indexOf("\nfunction createIinaPluginApi", leaseBindingStart);
assert.ok(leaseBindingStart >= 0 && leaseBindingEnd > leaseBindingStart, "realm lease binding is missing");
const leaseOutcome = vm.runInNewContext(`
${frontendSource.slice(leaseBindingStart, leaseBindingEnd)}
(() => {
  const entryLease = {
    active: true,
    contextId: "entry-context-1",
    identifier: "io.iina.realm-test",
    role: "entry",
  };
  const globalLease = {
    active: true,
    contextId: "global-context-1",
    identifier: "io.iina.realm-test",
    role: "global",
  };
  let entryActionCount = 0;
  let entryLogCount = 0;
  let globalActionCount = 0;
  const entryCapability = bindPluginApiToRealmLease({
    menu: { item: () => ({ trigger: () => { entryActionCount += 1; } }) },
    console: { log: () => { entryLogCount += 1; } },
  }, entryLease);
  const globalCapability = bindPluginApiToRealmLease({
    menu: { item: () => { globalActionCount += 1; } },
  }, globalLease);
  const oldEntryItem = entryCapability.menu.item();
  oldEntryItem.trigger();
  entryLease.active = false;
  let currentEntryError = "";
  let retainedItemError = "";
  try { entryCapability.menu.item(); } catch (error) { currentEntryError = error.message; }
  try { oldEntryItem.trigger(); } catch (error) { retainedItemError = error.message; }
  entryCapability.console.log("retired context log");
  globalCapability.menu.item();
  return { entryActionCount, entryLogCount, globalActionCount, currentEntryError, retainedItemError };
})()
`);
assert.match(leaseOutcome.currentEntryError, /context is no longer active/);
assert.match(leaseOutcome.retainedItemError, /context is no longer active/);
assert.equal(leaseOutcome.entryActionCount, 1, "retired entry actions must not reach the replacement runtime");
assert.equal(leaseOutcome.entryLogCount, 1, "retired context logs stay available to its own Developer Tool");
assert.equal(leaseOutcome.globalActionCount, 1, "retiring entry must not revoke the global instance lease");

const scripts = {
  "entry.js": `
globalThis.realmMarker = "entry";
Array.prototype.pluginRealmMarker = "entry";
const first = require("./shared");
const second = require("./shared.js");
const listenerId = iina.event.on("iina.file-loaded", (value) => {
  globalThis.callbackValue = value;
  return value === 42;
});
iina.capture({
  listenerId,
  sameModule: first === second,
  loads: first.loads,
  marker: first.marker,
  mpvValue: iina.mpv.getNumber("speed"),
  windowType: typeof window,
  documentType: typeof document,
  fetchType: typeof fetch,
  capabilityParentHidden: globalThis.parent === undefined,
  capabilityDocumentHidden: globalThis.document === undefined,
  tauriHidden: globalThis.__TAURI__ === undefined,
});
`,
  "global.js": `
globalThis.realmMarker = "global";
Array.prototype.pluginRealmMarker = "global";
const shared = require("./shared");
iina.capture({ loads: shared.loads, marker: shared.marker });
`,
  "shared.js": `
globalThis.sharedLoads = (globalThis.sharedLoads || 0) + 1;
module.exports = {
  loads: globalThis.sharedLoads,
  marker: Array.prototype.pluginRealmMarker,
};
`,
  "inspect.js": `
module.exports = {
  marker: globalThis.realmMarker,
  callbackValue: globalThis.callbackValue,
};
`,
};

function newVmRealm(identifier, role) {
  const context = vm.createContext({ __TAURI__: { core: "must-not-leak" } });
  const realmWindow = vm.runInContext("globalThis", context);
  let disposeCount = 0;
  const host = createPluginRealmHost({
    identifier,
    role,
    realmWindow,
    dispose: () => { disposeCount += 1; },
  });
  return { context, host, disposeCount: () => disposeCount };
}

function resolvedModulePath(fromPath, request) {
  assert.equal(typeof request, "string");
  assert.ok(request.startsWith("."));
  const parts = fromPath.split("/");
  parts.pop();
  for (const part of request.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") parts.pop();
    else parts.push(part);
  }
  const requested = parts.join("/");
  return [requested, `${requested}.js`, `${requested}/index.js`]
    .find((candidate) => typeof scripts[candidate] === "string");
}

function execute(host, path, api, moduleExports, asModule = false) {
  return host.evaluate({
    path,
    source: scripts[path],
    api,
    asModule,
    moduleExports,
    requireModule: (request) => {
      const resolved = resolvedModulePath(path, request);
      assert.ok(resolved, `Missing test module ${request}`);
      return execute(host, resolved, api, moduleExports, true);
    },
    pluginConsole: console,
  });
}

const entry = newVmRealm("io.iina.realm-test", "entry");
const global = newVmRealm("io.iina.realm-test", "global");
let entryCapture;
let globalCapture;
let entryCallback;
const entryApi = {
  capture: (value) => { entryCapture = value; },
  event: {
    on: (_name, callback) => {
      entryCallback = callback;
      return "event_1";
    },
  },
  mpv: { getNumber: () => 7 },
};
const globalApi = {
  capture: (value) => { globalCapture = value; },
};

const entryModules = new Map();
const globalModules = new Map();
execute(entry.host, "entry.js", entryApi, entryModules);
execute(global.host, "global.js", globalApi, globalModules);

assert.equal(entryCapture.listenerId, "event_1", "sync API return values must stay synchronous");
assert.equal(entryCapture.sameModule, true, "relative CommonJS modules must be cached per realm");
assert.equal(entryCapture.loads, 1);
assert.equal(entryCapture.marker, "entry");
assert.equal(entryCapture.mpvValue, 7);
assert.equal(entryCapture.windowType, "undefined", "player DOM must not be an ambient plugin binding");
assert.equal(entryCapture.documentType, "undefined", "player document must not be an ambient plugin binding");
assert.equal(entryCapture.fetchType, "undefined", "network access must stay behind iina.http permissions");
assert.equal(entryCapture.capabilityParentHidden, true, "the capability global must not expose the player Window");
assert.equal(entryCapture.capabilityDocumentHidden, true, "the capability global must not expose a DOM document");
assert.equal(entryCapture.tauriHidden, true, "Tauri IPC globals must not enter a plugin realm");
assert.equal(globalCapture.loads, 1, "global must own an independent CommonJS cache");
assert.equal(globalCapture.marker, "global");

assert.equal(globalThis.realmMarker, undefined);
assert.equal(Array.prototype.pluginRealmMarker, undefined, "plugin prototype writes must not reach the player realm");
assert.equal(vm.runInContext("globalThis.realmMarker", entry.context), undefined, "the iframe Window must not be the plugin global object");
assert.equal(vm.runInContext("globalThis.realmMarker", global.context), undefined, "globalEntry must not receive its iframe Window");
assert.equal(vm.runInContext("Array.prototype.pluginRealmMarker", entry.context), "entry");
assert.equal(vm.runInContext("Array.prototype.pluginRealmMarker", global.context), "global");
assert.notEqual(Object.getPrototypeOf(entryCallback), Function.prototype, "callbacks must be created in the plugin realm");
assert.equal(entryCallback(42), true, "synchronous callback return values must cross the capability binding");
const entryBytes = entry.host.createUint8Array([0, 127, 255]);
assert.deepEqual(Array.from(entryBytes), [0, 127, 255]);
assert.equal(
  Object.getPrototypeOf(entryBytes),
  vm.runInContext("Uint8Array.prototype", entry.context),
  "native file-handle bytes must use the plugin realm's Uint8Array intrinsic",
);
const entryInspection = execute(entry.host, "inspect.js", entryApi, entryModules, true);
const globalInspection = execute(global.host, "inspect.js", globalApi, globalModules, true);
assert.equal(entryInspection.marker, "entry");
assert.equal(entryInspection.callbackValue, 42);
assert.equal(globalInspection.marker, "global");
assert.equal(globalInspection.callbackValue, undefined);

assert.equal(
  entry.host.evaluateDeveloper("globalThis.realmMarker"),
  "entry",
  "Developer Tool expressions must execute in the existing entry realm",
);
assert.equal(
  global.host.evaluateDeveloper("globalThis.realmMarker"),
  "global",
  "Developer Tool global entries must execute in the existing global realm",
);
assert.equal(entry.host.evaluateDeveloper('iina.mpv.getNumber("speed") + 1'), 8);
assert.equal(
  entry.host.evaluateDeveloper("if (true) { globalThis.devToolValue = 9; }"),
  undefined,
  "Developer Tool must accept statement programs as well as expressions",
);
assert.equal(entry.host.evaluateDeveloper("globalThis.devToolValue"), 9);
assert.equal(entry.host.developerGlobal().document, undefined);
assert.equal(entry.host.developerGlobal().__TAURI__, undefined);
assert.throws(() => entry.host.evaluateDeveloper("throw new Error('devtool failure')"), /devtool failure/);

const globalIdentityAcrossEntryReload = global.host.developerGlobal();
global.host.evaluateDeveloper("globalThis.reloadSentinel = { count: 17 };");
entry.host.destroy();
entry.host.destroy();
assert.equal(entry.disposeCount(), 1, "realm teardown must be idempotent");
assert.throws(
  () => execute(entry.host, "entry.js", entryApi, new Map()),
  /realm has been destroyed/,
  "unloaded plugin code must not be executable",
);
assert.throws(() => entry.host.createUint8Array([1]), /realm has been destroyed/);
assert.throws(() => entry.host.evaluateDeveloper("1 + 1"), /realm has been destroyed/);
const reloadedEntry = newVmRealm("io.iina.realm-test", "entry");
assert.strictEqual(
  global.host.developerGlobal(),
  globalIdentityAcrossEntryReload,
  "replacing a PlayerCore entry realm must preserve global object identity",
);
assert.equal(
  global.host.evaluateDeveloper("globalThis.reloadSentinel.count"),
  17,
  "replacing a PlayerCore entry realm must preserve global state",
);
assert.equal(globalModules.get("shared.js").loads, 1, "global CommonJS state must survive entry reload");
reloadedEntry.host.destroy();
global.host.destroy();
assert.equal(global.disposeCount(), 1);

console.log("Plugin entry/global realm isolation checks passed");
