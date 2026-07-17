const PLUGIN_REALM_AMBIENT_BINDINGS = Object.freeze([
  "globalThis",
  "window",
  "self",
  "parent",
  "top",
  "opener",
  "frames",
  "frameElement",
  "document",
  "location",
  "navigator",
  "localStorage",
  "sessionStorage",
  "indexedDB",
  "fetch",
  "XMLHttpRequest",
  "WebSocket",
  "EventSource",
  "Worker",
  "SharedWorker",
  "BroadcastChannel",
  "postMessage",
  "open",
  "alert",
  "confirm",
  "prompt",
  "__TAURI__",
  "__TAURI_INTERNALS__",
  "__TAURI_IPC__",
]);

const PLUGIN_REALM_STANDARD_GLOBALS = Object.freeze([
  "Infinity",
  "NaN",
  "undefined",
  "Object",
  "Function",
  "Boolean",
  "Symbol",
  "Error",
  "AggregateError",
  "EvalError",
  "RangeError",
  "ReferenceError",
  "SyntaxError",
  "TypeError",
  "URIError",
  "Number",
  "BigInt",
  "Math",
  "Date",
  "String",
  "RegExp",
  "Array",
  "Int8Array",
  "Uint8Array",
  "Uint8ClampedArray",
  "Int16Array",
  "Uint16Array",
  "Int32Array",
  "Uint32Array",
  "Float32Array",
  "Float64Array",
  "BigInt64Array",
  "BigUint64Array",
  "Map",
  "Set",
  "WeakMap",
  "WeakSet",
  "ArrayBuffer",
  "SharedArrayBuffer",
  "DataView",
  "Atomics",
  "JSON",
  "Promise",
  "Generator",
  "GeneratorFunction",
  "AsyncFunction",
  "Reflect",
  "Proxy",
  "Intl",
  "WebAssembly",
  "parseFloat",
  "parseInt",
  "isFinite",
  "isNaN",
  "decodeURI",
  "decodeURIComponent",
  "encodeURI",
  "encodeURIComponent",
  "escape",
  "unescape",
  "TextEncoder",
  "TextDecoder",
  "URL",
  "URLSearchParams",
  "atob",
  "btoa",
  "structuredClone",
  "queueMicrotask",
  "setTimeout",
  "clearTimeout",
  "setInterval",
  "clearInterval",
]);

const PLUGIN_REALM_TAURI_GLOBALS = Object.freeze([
  "__TAURI__",
  "__TAURI_INTERNALS__",
  "__TAURI_IPC__",
  "__TAURI_METADATA__",
]);

const PLUGIN_REALM_DOCUMENT = `<!doctype html>
<html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-eval'; connect-src 'none'; img-src 'none'; media-src 'none'; font-src 'none'; style-src 'none'; frame-src 'none'; worker-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'">
</head><body></body></html>`;

function normalizedSourceLabel(value) {
  return String(value ?? "unknown").replaceAll(/[^A-Za-z0-9._/-]/gu, "_");
}

function hideTauriGlobals(realmWindow) {
  for (const name of PLUGIN_REALM_TAURI_GLOBALS) {
    try {
      Reflect.deleteProperty(realmWindow, name);
      Object.defineProperty(realmWindow, name, {
        value: undefined,
        writable: false,
        enumerable: false,
        configurable: false,
      });
    } catch {
      // Dynamically created WebKit frames do not receive Tauri's bootstrap. If a
      // WebKit release exposes a non-configurable placeholder, the evaluator's
      // outer capability scope below still masks it from plugin source.
    }
  }
}

function realmRecord(RealmObject) {
  return Reflect.construct(RealmObject, []);
}

function defineRealmValue(target, name, value, writable = true) {
  Object.defineProperty(target, name, {
    value,
    writable,
    enumerable: false,
    configurable: false,
  });
}

function createCapabilityGlobal(realmWindow, RealmObject) {
  const capabilityGlobal = realmRecord(RealmObject);
  defineRealmValue(capabilityGlobal, "globalThis", capabilityGlobal, false);
  for (const name of PLUGIN_REALM_STANDARD_GLOBALS) {
    if (!(name in realmWindow) || name === "undefined") continue;
    let value = realmWindow[name];
    if (["setTimeout", "clearTimeout", "setInterval", "clearInterval", "queueMicrotask", "atob", "btoa"].includes(name)
      && typeof value === "function") {
      value = value.bind(realmWindow);
    }
    defineRealmValue(capabilityGlobal, name, value);
  }
  defineRealmValue(capabilityGlobal, "undefined", undefined, false);
  for (const name of PLUGIN_REALM_AMBIENT_BINDINGS) {
    if (name === "globalThis" || Object.prototype.hasOwnProperty.call(capabilityGlobal, name)) continue;
    defineRealmValue(capabilityGlobal, name, undefined, false);
  }
  return capabilityGlobal;
}

/**
 * Bind the IINA plugin evaluator to an already-created JavaScript realm.
 *
 * Production passes a hidden WebKit iframe Window. Tests pass a Node vm global
 * so that global/prototype separation and synchronous callback identity are
 * executable contracts rather than source-string assertions.
 */
