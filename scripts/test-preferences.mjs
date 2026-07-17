import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  ADVANCED_HELP_URL,
  IINA_DEFAULT_OSC_TOOLBAR_BUTTONS,
  IINA_OSC_TOOLBAR_BUTTONS,
  PREFERENCE_PANE_ORDER,
  PREFERENCE_PANES,
  SUBTITLE_ENCODING_OPTIONS,
  advancedUserOptionsWithAdded,
  advancedUserOptionsWithEdit,
  advancedUserOptionsWithRemoved,
  buildIinaWindowGeometry,
  normalizeAdvancedUserOptions,
  normalizePreferenceNumber,
  normalizePreferenceTokens,
  normalizeIinaOscToolbarButtons,
  parseIinaWindowGeometry,
  preferenceColorInputState,
  preferenceColorValue,
  preferenceAudioDeviceOptions,
  preferenceControlByKey,
  preferenceControlEnabledForValues,
} from "../src/preference-panes.js";

assert.deepEqual(PREFERENCE_PANE_ORDER, [
  "general",
  "ui",
  "video_audio",
  "subtitle",
  "network",
  "control",
  "keybindings",
  "advanced",
  "plugins",
  "utilities",
]);

const general = PREFERENCE_PANES.find((pane) => pane.id === "general");
assert.ok(general);
assert.deepEqual(general.sections.map((section) => section.title), [
  "Behavior:",
  "History:",
  "Playlist:",
  "Screenshots:",
]);
for (const [key, expectedDefault] of [
  ["actionAfterLaunch", 0],
  ["alwaysOpenInNewWindow", true],
  ["quitWhenNoOpenedWindow", false],
  ["keepOpenOnFileEnd", true],
  ["resumeLastPosition", true],
  ["useLegacyFullScreen", false],
  ["blackOutMonitor", false],
  ["autoSwitchToMusicMode", true],
  ["pauseWhenOpen", false],
  ["fullScreenWhenOpen", false],
  ["pauseWhenMinimized", false],
  ["pauseWhenInactive", false],
  ["playWhenEnteringFullScreen", false],
  ["pauseWhenLeavingFullScreen", false],
  ["pauseWhenGoesToSleep", true],
  ["recordPlaybackHistory", true],
  ["recordRecentFiles", true],
  ["trackAllFilesInRecentOpenMenu", true],
  ["playlistAutoAdd", true],
  ["playlistAutoPlayNext", true],
  ["playlistShowMetadata", true],
  ["playlistShowMetadataInMusicMode", true],
]) {
  assert.equal(preferenceControlByKey(key).defaultValue, expectedDefault, `${key} General default`);
}
assert.deepEqual(preferenceControlByKey("actionAfterLaunch").options.map(([value, label]) => [value, label]), [
  [0, "Show welcome window"],
  [1, "Show open file panel"],
  [2, "Do nothing"],
]);
assert.deepEqual(preferenceControlByKey("actionAfterLaunch").options[2][2], {
  table: "PrefGeneralViewController",
  key: "JfZ-D8-aF2.title",
});
assert.equal(preferenceControlByKey("trackAllFilesInRecentOpenMenu").dependsOn, "recordRecentFiles");
assert.equal(
  preferenceControlEnabledForValues(preferenceControlByKey("trackAllFilesInRecentOpenMenu"), {
    recordRecentFiles: false,
  }),
  false,
);
assert.equal(preferenceControlByKey("playlistShowMetadataInMusicMode").dependsOn, "playlistShowMetadata");
assert.equal(
  preferenceControlEnabledForValues(preferenceControlByKey("playlistShowMetadataInMusicMode"), {
    playlistShowMetadata: false,
  }),
  false,
);
assert.equal(preferenceControlByKey("playlistAutoAdd").restartRequired, true);
assert.equal(
  preferenceControlByKey("playlistAutoAdd").hint,
  "Requires restarting IINA to take effect.",
);
assert.deepEqual(general.sections[3].controls.map((control) => control.key), [
  "screenshotSaveToFile",
  "screenShotFolder",
  "screenShotFormat",
  "screenshotCopyToClipboard",
  "screenShotIncludeSubtitle",
  "screenshotShowPreview",
]);
assert.equal(
  preferenceControlByKey("screenShotTemplate"),
  undefined,
  "the runtime screenshot-template preference is not exposed by IINA 1.3.5's General XIB",
);

