import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import {
  IINA_DEFAULT_PLUGIN_REPOSITORIES,
  defaultPluginRepositoryRows,
  pluginReorderFinalIndex,
  retainedPluginPreferenceSelection,
} from "../src/plugin-preferences.js";

assert.deepEqual(IINA_DEFAULT_PLUGIN_REPOSITORIES, [
  { repository: "iina/plugin-demo", identifier: "io.iina.demo" },
  { repository: "iina/plugin-online-media", identifier: "io.iina.ytdl" },
  { repository: "iina/plugin-userscript", identifier: "io.iina.userscript" },
]);
assert.deepEqual(
  defaultPluginRepositoryRows([{ identifier: "io.iina.ytdl" }]).map((plugin) => plugin.installed),
  [false, true, false],
);
assert.equal(
  retainedPluginPreferenceSelection("io.iina.ytdl", [{ identifier: "io.iina.ytdl" }]),
  "io.iina.ytdl",
);
assert.equal(retainedPluginPreferenceSelection("missing", [{ identifier: "io.iina.ytdl" }]), null);
assert.equal(pluginReorderFinalIndex(0, 3, 3), 2);
assert.equal(pluginReorderFinalIndex(2, 0, 3), 0);
assert.equal(pluginReorderFinalIndex(1, 2, 3), 1);
assert.equal(pluginReorderFinalIndex(-1, 1, 3), null);

const root = join(import.meta.dirname, "..");
const [
  main,
  html,
  css,
  referenceController,
  referenceXib,
  referenceWindowController,
  commandSource,
  auxiliaryWindowSource,
  pluginSource,
  libSource,
] = await Promise.all([
  readFile(join(root, "src", "main.js"), "utf8"),
  readFile(join(root, "src", "index.html"), "utf8"),
  readFile(join(root, "src", "styles.css"), "utf8"),
  readFile(join(root, "参考", "iina", "iina", "PrefPluginViewController.swift"), "utf8"),
  readFile(join(root, "参考", "iina", "iina", "PrefPluginViewController.xib"), "utf8"),
  readFile(join(root, "参考", "iina", "iina", "PreferenceWindowController.swift"), "utf8"),
  readFile(join(root, "src-tauri", "src", "commands.rs"), "utf8"),
  readFile(join(root, "src-tauri", "src", "auxiliary_player_windows.rs"), "utf8"),
  readFile(join(root, "src-tauri", "src", "plugins.rs"), "utf8"),
  readFile(join(root, "src-tauri", "src", "lib.rs"), "utf8"),
]);

for (const repository of IINA_DEFAULT_PLUGIN_REPOSITORIES) {
  assert.match(referenceController, new RegExp(repository.repository.replace("/", "\\/")));
  assert.match(referenceController, new RegExp(repository.identifier.replaceAll(".", "\\.")));
}
assert.match(referenceXib, /width="480" height="545"/);
assert.match(referenceXib, /width="160" height="400"/);
assert.match(referenceXib, /rowHeight="36"/);
assert.match(referenceXib, /width="195" height="21"/);
assert.match(referenceXib, /<segment label="Permissions" selected="YES"\/>/);
assert.match(referenceXib, /<segment label="About" tag="1"\/>/);
assert.match(referenceXib, /<segment label="Preferences"\/>/);
assert.match(referenceXib, /width="480" height="297"/);
assert.match(referenceXib, /width="438" height="101"/);
assert.match(referenceXib, /title="Show in Finder"/);
assert.match(referenceController, /var preferenceContentIsScrollable: Bool\s*\{\s*return false\s*\}/s);
assert.match(referenceController, /registerForDraggedTypes\(\[\.iinaPluginID\]\)/);
assert.match(referenceController, /JavascriptPlugin\.savePluginOrder\(\)/);
assert.match(referenceController, /func showPlugin\(_ sender: Any\)/);
assert.match(referenceWindowController, /let isScrollable = vc\.preferenceContentIsScrollable/);

