import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import vm from "node:vm";
import { buildSafariExtensionBundle } from "./build-safari-extension.mjs";
import { readReferencePackageIdentity } from "./package-identity.mjs";

const root = join(import.meta.dirname, "..");
const browserRoot = join(root, "browser");
const referencePackageIdentity = readReferencePackageIdentity(root);

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function readPlist(path) {
  return JSON.parse(
    execFileSync("plutil", ["-convert", "json", "-o", "-", path], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    }),
  );
}

function makeAnchorDocument() {
  const anchors = [];
  return {
    anchors,
    document: {
      createElement(tagName) {
        assert.equal(tagName, "a");
        const anchor = {
          clicked: false,
          href: "",
          click() {
            this.clicked = true;
          },
        };
        anchors.push(anchor);
        return anchor;
      },
      body: {
        appendChild(anchor) {
          assert.ok(anchors.includes(anchor));
        },
      },
    },
  };
}

function executeMv3Injection(injection) {
  assert.equal(typeof injection.func, "function");
  assert.ok(Array.isArray(injection.args));
  const capture = makeAnchorDocument();
  const previousDocument = globalThis.document;
  globalThis.document = capture.document;
  try {
    injection.func(...injection.args);
  } finally {
    if (previousDocument === undefined) delete globalThis.document;
    else globalThis.document = previousDocument;
  }
  assert.equal(capture.anchors.length, 1);
  assert.equal(capture.anchors[0].clicked, true);
  return capture.anchors[0].href;
}

function executeMv2Injection(code) {
  const capture = makeAnchorDocument();
  vm.runInNewContext(code, { document: capture.document });
  assert.equal(capture.anchors.length, 1);
  assert.equal(capture.anchors[0].clicked, true);
  return capture.anchors[0].href;
}