const ui = PREFERENCE_PANES.find((pane) => pane.id === "ui");
assert.ok(ui);
assert.deepEqual(ui.sections.map((section) => section.title), [
  "Appearance:",
  "Window:",
  "On Screen Controller:",
  "On Screen Display:",
  "Thumbnail Preview:",
  "Picture-in-Picture:",
]);
for (const [key, expectedDefault] of [
  ["themeMaterial", 0],
  ["usePhysicalResolution", true],
  ["initialWindowSizePosition", ""],
  ["resizeWindowTiming", 1],
  ["resizeWindowOption", 2],
  ["alwaysFloatOnTop", false],
  ["alwaysShowOnTopIcon", false],
  ["oscPosition", 0],
  ["controlBarAutoHideTimeout", 2.5],
  ["controlBarStickToCenter", true],
  ["showChapterPos", false],
  ["showRemainingTime", false],
  ["arrowBtnAction", 0],
  ["enableOSD", true],
  ["osdAutoHideTimeout", 1],
  ["osdTextSize", 20],
  ["displayTimeAndBatteryInFullScreen", false],
  ["enableThumbnailPreview", true],
  ["maxThumbnailPreviewCacheSize", 500],
  ["windowBehaviorWhenPip", 0],
  ["pauseWhenPip", false],
  ["togglePipByMinimizingWindow", false],
]) {
  assert.deepEqual(preferenceControlByKey(key).defaultValue, expectedDefault, `${key} UI default`);
}
assert.deepEqual(preferenceControlByKey("themeMaterial").options, [
  [0, "Dark"],
  [2, "Light"],
  [4, "Match System Appearance"],
]);
assert.deepEqual(preferenceControlByKey("resizeWindowTiming").options.map(([value]) => value), [0, 1, 2]);
assert.deepEqual(preferenceControlByKey("resizeWindowOption").options.map(([value]) => value), [0, 1, 2, 3, 4]);
assert.equal(
  preferenceControlEnabledForValues(preferenceControlByKey("resizeWindowOption"), { resizeWindowTiming: 2 }),
  false,
);
assert.equal(
  preferenceControlEnabledForValues(preferenceControlByKey("resizeWindowOption"), { resizeWindowTiming: 1 }),
  true,
);
assert.deepEqual(preferenceControlByKey("controlBarToolbarButtons").defaultValue, [2, 1, 0]);
assert.deepEqual(IINA_DEFAULT_OSC_TOOLBAR_BUTTONS, [2, 1, 0]);
assert.deepEqual(IINA_OSC_TOOLBAR_BUTTONS.map(({ value }) => value), [0, 1, 2, 3, 4, 5, 6]);
assert.deepEqual(IINA_OSC_TOOLBAR_BUTTONS.map(({ l10n }) => l10n.key), [
  "osc_toolbar.settings",
  "osc_toolbar.playlist",
  "osc_toolbar.pip",
  "osc_toolbar.full_screen",
  "osc_toolbar.music_mode",
  "osc_toolbar.sub_track",
  "osc_toolbar.screenshot",
]);
assert.deepEqual(normalizeIinaOscToolbarButtons(undefined), [2, 1, 0]);
assert.deepEqual(normalizeIinaOscToolbarButtons([2, 2, 8, 1, "0", 6, 5, 4]), [2, 1, 0, 6, 5]);
assert.deepEqual(normalizeIinaOscToolbarButtons([]), []);

assert.deepEqual(parseIinaWindowGeometry("x720%+20-30%"), {
  sizeEnabled: true,
  sizeDimension: "height",
  sizeValue: "720",
  sizeUnit: "percent",
  positionEnabled: true,
  xOffset: "20",
  xUnit: "point",
  xAnchor: "left",
  yOffset: "30",
  yUnit: "percent",
  yAnchor: "top",
});
assert.equal(buildIinaWindowGeometry(parseIinaWindowGeometry("x720%+20-30%")), "x720%+20-30%");
assert.equal(buildIinaWindowGeometry({
  ...parseIinaWindowGeometry(""),
  sizeEnabled: true,
  positionEnabled: true,
}), "1280+20-20");
assert.equal(parseIinaWindowGeometry("bad geometry").sizeEnabled, false);

const uiXib = readFileSync(
  new URL("../参考/iina/iina/Base.lproj/PrefUIViewController.xib", import.meta.url),
  "utf8",
);
const uiController = readFileSync(
  new URL("../参考/iina/iina/PrefUIViewController.swift", import.meta.url),
  "utf8",
);
assert.match(
  uiController,
  /return \[sectionAppearanceView, sectionWindowView, sectionOSCView, sectionOSDView, sectionThumbnailView, sectionPictureInPictureView\]/,
);
for (const key of [
  "usePhysicalResolution",
  "resizeWindowOption",
  "alwaysFloatOnTop",
  "alwaysShowOnTopIcon",
  "oscPosition",
  "controlBarStickToCenter",
  "showChapterPos",
  "showRemainingTime",
  "arrowBtnAction",
  "controlBarAutoHideTimeout",
  "enableOSD",
  "osdAutoHideTimeout",
  "osdTextSize",
  "displayTimeAndBatteryInFullScreen",
  "enableThumbnailPreview",
  "maxThumbnailPreviewCacheSize",
  "pauseWhenPip",
  "togglePipByMinimizingWindow",
]) {
  assert.match(uiXib, new RegExp(`keyPath="values\\.${key}"`), `${key} reference XIB binding`);
}

