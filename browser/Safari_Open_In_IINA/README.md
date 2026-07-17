# Open in IINA for Safari

This directory reproduces the Safari App Extension shipped by IINA 1.3.5. The
extension opens the active page or a selected link through
`iina://weblink?url=...`, so it uses the same URL-scheme path as the Chrome and
Firefox artifacts while retaining Safari's exact reference toolbar and
current-page/link context-menu behavior. It is a legacy
`SFSafariExtensionHandler` App Extension, not a converted WebExtension.

`scripts/build-safari-extension.mjs` compiles the Swift handler directly with
the local macOS SDK for arm64 and x86_64. `scripts/package-macos.mjs` uses that
same offline builder to create `OpenInIINA.appex`, embeds it under
`IINA.app/Contents/PlugIns`, and signs it with the reference sandbox
entitlements before sealing the outer app. No standalone Xcode project or
network dependency is required.

The local package workflow uses an ad-hoc signature only. Safari/App Store
Developer signing, notarization, distribution metadata, and a real Safari
enable-and-click smoke test require an unlocked Apple development environment
and are deliberately outside this source-level build claim.
