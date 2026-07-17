import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const read = (path) => readFile(join(root, path), "utf8");
const [
  native,
  backend,
  commands,
  library,
  buildScript,
  plist,
  reference,
  referencePlayer,
  referenceExtensions,
  referenceMenu,
] =
  await Promise.all([
    read("src-tauri/src/native_application.m"),
    read("src-tauri/src/native_application.rs"),
    read("src-tauri/src/commands.rs"),
    read("src-tauri/src/lib.rs"),
    read("src-tauri/build.rs"),
    read("src-tauri/IINA-Info.plist"),
    read("参考/iina/iina/AppDelegate.swift"),
    read("参考/iina/iina/PlayerCore.swift"),
    read("参考/iina/iina/Extensions.swift"),
    read("参考/iina/iina/Base.lproj/MainMenu.xib"),
  ]);

for (const contract of [
  "NSApplication.shared.servicesProvider = self",
  "func droppedText(_ pboard: NSPasteboard",
  "PlayerCore.active.openURLString(url)",
  "func applicationDockMenu(_ sender: NSApplication) -> NSMenu?",
]) assert.ok(reference.includes(contract), contract);
assert.match(referencePlayer, /static var active: PlayerCore[\s\S]*?NSApp\.mainWindow\?\.windowController as\? PlayerWindowController[\s\S]*?return first/);
assert.match(referencePlayer, /func openURLString\(_ str: String\)[\s\S]*?str == "-"[\s\S]*?str\.first == "\/"[\s\S]*?URL\(fileURLWithPath: str\)/);
assert.match(referenceExtensions, /static let urlAllowed:[\s\S]*?urlHostAllowed[\s\S]*?urlUserAllowed[\s\S]*?urlPasswordAllowed[\s\S]*?urlPathAllowed[\s\S]*?urlQueryAllowed[\s\S]*?urlFragmentAllowed[\s\S]*?"%"/);
assert.match(referenceMenu, /<menuItem title="Open\.\.\."[\s\S]*?<action selector="openFile:"/);

assert.match(native, /NSApp\.servicesProvider = self/);
assert.match(native, /stringForType:NSPasteboardTypeString/);
assert.match(native, /\(__bridge void \*\)NSApp\.mainWindow/);
assert.match(native, /IIMANormalizedOpenURLString/);
assert.match(native, /@available\(macOS 14\.0, \*\)/);
for (const allowedSet of [
  "URLHostAllowedCharacterSet",
  "URLUserAllowedCharacterSet",
  "URLPasswordAllowedCharacterSet",
  "URLPathAllowedCharacterSet",
  "URLQueryAllowedCharacterSet",
  "URLFragmentAllowedCharacterSet",
]) assert.ok(native.includes(allowedSet), allowedSet);
assert.match(native, /addCharactersInString:@"%"/);
assert.match(native, /url\.isFileURL \? url\.path : url\.absoluteString/);
assert.match(native, /@selector\(applicationDockMenu:\)/);
assert.match(native, /initWithTitle:title\s+action:@selector\(openFile:\)/);
assert.equal((native.match(/\[menu addItem:/g) || []).length, 1, "Dock menu must contain only Open...");
assert.match(native, /object_setClass\(delegate, subclass\)/);
assert.match(native, /object_setClass\(delegate, self\.originalDelegateClass\)/);
assert.doesNotMatch(native, /NSApp\.delegate\s*=/);
assert.doesNotMatch(native, /setDelegate:/);

assert.match(backend, /menu::handle_iina_menu_event\(&app, "iina\.open"\)/);
assert.match(backend, /commands::open_service_url_in_active_player/);
assert.match(backend, /state\.note_external_open_request\(\);[\s\S]*?normalize_open_url_string/);
assert.match(backend, /CStr::from_ptr\(url\)[\s\S]*?\.to_str\(\)/);
assert.match(backend, /localization::menu_title\("Open\.\.\."\)/);
const serviceRoute = commands.match(
  /fn service_open_url_route[\s\S]*?\n\}\n\nfn service_player_controller_session_label/,
)?.[0] ?? "";
assert.match(serviceRoute, /active_player_session_label\.unwrap_or\("main"\)/);
assert.match(serviceRoute, /if url\.is_empty\(\)[\s\S]*?return Ok\(None\)/);
const activeResolver = commands.match(
  /fn service_active_player_session_label[\s\S]*?\n\}\n\n#\[derive/,
)?.[0] ?? "";
assert.match(activeResolver, /window\.ns_window\(\)/);
assert.match(activeResolver, /native_window != native_main_window/);
assert.match(activeResolver, /service_player_controller_session_label/);
assert.match(commands, /window_label\.starts_with\("mini-player"\)[\s\S]*?player_session_label_for_window\(window_label\)/);
assert.doesNotMatch(serviceRoute, /last_active_player_session_label\(\)/);
assert.doesNotMatch(activeResolver, /last_active_player_session_label\(\)/);
assert.match(commands, /pub\(crate\) fn open_service_url_in_active_player[\s\S]*?Result<Option<PlayerState>, String>[\s\S]*?open_media_paths_in_window/);
assert.match(commands, /macos_service_url_uses_main_window_player_or_first_without_new_window_routing/);
assert.match(commands, /let plugin_disables_ui = is_plugin_disable_ui_player_window\(window\)[\s\S]*?restored_initial_frame = if plugin_disables_ui/);

assert.match(plist, /<key>NSMessage<\/key>\s*<string>droppedText<\/string>/);
assert.match(plist, /<key>NSSendTypes<\/key>\s*<array>\s*<string>NSStringPboardType<\/string>/);
assert.match(buildScript, /\.file\("src\/native_application\.m"\)/);
assert.match(library, /mod native_application;/);
assert.match(library, /native_application::install\(app\.handle\(\)\)/);
assert.match(library, /fn cleanup_before_exit[\s\S]*?native_application::shutdown\(\)/);
assert.match(
  library,
  /RunEvent::ExitRequested \{ \.\. \} \| tauri::RunEvent::Exit[\s\S]*?cleanup_before_exit\(app\)/,
);
assert.match(library, /EXIT_CLEANUP_STARTED\.swap\(true, Ordering::AcqRel\)/);

if (process.platform === "darwin") {
  const syntax = spawnSync(
    "xcrun",
    [
      "--sdk", "macosx", "clang", "-fsyntax-only", "-x", "objective-c",
      "-fobjc-arc", "-fblocks", "-Wall", "-Wextra", "-Werror",
      "-Wno-deprecated-declarations", "src-tauri/src/native_application.m",
    ],
    { cwd: root, encoding: "utf8" },
  );
  assert.equal(
    syntax.status,
    0,
    `Objective-C -Werror syntax check failed:\n${syntax.stdout}${syntax.stderr}`,
  );
}

console.log("Native Dock menu and NSServices bridge contract checks passed");
