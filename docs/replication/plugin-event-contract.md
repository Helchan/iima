# IINA 1.3.5 Plugin Event API Contract

This inventory is derived from `参考/iina/iina/EventController.swift`,
`JavascriptAPI/JavascriptAPIEvent.swift`, the `events.emit` call sites in `PlayerCore.swift`,
`MainWindowController.swift`, `PluginOverlayView.swift`, and `PluginSidebarView.swift`, plus
`MPVController.handleEvent` / `handlePropertyChange`.

## IINA-owned events

| Event | Arguments | Reference timing | Tauri source |
| --- | --- | --- | --- |
| `iina.window-loaded` | none | Main player window finished loading | Emitted once after that player window's plugin entries have registered listeners |
| `iina.window-size-adjusted` | `{x,y,width,height}` | IINA automatically applied a media-driven window frame | Native playback window resize completion |
| `iina.window-moved` | `{x,y,width,height}` | AppKit window move notification | Tauri native `WindowEvent::Moved` |
| `iina.window-resized` | `{x,y,width,height}` | AppKit window resize notification | Tauri native `WindowEvent::Resized` |
| `iina.window-fs.changed` | `Bool` | Entered or left fullscreen | Authoritative native player-window status transition |
| `iina.window-screen.changed` | none | Window changed screen | Native scale-factor/screen transition |
| `iina.window-miniaturized` | none | Window became minimized | Native lifecycle `is_minimized` transition |
| `iina.window-deminiaturized` | none | Window left minimized state | Native lifecycle `is_minimized` transition |
| `iina.window-main.changed` | `Bool` | Window became/resigned main | Native focus transition |
| `iina.window-will-close` | none | Player window is about to close | Native close request, with WebView unload fallback |
| `iina.music-mode.changed` | `Bool` | Entered or left Music Mode | Per-player mode transition |
| `iina.pip.changed` | `Bool` | Entered or left system PIP | Per-player native PIP transition |
| `iina.file-started` | none | Handling native mpv `start-file` | Ordered mpv event bridge, immediately before `mpv.start-file` |
| `iina.file-loaded` | absolute URL string | Handling native mpv `file-loaded` | Ordered mpv event bridge, immediately before `mpv.file-loaded` |
| `iina.mpv-inititalized` | none | `startMPV()` completed | The reference emits this before it constructs player plugin instances, so a normal player plugin cannot observe it; the spelling is intentionally preserved |
| `iina.thumbnails-ready` | none | Thumbnail generation completed | First complete progress event for one generation |
| `iina.plugin-overlay-loaded` | none | Overlay or plugin sidebar WebView navigation finished | Sandboxed overlay/sidebar iframe load |

There is no IINA 1.3.5 `iina.pause`, `iina.resume`, `iina.stop`, or `iina.file-error`
event. Those states are observable through the exact mpv events and changed properties below. A
file-open failure therefore produces `mpv.end-file` (and normally `mpv.idle` plus
`mpv.idle-active.changed(true)`), while the application-owned alert remains separate.

## Native mpv events

IINA emits `mpv.<mpv_event_name>` with **no arguments** after it handles every non-`none`
client event. The locally decoded IINA 1.3.5 surface is:

- `mpv.shutdown`, `mpv.log-message`, `mpv.get-property-reply`,
  `mpv.set-property-reply`, and `mpv.command-reply`;
- `mpv.start-file`, `mpv.end-file`, `mpv.file-loaded`, and `mpv.idle`;
- `mpv.tick`, `mpv.client-message`, `mpv.video-reconfig`, and
  `mpv.audio-reconfig`;
- `mpv.seek`, `mpv.playback-restart`, `mpv.property-change`,
  `mpv.queue-overflow`, and `mpv.hook`.

The generic event intentionally does not expose the Rust structured `start_file`, `end_file`, or
hook payload: IINA's public Event API also calls those listeners without arguments. The dedicated
hook API continues to receive its own callback and `next` continuation.

## Changed-property events

For each observed native property event, IINA first emits
`mpv.<property>.changed(value)`, then the generic `mpv.property-change`. The built-in observation
contract and argument types are:

| Argument type | Properties |
| --- | --- |
| `Bool` | `pause`, `deinterlace`, `mute`, `fullscreen`, `ontop`, `idle-active` |
| integer `Number` | `vid`, `aid`, `sid`, `secondary-sid`, `chapter`, `video-rotate`, `contrast`, `brightness`, `gamma`, `hue`, `saturation`, `video-params/rotate` |
| floating-point `Number` | `volume`, `audio-delay`, `speed`, `sub-delay`, `sub-scale`, `sub-pos`, `window-scale` |
| `String` | `loop-playlist`, `loop-file`, `hwdec`, `media-title`, `video-params/primaries`, `video-params/gamma` |
| `0` (`MPV_FORMAT_NONE`) | `track-list`, `vf`, `af` |

Registering `mpv.<other-property>.changed` dynamically asks that concrete player's libmpv client
to observe the property as `MPV_FORMAT_DOUBLE`, matching `JavascriptAPIEvent.on` and
`MPVController.observe(property:)`.

## Delivery and isolation

Each player owns a separate monotonic event cursor and a bounded 512-record native event log.
The mpv executor hands newly drained events to `PlayerState` through a lossless queue rather than
reconstructing them from the diagnostic 50-event tail. The background bridge emits only the new
batch to that player's WebView; snapshot delivery is a second path with the same cursor. The
frontend discards an already-consumed cursor, so command snapshots and native wakeups cannot
double-deliver an event, while repeated `seek`, `playback-restart`, `video-reconfig`, or same-value
property events retain distinct cursors and remain distinct callbacks.

Listeners live in one plugin runtime owned by one player WebView. Disabling, removing, reloading,
or closing the plugin runtime clears its listener map; an event from another player session is
never routed into it.
