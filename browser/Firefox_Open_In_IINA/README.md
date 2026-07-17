# Open In IINA (Firefox)

This is the IINA 1.3.5 Manifest V2 extension source. It shares the reference
toolbar, popup, options, and page/link/video/audio context-menu behavior with
the Chrome extension while using Firefox's `browser_action` and MV2 script
injection APIs. All actions dispatch through `iina://open`; the popup's separate
window action adds `new_window=1`.

For local testing, load this directory as a temporary extension. Mozilla Add-ons
packaging, signing, and review metadata are intentionally not generated here.
