# Packaged Interaction Gate

This gate exists because source contracts, browser mocks, and a launchable package prove different
things. A locally reproducible interaction is not considered matched until the final packaged app
has exercised the complete macOS event path and the visible result has been checked.

## Why the earlier gate missed regressions

IINA's original AppKit controllers directly own keyboard responders, mouse tracking, fullscreen,
OSC timers, and mpv callbacks. The Tauri port splits that ownership across AppKit, WKWebView,
JavaScript, Rust, libmpv, and a native video child window. Every implicit AppKit behavior therefore
needs an explicit bridge.

The previous acceptance process mixed four different evidence levels:

1. **Logic verified** — a pure parser/state function has tests.
2. **Bridge verified** — the Objective-C/Rust/JavaScript ABI and routing have executable tests.
3. **Packaged interaction verified** — real input reaches the final `.app`, changes authoritative
   player/window state, and produces the expected visible UI.
4. **External acceptance** — validation genuinely requires unavailable hardware, credentials,
   network services, signing authority, stores, or another host.

Browser mock tests previously converted ordinary mpv key actions into typed state/OSD updates,
while production forwarded the same text as a raw mpv command. That semantic drift let keyboard
smoke tests pass without proving the packaged keyboard, window, or OSD path. The native input bridge
also covered scroll, pressure, and magnify, but not `mouseMoved` or `keyDown`. Both are migration
omissions; allowing them into a package is an acceptance-design omission.

## 0.9.3 corrective architecture

- AppKit sends player-window mouse movement and key-down metadata through the owning Tauri window.
  Mouse movement remains pass-through. Escape is emitted once and then swallowed natively so AppKit
  cannot intercept it before WebKit and WebKit cannot receive a duplicate.
- Native Escape is reconstructed as a bubbling `KeyboardEvent` and enters the same window key
  handler as ordinary keys. Modal, crop, context-menu, plugin, sidebar, and current Key Binding
  precedence therefore remain shared rather than being bypassed by a fullscreen special case.
- Production and browser mock now share one ordinary-mpv-action classifier. Recognized actions use
  typed player/window commands and unrecognized actions retain their original raw mpv command.
- Relative seek reads its amount and mode from the currently active Key Binding. No seek amount is
  a product constant. The IINA Default and Movist profiles intentionally use different values, and
  automated coverage includes `seek 12 exact` and `seek -7`.
- Shortcut execution is serialized so repeated volume/seek input observes the state returned by the
  preceding command instead of racing on one stale frontend snapshot.

## 0.9.4 native event crash correction

The first `0.9.3` packaged build exposed another bridge-specific acceptance gap: moving the pointer
into a player window could terminate the process. `IIMAEmitPlayerInput` classified `MouseMoved`
correctly, but then read `NSEvent.phase` and `NSEvent.momentumPhase` as if every non-key event were a
scroll event. AppKit raises an Objective-C exception when those selectors are used on a mouse-moved
event; allowing that exception to cross the native C callback boundary ends in an abort.

- Scroll events alone read both `phase` and `momentumPhase`; magnify reads only `phase`; key-down
  uses the shared phase field for modifier flags; mouse movement and pressure read neither value.
- The source contract rejects unconditional phase access, while a compiled AppKit harness creates
  real `MouseMoved` and `KeyDown` events and invokes the production emitter. This closes the gap that
  JavaScript/browser mocks could not exercise.
- Release acceptance now requires opening the final `.app` from Finder, entering the real playback
  window with the pointer, observing continued playback, and confirming that no new macOS crash
  report was generated. The final `0.9.4` package passed that sequence.

## Required release matrix

| Boundary | Packaged acceptance |
| --- | --- |
| Fullscreen OSC | Move the pointer over bare video: OSC and cursor appear immediately. Keep still: both hide after the configured timeout. Hover OSC/title: hiding remains suspended. |
| Escape | A configured Escape action follows the active profile. If Escape is unbound, AppKit's normal fallback still exits native fullscreen exactly once, matching profiles such as Movist Default. |
| Seek | Test IINA Default and a non-default profile such as Movist Default (`-10/+10`) or an imported `RIGHT seek 12 exact` / `LEFT seek -7`; each press changes the authoritative position by that configured amount and shows current/total time plus progress in the top-left OSD. |
| Volume | Up/Down follow the active configured amounts, show the volume/progress OSD, and repeated presses accumulate once per event without rollback. |
| Focus | Repeat keyboard tests after clicking bare video, the title text, OSC, and an open sidebar. Typing controls keep their editing semantics. |
| Controls | PIP, Playlist/Chapters, Quick Settings, mute, transport, timeline, and overflow buttons each respond once even when a 100 ms state refresh occurs between pointer-down and pointer-up. |
| Title and cursor | Filename and blank title regions both drag after the native threshold; video content keeps the arrow cursor; mouse buttons do not acquire a blue focus ring. |
| Resize and shape | Sample multiple held-drag intermediate frames. The native video child follows continuously, display aspect remains authoritative, and all four windowed corners remain rounded. |
| Window lifecycle | Re-enter fullscreen and reopen/reuse player windows repeatedly; native monitors do not duplicate events or leave callbacks attached to closed windows. |
| Multi-window | Main, secondary, and Mini Player input changes only the session that owns the receiving window. |

Plain mouse, keyboard, focus, fullscreen, window resize, title dragging, Finder, clipboard, and drop
behavior are local packaged-interaction work on a macOS development host. They must not be labeled
external merely because a browser mock cannot drive them. Physical Touch Bar/media keys, HDR and
multi-display combinations not present on the host, accounts/Keychain providers, network services,
Apple signing/notarization, and store delivery remain legitimate external boundaries.

Until every row has a stable system-level automation driver, release evidence must record both the
automated logic/bridge gates and a dated real packaged interaction run. A long playback soak or a
single fullscreen toggle is not a substitute for this matrix.
