# Plugin synchronous bridge

IINA 1.3.5 exposes several JavaScriptCore APIs as immediate values or `void`, not Promises. The
Tauri port preserves that shape with a parent-WebView synchronous transport while keeping the
plugin entry/global realms unable to access Tauri IPC, `XMLHttpRequest`, the player DOM, or the
transport grant.

## Transport and ownership

- `src/plugin-sync.js` obtains one opaque 256-bit grant for an enabled plugin and performs a
  synchronous `POST` to `iima-plugin-sync://localhost/invoke`.
- `src-tauri/src/plugin_sync.rs` accepts only the packaged `tauri://localhost` origin (plus the
  exact development origin in debug builds), implements the narrow CORS `OPTIONS` policy, and
  validates the direct `POST` origin, request shape, JSON depth/size, method allowlist, owner
  WebView, enabled plugin, and idle lifetime.
- Grants are replaced when the same plugin runtime reloads and are revoked on explicit unload,
  plugin disable/removal, owner-window teardown, or application exit. Every file-handle token is
  attached to its exact grant and is closed after the grant registry lock is released.
- The grant and `XMLHttpRequest` constructor remain closure-private in the player realm. Only
  capability functions capture `invokeSync`; plugin code receives neither the descriptor nor the
  transport object.

The synchronous surface currently covers Core history/window snapshots and frame writes, typed
mpv get/set/command, file text/list/mutation and binary handles, the synchronous Utils methods,
`standaloneWindow.isOpen`, and WebSocket `createServer`/`startServer`. Reference Promise methods
such as HTTP, Utils `exec`/`chooseFile`, WebSocket `sendText`, and subtitle-provider work remain
Promises.

## File compatibility details

- The shared path preflight follows `JavascriptAPI.parsePath` ordering. `@tmp/` and `@data/`
  bypass `file-system` in both realms; `@video/`, `@audio/`, and IINA's broad `@sub` prefix do so
  only in a player-entry realm. Every other File API path throws the exact synchronous Info.json
  permission exception before native dispatch. `Utils.fileInPath` returns `false` and
  `Utils.resolvePath`/`exec` return `null` when that declaration is absent, while `Utils.open`
  still permits HTTP(S) and private/player-track paths. A global controller has no `PlayerCore`,
  so track and `@current` strings there remain ordinary relative paths and fail the reference
  absolute-path gate even though both realms share one Tauri owner window.
- `core.open` applies the same permission ordering with `forceLocalPath: false`, so URL and
  relative inputs still require `file-system`; an unavailable `@current` logs/no-ops instead of
  throwing. HTTP download destinations are synchronously parsed before Promise creation:
  `@tmp/`/`@data/` need no file permission, while bare tokens, external destinations, and invalid
  track/current paths fail immediately.
- `StringEncodingName.swift` is the source of truth. The generated macOS decoder contains all
  128 CoreFoundation external cases and all 23 Foundation built-ins, uses exact case-sensitive
  names, and never substitutes lossy UTF-8.
- Text writes use a same-directory create-new temporary file plus flush/sync and atomic rename.
  Only the private `@tmp/<plugin>` or `@data/<plugin>` root is created automatically; missing
  nested parents remain an error as in `String.write(..., atomically: true)`.
- `list` reproduces the reference's actual direct-child behavior even when `includeSubDir` is
  true, returns `isDir` on the wire, follows directory symlinks for that flag, and reports a
  dangling symlink as non-directory.
- Reading or `readToEnd` on a write-only handle returns `null`. Successful reads construct the
  `Uint8Array` with the owning plugin realm's intrinsic, so `instanceof Uint8Array` is true inside
  that realm.

Deliberate hardening remains at unsafe-input boundaries: private paths containing parent/root
components are rejected component-wise instead of retaining IINA 1.3.5's string-prefix escape
weakness; process arguments are passed as arguments instead of being concatenated into the
reference shell command; and file/process/protocol sizes and counts stay bounded. These do not
change valid plugin inputs.

## Verification

Source-level gates:

```sh
npm run plugins:test
xcrun --sdk macosx clang -fobjc-arc -fblocks -Wall -Wextra -Werror \
  -fsyntax-only src-tauri/src/native_text_encoding.m
```

The packaged WebKit probe is intentionally separate because it launches a real `.app` and binds a
loopback WebSocket listener:

```sh
npm run package:mac -- --skip-dmg
npm run plugins:webkit-probe
```

`scripts/test-plugin-sync-webkit.mjs` refuses an `.app` older than any bridge source. It launches
the fresh bundle under an isolated home with a fixture plugin, then verifies real WebKit's direct
custom-scheme `POST`, backend origin/grant/method/content-type enforcement, immediate non-Promise
results and synchronous errors, complete encoding and file/list/handle behavior, realm-local typed
arrays and absent ambient host globals, synchronous WebSocket creation/start, and grant/file-handle
cleanup on application exit. Deterministic Rust tests separately cover the `OPTIONS` response. The
temporary home is removed after success and preserved after failure; pass `--keep` to retain a
successful fixture.