function expectedOpenUrl(url, suffix = "") {
  const encoded = encodeURIComponent(url).replace(/'/g, "%27");
  return `iina://open?url=${encoded}${suffix}`;
}

function verifyWebExtension(directoryName, expectedManifestVersion) {
  const directory = join(browserRoot, directoryName);
  const manifest = readJson(join(directory, "manifest.json"));
  assert.equal(manifest.manifest_version, expectedManifestVersion);
  assert.equal(manifest.name, "Open In IINA");
  assert.ok(manifest.permissions.includes("tabs"));
  assert.ok(manifest.permissions.includes("activeTab"));
  assert.ok(manifest.permissions.includes("contextMenus"));
  assert.ok(manifest.permissions.includes("storage"));

  for (const asset of [
    "background.js",
    "common.js",
    "icon.png",
    "icon16.png",
    "icon48.png",
    "icon128.png",
    "options.html",
    "options.js",
    "popup.html",
    "popup.js",
  ]) {
    assert.ok(existsSync(join(directory, asset)), `${directoryName} is missing ${asset}`);
  }

  if (expectedManifestVersion === 3) {
    assert.equal(manifest.background.service_worker, "background.js");
    assert.equal(manifest.background.type, "module");
    assert.ok(manifest.permissions.includes("scripting"));
    assert.equal(manifest.action.default_title, "Open In IINA");
  } else {
    assert.equal(manifest.background.page, "background.html");
    assert.equal(manifest.applications.gecko.id, "open_in_iina_firefox@iina.io");
    assert.equal(manifest.browser_action.default_title, "Open In IINA");
  }

  const common = readFileSync(join(directory, "common.js"), "utf8");
  assert.match(common, /iina:\/\/open\?/);
  assert.match(common, /params\.push\("new_window=1"\)/);
  assert.match(common, /encodeURIComponent\(url\)\.replace\(\/'\/g, '%27'\)/);

  for (const file of readdirSync(directory).filter((entry) => entry.endsWith(".js"))) {
    execFileSync(process.execPath, ["--check", join(directory, file)], { stdio: "pipe" });
  }
  return { directory, manifest };
}

const chromeExtension = verifyWebExtension("Chrome_Open_In_IINA", 3);
const firefoxExtension = verifyWebExtension("Firefox_Open_In_IINA", 2);
assert.deepEqual(
  chromeExtension.manifest,
  readJson(join(root, "参考", "iina", "browser", "Chrome_Open_In_IINA", "manifest.json")),
  "Chrome manifest differs from IINA 1.3.5",
);
assert.deepEqual(
  firefoxExtension.manifest,
  readJson(join(root, "参考", "iina", "browser", "Firefox_Open_In_IINA", "manifest.json")),
  "Firefox manifest differs from IINA 1.3.5",
);

const previousChrome = globalThis.chrome;
try {
  let mv3Injection;
  globalThis.chrome = {
    scripting: {
      executeScript(injection) {
        mv3Injection = injection;
      },
    },
  };
  const commonModule = await import(
    `${pathToFileURL(join(chromeExtension.directory, "common.js")).href}?route-contract`
  );
  const testUrl = "https://example.com/watch?v=a b&quote='&unicode=影片";
  for (const [options, suffix] of [
    [{}, ""],
    [{ mode: "fullScreen" }, "&full_screen=1"],
    [{ mode: "pip" }, "&pip=1"],
    [{ mode: "enqueue" }, "&enqueue=1"],
    [{ mode: "newWindow", newWindow: true }, "&new_window=1"],
  ]) {
    mv3Injection = undefined;
    commonModule.openInIINA(37, testUrl, options);
    assert.equal(mv3Injection.target.tabId, 37);
    assert.equal(executeMv3Injection(mv3Injection), expectedOpenUrl(testUrl, suffix));
  }

  let mv2Injection;
  globalThis.chrome = {
    tabs: {
      executeScript(tabId, injection) {
        mv2Injection = { tabId, ...injection };
      },
    },
  };
  commonModule.openInIINA(41, testUrl, { mode: "newWindow", newWindow: true });
  assert.equal(mv2Injection.tabId, 41);
  assert.equal(
    executeMv2Injection(mv2Injection.code),
    expectedOpenUrl(testUrl, "&new_window=1"),
  );

  const contextItems = [];
  const contextInjections = [];
  let contextClick;
  let toolbarClick;
  let storedOptions = { iconAction: "clickOnly", iconActionOption: "direct" };
  const activeTab = { id: 23, url: "https://example.com/current" };
  const browserAction = {
    onClicked: {
      addListener(listener) {
        toolbarClick = listener;
      },
    },
    setPopup() {},
  };
  globalThis.chrome = {
    action: browserAction,
    browserAction,
    contextMenus: {
      create(item) {
        contextItems.push(item);
      },
      onClicked: {
        addListener(listener) {
          contextClick = listener;
        },
      },
    },
    scripting: {
      executeScript(injection) {
        contextInjections.push(injection);
      },
    },
    storage: {
      sync: {
        get(defaults, callback) {
          callback({ ...defaults, ...storedOptions });
        },
      },
    },
    tabs: {
      TAB_ID_NONE: -1,
      query(_query, callback) {
        callback([activeTab]);
      },
    },
  };
  await import(`${pathToFileURL(join(chromeExtension.directory, "background.js")).href}?menus`);
  assert.deepEqual(
    contextItems.map(({ id, contexts }) => [id, contexts]),
    [
      ["openiniina_page", ["page"]],
      ["openiniina_link", ["link"]],
      ["openiniina_video", ["video"]],
      ["openiniina_audio", ["audio"]],
    ],
  );

  for (const [kind, property] of [
    ["page", "pageUrl"],
    ["link", "linkUrl"],
    ["video", "srcUrl"],
    ["audio", "srcUrl"],
  ]) {
    const url = `https://example.com/${kind}`;
    contextClick({ menuItemId: `openiniina_${kind}`, [property]: url }, activeTab);
    assert.equal(executeMv3Injection(contextInjections.pop()), expectedOpenUrl(url));
  }

  storedOptions = { iconAction: "clickOnly", iconActionOption: "pip" };
  toolbarClick();
  assert.equal(
    executeMv3Injection(contextInjections.pop()),
    expectedOpenUrl(activeTab.url, "&pip=1"),
  );

  const popupItems = ["normal", "fullScreen", "pip", "newWindow", "enqueue"].map((mode) => ({
    id: `open-${mode}`,
    listeners: {},
    addEventListener(type, listener) {
      this.listeners[type] = listener;
    },
  }));
  const previousDocument = globalThis.document;
  globalThis.document = {
    getElementsByClassName(name) {
      assert.equal(name, "menu-item");
      return popupItems;
    },
  };
  try {
    await import(`${pathToFileURL(join(chromeExtension.directory, "popup.js")).href}?popup`);
    const previousLog = console.log;
    console.log = () => {};
    try {
      popupItems.find(({ id }) => id === "open-newWindow").listeners.click();
    } finally {
      console.log = previousLog;
    }
  } finally {
    if (previousDocument === undefined) delete globalThis.document;
    else globalThis.document = previousDocument;
  }
  assert.equal(
    executeMv3Injection(contextInjections.pop()),
    expectedOpenUrl(activeTab.url, "&new_window=1"),
  );
} finally {
  if (previousChrome === undefined) delete globalThis.chrome;
  else globalThis.chrome = previousChrome;
}

const safariDirectory = join(browserRoot, "Safari_Open_In_IINA");
const safariReference = join(root, "参考", "iina", "OpenInIINA");
for (const file of [
  "SafariExtensionHandler.swift",
  "open-in-iina.js",
  "ToolbarItemIcon.pdf",
  "OpenInIINA.entitlements",
  "Info.plist",
]) {
  assert.ok(existsSync(join(safariDirectory, file)), `Safari extension is missing ${file}`);
}
for (const file of ["open-in-iina.js", "ToolbarItemIcon.pdf", "OpenInIINA.entitlements"]) {
  assert.deepEqual(
    readFileSync(join(safariDirectory, file)),
    readFileSync(join(safariReference, file)),
    `Safari ${file} differs from IINA 1.3.5`,
  );
}

const safariHandler = readFileSync(join(safariDirectory, "SafariExtensionHandler.swift"), "utf8");
const referenceSafariHandler = readFileSync(
  join(safariReference, "SafariExtensionHandler.swift"),
  "utf8",
);
assert.equal(
  safariHandler.slice(safariHandler.indexOf("import SafariServices")),
  referenceSafariHandler.slice(referenceSafariHandler.indexOf("import SafariServices")),
  "Safari handler implementation differs from IINA 1.3.5",
);
assert.match(safariHandler, /iina:\/\/weblink\?url=/);
assert.doesNotMatch(safariHandler, /iina:\/\/open\?/);
assert.match(safariHandler, /case "OpenInIINA":/);
assert.match(safariHandler, /case "OpenLinkInIINA":/);
assert.match(safariHandler, /validationHandler\(userInfo\?\["url"\] as\? String == nil, nil\)/);

const safariScript = readFileSync(join(safariDirectory, "open-in-iina.js"), "utf8");
const safariListeners = {};
let safariUserInfo;
const safariContext = {
  Node: { ELEMENT_NODE: 1 },
  document: {
    addEventListener(type, listener) {
      safariListeners[type] = listener;
    },
  },
  safari: {
    extension: {
      setContextMenuEventUserInfo(_event, userInfo) {
        safariUserInfo = userInfo;
      },
    },
  },
};
vm.runInNewContext(safariScript, safariContext);
const link = {
  href: "https://example.com/linked-video",
  nodeName: "A",
  nodeType: 1,
  parentNode: null,
};
safariListeners.contextmenu({
  target: { nodeName: "SPAN", nodeType: 1, parentNode: link },
});
assert.equal(safariUserInfo.url, link.href);
assert.deepEqual(Object.keys(safariUserInfo), ["url"]);

const safariInfo = readPlist(join(safariDirectory, "Info.plist"));
const referenceSafariInfo = readPlist(join(safariReference, "Info.plist"));
const resolvedReferenceExtension = structuredClone(referenceSafariInfo.NSExtension);
resolvedReferenceExtension.NSExtensionPrincipalClass = "OpenInIINA.SafariExtensionHandler";
assert.equal(safariInfo.CFBundlePackageType, "XPC!");
assert.equal(safariInfo.LSMinimumSystemVersion, "10.13");
assert.equal(safariInfo.CFBundleShortVersionString, "0.9.4");
assert.equal(safariInfo.CFBundleVersion, "94");
assert.deepEqual(safariInfo.NSExtension, resolvedReferenceExtension);
assert.deepEqual(
  safariInfo.SFSafariExtensionBundleIdentifiersToUninstall,
  referenceSafariInfo.SFSafariExtensionBundleIdentifiersToUninstall,
);
assert.equal(safariInfo.NSExtension.NSExtensionPointIdentifier, "com.apple.Safari.extension");
assert.equal(
  safariInfo.NSExtension.NSExtensionPrincipalClass,
  "OpenInIINA.SafariExtensionHandler",
);
assert.deepEqual(
  safariInfo.NSExtension.SFSafariContextMenu.map(({ Command, Text }) => [Command, Text]),
  [
    ["OpenInIINA", "Open Current Page in IINA"],
    ["OpenLinkInIINA", "Open Link in IINA"],
  ],
);
assert.equal(safariInfo.NSExtension.SFSafariWebsiteAccess.Level, "All");

const tauriConfig = readJson(join(root, "src-tauri", "tauri.conf.json"));
const hostInfo = readPlist(join(root, "src-tauri", "IINA-Info.plist"));
assert.equal(tauriConfig.bundle.macOS.infoPlist, "IINA-Info.plist");
assert.ok(
  hostInfo.CFBundleURLTypes.some(({ CFBundleURLSchemes }) =>
    CFBundleURLSchemes?.includes("iina"),
  ),
  "The Tauri host bundle does not register the iina URL scheme",
);

const temporaryRoot = mkdtempSync(join(tmpdir(), "iima-safari-extension-"));
try {
  const extensionPath = join(temporaryRoot, "IINA.app", "Contents", "PlugIns", "OpenInIINA.appex");
  const bundleIdentifier = `${tauriConfig.identifier}.OpenInIINA`;
  const built = await buildSafariExtensionBundle({
    sourceDirectory: safariDirectory,
    extensionPath,
    bundleIdentifier,
    shortVersion: tauriConfig.version,
    bundleVersion: referencePackageIdentity.buildVersion,
    workDirectory: join(temporaryRoot, "work"),
  });
  for (const path of [
    built.binaryPath,
    built.infoPath,
    join(extensionPath, "Contents", "Resources", "open-in-iina.js"),
    join(extensionPath, "Contents", "Resources", "ToolbarItemIcon.pdf"),
  ]) {
    assert.ok(existsSync(path), `Built Safari extension is missing ${path}`);
  }

  const architectures = new Set(
    execFileSync("lipo", ["-archs", built.binaryPath], { encoding: "utf8" })
      .trim()
      .split(/\s+/),
  );
  assert.deepEqual(architectures, new Set(["arm64", "x86_64"]));
  assert.match(
    execFileSync("otool", ["-L", built.binaryPath], { encoding: "utf8" }),
    /SafariServices\.framework/,
  );
  const builtInfo = readPlist(built.infoPath);
  assert.equal(builtInfo.CFBundleIdentifier, bundleIdentifier);
  assert.equal(builtInfo.CFBundleShortVersionString, tauriConfig.version);
  assert.equal(builtInfo.CFBundleVersion, referencePackageIdentity.buildVersion);
  assert.deepEqual(builtInfo.NSExtension, safariInfo.NSExtension);

  execFileSync(
    "codesign",
    [
      "--force",
      "--sign",
      "-",
      "--entitlements",
      join(safariDirectory, "OpenInIINA.entitlements"),
      extensionPath,
    ],
    { stdio: "pipe" },
  );
  execFileSync("codesign", ["--verify", "--strict", "--verbose=2", extensionPath], {
    stdio: "pipe",
  });
  const signature = spawnSync("codesign", ["-dvv", "--entitlements", ":-", extensionPath], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  assert.equal(signature.status, 0);
  const signatureDescription = `${signature.stdout}\n${signature.stderr}`;
  assert.match(signatureDescription, /Signature=adhoc/);
  assert.match(signatureDescription, /com\.apple\.security\.app-sandbox/);

  const failedExtensionPath = join(temporaryRoot, "failed", "OpenInIINA.appex");
  await assert.rejects(
    buildSafariExtensionBundle({
      sourceDirectory: safariDirectory,
      extensionPath: failedExtensionPath,
      bundleIdentifier,
      shortVersion: tauriConfig.version,
      bundleVersion: referencePackageIdentity.buildVersion,
      workDirectory: join(temporaryRoot, "failed-work"),
      architectures: ["../invalid"],
    }),
    /Invalid Safari extension architecture/,
  );
  assert.equal(existsSync(failedExtensionPath), false, "Failed build left a partial .appex");
} finally {
  rmSync(temporaryRoot, { recursive: true, force: true });
}

const builderSource = readFileSync(join(root, "scripts", "build-safari-extension.mjs"), "utf8");
for (const contract of [
  "swiftc",
  "-application-extension",
  "_NSExtensionMain",
  "SafariServices",
  "lipo",
]) {
  assert.ok(builderSource.includes(contract), `Safari builder is missing ${contract}`);
}
assert.doesNotMatch(builderSource, /spawnSync\(["'](?:curl|npm|xcodebuild)["']/);

const packageSource = readFileSync(join(root, "scripts", "package-macos.mjs"), "utf8");
for (const contract of [
  "buildSafariExtensionBundle",
  "verifySafariExtension",
  "OpenInIINA.appex",
  "OpenInIINA.entitlements",
  "appPluginsDir",
  "CFBundleURLTypes.0.CFBundleURLSchemes.0",
  "SafariServices.framework",
]) {
  assert.ok(packageSource.includes(contract), `Packaging is missing Safari contract ${contract}`);
}
const buildCallIndex = packageSource.lastIndexOf("await buildSafariExtension();");
const extensionSignIndex = packageSource.lastIndexOf("safariExtensionEntitlements,");
const extensionVerifyIndex = packageSource.lastIndexOf("verifySafariExtension();");
assert.ok(buildCallIndex >= 0 && extensionSignIndex > buildCallIndex);
assert.ok(extensionVerifyIndex > extensionSignIndex);

console.log(
  "Chrome/Firefox URL, menu, new-window, Safari source, universal .appex, and ad-hoc entitlement contracts pass",
);