const generalXib = readFileSync(
  new URL("../参考/iina/iina/Base.lproj/PrefGeneralViewController.xib", import.meta.url),
  "utf8",
);
const generalController = readFileSync(
  new URL("../参考/iina/iina/PrefGeneralViewController.swift", import.meta.url),
  "utf8",
);
assert.match(generalController, /return \[behaviorView, historyView, playlistView, screenshotsView\]/);
assert.equal(
  preferenceControlByKey("autoSwitchToMusicMode").label,
  'Switch to "music mode" automatically when playing an audio file',
);
assert.equal(preferenceControlByKey("screenShotFolder").label, "Destination:");
assert.equal(preferenceControlByKey("screenShotFormat").label, "Format:");
for (const [id, title] of [
  ["KfL-r7-Uav", 'Switch to &quot;music mode&quot; automatically when playing an audio file'],
  ["PhL-vP-i7L", "Destination:"],
  ["oJY-Iz-3sO", "Format:"],
]) {
  assert.ok(
    generalXib.includes(`title="${title}"`) && generalXib.includes(`id="${id}"`),
    `${id} exact General XIB title`,
  );
}
for (const key of [
  "useLegacyFullScreen",
  "blackOutMonitor",
  "pauseWhenMinimized",
  "pauseWhenInactive",
  "playWhenEnteringFullScreen",
  "pauseWhenLeavingFullScreen",
  "pauseWhenGoesToSleep",
  "playlistAutoAdd",
  "playlistAutoPlayNext",
  "playlistShowMetadata",
  "playlistShowMetadataInMusicMode",
  "recordRecentFiles",
  "trackAllFilesInRecentOpenMenu",
]) {
  assert.match(generalXib, new RegExp(`keyPath="values\\.${key}"`), `${key} reference XIB binding`);
}

const codec = PREFERENCE_PANES.find((pane) => pane.id === "video_audio");
assert.ok(codec);
assert.deepEqual(codec.sections.map((section) => section.title), ["Video:", "Audio:"]);
assert.deepEqual(codec.sections[0].controls.map((control) => control.key), [
  "videoThreads",
  "hardwareDecoder",
  "forceDedicatedGPU",
  "loadIccProfile",
  "enableHdrSupport",
  "enableToneMapping",
  "toneMappingTargetPeak",
  "toneMappingAlgorithm",
]);
assert.deepEqual(codec.sections[1].controls.map((control) => control.key), [
  "audioThreads",
  "audioLanguage",
  "spdifOutput",
  "enableInitialVolume",
  "maxVolume",
  "audioDevice",
]);
const codecXib = readFileSync(
  new URL("../参考/iina/iina/Base.lproj/PrefCodecViewController.xib", import.meta.url),
  "utf8",
);
const codecController = readFileSync(
  new URL("../参考/iina/iina/PrefCodecViewController.swift", import.meta.url),
  "utf8",
);
for (const key of [
  "videoThreads",
  "hardwareDecoder",
  "forceDedicatedGPU",
  "loadIccProfile",
  "enableHdrSupport",
  "enableToneMapping",
  "toneMappingTargetPeak",
  "toneMappingAlgorithm",
  "audioThreads",
  "spdifAC3",
  "spdifDTS",
  "spdifDTSHD",
  "enableInitialVolume",
  "initialVolume",
  "maxVolume",
]) {
  assert.match(codecXib, new RegExp(`keyPath="values\\.${key}"`), `${key} visible Codec XIB binding`);
}
assert.match(codecController, /audioLangTokenField\.commaSeparatedValues/);
assert.match(codecController, /Preference\.set\(device\["name"\]!?, for: \.audioDevice\)/);
assert.match(codecController, /Preference\.set\(device\["description"\]!?, for: \.audioDeviceDesc\)/);

for (const [key, expectedDefault] of [
  ["videoThreads", 0],
  ["hardwareDecoder", 1],
  ["forceDedicatedGPU", false],
  ["loadIccProfile", true],
  ["enableHdrSupport", true],
  ["enableToneMapping", false],
  ["toneMappingTargetPeak", 0],
  ["toneMappingAlgorithm", 0],
  ["audioThreads", 0],
  ["audioLanguage", ""],
  ["enableInitialVolume", false],
  ["maxVolume", 100],
  ["audioDevice", "auto"],
  ["enableCache", true],
  ["defaultCacheSize", 153600],
  ["secPrefech", 36000],
  ["userAgent", ""],
  ["httpProxy", ""],
  ["transportRTSPThrough", 1],
  ["ytdlEnabled", true],
  ["ytdlSearchPath", ""],
  ["ytdlRawOptions", ""],
]) {
  assert.equal(preferenceControlByKey(key).defaultValue, expectedDefault, `${key} default`);
}

const hardwareDecoder = preferenceControlByKey("hardwareDecoder");
assert.equal(hardwareDecoder.defaultValue, 1);
assert.deepEqual(hardwareDecoder.options.map(([value]) => value), [0, 1, 2]);
assert.deepEqual(hardwareDecoder.options.map(([, label]) => label), ["Disabled", "Auto", "Auto (Copy)"]);