export function createPluginRealmHost({
  identifier,
  role,
  realmWindow,
  dispose = () => {},
}) {
  if (!realmWindow || typeof realmWindow.Function !== "function" || typeof realmWindow.Object !== "function") {
    throw new Error("Plugin realm did not expose JavaScript intrinsics");
  }
  if (!identifier || !["entry", "global"].includes(role)) {
    throw new Error("Plugin realm requires an identifier and entry/global role");
  }

  hideTauriGlobals(realmWindow);
  const RealmFunction = realmWindow.Function;
  const RealmObject = realmWindow.Object;
  const capabilityGlobal = createCapabilityGlobal(realmWindow, RealmObject);
  const ambientParameters = PLUGIN_REALM_AMBIENT_BINDINGS.join(", ");
  const ambientArguments = ["arguments[5]"]
    .concat(PLUGIN_REALM_AMBIENT_BINDINGS.slice(1).map(() => "undefined"))
    .join(", ");
  let disposed = false;

  const host = {
    identifier,
    role,
    get destroyed() {
      return disposed;
    },
    createUint8Array(bytes) {
      if (disposed) throw new Error(`Plugin ${identifier} ${role} realm has been destroyed`);
      return realmWindow.Uint8Array.from(bytes || []);
    },
    evaluate({
      path,
      source,
      api,
      asModule = false,
      moduleExports,
      requireModule,
      pluginConsole,
    }) {
      if (disposed) throw new Error(`Plugin ${identifier} ${role} realm has been destroyed`);
      if (typeof source !== "string") throw new Error(`Plugin script not found: ${path}`);
      if (!(moduleExports instanceof Map)) throw new Error("Plugin realm requires a CommonJS module cache");
      if (asModule && moduleExports.has(path)) return moduleExports.get(path);

      const module = realmRecord(RealmObject);
      module.exports = realmRecord(RealmObject);
      if (asModule) moduleExports.set(path, module.exports);
      const require = (request) => requireModule(request);
      capabilityGlobal.iina = api;
      capabilityGlobal.console = pluginConsole;
      capabilityGlobal.require = require;
      capabilityGlobal.module = module;
      capabilityGlobal.exports = module.exports;
      const sourceUrl = `iina-plugin/${normalizedSourceLabel(identifier)}/${role}/${normalizedSourceLabel(path)}`;
      const evaluator = Reflect.construct(RealmFunction, [
        "iina",
        "module",
        "exports",
        "require",
        "console",
        `"use strict";
return (function(${ambientParameters}) {
  "use strict";
  return (function(iina, module, exports, require, console) {
    "use strict";
${source}
  }).call(undefined, iina, module, exports, require, console);
}).call(undefined, ${ambientArguments});
//# sourceURL=${sourceUrl}`,
      ]);
      evaluator(api, module, module.exports, require, pluginConsole, capabilityGlobal);
      if (!asModule) return undefined;
      moduleExports.set(path, module.exports);
      return module.exports;
    },
    evaluateDeveloper(source) {
      if (disposed) throw new Error(`Plugin ${identifier} ${role} realm has been destroyed`);
      if (typeof source !== "string" || !source.trim()) return undefined;
      const sourceUrl = `iina-plugin/${normalizedSourceLabel(identifier)}/${role}/DeveloperTool`;
      const buildEvaluator = (body) => Reflect.construct(RealmFunction, [
        "iina",
        "module",
        "exports",
        "require",
        "console",
        `"use strict";
return (function(${ambientParameters}) {
  "use strict";
  return (function(iina, module, exports, require, console) {
    "use strict";
${body}
  }).call(undefined, iina, module, exports, require, console);
}).call(undefined, ${ambientArguments});
//# sourceURL=${sourceUrl}`,
      ]);
      let evaluator;
      try {
        evaluator = buildEvaluator(`return (\n${source}\n);`);
      } catch {
        // JavaScriptCore's evaluateScript accepts both expressions and statement programs. A
        // Function-backed realm needs the explicit fallback to preserve that console behavior.
        evaluator = buildEvaluator(source);
      }
      return evaluator(
        capabilityGlobal.iina,
        capabilityGlobal.module,
        capabilityGlobal.exports,
        capabilityGlobal.require,
        capabilityGlobal.console,
        capabilityGlobal,
      );
    },
    developerGlobal() {
      if (disposed) throw new Error(`Plugin ${identifier} ${role} realm has been destroyed`);
      return capabilityGlobal;
    },
    destroy() {
      if (disposed) return;
      disposed = true;
      dispose();
    },
  };
  return host;
}

/**
 * Create one hidden WebKit realm for one plugin role. Entry and global scripts
 * intentionally receive different frames, globals, intrinsics, and CommonJS
 * caches, matching IINA's separate JavaScriptCore contexts while retaining
 * direct capability calls and synchronous callback return values.
 */
export async function createWebKitPluginRealm({
  identifier,
  role,
  documentObject = globalThis.document,
}) {
  if (!documentObject?.createElement) {
    throw new Error("Plugin WebKit realms require a document");
  }
  const frame = documentObject.createElement("iframe");
  frame.className = "plugin-runtime-realm";
  frame.title = `${identifier} ${role} plugin runtime`;
  frame.hidden = true;
  frame.tabIndex = -1;
  frame.setAttribute("aria-hidden", "true");
  frame.setAttribute("sandbox", "allow-scripts allow-same-origin");
  frame.referrerPolicy = "no-referrer";

  const loaded = new Promise((resolve) => {
    frame.addEventListener("load", resolve, { once: true });
  });
  frame.srcdoc = PLUGIN_REALM_DOCUMENT;
  const parent = documentObject.body || documentObject.documentElement;
  if (!parent) throw new Error("Plugin WebKit realm has no document host");
  parent.append(frame);
  await loaded;

  const realmWindow = frame.contentWindow;
  if (!realmWindow) {
    frame.remove();
    throw new Error("Plugin WebKit realm failed to initialize");
  }
  return createPluginRealmHost({
    identifier,
    role,
    realmWindow,
    dispose: () => {
      try {
        frame.srcdoc = "";
      } catch {
        // Removing the browsing context is the authoritative teardown.
      }
      frame.remove();
    },
  });
}
