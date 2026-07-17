# Open In IINA (Chrome)

This is the IINA 1.3.5 Manifest V3 extension source. The toolbar, popup, and
page/link/video/audio context menus dispatch through the host application's
registered `iina://open` URL scheme. The popup preserves the reference direct,
fullscreen, Picture-in-Picture, new-window, and enqueue actions; new-window uses
the explicit `new_window=1` query parameter.

For local testing, load this directory as an unpacked extension. Chrome Web
Store packaging, signing, and review metadata are intentionally not generated
by this repository.