const toneMappingAlgorithm = preferenceControlByKey("toneMappingAlgorithm");
assert.equal(toneMappingAlgorithm.defaultValue, 0);
assert.deepEqual(toneMappingAlgorithm.options.map(([value]) => value), [
  0,
  1,
  2,
  3,
  4,
  5,
  6,
  7,
]);
assert.deepEqual(toneMappingAlgorithm.options.map(([, , metadata]) => metadata.algorithm), [
  "auto",
  "clip",
  "mobius",
  "reinhard",
  "hable",
  "bt.2390",
  "gamma",
  "linear",
]);
assert.equal(toneMappingAlgorithm.dependsOn, "enableToneMapping");
assert.equal(preferenceControlEnabledForValues(toneMappingAlgorithm, { enableToneMapping: false }), false);
assert.equal(preferenceControlEnabledForValues(toneMappingAlgorithm, { enableToneMapping: true }), true);

const spdif = preferenceControlByKey("spdifDTSHD");
assert.equal(spdif.key, "spdifOutput");
assert.deepEqual(spdif.items.map((item) => item.key), ["spdifAC3", "spdifDTS", "spdifDTSHD"]);
assert.deepEqual(spdif.items.map((item) => item.defaultValue), [false, false, false]);

const initialVolume = preferenceControlByKey("initialVolume");
assert.equal(initialVolume.key, "enableInitialVolume");
assert.equal(initialVolume.valueKey, "initialVolume");
assert.equal(initialVolume.valueDefault, 100);
assert.equal(preferenceControlByKey("audioLanguage").type, "tokens");
assert.equal(normalizePreferenceTokens(" EN, ja\nzh-CN, en,  "), "en,ja,zh-cn");

const maxVolume = preferenceControlByKey("maxVolume");
assert.equal(maxVolume.min, 100);
assert.equal(maxVolume.max, 1000);
assert.equal(normalizePreferenceNumber(maxVolume, 99), 100);
assert.equal(normalizePreferenceNumber(maxVolume, 1200), 1000);
assert.equal(normalizePreferenceNumber(maxVolume, 155.6), 156);

for (const key of ["videoThreads", "audioThreads", "defaultCacheSize", "secPrefech"]) {
  assert.equal(preferenceControlByKey(key).min, 0, `${key} must retain the XIB minimum`);
}

const network = PREFERENCE_PANES.find((pane) => pane.id === "network");
assert.ok(network);
assert.deepEqual(network.sections.map((section) => section.title), ["Cache:", "Network:", "youtube-dl:"]);
assert.deepEqual(network.sections.flatMap((section) => section.controls.map((control) => control.key)), [
  "enableCache",
  "defaultCacheSize",
  "secPrefech",
  "userAgent",
  "httpProxy",
  "transportRTSPThrough",
  "ytdlEnabled",
  "ytdlSearchPath",
  "ytdlRawOptions",
]);
assert.equal(preferenceControlByKey("defaultCacheSize").defaultValue, 153600);
assert.equal(preferenceControlByKey("secPrefech").defaultValue, 36000);
assert.equal(preferenceControlByKey("defaultCacheSize").dependsOn, "enableCache");
assert.equal(preferenceControlEnabledForValues(preferenceControlByKey("secPrefech"), { enableCache: false }), false);
assert.equal(preferenceControlEnabledForValues(preferenceControlByKey("ytdlRawOptions"), { ytdlEnabled: false }), false);
assert.deepEqual(preferenceControlByKey("transportRTSPThrough").options.map(([value]) => value), [0, 1, 2, 3]);
assert.equal(preferenceControlByKey("transportRTSPThrough").defaultValue, 1);
assert.equal(preferenceControlByKey("httpProxy").prefix, "http://");
assert.equal(preferenceControlByKey("ytdlSearchPath").type, "text", "the reference XIB uses a path text field");
assert.equal(
  preferenceControlByKey("ytdlEnabled").helpUrl,
  "https://github.com/rg3/youtube-dl/blob/master/README.md#readme",
);
const networkXib = readFileSync(
  new URL("../参考/iina/iina/Base.lproj/PrefNetworkViewController.xib", import.meta.url),
  "utf8",
);
for (const key of [
  "enableCache",
  "defaultCacheSize",
  "secPrefech",
  "userAgent",
  "transportRTSPThrough",
  "httpProxy",
  "ytdlEnabled",
  "ytdlSearchPath",
  "ytdlRawOptions",
]) {
  assert.match(networkXib, new RegExp(`keyPath="values\\.${key}"`), `${key} visible Network XIB binding`);
}