assert.match(html, /id="plugin-github-default-list"[^>]+role="listbox"/);
assert.match(html, /id="plugin-github-spinner"[^>]+hidden/);
assert.match(html, /Please enter the full URL of the GitHub repository/);
assert.doesNotMatch(html, /id="plugin-page-modal"|id="plugin-page-content"/);

assert.match(css, /\.plugin-manager\s*\{[^}]*width: 480px;[^}]*height: 100%;[^}]*max-height: 545px;[^}]*min-height: 0;/s);
assert.match(css, /\.plugin-manager-workspace\s*\{[^}]*bottom: 8px;[^}]*grid-template-columns: 160px 312px;[^}]*max-height: 400px;[^}]*min-height: 0;/s);
assert.match(css, /\.plugin-manager-list\s*\{[^}]*width: 160px;[^}]*height: 100%;[^}]*max-height: 400px;[^}]*min-height: 0;/s);
assert.match(css, /\.plugin-manager-row\s*\{[^}]*height: 36px;/s);
assert.match(css, /\.plugin-manager-row\.is-drop-before\s*\{[^}]*box-shadow:/s);
assert.match(css, /\.plugin-manager-row\.is-drop-after\s*\{[^}]*box-shadow:/s);
assert.match(css, /\.plugin-manager-segments\s*\{[^}]*width: 195px;[^}]*height: 21px;/s);
assert.match(css, /\.preferences-content:has\(\.pref-pane\[data-pane="plugins"\]\)\s*\{[^}]*min-height: 0;[^}]*overflow: hidden;/s);
assert.match(css, /\.pref-pane\[data-pane="plugins"\]\s*\{[^}]*height: min\(545px, 100%\);[^}]*min-height: 0;/s);
assert.match(css, /\.plugin-github-window\s*\{[^}]*width: min\(480px,[^}]*height: min\(297px,/s);
assert.match(css, /\.plugin-github-default-list\s*\{[^}]*width: 438px;[^}]*height: 101px;/s);
assert.doesNotMatch(css, /\.plugin-page-modal\s*\{|\.plugin-page-window\s*\{/);

for (const contract of [
  'let activePluginPreferenceTab = "permissions";',
  'activePluginPreferenceTab = "permissions";',
  'frame.src = "about:blank";',
  "frame.removeAttribute(\"srcdoc\")",
  'get_plugin_page_contents", { identifier: plugin.identifier }',
  "renderPluginPermissionsPage(content, plugin)",
  "renderPluginAboutPage(content, plugin, contents, refresh)",
  "renderPluginPreferencesPage(content, plugin, contents?.preference_html)",
  '["permissions", "Permissions"]',
  '["about", "About"]',
  '["preferences", "Preferences"]',
  "renderPluginGithubDefaultRepositories(await invoke(\"get_plugins\"))",
  "setPluginGithubBusy(true)",
  "async function refreshPlayerPluginRuntimes()",
  "applyPluginPreferenceWindowContext(resolvedContext.selectedPluginIdentifier)",
  "selectedPluginIdentifier = null,",
  "async function requestPendingPluginInstallDrain",
  "if (!isPreferencesAuxiliaryWindow) return;",
  'invoke("has_pending_plugin_installs")',
  "showPreferencesPanel({ drainPendingPluginInstalls: true })",
  "if (resolvedContext?.drainPendingPluginInstalls)",
  "applyPluginPreferenceWindowContext(record.identifier)",
  'row.draggable = plugins.length > 1;',
  'event.dataTransfer.setData("application/x-iina-plugin-id", plugin.identifier)',
  "pluginReorderFinalIndex(",
  'invoke("reorder_plugin", { identifier, destinationIndex })',
  'invoke("reveal_plugin_in_finder", { identifier: plugin.identifier })',
  'reveal.textContent = trKey("Localizable", "pl_menu.show_in_finder", "Show in Finder")',
]) {
  assert.ok(main.includes(contract), `Missing embedded plugin Preferences contract: ${contract}`);
}
assert.doesNotMatch(
  main,
  /showPluginPagePanel|closePluginPagePanel|pluginPageModal|pluginPageContent|pluginPageCloseButton/,
);

const pluginManagerSource = main.slice(
  main.indexOf("function renderPluginManager(control)"),
  main.indexOf("function currentKeyBindingProfileName()"),
);
assert.doesNotMatch(pluginManagerSource, /await refreshPluginRuntimes\(\);/);
assert.ok(
  (pluginManagerSource.match(/await refreshPlayerPluginRuntimes\(\);/g) || []).length >= 4,
  "Plugin install, enable, update, and removal must refresh player hosts",
);
const systemToggleSource = pluginManagerSource.slice(
  pluginManagerSource.indexOf('systemToggleInput.addEventListener("change"'),
  pluginManagerSource.indexOf('install.addEventListener("click"'),
);
assert.doesNotMatch(systemToggleSource, /refreshPlayerPluginRuntimes\(\)/);
for (const eventName of ["dragenter", "dragover", "drop", "dragend"]) {
  const eventSource = pluginManagerSource
    .split(`row.addEventListener("${eventName}"`)[1]
    .split("});")[0];
  assert.match(
    eventSource,
    /event\.stopPropagation\(\)/,
    `Plugin ${eventName} must not reach the document media-drop handlers`,
  );
}
assert.match(
  pluginSource,
  /fn reorders_installed_plugins_persists_boundaries_and_rejects_invalid_moves_without_mutation\(\)/,
);

const claimSource = commandSource
  .split("pub fn claim_pending_plugin_install(")[1]
  .split("#[tauri::command]")[0];
assert.match(claimSource, /window\.label\(\) != crate::auxiliary_player_windows::PREFERENCES_WINDOW_LABEL/);
const inspectSource = commandSource
  .split("pub fn has_pending_plugin_installs(")[1]
  .split("#[tauri::command]")[0];
assert.match(inspectSource, /window\.label\(\) != "main"/);
assert.match(auxiliaryWindowSource, /drain_pending_plugin_installs: bool/);
assert.match(auxiliaryWindowSource, /context\.drain_pending_plugin_installs = drain_pending_plugin_installs/);

const reorderCommand = commandSource
  .split("pub fn reorder_plugin(")[1]
  .split("#[tauri::command]")[0];
assert.match(reorderCommand, /identifier: String/);
assert.match(reorderCommand, /destination_index: usize/);
assert.doesNotMatch(reorderCommand, /path: String/);
const revealCommand = commandSource
  .split("pub fn reveal_plugin_in_finder(")[1]
  .split("#[tauri::command]")[0];
assert.match(revealCommand, /plugins::installed_root\(&app, &identifier\)\?/);
assert.match(revealCommand, /native_file::reveal\(&\[root\]\)/);
assert.doesNotMatch(revealCommand, /path: String/);
const reorderBackend = pluginSource
  .split("fn reorder_in_root(")[1]
  .split("pub fn installed_root(")[0];
assert.match(reorderBackend, /plugin_root_for_identifier\(root, identifier\)\?/);
assert.match(reorderBackend, /plugin_filesystem_transaction_lock\(\)\?/);
assert.match(reorderBackend, /state\.order = order/);
const installedRootBackend = pluginSource
  .split("pub fn installed_root(")[1]
  .split("pub fn remove(")[0];
assert.match(installedRootBackend, /plugin_root_for_identifier\(&root, identifier\)\?\.root/);
for (const command of ["reorder_plugin", "reveal_plugin_in_finder"]) {
  assert.ok(libSource.match(new RegExp(`\\b${command}\\b`, "g")).length >= 2);
}

console.log("Plugin Preferences master-detail, embedded pages, and GitHub sheet contracts pass");