const control = PREFERENCE_PANES.find((pane) => pane.id === "control");
assert.ok(control);
assert.deepEqual(
  control.sections.map((section) => section.title),
  ["Trackpad:", "Mouse:"],
  "PrefControlViewController.sectionViews places Trackpad before Mouse",
);
assert.deepEqual(control.sections[0].controls.map(({ key }) => key), [
  "pinchAction",
  "forceTouchAction",
]);
assert.deepEqual(control.sections[1].controls.map(({ key }) => key), [
  "verticalScrollAction",
  "horizontalScrollAction",
  "useExactSeek",
  "exactSeekHint",
  "relativeSeekAmount",
  "volumeScrollAmount",
  "videoViewAcceptsFirstMouse",
  "singleClickAction",
  "doubleClickAction",
  "rightClickAction",
  "middleClickAction",
]);
for (const [key, label, xibKey] of [
  ["verticalScrollAction", "Scroll vertically to:", "hNU-dJ-5Cj.title"],
  ["horizontalScrollAction", "Scroll horizontally to:", "w1Y-jy-vNc.title"],
  ["useExactSeek", "Seek type:", "1yb-m4-1sK.title"],
  ["exactSeekHint", "Exact seek is more precise but can cause lag and higher CPU usage.", "6FZ-Qm-0k0.title"],
  ["relativeSeekAmount", "Sensitivity for normal seek:", "of2-ec-OkC.title"],
  ["volumeScrollAmount", "Sensitivity for volume:", "YQH-1a-WcM.title"],
  ["videoViewAcceptsFirstMouse", "Accepts first mouse click when not focused", "gZK-ry-lQs.title"],
]) {
  const preferenceControl = preferenceControlByKey(key);
  assert.equal(preferenceControl.label, label, `${key} exact XIB label`);
  assert.deepEqual(preferenceControl.l10n, {
    table: "PrefControlViewController",
    key: xibKey,
  });
}
assert.equal(preferenceControlByKey("videoViewAcceptsFirstMouse").defaultValue, false);
const controlController = readFileSync(
  new URL("../参考/iina/iina/PrefControlViewController.swift", import.meta.url),
  "utf8",
);
assert.match(controlController, /return \[sectionTrackpadView, sectionMouseView\]/);
const controlXib = readFileSync(
  new URL("../参考/iina/iina/Base.lproj/PrefControlViewController.xib", import.meta.url),
  "utf8",
);
assert.match(controlXib, /keyPath="values\.videoViewAcceptsFirstMouse"/);

assert.deepEqual(
  preferenceAudioDeviceOptions(
    [{ name: "auto", description: "Autoselect device" }],
    "coreaudio/77",
    "Studio Display",
  ),
  [
    {
      value: "auto",
      description: "Autoselect device",
      label: "[Autoselect device] auto",
      missing: false,
    },
    {
      value: "coreaudio/77",
      description: "Studio Display",
      label: "[Studio Display (missing)] coreaudio/77",
      missing: true,
    },
  ],
);

const screenshotFolder = preferenceControlByKey("screenShotFolder");
assert.equal(screenshotFolder.type, "folder");
assert.equal(screenshotFolder.pickerCommand, "choose_screenshot_folder");

const mainSource = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
assert.doesNotMatch(mainSource, /const PREFERENCE_PANES\s*=/, "pane definitions must stay in the testable module");
assert.match(mainSource, /invoke\(control\.pickerCommand, control\.pickerArgs \|\| \{\}\)/);
for (const type of [
  "hint",
  "font",
  "color",
  "tokens",
  "advanced-actions",
  "advanced-options",
  "advanced-config-directory",
]) {
  assert.match(mainSource, new RegExp(`control\\.type === "${type}"`), `${type} preference renderer`);
}
assert.match(mainSource, /preferenceColorValue\(colorInput\.value, alphaInput\.value\)/);
assert.match(mainSource, /renderPreferenceLanguageTokenField\(control, value, disabled\)/);
assert.match(mainSource, /persistPreferenceLanguageTokens\(control\.key, tokens\)/);
assert.match(mainSource, /subLang: ""/);
assert.match(mainSource, /toneMappingAlgorithm: 0/);
assert.match(mainSource, /touchbarShowRemainingTime: true/);
assert.match(mainSource, /logLevel: 1/);
assert.match(mainSource, /userOptions: \[\]/);
assert.match(mainSource, /useUserDefinedConfDir: false/);
assert.match(
  mainSource,
  /parent\.bottom - event\.clientY - \(oscDragState\?\.offsetBottom \?\? 0\)/,
  "floating OSC persists its bottom-edge origin without adding half its height",
);
assert.match(mainSource, /tauriListen\("iima-player-window-status"/);
assert.match(mainSource, /invoke\("get_player_window_status"\)/);
const playerHtml = readFileSync(new URL("../src/index.html", import.meta.url), "utf8");
assert.match(playerHtml, /id="fullscreen-title"/);
const nativeWindowSource = readFileSync(
  new URL("../src-tauri/src/native_window.m", import.meta.url),
  "utf8",
);
assert.match(nativeWindowSource, /IOPSCopyPowerSourcesInfo/);
assert.match(nativeWindowSource, /kIOPSInternalBatteryType/);

const subtitle = PREFERENCE_PANES.find((pane) => pane.id === "subtitle");
assert.ok(subtitle);
assert.deepEqual(subtitle.sections.map((section) => section.title), [
  "Auto Load:",
  "ASS Subtitles:",
  "Text Subtitles:",
  "Position:",
  "Online Subtitles:",
  "Other:",
]);
assert.deepEqual(subtitle.sections[0].controls.map((control) => control.key), [
  "subAutoLoadIINA",
  "subAutoLoadAdvancedDisclosure",
  "subAutoLoadPriorityString",
  "subAutoLoadSearchPath",
]);
assert.equal(preferenceControlByKey("subAutoLoadIINA").defaultValue, 2);
assert.equal(preferenceControlByKey("subAutoLoadIINA").label, "", "the reference auto-load popup has no row label");
assert.deepEqual(preferenceControlByKey("subAutoLoadIINA").options.map(([value, label]) => [value, label]), [
  [0, "Disabled"],
  [1, "Subtitles containing media filename"],
  [2, "Detect intelligently by IINA"],
]);
assert.deepEqual(preferenceControlByKey("subAutoLoadIINA").options[0][2], {
  table: "PrefSubViewController",
  key: "u31-eV-Arr.title",
});
for (const key of [
  "ignoreAssStyles",
  "subOverrideLevel",
  "subTextFont",
  "subTextSize",
  "subTextColor",
  "subBgColor",
  "subBold",
  "subItalic",
  "subBlur",
  "subSpacing",
  "subBorderSize",
  "subBorderColor",
  "subShadowSize",
  "subShadowColor",
  "subAlignX",
  "subAlignY",
  "subMarginX",
  "subMarginY",
  "subPos",
  "displayInLetterBox",
  "subScaleWithWindow",
  "subLang",
  "defaultEncoding",
]) {
  assert.ok(preferenceControlByKey(key), `${key} must be represented by the Subtitle pane`);
}
assert.equal(
  preferenceControlByKey("subOverrideLevel").dependsOn,
  undefined,
  "IINA 1.3.5 leaves the override-level slider enabled so its next active value can be staged",
);
assert.equal(
  preferenceControlEnabledForValues(preferenceControlByKey("subOverrideLevel"), { ignoreAssStyles: false }),
  true,
);
assert.deepEqual(
  {
    type: preferenceControlByKey("subOverrideLevel").type,
    min: preferenceControlByKey("subOverrideLevel").min,
    max: preferenceControlByKey("subOverrideLevel").max,
    step: preferenceControlByKey("subOverrideLevel").step,
    options: preferenceControlByKey("subOverrideLevel").options,
  },
  {
    type: "slider",
    min: 0,
    max: 2,
    step: 1,
    options: [[0, "yes"], [1, "force"], [2, "strip"]],
  },
  "the ASS override level keeps IINA's three-stop slider and transformed label",
);
assert.equal(preferenceControlByKey("subLang").type, "tokens");
assert.equal(normalizePreferenceTokens(" EN, ja\nzh-CN, en,  "), "en,ja,zh-cn");
assert.deepEqual(preferenceColorInputState("1/0.5/0/0.25"), {
  hex: "#ff8000",
  alpha: 0.25,
  preservedRaw: false,
});
assert.deepEqual(
  preferenceColorInputState({ __iimaUserDefaultsPlistValue: { type: "data", value: "00ff" } }, "0/0/0/0"),
  { hex: "#000000", alpha: 0, preservedRaw: true },
);
assert.equal(preferenceColorValue("#804020", 0.5), "0.501961/0.25098/0.12549/0.5");
assert.equal(preferenceControlByKey("defaultEncoding").type, "select");
assert.equal(SUBTITLE_ENCODING_OPTIONS.length, 44);
assert.deepEqual(SUBTITLE_ENCODING_OPTIONS[0], ["auto", "Auto detect"]);
assert.deepEqual(SUBTITLE_ENCODING_OPTIONS.at(-1), ["LATIN-9", "Western European (LATIN-9)"]);
assert.equal(preferenceControlByKey("autoSearchThreshold"), undefined, "the XIB exposes only the fixed 20 minute hint");
for (const [key, expectedDefault] of [
  ["onlineSubProvider", ":opensubtitles"],
  ["openSubUsername", ""],
  ["assrtToken", ""],
  ["autoSearchOnlineSub", false],
]) {
  assert.deepEqual(preferenceControlByKey(key).defaultValue, expectedDefault, `${key} Subtitle default`);
}
assert.equal(preferenceControlByKey("onlineSubProvider").label, "Download subtitles from:");
assert.equal(preferenceControlByKey("assrtToken").label, "Assrt API token:");
assert.equal(preferenceControlByKey("autoSearchOnlineSub").label, "Search online subtitles automatically");
assert.equal(
  preferenceControlByKey("openSubUsername").helpUrl,
  "https://github.com/iina/iina/wiki/Download-Online-Subtitles#opensubtitles",
);
assert.equal(
  preferenceControlByKey("assrtToken").helpUrl,
  "https://github.com/iina/iina/wiki/Download-Online-Subtitles#assrt",
);
assert.equal(preferenceControlByKey("subBorderSize").label, "Size:");
assert.equal(preferenceControlByKey("subBorderColor").label, "Color:");
assert.equal(preferenceControlByKey("subShadowSize").label, "Offset:");
assert.equal(preferenceControlByKey("subShadowColor").label, "Color:");

const subtitlePreferenceSource = readFileSync(
  new URL("../参考/iina/iina/Preference.swift", import.meta.url),
  "utf8",
);
const subtitleXib = readFileSync(
  new URL("../参考/iina/iina/Base.lproj/PrefSubViewController.xib", import.meta.url),
  "utf8",
);
for (const title of [
  "Disabled",
  "Subtitles containing media filename",
  "Detect intelligently by IINA",
]) {
  assert.match(subtitleXib, new RegExp(`<menuItem title="${title}"`), `${title} auto-load menu title`);
}
for (const [id, title] of [
  ["DUA-7H-Yje", "Advanced"],
  ["X7X-18-FJg", "Advanced"],
  ["TPK-CJ-HX7", "Download subtitles from:"],
  ["0aU-O1-R6G", "Assrt API token:"],
  ["C3p-uP-8u8", "Search online subtitles automatically"],
  ["Z2M-dh-eq1", "This option will be stored as ISO 639-2 language code and will works for both mpv and opensubtitles."],
]) {
  assert.ok(
    subtitleXib.includes(`title="${title}"`) && subtitleXib.includes(`id="${id}"`),
    `${id} exact reference title`,
  );
}
assert.match(subtitleXib, /selector="openSubHelpBtnAction:"/);
assert.match(subtitleXib, /selector="assrtHelpBtnAction:"/);
for (const key of [
  "subAutoLoadPriorityString",
  "subAutoLoadSearchPath",
  "ignoreAssStyles",
  "subOverrideLevel",
  "subTextFont",
  "subTextSize",
  "subTextColor",
  "subBgColor",
  "subBold",
  "subItalic",
  "subBlur",
  "subSpacing",
  "subBorderSize",
  "subBorderColor",
  "subShadowSize",
  "subShadowColor",
  "subAlignX",
  "subAlignY",
  "subMarginX",
  "subMarginY",
  "subPos",
  "displayInLetterBox",
  "subScaleWithWindow",
]) {
  assert.match(subtitlePreferenceSource, new RegExp(`static let ${key} = Key\\("${key}"\\)`), `${key} reference key`);
  assert.match(subtitleXib, new RegExp(`keyPath="values\\.${key}"`), `${key} visible XIB binding`);
}
assert.match(
  subtitleXib,
  /<sliderCell[^>]*maxValue="2"[^>]*numberOfTickMarks="3"[^>]*allowsTickMarkValuesOnly="YES"/,
  "reference ASS override level is a three-stop discrete slider",
);
assert.match(mainSource, /control\.type === "slider"/);
assert.match(mainSource, /input\.type = "range"/);
assert.doesNotMatch(subtitleXib, /keyPath="values\.autoSearchThreshold"/, "the threshold key is runtime-only in IINA 1.3.5");

const keyBindingXib = readFileSync(
  new URL("../参考/iina/iina/Base.lproj/PrefKeyBindingViewController.xib", import.meta.url),
  "utf8",
);
assert.equal(preferenceControlByKey("useMediaKeys").label, "Use system media control");
assert.ok(keyBindingXib.includes('title="Use system media control" bezelStyle="regularSquare"'));

const logLevel = preferenceControlByKey("logLevel");
assert.equal(logLevel.defaultValue, 1);
assert.deepEqual(logLevel.options.map(([value, label]) => [value, label]), [
  [0, "Verbose"],
  [1, "Debug"],
  [2, "Warning"],
  [3, "Error"],
]);

const advanced = PREFERENCE_PANES.find((pane) => pane.id === "advanced");
assert.ok(advanced);
assert.deepEqual(advanced.sections.map((section) => section.title), [
  "Advanced Settings:",
  "Logging:",
  "Settings:",
]);
assert.deepEqual(advanced.sections.flatMap((section) => section.controls.map((control) => control.key)), [
  "enableAdvancedSettings",
  "advancedRestartHint",
  "logLevel",
  "enableLogging",
  "advancedLogActions",
  "useMpvOsd",
  "userOptions",
  "useUserDefinedConfDir",
  "userDefinedConfDir",
]);
assert.equal(preferenceControlByKey("enableAdvancedSettings").helpUrl, ADVANCED_HELP_URL);
assert.equal(preferenceControlByKey("enableAdvancedSettings").helpCommand, "open_advanced_help");
for (const [key, expectedDefault] of [
  ["enableAdvancedSettings", false],
  ["enableLogging", false],
  ["useMpvOsd", false],
  ["userOptions", []],
  ["useUserDefinedConfDir", false],
  ["userDefinedConfDir", "~/.config/mpv/"],
]) {
  assert.deepEqual(preferenceControlByKey(key).defaultValue, expectedDefault, `${key} Advanced default`);
}
for (const key of ["logLevel", "enableLogging", "useMpvOsd", "userOptions", "useUserDefinedConfDir"]) {
  const control = preferenceControlByKey(key);
  assert.equal(preferenceControlEnabledForValues(control, { enableAdvancedSettings: false }), false, `${key} off`);
  assert.equal(preferenceControlEnabledForValues(control, { enableAdvancedSettings: true }), true, `${key} on`);
}
const configDirectory = preferenceControlByKey("userDefinedConfDir");
assert.equal(configDirectory.type, "advanced-config-directory");
assert.equal(configDirectory.pickerCommand, "choose_advanced_config_directory");
assert.equal(
  preferenceControlEnabledForValues(configDirectory, {
    enableAdvancedSettings: true,
    useUserDefinedConfDir: false,
  }),
  false,
);
assert.equal(
  preferenceControlEnabledForValues(configDirectory, {
    enableAdvancedSettings: true,
    useUserDefinedConfDir: true,
  }),
  true,
);
const logActions = preferenceControlByKey("advancedLogActions");
assert.deepEqual(logActions.actions.map(({ command, label }) => [command, label]), [
  ["show_log_viewer", "Show log viewer"],
  ["open_log_directory", "Open log directory"],
]);
assert.equal(preferenceControlByKey("userOptions").type, "advanced-options");
assert.deepEqual(normalizeAdvancedUserOptions([["profile", "gpu-hq"], ["broken"], [1, "x"]]), [
  ["profile", "gpu-hq"],
]);
assert.deepEqual(advancedUserOptionsWithAdded([]), [["name", "value"]]);
assert.deepEqual(
  advancedUserOptionsWithEdit([["profile", "gpu-hq"]], 0, 1, "gpu-next"),
  [["profile", "gpu-next"]],
);
assert.equal(advancedUserOptionsWithEdit([["profile", "gpu-hq"]], 0, 1, ""), null);
assert.deepEqual(
  advancedUserOptionsWithRemoved([["profile", "gpu-hq"], ["cache", "yes"]], 0),
  [["cache", "yes"]],
);

const advancedXib = readFileSync(
  new URL("../参考/iina/iina/Base.lproj/PrefAdvancedViewController.xib", import.meta.url),
  "utf8",
);
const advancedControllerSource = readFileSync(
  new URL("../参考/iina/iina/PrefAdvancedViewController.swift", import.meta.url),
  "utf8",
);
for (const key of ["useMpvOsd", "enableLogging", "logLevel", "useUserDefinedConfDir", "userDefinedConfDir"]) {
  assert.match(subtitlePreferenceSource, new RegExp(`static let ${key} = Key\\("${key}"\\)`), `${key} reference key`);
  assert.match(advancedXib, new RegExp(`keyPath="values\\.${key}"`), `${key} visible XIB binding`);
}
assert.match(subtitlePreferenceSource, /\.enableAdvancedSettings: false/);
assert.match(subtitlePreferenceSource, /\.useMpvOsd: false/);
assert.match(subtitlePreferenceSource, /\.enableLogging: false/);
assert.match(subtitlePreferenceSource, /\.logLevel: Logger\.Level\.debug\.rawValue/);
assert.match(subtitlePreferenceSource, /\.userOptions: \[\[String\]\]\(\)/);
assert.match(subtitlePreferenceSource, /\.useUserDefinedConfDir: false/);
assert.match(subtitlePreferenceSource, /\.userDefinedConfDir: "~\/\.config\/mpv\/"/);
for (const selector of [
  "openLogDir:",
  "showLogWindow:",
  "addOptionBtnAction:",
  "removeOptionBtnAction:",
  "chooseDirBtnAction:",
  "helpBtnAction:",
]) {
  assert.match(advancedXib, new RegExp(`selector="${selector}"`), `${selector} XIB action`);
}
assert.match(advancedControllerSource, /options\.append\(\["name", "value"\]\)/);
assert.match(advancedControllerSource, /guard !value\.isEmpty/);
assert.match(advancedControllerSource, /MPV-Options-and-Properties/);

const commandSource = readFileSync(new URL("../src-tauri/src/commands.rs", import.meta.url), "utf8");
const libSource = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
const auxiliaryWindowSource = readFileSync(
  new URL("../src-tauri/src/auxiliary_windows.rs", import.meta.url),
  "utf8",
);
const logViewerSource = readFileSync(new URL("../src/log.js", import.meta.url), "utf8");
for (const command of [
  "choose_advanced_config_directory",
  "open_log_directory",
  "show_log_viewer",
  "open_advanced_help",
]) {
  assert.match(commandSource, new RegExp(`pub fn ${command}\\(`), `${command} native implementation`);
  assert.ok(libSource.match(new RegExp(command, "g"))?.length >= 2, `${command} registration`);
}
assert.match(commandSource, /set_title\("Choose config directory"\)/);
assert.match(commandSource, /set_can_create_directories\(false\)/);
assert.match(commandSource, /auxiliary_windows::show_log_viewer_window/);
assert.match(auxiliaryWindowSource, /WebviewUrl::App\("log\.html"\.into\(\)\)/);
assert.match(auxiliaryWindowSource, /\.inner_size\(600\.0, 335\.0\)/);
assert.match(logViewerSource, /get_log_snapshot/);
assert.match(logViewerSource, /save_log_records/);
assert.match(commandSource, /ADVANCED_HELP_URL/);

console.log("Preference pane contracts pass");
