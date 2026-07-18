import {
  initializeLocalization,
  tr,
  trFormat,
  trKey,
  trKeyFormat,
} from "./localization.js";
import {
  IINA_DEFAULT_INPUT_CONF,
  activeKeyMappingsLastWins,
  generateInputConf as generateKeyMappingInputConf,
  keyMappingConflictState,
  keyMappingPreferenceRows,
  keyMappingSignature as normalizedKeyMappingSignature,
  mpvKeyFromParts,
  normalizeKeyMapping as normalizeKeyMappingModel,
  normalizeModifiers,
  normalizeMpvKey,
  parseInputConf as parseKeyMappingInputConf,
  removeShadowedKeyMappings,
  serializeKeyMapping,
} from "./key-mapping.js";
import {
  isModifierOnlyKeyboardEvent,
  macOSModifierSymbols,
  macOSReadableKey,
  macOSReadableKeyboardEvent,
  macOSReadableSavedFilterShortcut,
  mpvKeyTokenFromKeyboardEvent,
} from "./key-display.js";
import {
  DEFAULT_DOUBLE_CLICK_INTERVAL_MS,
  MouseClickAction,
  dispatchMouseClickAction,
  normalizeMouseClickAction,
} from "./mouse-actions.js";
import { FirstMouseGate } from "./first-mouse.js";
import { reconcileOscToolbarLayout } from "./osc-toolbar-layout.js";
import { iinaTimelineSeekPlan } from "./timeline-seek.js";
import {
  IinaScrollGestureState,
  NSEventPhase,
  PinchAction,
  ScrollAction,
  exceedsWindowDragThreshold,
  iinaScrollAmount,
  normalizePinchAction,
  normalizeScrollAction,
  phaseContains,
} from "./input-gestures.js";
import {
  IINA_DEFAULT_OSC_TOOLBAR_BUTTONS,
  IINA_OSC_TOOLBAR_BUTTONS,
  PREFERENCE_PANES,
  advancedUserOptionsWithAdded,
  advancedUserOptionsWithEdit,
  advancedUserOptionsWithRemoved,
  buildIinaWindowGeometry,
  normalizePreferenceNumber,
  normalizeAdvancedUserOptions,
  normalizeIinaOscToolbarButtons,
  parseIinaWindowGeometry,
  preferenceAudioDeviceOptions,
  preferenceColorInputState,
  preferenceColorValue,
  preferenceControlEnabledForValues,
  preferenceDisclosureChildren,
  preferenceTopLevelControls,
} from "./preference-panes.js";
import {
  buildPreferenceSearchEntries,
  filterPreferenceSearchEntries,
  nextPreferenceSearchIndex,
  normalizePreferenceSearchQuery,
  normalizePreferenceSearchTerm,
  preferenceSearchTargetKeys,
} from "./preference-search.js";
import {
  expandPreferenceCollapseForSearch,
  setPreferenceCollapseOpen,
  togglePreferenceCollapse,
} from "./preference-collapse.js";
import {
  appendUniqueLanguageTokens,
  iinaLanguageTokenCompletions,
  languageTokenFromEditingString,
  languageTokensFromCsv,
  loadIinaLanguageCatalog,
  nextLanguageCompletionIndex,
  serializeLanguageTokens,
} from "./language-token-field.js";
import {
  PLAYLIST_DROP_REVEAL_DELAY_MS,
  normalizePlaylistIndexes,
  playlistContextMenuModel,
  playlistContextTargetIndexes,
  playlistDropPathMayBePlayable,
  playlistDropShouldReveal,
  playlistDropTargets,
  playlistPasteDestination,
  playlistRowInsertionIndex,
  playlistSelectionAfterAction,
  reorderedPlaylistItems,
} from "./playlist-actions.js";
import {
  playlistDurationSummary,
  playlistMetadata,
  playlistProgressFraction,
} from "./playlist-presentation.js";
import {
  quickSettingsTrackRows,
  subtitleTextStyleAvailable,
  subtitleTrackSections,
  trackStatusBadgesForQuickSettings,
} from "./quick-settings.js";
import {
  cancelOnlineSubtitleSelection,
  planOnlineSubtitleSearchResult,
  selectOnlineSubtitleCandidate,
} from "./online-subtitle-flow.js";
import { xmlRpcDecodeResponse, xmlRpcEncodeCall } from "./plugin-xmlrpc.js";
import { invokePluginMpvHook } from "./plugin-mpv-hooks.js";
import { decodePluginMpvValue, encodePluginMpvValue } from "./plugin-mpv-values.js";
import {
  defaultPluginRepositoryRows,
  pluginReorderFinalIndex,
  retainedPluginPreferenceSelection,
} from "./plugin-preferences.js";
import { createWebKitPluginRealm } from "./plugin-realm.js";
import {
  createPluginSyncTransport,
  pluginFileHandleReadValue,
  pluginPathForApi,
  pluginWebSocketHandlerValue,
  pluginWebSocketPort,
  withPluginFileSystemPermission,
} from "./plugin-sync.js";
import {
  consumePluginMpvEventBatch,
  normalizePluginEventName,
  pluginChangedMpvProperty,
} from "./plugin-events.js";
import { isMacOSHost } from "./platform.js";

const runningOnMacOS = isMacOSHost({
  userAgentDataPlatform: navigator.userAgentData?.platform,
  platform: navigator.platform,
  userAgent: navigator.userAgent,
});
if (runningOnMacOS) document.documentElement.classList.add("platform-macos");

await initializeLocalization();

const tauriInvoke = window.__TAURI__?.core?.invoke;
const tauriConvertFileSrc = window.__TAURI__?.core?.convertFileSrc;
const tauriListen = window.__TAURI__?.event?.listen;
const tauriEmitTo = window.__TAURI__?.event?.emitTo;
const windowQuery = new URLSearchParams(window.location.search);
const isMiniPlayerWindow = windowQuery.has("mini-player");
const auxiliaryWindowRole = windowQuery.get("window-role");
const AUXILIARY_WINDOW_ROLES = new Set(["open-url", "video-filter", "audio-filter", "preferences"]);
const isAuxiliaryWindow = AUXILIARY_WINDOW_ROLES.has(auxiliaryWindowRole);
const isOpenUrlAuxiliaryWindow = auxiliaryWindowRole === "open-url";
const isFilterAuxiliaryWindow = auxiliaryWindowRole === "video-filter" || auxiliaryWindowRole === "audio-filter";
const isPreferencesAuxiliaryWindow = auxiliaryWindowRole === "preferences";
const isPrimaryPlayerWindow = !isAuxiliaryWindow && !isMiniPlayerWindow && !windowQuery.has("player-session");
const managedPluginIdentifier = windowQuery.get("plugin-managed");
const managedPluginInstanceId = Number(windowQuery.get("plugin-instance"));
const managedPluginUserLabel = windowQuery.has("plugin-user-label")
  ? windowQuery.get("plugin-user-label")
  : null;
const managedPluginEnablesAll = windowQuery.get("plugin-enable-all") === "1";
const managedPluginDisablesUi = windowQuery.get("plugin-disable-ui") === "1";
const isManagedPluginPlayerWindow = Boolean(managedPluginIdentifier)
  && Number.isSafeInteger(managedPluginInstanceId)
  && managedPluginInstanceId > 0;
document.documentElement.classList.toggle("plugin-managed-ui-disabled", isManagedPluginPlayerWindow && managedPluginDisablesUi);
const PLUGIN_INPUT_PRIORITY_LOW = 100;
const PLUGIN_INPUT_PRIORITY_HIGH = 200;
const PLUGIN_INPUT_MOUSE = "*mouse";
const PLUGIN_INPUT_RIGHT_MOUSE = "*rightMouse";
const PLUGIN_INPUT_OTHER_MOUSE = "*otherMouse";
const OSC_SPEED_VALUES = [0.03125, 0.0625, 0.125, 0.25, 0.5, 1, 2, 4, 8, 16, 32];
const OSC_NORMAL_SPEED_INDEX = OSC_SPEED_VALUES.indexOf(1);
const TIME_PRECISION_KEYS = ["1s", "100ms", "10ms", "1ms"];
const TIME_PRECISION_LABELS = ["1 second", "100 milliseconds", "10 milliseconds", "1 millisecond"];
const IINA_PRIVATE_COMMANDS = new Set([
  "toggle-pip",
  "open-file",
  "open-url",
  "audio-panel",
  "video-panel",
  "sub-panel",
  "playlist-panel",
  "chapter-panel",
  "toggle-music-mode",
  "toggle-flip",
  "toggle-mirror",
  "bigger-window",
  "smaller-window",
  "fit-to-screen",
  "save-playlist",
  "delete-current-file",
  "delete-current-file-hard",
  "find-online-subs",
  "save-downloaded-sub",
]);
const IINA_PRIVATE_COMMAND_LABELS = {
  "toggle-pip": "Toggle Picture-in-Picture",
  "open-file": "Open File",
  "open-url": "Open URL",
  "audio-panel": "Show Audio Panel",
  "video-panel": "Show Video Panel",
  "sub-panel": "Show Subtitle Panel",
  "playlist-panel": "Show Playlist",
  "chapter-panel": "Show Chapters",
  "toggle-music-mode": "Toggle Music Mode",
  "toggle-flip": "Toggle Flip",
  "toggle-mirror": "Toggle Mirror",
  "bigger-window": "Bigger Window",
  "smaller-window": "Smaller Window",
  "fit-to-screen": "Fit to Screen",
  "save-playlist": "Save Current Playlist",
  "delete-current-file": "Delete Current File",
  "delete-current-file-hard": "Delete Current File Permanently",
  "find-online-subs": "Find Online Subtitles",
  "save-downloaded-sub": "Save Downloaded Subtitle",
};
const MINI_PLAYER_CONTROL_HEIGHT = 72;
const MINI_PLAYER_PLAYLIST_HEIGHT = 300;
const MINI_PLAYER_AUTO_HIDE_PLAYLIST_HEIGHT = 200;
const PLUGIN_SHIFTED_KEY_CHARS = new Set(["~", "!", "@", "#", "$", "%", "^", "&", "*", "(", ")", "_", "+", "{", "}", "|", ":", "\"", "<", ">", "?"]);
const PLUGIN_SHIFTED_KEY_MAP = {
  "-": "_",
  "=": "PLUS",
  "1": "!",
  "2": "@",
  "3": "SHARP",
  "4": "$",
  "5": "%",
  "6": "^",
  "7": "&",
  "8": "*",
  "9": "(",
  "0": ")",
  "[": "{",
  "]": "}",
  "\\": "|",
  ";": ":",
  "'": "\"",
  ",": "<",
  ".": ">",
  "/": "?",
  "`": "~",
};

const FILTER_PRESETS = {
  video: [
    {
      id: "crop",
      label: "Crop",
      params: [
        { name: "w", label: "Width (default: video width)", type: "text", defaultValue: "" },
        { name: "h", label: "Height (default: video height)", type: "text", defaultValue: "" },
        { name: "x", label: "Origin X (default: center)", type: "text", defaultValue: "" },
        { name: "y", label: "Origin Y (default: center)", type: "text", defaultValue: "" },
      ],
    },
    {
      id: "expand",
      label: "Expand",
      params: [
        { name: "w", label: "Width (default: video width)", type: "text", defaultValue: "" },
        { name: "h", label: "Height (default: video height)", type: "text", defaultValue: "" },
        { name: "x", label: "Origin X (default: center)", type: "text", defaultValue: "" },
        { name: "y", label: "Origin Y (default: center)", type: "text", defaultValue: "" },
        { name: "aspect", label: "Expands to fit an aspect (e.g: 4/3)", type: "text", defaultValue: "0" },
        { name: "round", label: "Rounds up to make divisible by this value", type: "text", defaultValue: "1" },
      ],
    },
    {
      id: "sharpen",
      label: "Sharpen",
      params: [
        { name: "msize", label: "Matrix size", type: "int", min: 3, max: 13, step: 2, defaultValue: 5 },
        { name: "amount", label: "Amount", type: "float", min: 0, max: 1.5, defaultValue: 0 },
      ],
    },
    {
      id: "blur",
      label: "Blur",
      params: [
        { name: "msize", label: "Matrix size", type: "int", min: 3, max: 13, step: 2, defaultValue: 5 },
        { name: "amount", label: "Amount", type: "float", min: 0, max: 1.5, defaultValue: 0 },
      ],
    },
    {
      id: "delogo",
      label: "Delogo",
      params: [
        { name: "x", label: "Origin X", type: "text", defaultValue: "1" },
        { name: "y", label: "Origin Y", type: "text", defaultValue: "1" },
        { name: "w", label: "Width", type: "text", defaultValue: "1" },
        { name: "h", label: "Height", type: "text", defaultValue: "1" },
      ],
    },
    { id: "negative", label: "Negative", params: [] },
    { id: "vflip", label: "Flip", params: [] },
    { id: "hflip", label: "Mirror", params: [] },
    {
      id: "lut3d",
      label: "3D LUT",
      params: [
        { name: "file", label: "File path (you can also drag & drop the file into the input box)", type: "text", defaultValue: "" },
        { name: "interp", label: "Interpolation mode", type: "choose", choices: ["nearest", "trilinear", "tetrahedral"], defaultValue: "nearest" },
      ],
    },
    {
      id: "custom_mpv",
      label: "Custom (mpv)",
      params: [
        { name: "name", label: "Filter name", type: "text", defaultValue: "" },
        { name: "string", label: "Filter value", type: "text", defaultValue: "" },
      ],
    },
    {
      id: "custom_ffmpeg",
      label: "Custom (FFmpeg)",
      params: [
        { name: "name", label: "Filter name", type: "text", defaultValue: "" },
        { name: "string", label: "Filter value", type: "text", defaultValue: "" },
      ],
    },
  ],
  audio: [
    {
      id: "custom_mpv",
      label: "Custom (mpv)",
      params: [
        { name: "name", label: "Filter name", type: "text", defaultValue: "" },
        { name: "string", label: "Filter value", type: "text", defaultValue: "" },
      ],
    },
    {
      id: "custom_ffmpeg",
      label: "Custom (FFmpeg)",
      params: [
        { name: "name", label: "Filter name", type: "text", defaultValue: "" },
        { name: "string", label: "Filter value", type: "text", defaultValue: "" },
      ],
    },
  ],
};

const mockState = {
  mode: "initial",
  current_url: null,
  file_loading: false,
  playback_error: null,
  media_title: "IINA",
  music_title: "IINA",
  music_album: "",
  music_artist: "",
  media_info: null,
  duration_seconds: 0,
  position_seconds: 0,
  volume: 100,
  speed: 1,
  muted: false,
  paused: true,
  loop_mode: "off",
  ab_loop: { a_seconds: 0, b_seconds: 0, count: "inf", status: "cleared" },
  audio_devices: [{ name: "auto", description: "Autoselect device" }],
  audio_device: "auto",
  video_filters: [],
  audio_filters: [],
  playlist: [],
  playlist_cache: { items: [], total_duration_seconds: null },
  recent_documents: [],
  last_playback: null,
  chapters: [],
  tracks: {
    video: [{ id: 1, title: "Default Video Track", selected: true }],
    audio: [{ id: 1, title: "Default Audio Track", selected: true }],
    subtitles: [{ id: 0, title: "None", selected: true }],
  },
  second_subtitle_id: 0,
  sidebar: { visible: false, tab: "playlist" },
  quick_settings: {
    deinterlace: false,
    hardware_decoding: true,
    hdr_available: false,
    hdr_enabled: true,
    audio_eq: Array(10).fill(0),
    audio_eq_active: false,
    sub_text_color: "1/1/1/1",
    sub_text_size: 55,
    sub_border_color: "0/0/0/1",
    sub_border_size: 3,
    sub_background_color: "1/1/1/0",
    sub_font: "sans-serif",
    video_aspect: "Default",
    video_crop: "None",
    custom_crop: null,
    video_rotate: 0,
    video_flipped: false,
    video_mirrored: false,
    brightness: 0,
    contrast: 0,
    saturation: 0,
    gamma: 0,
    hue: 0,
    audio_delay: 0,
    sub_delay: 0,
    sub_scale: 1,
    sub_pos: 100,
  },
  osc_visible: true,
  pip_active: false,
  osd_message: null,
  mpv_properties: {
    path: null,
    "media-title": "IINA",
    duration: 0,
    "time-pos": 0,
    "percent-pos": 0,
    pause: true,
    volume: 100,
    speed: 1,
    mute: false,
    chapter: 0,
    chapters: 0,
    "playlist-count": 0,
    "playlist-pos": -1,
    "track-list/count": 3,
    vid: 1,
    aid: 1,
    sid: 0,
    "secondary-sid": 0,
    "idle-active": true,
  },
};

const mockPreferences = {
  values: {
    actionAfterLaunch: 0,
    alwaysOpenInNewWindow: true,
    enableCmdN: false,
    recordPlaybackHistory: true,
    recordRecentFiles: true,
    trackAllFilesInRecentOpenMenu: true,
    receiveBetaUpdate: false,
    updaterAutomaticallyChecks: false,
    updaterCheckInterval: 86400,
    suppressCannotPreventDisplaySleep: false,
    quitWhenNoOpenedWindow: false,
    themeMaterial: 0,
    softVolume: 100,
    maxVolume: 100,
    pauseWhenOpen: false,
    fullScreenWhenOpen: false,
    keepOpenOnFileEnd: true,
    resumeLastPosition: true,
    useLegacyFullScreen: false,
    legacyFullScreenAnimation: false,
    blackOutMonitor: false,
    pauseWhenMinimized: false,
    pauseWhenInactive: false,
    playWhenEnteringFullScreen: false,
    pauseWhenLeavingFullScreen: false,
    pauseWhenGoesToSleep: true,
    usePhysicalResolution: true,
    initialWindowSizePosition: "",
    resizeWindowTiming: 1,
    resizeWindowOption: 2,
    alwaysFloatOnTop: false,
    alwaysShowOnTopIcon: false,
    oscPosition: 0,
    controlBarToolbarButtons: [...IINA_DEFAULT_OSC_TOOLBAR_BUTTONS],
    controlBarPositionHorizontal: 0.5,
    controlBarPositionVertical: 0.1,
    controlBarAutoHideTimeout: 2.5,
    controlBarStickToCenter: true,
    showChapterPos: false,
    arrowBtnAction: 0,
    showRemainingTime: false,
    timeDisplayPrecision: 0,
    touchbarShowRemainingTime: true,
    enableOSD: true,
    osdAutoHideTimeout: 1,
    osdTextSize: 20,
    displayTimeAndBatteryInFullScreen: false,
    playlistWidth: 270,
    prefetchPlaylistVideoDuration: true,
    playlistAutoAdd: true,
    playlistAutoPlayNext: true,
    playlistShowMetadata: true,
    playlistShowMetadataInMusicMode: true,
    autoSwitchToMusicMode: true,
    musicModeShowPlaylist: false,
    musicModeShowAlbumArt: true,
    enableThumbnailPreview: true,
    maxThumbnailPreviewCacheSize: 500,
    windowBehaviorWhenPip: 0,
    pauseWhenPip: false,
    togglePipByMinimizingWindow: false,
    enableThumbnailForRemoteFiles: false,
    thumbnailWidth: 240,
    videoThreads: 0,
    hardwareDecoder: 1,
    forceDedicatedGPU: false,
    loadIccProfile: true,
    enableHdrSupport: true,
    enableToneMapping: false,
    toneMappingTargetPeak: 0,
    toneMappingAlgorithm: 0,
    audioThreads: 0,
    audioLanguage: "",
    spdifAC3: false,
    spdifDTS: false,
    spdifDTSHD: false,
    enableInitialVolume: false,
    initialVolume: 100,
    audioDevice: "auto",
    audioDeviceDesc: "Autoselect device",
    subAutoLoadIINA: 2,
    subAutoLoadPriorityString: "",
    subAutoLoadSearchPath: "./*",
    ignoreAssStyles: false,
    subOverrideLevel: 2,
    subTextFont: "sans-serif",
    subTextSize: 55,
    subTextColor: "1/1/1/1",
    subBgColor: "0/0/0/0",
    subBold: false,
    subItalic: false,
    subBlur: 0,
    subSpacing: 0,
    subBorderSize: 3,
    subBorderColor: "0/0/0/1",
    subShadowSize: 0,
    subShadowColor: "0/0/0/0",
    subAlignX: 1,
    subAlignY: 2,
    subMarginX: 25,
    subMarginY: 22,
    subPos: 100,
    displayInLetterBox: true,
    subScaleWithWindow: true,
    onlineSubProvider: ":opensubtitles",
    onlineSubSource: 1,
    openSubUsername: "",
    subLang: "",
    assrtToken: "",
    autoSearchOnlineSub: false,
    autoSearchThreshold: 20,
    defaultEncoding: "auto",
    enableCache: true,
    defaultCacheSize: 153600,
    secPrefech: 36000,
    userAgent: "",
    transportRTSPThrough: 1,
    httpProxy: "",
    ytdlEnabled: true,
    ytdlSearchPath: "",
    ytdlRawOptions: "",
    useExactSeek: 0,
    followGlobalSeekTypeWhenAdjustSlider: false,
    verticalScrollAction: 0,
    horizontalScrollAction: 1,
    singleClickAction: 3,
    doubleClickAction: 1,
    rightClickAction: 2,
    middleClickAction: 0,
    pinchAction: 0,
    forceTouchAction: 0,
    videoViewAcceptsFirstMouse: false,
    relativeSeekAmount: 3,
    volumeScrollAmount: 3,
    useMediaKeys: true,
    useAppleRemote: false,
    inputConfigs: {},
    currentInputConfigName: "IINA Default",
    displayKeyBindingRawValues: false,
    modeledKeyBindings: null,
    enableAdvancedSettings: false,
    useMpvOsd: false,
    enableLogging: false,
    logLevel: 1,
    userOptions: [],
    useUserDefinedConfDir: false,
    userDefinedConfDir: "~/.config/mpv/",
    iinaEnablePluginSystem: false,
    screenshotSaveToFile: true,
    screenshotCopyToClipboard: false,
    screenShotFolder: "~/Pictures/Screenshots",
    screenShotIncludeSubtitle: true,
    screenShotFormat: 0,
    screenShotTemplate: "%F-%n",
    screenshotShowPreview: true,
    savedVideoFilters: [],
    savedAudioFilters: [],
  },
};

const mockKeyBindingProfiles = new Map([
  [
    "IINA Default",
    {
      name: "IINA Default",
      fileName: "iina-default-input.conf",
      kind: "builtin",
      readOnly: true,
      path: null,
      contents: IINA_DEFAULT_INPUT_CONF,
    },
  ],
  [
    "mpv Default",
    {
      name: "mpv Default",
      fileName: "input.conf",
      kind: "builtin",
      readOnly: true,
      path: null,
      contents: "# mpv default bindings\nSPACE cycle pause\nq quit\n",
    },
  ],
  [
    "VLC Default",
    {
      name: "VLC Default",
      fileName: "vlc-default-input.conf",
      kind: "builtin",
      readOnly: true,
      path: null,
      contents: "# VLC-style bindings\nSPACE cycle pause\nMeta+RIGHT seek 10\nMeta+LEFT seek -10\n",
    },
  ],
  [
    "Movist Default",
    {
      name: "Movist Default",
      fileName: "movist-default-input.conf",
      kind: "builtin",
      readOnly: true,
      path: null,
      contents: "# Movist-style bindings\nSPACE cycle pause\nRIGHT seek 5\nLEFT seek -5\n",
    },
  ],
]);

function mockKeyBindingProfile(name) {
  const normalized = String(name || "").toLocaleLowerCase();
  return Array.from(mockKeyBindingProfiles.values())
    .find((profile) => profile.name.toLocaleLowerCase() === normalized) ?? null;
}

function mockKeyBindingProfileDescriptor(profile) {
  const { contents: _, ...descriptor } = profile;
  return structuredClone(descriptor);
}

function mockKeyBindingProfileDocument(profile) {
  return {
    profile: mockKeyBindingProfileDescriptor(profile),
    contents: profile.contents,
  };
}

function validateMockKeyBindingProfileName(name) {
  const value = String(name ?? "");
  const containsForbiddenCharacter = value.includes("/") || /[\\<>:"|?*\u0000-\u001f]/u.test(value);
  if (!value || value !== value.trim() || containsForbiddenCharacter || value === "." || value === "..") {
    throw new Error(`KEY_BINDING_PROFILE_INVALID_NAME: ${JSON.stringify(value)}`);
  }
  if (mockKeyBindingProfile(value)) {
    throw new Error(`KEY_BINDING_PROFILE_CONFLICT: a profile named ${JSON.stringify(value)} already exists`);
  }
  return value;
}

function createMockKeyBindingProfile(name, contents = "") {
  const validatedName = validateMockKeyBindingProfileName(name);
  const profile = {
    name: validatedName,
    fileName: `${validatedName}.conf`,
    kind: "user",
    readOnly: false,
    path: `/tmp/iima-config/input_conf/${validatedName}.conf`,
    contents: String(contents ?? ""),
  };
  mockKeyBindingProfiles.set(validatedName, profile);
  return profile;
}


let activePreferencePane = "general";
let preferenceSearchQuery = "";
let preferenceSearchCompletionIndex = -1;
let preferenceSearchHighlightTimer = 0;
let preferenceSearchHighlightGeneration = 0;
let preferenceLanguageCatalog = [];
let preferenceTokenSaveQueue = Promise.resolve();
const preferenceLanguageCatalogReady = loadIinaLanguageCatalog()
  .then((languages) => {
    preferenceLanguageCatalog = languages;
    return languages;
  })
  .catch(() => []);
let keyBindingFilterMode = "all";
let keyBindingProfiles = [];
let keyBindingProfilesLoading = false;
let keyBindingProfileBusy = false;
let keyBindingProfileError = "";
let keyBindingProfileLoadRequest = 0;
let keyBindingProfileSaveRevision = 0;
let keyBindingProfileSaveQueue = Promise.resolve();
let mockThumbnailCacheSizeBytes = 0;
let preferenceSheetResolver = null;
let advancedOptionSelectedIndex = -1;
let advancedOptionError = "";

async function invoke(command, args = {}) {
  if (tauriInvoke) return tauriInvoke(command, args);
  return mockInvoke(command, args);
}

async function mockInvoke(command, args) {
  if (command === "get_player_snapshot") return structuredClone(mockState);
  if (command === "complete_initial_launch") return "welcome-window";
  if (command === "refresh_player_menu") return null;
  if (command === "set_window_presentation_mode") return args.mode;
  if (command === "set_mini_player_layout") {
    const width = Math.max(300, window.innerWidth || 300);
    const aspect = Number(args.videoAspect) > 0 ? Number(args.videoAspect) : 1;
    const videoHeight = args.videoVisible ? Math.round(width / aspect) : 0;
    const playlistHeight = args.playlistVisible ? MINI_PLAYER_PLAYLIST_HEIGHT : 0;
    return {
      width,
      height: videoHeight + MINI_PLAYER_CONTROL_HEIGHT + playlistHeight,
      video_height: videoHeight,
      playlist_height: playlistHeight,
      playlist_visible: Boolean(args.playlistVisible),
    };
  }
  if (command === "get_preferences") return structuredClone(mockPreferences);
  if (command === "get_preference_snapshot") {
    return { revision: 0, preferences: structuredClone(mockPreferences) };
  }
  if (command === "list_key_binding_profiles") {
    return Array.from(mockKeyBindingProfiles.values()).map(mockKeyBindingProfileDescriptor);
  }
  if (command === "read_key_binding_profile") {
    const profile = mockKeyBindingProfile(args.name);
    if (!profile) throw new Error(`KEY_BINDING_PROFILE_NOT_FOUND: ${JSON.stringify(args.name)}`);
    return mockKeyBindingProfileDocument(profile);
  }
  if (command === "create_key_binding_profile") {
    return mockKeyBindingProfileDocument(createMockKeyBindingProfile(args.name));
  }
  if (command === "duplicate_key_binding_profile") {
    const source = mockKeyBindingProfile(args.sourceName);
    if (!source) throw new Error(`KEY_BINDING_PROFILE_NOT_FOUND: ${JSON.stringify(args.sourceName)}`);
    return mockKeyBindingProfileDocument(createMockKeyBindingProfile(args.newName, source.contents));
  }
  if (command === "save_key_binding_profile") {
    const profile = mockKeyBindingProfile(args.name);
    if (!profile) throw new Error(`KEY_BINDING_PROFILE_NOT_FOUND: ${JSON.stringify(args.name)}`);
    if (profile.readOnly) throw new Error(`KEY_BINDING_PROFILE_READ_ONLY: ${JSON.stringify(profile.name)}`);
    profile.contents = String(args.contents ?? "");
    return mockKeyBindingProfileDocument(profile);
  }
  if (command === "delete_key_binding_profile") {
    const profile = mockKeyBindingProfile(args.name);
    if (!profile) throw new Error(`KEY_BINDING_PROFILE_NOT_FOUND: ${JSON.stringify(args.name)}`);
    if (profile.readOnly) throw new Error(`KEY_BINDING_PROFILE_READ_ONLY: ${JSON.stringify(profile.name)}`);
    mockKeyBindingProfiles.delete(profile.name);
    return null;
  }
  if (command === "get_key_binding_profile_path" || command === "reveal_key_binding_profile") {
    const profile = mockKeyBindingProfile(args.name);
    if (!profile) throw new Error(`KEY_BINDING_PROFILE_NOT_FOUND: ${JSON.stringify(args.name)}`);
    if (profile.readOnly || !profile.path) {
      throw new Error(`KEY_BINDING_PROFILE_READ_ONLY: ${JSON.stringify(profile.name)}`);
    }
    return profile.path;
  }
  if (command === "get_updater_status" || command === "check_for_updates") {
    const receiveBeta = Boolean(mockPreferences.values.receiveBetaUpdate);
    return {
      available: false,
      can_check_for_updates: false,
      automatically_checks_for_updates: Boolean(mockPreferences.values.updaterAutomaticallyChecks),
      update_check_interval: Number(mockPreferences.values.updaterCheckInterval),
      receive_beta_updates: receiveBeta,
      feed_url: receiveBeta
        ? "https://www.iina.io/appcast-beta.xml"
        : "https://www.iina.io/appcast.xml",
      framework_version: "",
      error: "Sparkle is available only in the packaged macOS app",
    };
  }
  if (command === "set_preference") {
    mockPreferences.values[args.change.key] = args.change.value;
    if (args.change.key === "recordRecentFiles" && args.change.value === false) {
      mockPreferences.values.recentDocuments = [];
      mockState.recent_documents = [];
    }
    return structuredClone(mockPreferences);
  }
  if (command === "login_opensubtitles_account") {
    mockPreferences.values.openSubUsername = String(args.username || "").trim();
    return structuredClone(mockPreferences);
  }
  if (command === "logout_opensubtitles_account") {
    mockPreferences.values.openSubUsername = "";
    return structuredClone(mockPreferences);
  }
  if (command === "set_default_application") {
    const successCount = Number(Boolean(args.video)) * 42
      + Number(Boolean(args.audio)) * 29
      + Number(Boolean(args.playlist)) * 8;
    return { success_count: successCount, failed_count: 0 };
  }
  if (command === "restore_suppressed_alerts") {
    mockPreferences.values.suppressCannotPreventDisplaySleep = false;
    return structuredClone(mockPreferences);
  }
  if (command === "clear_saved_playback_progress") return null;
  if (command === "clear_playback_history") {
    mockState.recent_documents = [];
    mockState.last_playback = null;
    return structuredClone(mockState);
  }
  if (command === "clear_recent_documents") {
    mockState.recent_documents = [];
    return structuredClone(mockState);
  }
  if (command === "open_browser_extension") return null;
  if (command === "choose_screenshot_folder") {
    return "/tmp/iima-screenshots";
  }
  if (command === "choose_advanced_config_directory") return "/tmp/iima-mpv-config";
  if (command === "open_log_directory") return "/tmp/iima-logs";
  if (command === "show_log_viewer") return "/tmp/iima-logs/iina.log";
  if (command === "open_advanced_help") {
    return "https://github.com/iina/iina/wiki/MPV-Options-and-Properties";
  }
  if (command === "export_key_bindings_config") {
    return `/tmp/${args.filename || "input.conf"}`;
  }
  if (command === "open_media") {
    const path = args.path ?? "Untitled Media";
    return mockOpenMediaPaths([path]);
  }
  if (command === "open_media_paths" || command === "open_dropped_media_paths") {
    return mockOpenMediaPaths(args.paths ?? []);
  }
  if (command === "open_media_dialog") {
    return null;
  }
  if (command === "open_media_dialog_new_window") {
    return null;
  }
  if (command === "open_media_in_new_window") {
    return "player-mock";
  }
  if (command === "enqueue_media_paths") {
    return mockEnqueueMediaPaths(args.paths ?? []);
  }
  if (command === "enqueue_media_dialog") {
    return null;
  }
  if (command === "load_external_track_dialog") {
    if (!mockState.current_url) return null;
    const kind = args.kind === "audio" ? "audio" : "subtitles";
    const extension = kind === "audio" ? "flac" : "srt";
    applyMockCommand({
      type: "load-external-track",
      kind,
      path: `/tmp/external-${kind}.${extension}`,
    });
    return structuredClone(mockState);
  }
  if (command === "choose_subtitle_font_dialog") {
    applyMockCommand({ type: "set-subtitle-font", font: "Helvetica Neue" });
    return structuredClone(mockState);
  }
  if (command === "toggle_music_mode") {
    mockState.mode = mockState.mode === "mini-player" ? (mockState.current_url ? "player" : "initial") : "mini-player";
    mockState.osd_message = mockState.mode === "mini-player" ? "Music Mode" : null;
    syncMockMpvProperties();
    return structuredClone(mockState);
  }
  if (command === "close_mini_player") {
    applyMockCommand({ type: "stop" });
    return structuredClone(mockState);
  }
  if (command === "toggle_picture_in_picture") {
    mockState.pip_active = !mockState.pip_active;
    mockState.osd_message = mockState.pip_active ? "Picture in Picture" : "Picture in Picture Closed";
    return structuredClone(mockState);
  }
  if (command === "search_online_subtitles") {
    if (!mockState.current_url) throw new Error("No media is open");
    const providerId = String(args.providerId || getPreferenceValue("onlineSubProvider") || ":opensubtitles");
    const providerName = {
      ":opensubtitles": "opensubtitles.com",
      ":assrt": "assrt.net",
      ":shooter": "shooter.cn",
    }[providerId] || "opensubtitles.com";
    return {
      provider_id: providerId,
      provider_name: providerName,
      query: titleFromPath(mockState.current_url).replace(/\.[^.]+$/, ""),
      candidates: [
        {
          id: "online-subtitle-mock-1",
          name: "Sample English.srt",
          left: "en  Downloads 1  Rating 5.0",
          right: "2026-01-01",
        },
      ],
    };
  }
  if (command === "download_online_subtitles") {
    const selected = Array.from(args.candidates || []);
    if (!selected.length) throw new Error("Select at least one subtitle");
    for (const [index] of selected.entries()) {
      applyMockCommand({
        type: "load-external-track",
        kind: "subtitles",
        path: `/tmp/iima-online-subtitles/[${index + 1}]-Sample-English.srt`,
      });
    }
    mockState.osd_message = `Downloaded ${selected.length} subtitle file${selected.length === 1 ? "" : "s"}`;
    return {
      player: structuredClone(mockState),
      downloaded_paths: selected.map((_, index) => `/tmp/iima-online-subtitles/[${index + 1}]-Sample-English.srt`),
    };
  }
  if (command === "get_plugins") return [];
  if (command === "get_plugin_page_contents") return { preference_html: null, help_html: null, help_url: null };
  if (command === "get_plugin_runtime_specs") return [];
  if (command === "install_plugin_dialog") return null;
  if (command === "install_plugin_from_github") {
    const source = String(args.source || "").trim();
    if (!source) throw new Error("Plugin source is required");
    return {
      status: "permission-confirmation",
      confirmation: {
        token: "plugin-permission-mock",
        plugin: {
          name: "GitHub Plugin",
          identifier: "io.iina.github",
          version: "1.0.0",
          permissions: [{ id: "network-request", dangerous: true }],
          allowed_domains: ["example.com"],
        },
        permissions: [{ id: "network-request", dangerous: true }],
        only_added: false,
      },
    };
  }
  if (command === "confirm_plugin_permissions") {
    return {
      status: "installed",
      record: { name: "GitHub Plugin", identifier: "io.iina.github", version: "1.0.0", enabled: true },
    };
  }
  if (command === "cancel_plugin_permissions") return true;
  if (command === "confirm_plugin_reinstall") {
    return { name: "GitHub Plugin", identifier: "io.iina.github", version: "1.0.0" };
  }
  if (command === "cancel_plugin_reinstall") return true;
  if (command === "check_plugin_github_update") return null;
  if (command === "update_plugin_from_github") {
    return {
      status: "installed",
      record: { identifier: args.identifier, version: "1.0.1", enabled: true },
    };
  }
  if (command === "set_plugin_enabled" || command === "reorder_plugin" || command === "remove_plugin") return [];
  if (command === "reveal_plugin_in_finder") return null;
  if (command === "set_plugin_menu_items") return null;
  if (command === "plugin_http_request") return { status_code: 200, reason: "", data: {}, text: "{}" };
  if (command === "plugin_http_download") return { destination: args.destination };
  if (command === "plugin_file_exists") return false;
  if (command === "plugin_file_list") return [];
  if (command === "plugin_file_read") return "";
  if (command === "plugin_file_write" || command === "plugin_file_delete" || command === "plugin_file_move" || command === "plugin_file_show_in_finder") return null;
  if (command === "plugin_websocket_create_server") {
    return { generation: Date.now(), port: Number(args.port), host: "127.0.0.1" };
  }
  if (command === "plugin_websocket_start_server" || command === "plugin_websocket_stop") return true;
  if (command === "plugin_websocket_send_text") return "no_connection";
  if (command === "plugin_utils_file_in_path") return false;
  if (command === "plugin_utils_resolve_path") return String(args.path || "");
  if (command === "plugin_utils_exec") return { status: 0, stdout: "", stderr: "" };
  if (command === "plugin_utils_ask") return false;
  if (command === "plugin_utils_prompt" || command === "plugin_utils_choose_file") return null;
  if (command === "plugin_utils_open") return true;
  if (command === "plugin_mpv_command" || command === "plugin_mpv_set") return structuredClone(mockState);
  if (
    command === "plugin_mpv_add_hook"
    || command === "plugin_mpv_continue_hook"
    || command === "plugin_mpv_observe_property"
  ) return true;
  if (command === "plugin_mpv_remove_hooks") return 0;
  if (command === "save_downloaded_subtitle_dialog") return null;
  if (command === "playlist_play_next") {
    mockPlayPlaylistItemsNext(args.indexes ?? []);
    return structuredClone(mockState);
  }
  if (command === "playlist_remove_items") {
    for (const index of normalizePlaylistIndexes(args.indexes ?? [], mockState.playlist.length).reverse()) {
      applyMockCommand({ type: "remove-playlist-item", index });
    }
    return structuredClone(mockState);
  }
  if (command === "playlist_insert_items") {
    return mockInsertPlaylistItems(args.paths ?? [], args.destination ?? mockState.playlist.length);
  }
  if (command === "playlist_copy_items") return (args.indexes ?? []).length;
  if (command === "playlist_can_paste_filenames") return false;
  if (command === "playlist_paste_items" || command === "playlist_add_url_dialog") return null;
  if (command === "playlist_open_items_in_new_window") return "player-mock";
  if (command === "playlist_trash_items") {
    const indexes = normalizePlaylistIndexes(args.indexes ?? [], mockState.playlist.length)
      .filter((index) => !/^[^:/?#]+:(?:\/\/)?/.test(String(mockState.playlist[index]?.path || "")));
    for (const index of [...indexes].reverse()) applyMockCommand({ type: "remove-playlist-item", index });
    return { player: structuredClone(mockState), trashed_indexes: indexes, failures: [] };
  }
  if (command === "playlist_open_network_items" || command === "playlist_reveal_items") {
    return (args.indexes ?? []).length;
  }
  if (command === "playlist_copy_network_urls") return "";
  if (command === "execute_iina_command") {
    return mockExecuteIinaCommand(args.action);
  }
  if (command === "player_command") {
    applyMockCommand(args.command);
    return structuredClone(mockState);
  }
  if (command === "generate_media_thumbnails") {
    return {
      source_path: args.path,
      width: args.width ?? mockPreferences.values.thumbnailWidth,
      requested_count: args.count ?? 100,
      thumbnails: [],
      progress: 1,
      ready: true,
      cache_hit: false,
      cancelled: false,
    };
  }
  if (command === "cancel_media_thumbnails") return null;
  if (command === "get_thumbnail_cache_stats") return { size_bytes: mockThumbnailCacheSizeBytes };
  if (command === "clear_thumbnail_cache") {
    mockThumbnailCacheSizeBytes = 0;
    return { size_bytes: 0 };
  }
  if (command === "capture_current_screenshot") {
    if (!mockState.current_url) throw new Error("No media is open");
    return {
      source_path: mockState.current_url,
      time_seconds: Number(mockState.position_seconds) || 0,
      path: "./assets/iina/app-icon.png",
      format: "png",
      saved_to_file: true,
      copied_to_clipboard: false,
      show_preview: true,
    };
  }
  if (command === "reveal_screenshot_folder") {
    return "/tmp/iima-screenshots";
  }
  if (command === "reveal_screenshot_file" || command === "open_screenshot_file" || command === "delete_screenshot_file") {
    return null;
  }
  if (command === "get_player_window_status") {
    return {
      fullscreen: Boolean(mockState.window_fullscreen),
      alwaysOnTop: Boolean(mockState.window_always_on_top),
      batteryCapacity: null,
      batteryCharging: null,
    };
  }
  if (command === "toggle_window_fullscreen") {
    mockState.window_fullscreen = !Boolean(mockState.window_fullscreen);
    return mockState.window_fullscreen;
  }
  if (command === "set_window_fullscreen") {
    mockState.window_fullscreen = Boolean(args.fullscreen);
    return mockState.window_fullscreen;
  }
  if (command === "resize_player_window_by_magnification") {
    return { action: "magnify", changed: true, frame: { x: 0, y: 0, width: 900, height: 506.25 } };
  }
  if (command === "start_player_window_drag") return null;
  if (command === "toggle_window_always_on_top") {
    mockState.window_always_on_top = !Boolean(mockState.window_always_on_top);
    return mockState.window_always_on_top;
  }
  if (command === "get_mpv_observer_contract") {
    return [
      { name: "track-list", format: "none" },
      { name: "pause", format: "flag" },
      { name: "volume", format: "double" },
      { name: "idle-active", format: "flag" },
    ];
  }
  if (command === "get_libmpv_runtime_status") {
    const symbols = [
      "mpv_create",
      "mpv_initialize",
      "mpv_command",
      "mpv_observe_property",
      "mpv_wait_event",
      "mpv_render_context_create",
      "mpv_render_context_render",
      "mpv_render_context_report_swap",
    ].map((name) => ({ name, resolved: false }));
    return {
      available: false,
      path: null,
      load_error: "libmpv dylib was not found",
      missing_symbols: symbols.map((symbol) => symbol.name),
      symbols,
    };
  }
  if (command === "get_replication_catalog") {
    return { reference_branch: "release/1.3.5", reference_commit: "45187444" };
  }
  if (command === "get_native_video_renderer_status") {
    return { installed: false, attached: false, pip_available: true, pip_active: false, backend: "unavailable" };
  }
  return {};
}

function mockOpenMediaPaths(paths) {
  const path = paths[0] ?? "Untitled Media";
  const title = titleFromPath(path);
  const previewParams = new URLSearchParams(window.location.search);
  const previewDuration = Math.max(0, Number(previewParams.get("duration")) || 0);
  const previewPosition = Math.max(0, Math.min(previewDuration, Number(previewParams.get("position")) || 0));
  const previewAbA = Math.max(0, Math.min(previewDuration, Number(previewParams.get("abA")) || 0));
  const previewAbB = Math.max(0, Math.min(previewDuration, Number(previewParams.get("abB")) || 0));
  const previewAbLoop = {
    a_seconds: previewAbA,
    b_seconds: previewAbB,
    count: "inf",
    status: previewAbA > 0 ? (previewAbB > 0 ? "b-set" : "a-set") : "cleared",
  };
  const audioOnly = previewParams.get("audioOnly") === "1";
  const previewPlaylist = [path, ...previewParams.getAll("playlist").filter(Boolean)];
  const recentDocuments = [
    { id: 1, path, title },
    ...mockState.recent_documents.filter((item) => item.path !== path),
  ]
    .slice(0, 10)
    .map((item, index) => ({ ...item, id: index + 1 }));
  Object.assign(mockState, {
    mode: isMiniPlayerWindow ? "mini-player" : "player",
    current_url: path,
    file_loading: false,
    playback_error: null,
    media_title: title,
    music_title: previewParams.get("musicTitle") || title,
    music_album: previewParams.get("album") || "",
    music_artist: previewParams.get("artist") || "",
    media_info: {
      probe_status: "unavailable",
      probe_message: "Metadata unavailable",
      format: null,
      duration_seconds: previewDuration || null,
      bit_rate: null,
      video_summary: null,
      audio_summary: null,
      subtitle_count: 0,
    },
    duration_seconds: previewDuration,
    position_seconds: previewPosition,
    speed: 1,
    paused: false,
    ab_loop: previewAbLoop,
    second_subtitle_id: 0,
    tracks: {
      video: audioOnly
        ? [{ id: 0, title: "None", selected: true, metadata: {} }]
        : [{ id: 1, title: "Default Video Track", selected: true, metadata: { demux_width: 1920, demux_height: 1080 } }],
      audio: [{ id: 1, title: "Default Audio Track", selected: true, metadata: {} }],
      subtitles: [{ id: 0, title: "None", selected: true, metadata: {} }],
    },
    recent_documents: recentDocuments,
    last_playback: { path, title, position_seconds: 0 },
    playlist: previewPlaylist.map((itemPath, index) => ({
      id: index + 1,
      path: itemPath,
      title: titleFromPath(itemPath),
      duration_seconds: index === 0 && previewDuration > 0 ? previewDuration : null,
      current: index === 0,
      playing: index === 0,
    })),
    playlist_cache: {
      items: previewPlaylist.map((itemPath, index) => ({
        path: itemPath,
        ready: true,
        duration_seconds: index === 0 && previewDuration > 0 ? previewDuration : null,
        playback_progress_seconds: index === 0 ? previewPosition : null,
        metadata_title: index === 0 ? (previewParams.get("musicTitle") || null) : null,
        metadata_album: index === 0 ? (previewParams.get("album") || null) : null,
        metadata_artist: index === 0 ? (previewParams.get("artist") || null) : null,
      })),
      total_duration_seconds: previewDuration,
    },
    osd_message: `Opening ${title}`,
  });
  syncMockMpvProperties();
  return structuredClone(mockState);
}

function mockEnqueueMediaPaths(paths) {
  const playablePaths = paths.filter(Boolean);
  if (!playablePaths.length) return structuredClone(mockState);
  if (!mockState.playlist.length) return mockOpenMediaPaths(playablePaths);

  const firstId = mockState.playlist.length + 1;
  mockState.playlist.push(
    ...playablePaths.map((path, offset) => ({
      id: firstId + offset,
      path,
      title: titleFromPath(path),
      duration_seconds: null,
      current: false,
      playing: false,
    }))
  );
  mockState.osd_message = `Added ${playablePaths.length} Files to Playlist`;
  syncMockMpvProperties();
  return structuredClone(mockState);
}

function mockInsertPlaylistItems(paths, destination) {
  const playablePaths = paths.filter(Boolean);
  if (!playablePaths.length) return structuredClone(mockState);
  if (!mockState.playlist.length) return mockOpenMediaPaths(playablePaths);
  const index = Math.max(0, Math.min(mockState.playlist.length, Number(destination) || 0));
  const inserted = playablePaths.map((path) => ({
    id: 0,
    path,
    title: titleFromPath(path),
    duration_seconds: null,
    current: false,
    playing: false,
  }));
  mockState.playlist.splice(index, 0, ...inserted);
  mockState.playlist.forEach((item, itemIndex) => { item.id = itemIndex + 1; });
  mockState.osd_message = `Added ${playablePaths.length} Files to Playlist`;
  syncMockMpvProperties();
  return structuredClone(mockState);
}

function mockExecuteIinaCommand(rawAction) {
  const action = String(rawAction || "").trim();
  const execution = { command: action };
  const sidebar = {
    "audio-panel": "audio",
    "video-panel": "video",
    "sub-panel": "subtitles",
    "playlist-panel": "playlist",
    "chapter-panel": "chapters",
  }[action];
  if (sidebar) {
    applyMockCommand({ type: "show-sidebar", tab: sidebar });
    execution.player = structuredClone(mockState);
  } else if (action === "toggle-flip") {
    applyMockCommand({ type: "set-video-flip", enabled: !mockState.quick_settings.video_flipped });
    execution.player = structuredClone(mockState);
  } else if (action === "toggle-mirror") {
    applyMockCommand({ type: "set-video-mirror", enabled: !mockState.quick_settings.video_mirrored });
    execution.player = structuredClone(mockState);
  } else if (action === "toggle-pip") {
    mockState.pip_active = !mockState.pip_active;
    execution.player = structuredClone(mockState);
  } else if (action === "toggle-music-mode") {
    mockState.mode = mockState.mode === "mini-player" ? (mockState.current_url ? "player" : "initial") : "mini-player";
    execution.player = structuredClone(mockState);
  } else if (["half", "normal", "double", "fit-to-screen", "bigger-window", "smaller-window"].includes(action)) {
    execution.windowResize = { action, changed: false, frame: { x: 0, y: 0, width: window.innerWidth, height: window.innerHeight } };
  } else if (action === "save-playlist" || action === "save-downloaded-sub") {
    execution.savedPath = null;
  } else if (action === "delete-current-file" || action === "delete-current-file-hard") {
    const currentIndex = mockState.playlist.findIndex((item) => item.current);
    if (currentIndex >= 0) mockState.playlist.splice(currentIndex, 1);
    execution.player = structuredClone(mockState);
  } else if (action === "open-file") {
    execution.frontendAction = "open";
  } else if (action === "open-url") {
    execution.frontendAction = "open-url";
  } else if (action === "find-online-subs") {
    execution.frontendAction = "find-online-subtitles";
  } else {
    throw new Error(`Unknown IINA command: ${action}`);
  }
  return execution;
}

function mockFilters(kind) {
  return kind === "audio" ? mockState.audio_filters : mockState.video_filters;
}

function addMockFilter(kind, raw, displayName) {
  const filter = parseFilterRawForUi(raw);
  if (!filter) {
    mockState.osd_message = "Invalid Filter";
    return;
  }
  mockFilters(kind).push(filter);
  mockState.osd_message = `Added Filter: ${displayName}`;
}

function removeMockFilter(kind, index) {
  const filters = mockFilters(kind);
  if (!Number.isInteger(Number(index)) || index < 0 || index >= filters.length) return;
  filters.splice(index, 1);
  mockState.osd_message = "Removed Filter";
}

function mockPlayerCommandForKeyBinding(rawAction) {
  const parts = String(rawAction ?? "").trim().split(/\s+/).filter(Boolean);
  if (parts[0] === "{default}") parts.shift();
  const [verb, property, value] = parts;
  if (verb === "cycle" && property === "pause") return { type: "toggle-pause" };
  if (verb === "cycle" && property === "mute") return { type: "toggle-mute" };
  if (verb === "seek" && Number.isFinite(Number(property))) {
    return { type: "seek", seconds: (Number(mockState.position_seconds) || 0) + Number(property) };
  }
  if (verb === "add" && property === "volume" && Number.isFinite(Number(value))) {
    return { type: "set-volume", volume: (Number(mockState.volume) || 0) + Number(value) };
  }
  if (verb === "multiply" && property === "speed" && Number.isFinite(Number(value))) {
    return { type: "multiply-speed", factor: Number(value) };
  }
  if (verb === "set" && property === "speed" && Number.isFinite(Number(value))) {
    return { type: "set-speed", speed: Number(value) };
  }
  if (verb === "frame-step") return { type: "frame-step", backwards: false };
  if (verb === "frame-back-step") return { type: "frame-step", backwards: true };
  if (verb === "playlist-next") return { type: "playlist-next" };
  if (verb === "playlist-prev") return { type: "playlist-prev" };
  if (verb === "ab-loop") return { type: "cycle-ab-loop" };
  if (verb === "stop" || verb === "quit") return { type: "stop" };
  if (verb === "cycle" && ["video", "audio", "sub"].includes(property)) {
    return { type: "cycle-track", kind: property === "sub" ? "subtitles" : property };
  }
  return null;
}

function applyMockCommand(command) {
  if (command.type === "key-binding-mpv-command") {
    const mapped = mockPlayerCommandForKeyBinding(command.action);
    if (mapped) applyMockCommand(mapped);
    else mockState.osd_message = String(command.action || "Key Binding");
    return;
  }
  if (command.type === "toggle-pause") {
    mockState.paused = !mockState.paused;
    mockState.osd_message = mockState.paused ? "Paused" : "Playing";
  }
  if (command.type === "set-volume") {
    mockState.volume = Math.max(0, Math.min(200, command.volume));
    mockState.osd_message = `Volume ${Math.round(mockState.volume)}%`;
  }
  if (command.type === "set-speed") {
    mockState.speed = Math.max(0.01, Math.min(100, command.speed));
    mockState.osd_message = `Speed ${mockState.speed.toFixed(2)}x`;
  }
  if (command.type === "multiply-speed") {
    mockState.speed = Math.max(0.01, Math.min(100, mockState.speed * command.factor));
    mockState.osd_message = `Speed ${mockState.speed.toFixed(2)}x`;
  }
  if (command.type === "toggle-mute") {
    mockState.muted = !mockState.muted;
    mockState.osd_message = mockState.muted ? "Muted" : "Unmuted";
  }
  if (command.type === "toggle-file-loop") {
    mockState.loop_mode = mockState.loop_mode === "file" ? "off" : "file";
    mockState.osd_message = mockState.loop_mode === "file" ? "File Loop" : "Loop Off";
  }
  if (command.type === "toggle-playlist-loop") {
    mockState.loop_mode = mockState.loop_mode === "playlist" ? "off" : "playlist";
    mockState.osd_message = mockState.loop_mode === "playlist" ? "Playlist Loop" : "Loop Off";
  }
  if (command.type === "cycle-ab-loop") {
    if (mockState.ab_loop.status === "cleared") {
      mockState.ab_loop = {
        ...mockState.ab_loop,
        a_seconds: Math.max(0.000001, Number(mockState.position_seconds) || 0),
        b_seconds: 0,
        status: "a-set",
      };
      mockState.osd_message = "A-B Loop: A";
    } else if (mockState.ab_loop.status === "a-set") {
      mockState.ab_loop = {
        ...mockState.ab_loop,
        b_seconds: Math.max(0.000001, Number(mockState.position_seconds) || 0),
        status: "b-set",
      };
      mockState.osd_message = "A-B Loop: B";
    } else {
      mockState.ab_loop = { a_seconds: 0, b_seconds: 0, count: "inf", status: "cleared" };
      mockState.osd_message = "A-B Loop: Cleared";
    }
  }
  if (command.type === "set-ab-loop-point") {
    const seconds = Math.max(0.000001, Number(command.seconds) || 0);
    if (command.point === "a" && mockState.ab_loop.status !== "cleared") {
      mockState.ab_loop.a_seconds = seconds;
      mockState.osd_message = "A-B Loop: A";
    }
    if (command.point === "b" && mockState.ab_loop.status === "b-set") {
      mockState.ab_loop.b_seconds = seconds;
      mockState.osd_message = "A-B Loop: B";
    }
  }
  if (command.type === "select-audio-device") {
    const device = mockState.audio_devices.find((item) => item.name === command.name);
    if (device) {
      mockState.audio_device = device.name;
      mockState.osd_message = `Audio Device: ${device.description}`;
    }
  }
  if (command.type === "add-filter") {
    addMockFilter(command.kind, command.filter, command.filter);
  }
  if (command.type === "remove-filter") {
    removeMockFilter(command.kind, command.index);
  }
  if (command.type === "toggle-saved-filter") {
    const filters = mockFilters(command.kind);
    const index = filters.findIndex((filter) => filterMatchesRaw(filter, command.filter));
    if (index >= 0) removeMockFilter(command.kind, index);
    else addMockFilter(command.kind, command.filter, command.name);
  }
  if (command.type === "stop") {
    Object.assign(mockState, {
      mode: "initial",
      current_url: null,
      file_loading: false,
      playback_error: null,
      media_title: "IINA",
      music_title: "IINA",
      music_album: "",
      music_artist: "",
      media_info: null,
      duration_seconds: 0,
      position_seconds: 0,
      speed: 1,
      paused: true,
      ab_loop: { a_seconds: 0, b_seconds: 0, count: "inf", status: "cleared" },
      playlist: [],
      chapters: [],
      tracks: {
        video: [{ id: 1, title: "Default Video Track", selected: true }],
        audio: [{ id: 1, title: "Default Audio Track", selected: true }],
        subtitles: [{ id: 0, title: "None", selected: true }],
      },
      second_subtitle_id: 0,
      osd_message: "Stopped",
    });
  }
  if (command.type === "frame-step") {
    const duration = Number(mockState.duration_seconds) || 0;
    const upperBound = duration > 0 ? duration : Number.MAX_SAFE_INTEGER;
    const delta = command.backwards ? -1 / 30 : 1 / 30;
    mockState.position_seconds = Math.max(0, Math.min(upperBound, (Number(mockState.position_seconds) || 0) + delta));
    mockState.paused = true;
    mockState.osd_message = command.backwards ? "Frame Back Step" : "Frame Step";
  }
  if (command.type === "playlist-next" || command.type === "playlist-prev") {
    selectMockPlaylistItem(command.type === "playlist-next" ? 1 : -1);
  }
  if (command.type === "select-playlist-item") {
    selectMockPlaylistIndex(command.index);
  }
  if (command.type === "move-playlist-items") {
    moveMockPlaylistItems(command.indexes, command.destination);
  }
  if (command.type === "remove-playlist-item") {
    removeMockPlaylistIndex(command.index);
  }
  if (command.type === "clear-playlist") {
    clearMockPlaylist();
  }
  if (command.type === "cycle-track") {
    cycleMockTrack(command.kind);
    mockState.osd_message = "Track Switched";
  }
  if (command.type === "select-track") {
    if (selectMockTrack(command.kind, command.id)) {
      mockState.osd_message = "Track Switched";
    }
  }
  if (command.type === "swap-subtitle-tracks") {
    const primaryId = selectedMockTrackId(mockState.tracks.subtitles);
    const secondaryId = Number(mockState.second_subtitle_id) || 0;
    if (primaryId !== secondaryId && selectMockTrack("subtitles", secondaryId)) {
      mockState.second_subtitle_id = primaryId;
      mockState.osd_message = "Track Switched";
    }
  }
  if (command.type === "load-external-track") {
    const kind = command.kind === "audio" ? "audio" : "subtitles";
    const path = String(command.path || "").trim();
    if (path) {
      const tracks = mockState.tracks[kind];
      const existing = tracks.find((track) => track.metadata?.external_filename === path);
      if (kind === "subtitles" && existing) {
        mockState.osd_message = "Reloading External Subtitle";
      } else {
        const id = Math.max(0, ...tracks.map((track) => Number(track.id) || 0)) + 1;
        mockState.tracks[kind] = tracks.map((track) => ({ ...track, selected: false }));
        mockState.tracks[kind].push({
          id,
          title: titleFromPath(path),
          selected: true,
          metadata: { external: true, external_filename: path },
        });
        mockState.osd_message = kind === "audio" ? "Loading External Audio" : "Loading External Subtitle";
      }
    }
  }
  if (command.type === "set-deinterlace") {
    mockState.quick_settings.deinterlace = Boolean(command.enabled);
    mockState.osd_message = mockState.quick_settings.deinterlace ? "Deinterlace On" : "Deinterlace Off";
  }
  if (command.type === "set-hardware-decoding") {
    const decoder = Number(command.decoder);
    mockState.quick_settings.hardware_decoding = Boolean(command.enabled) && decoder !== 0;
    mockState.osd_message = mockState.quick_settings.hardware_decoding
      ? "Hardware Decoding On"
      : "Hardware Decoding Off";
  }
  if (command.type === "set-hdr-enabled") {
    mockState.quick_settings.hdr_enabled = Boolean(command.enabled);
  }
  if (command.type === "set-video-aspect") {
    const aspect = String(command.aspect || "").trim();
    mockState.quick_settings.video_aspect = /^\d+(?:\.\d+)?:\d+(?:\.\d+)?$/.test(aspect) ? aspect : "Default";
    mockState.osd_message = `Aspect Ratio: ${mockState.quick_settings.video_aspect}`;
  }
  if (command.type === "set-video-crop") {
    const crop = String(command.crop || "").trim();
    mockState.quick_settings.video_crop = /^\d+(?:\.\d+)?:\d+(?:\.\d+)?$/.test(crop) ? crop : "None";
    mockState.quick_settings.custom_crop = null;
    mockState.osd_message = `Crop: ${mockState.quick_settings.video_crop}`;
  }
  if (command.type === "set-custom-video-crop") {
    const x = Math.trunc(Number(command.x));
    const y = Math.trunc(Number(command.y));
    const width = Math.trunc(Number(command.width));
    const height = Math.trunc(Number(command.height));
    if ([x, y, width, height].every(Number.isFinite) && x >= 0 && y >= 0 && width > 0 && height > 0) {
      mockState.quick_settings.video_crop = "";
      mockState.quick_settings.custom_crop = { x, y, width, height };
      mockState.osd_message = `Crop: (${x}, ${y}) (${width}x${height})`;
    }
  }
  if (command.type === "set-delogo-region") {
    const x = Math.trunc(Number(command.x));
    const y = Math.trunc(Number(command.y));
    const width = Math.trunc(Number(command.width));
    const height = Math.trunc(Number(command.height));
    const dimensions = selectedVideoDimensions(mockState);
    if (
      dimensions
      && [x, y, width, height].every(Number.isFinite)
      && x >= 0
      && y >= 0
      && width > 0
      && height > 0
      && x + width <= dimensions.width
      && y + height <= dimensions.height
    ) {
      const existing = mockState.video_filters.findIndex((filter) => filter?.label === "iina_delogo");
      if (existing >= 0) removeMockFilter("video", existing);
      addMockFilter(
        "video",
        `@iina_delogo:lavfi=[delogo=x=${x}:y=${y}:w=${width}:h=${height}]`,
        "Delogo",
      );
    } else {
      mockState.osd_message = "Delogo unavailable for current video";
    }
  }
  if (command.type === "remove-delogo") {
    const existing = mockState.video_filters.findIndex((filter) => filter?.label === "iina_delogo");
    if (existing >= 0) removeMockFilter("video", existing);
  }
  if (command.type === "set-video-rotate" && [0, 90, 180, 270].includes(Number(command.degrees))) {
    mockState.quick_settings.video_rotate = Number(command.degrees);
    mockState.osd_message = `Rotate ${mockState.quick_settings.video_rotate}°`;
  }
  if (command.type === "set-video-flip") {
    mockState.quick_settings.video_flipped = Boolean(command.enabled);
    mockState.osd_message = mockState.quick_settings.video_flipped ? "Vertical Flip" : "Vertical Flip Off";
  }
  if (command.type === "set-video-mirror") {
    mockState.quick_settings.video_mirrored = Boolean(command.enabled);
    mockState.osd_message = mockState.quick_settings.video_mirrored ? "Horizontal Mirror" : "Horizontal Mirror Off";
  }
  if (command.type === "set-video-equalizer") {
    const option = String(command.option || "");
    if (["brightness", "contrast", "saturation", "gamma", "hue"].includes(option)) {
      const value = Math.max(-100, Math.min(100, Number(command.value) || 0));
      mockState.quick_settings[option] = value;
      mockState.osd_message = `${option[0].toUpperCase()}${option.slice(1)}: ${value >= 0 ? "+" : ""}${value}`;
    }
  }
  if (command.type === "set-audio-delay") {
    mockState.quick_settings.audio_delay = Number(command.seconds) || 0;
    mockState.osd_message = `Audio Delay: ${formatSignedNumber(mockState.quick_settings.audio_delay)}s`;
  }
  if (command.type === "set-audio-equalizer") {
    const gains = Array.from(command.gains || []).slice(0, 10).map((gain) => {
      const value = Number(gain);
      return Number.isFinite(value) ? Math.max(-12, Math.min(12, value)) : 0;
    });
    mockState.quick_settings.audio_eq = [...gains, ...Array(Math.max(0, 10 - gains.length)).fill(0)];
    mockState.quick_settings.audio_eq_active = true;
  }
  if (command.type === "reset-audio-equalizer") {
    mockState.quick_settings.audio_eq = Array(10).fill(0);
    mockState.quick_settings.audio_eq_active = false;
  }
  if (command.type === "set-subtitle-style-color") {
    const color = normalizeMpvColor(command.color);
    if (color) {
      const key = {
        text: "sub_text_color",
        border: "sub_border_color",
        background: "sub_background_color",
      }[command.target];
      if (key) mockState.quick_settings[key] = color;
    }
  }
  if (command.type === "set-subtitle-text-size" && [30, 35, 40, 45, 50, 55, 60, 65, 70].includes(Number(command.size))) {
    mockState.quick_settings.sub_text_size = Number(command.size);
  }
  if (command.type === "set-subtitle-border-size" && [0, 0.25, 0.5, 1, 1.5, 2, 2.5, 3, 4, 5].includes(Number(command.size))) {
    mockState.quick_settings.sub_border_size = Number(command.size);
  }
  if (command.type === "set-subtitle-font" && !String(command.font || "").includes("\0")) {
    mockState.quick_settings.sub_font = String(command.font || "");
  }
  if (command.type === "set-sub-delay") {
    mockState.quick_settings.sub_delay = Number(command.seconds) || 0;
    mockState.osd_message = `Subtitle Delay: ${formatSignedNumber(mockState.quick_settings.sub_delay)}s`;
  }
  if (command.type === "set-sub-scale") {
    mockState.quick_settings.sub_scale = Math.max(0.1, Math.min(10, Number(command.scale) || 1));
    mockState.osd_message = `Subtitle Scale: ${mockState.quick_settings.sub_scale.toFixed(2)}`;
  }
  if (command.type === "set-sub-position") {
    mockState.quick_settings.sub_pos = Math.max(0, Math.min(100, Math.round(Number(command.position) || 0)));
    mockState.osd_message = `Subtitle Position: ${mockState.quick_settings.sub_pos}`;
  }
  if (command.type === "show-sidebar") {
    mockState.sidebar = { visible: true, tab: command.tab };
  }
  if (command.type === "hide-sidebar") {
    mockState.sidebar.visible = false;
  }
  if (command.type === "toggle-osc") {
    mockState.osc_visible = !mockState.osc_visible;
    mockState.osd_message = null;
  }
  if (command.type === "seek") {
    const duration = Number(mockState.duration_seconds) || 0;
    const upperBound = duration > 0 ? duration : Number.MAX_SAFE_INTEGER;
    mockState.position_seconds = Math.max(0, Math.min(upperBound, command.seconds));
    if (mockState.current_url && mockState.last_playback?.path === mockState.current_url) {
      mockState.last_playback.position_seconds = mockState.position_seconds;
    }
    mockState.osd_message = `Seek ${Math.round(mockState.position_seconds)}s`;
  }
  if (command.type === "seek-relative") {
    const duration = Number(mockState.duration_seconds) || 0;
    const upperBound = duration > 0 ? duration : Number.MAX_SAFE_INTEGER;
    mockState.position_seconds = Math.max(
      0,
      Math.min(upperBound, (Number(mockState.position_seconds) || 0) + (Number(command.seconds) || 0)),
    );
    if (mockState.current_url && mockState.last_playback?.path === mockState.current_url) {
      mockState.last_playback.position_seconds = mockState.position_seconds;
    }
    mockState.osd_message = `Seek ${Math.round(mockState.position_seconds)}s`;
  }
  syncMockMpvProperties();
}

function syncMockMpvProperties() {
  const duration = Math.max(0, Number(mockState.duration_seconds) || 0);
  const timePos = Math.max(
    0,
    Math.min(duration > 0 ? duration : Number.MAX_SAFE_INTEGER, Number(mockState.position_seconds) || 0)
  );
  const playlistPos = mockState.playlist.findIndex((item) => item.current);
  mockState.mpv_properties = {
    path: mockState.current_url,
    "media-title": mockState.media_title,
    duration,
    "time-pos": timePos,
    "percent-pos": duration > 0 ? Math.max(0, Math.min(100, (timePos / duration) * 100)) : 0,
    pause: mockState.paused,
    volume: mockState.volume,
    speed: mockState.speed,
    mute: mockState.muted,
    "ab-loop-a": Number(mockState.ab_loop?.a_seconds) || 0,
    "ab-loop-b": Number(mockState.ab_loop?.b_seconds) || 0,
    "ab-loop-count": mockState.ab_loop?.count || "inf",
    "audio-device": mockState.audio_device,
    chapter: currentMockChapterIndex(timePos),
    chapters: mockState.chapters.length,
    "playlist-count": mockState.playlist.length,
    "playlist-pos": playlistPos >= 0 ? playlistPos : -1,
    "track-list/count": mockState.tracks.video.length + mockState.tracks.audio.length + mockState.tracks.subtitles.length,
    vid: selectedMockTrackId(mockState.tracks.video),
    aid: selectedMockTrackId(mockState.tracks.audio),
    sid: selectedMockTrackId(mockState.tracks.subtitles),
    "secondary-sid": Number(mockState.second_subtitle_id) || 0,
    "idle-active": mockState.mode === "initial",
  };
}

function selectMockPlaylistItem(delta) {
  if (mockState.playlist.length <= 1) return;
  const current = Math.max(0, mockState.playlist.findIndex((item) => item.current));
  const next = (current + delta + mockState.playlist.length) % mockState.playlist.length;
  selectMockPlaylistIndex(next);
}

function selectMockPlaylistIndex(index) {
  if (index < 0 || index >= mockState.playlist.length) return;
  mockState.playlist = mockState.playlist.map((item, itemIndex) => ({
    ...item,
    current: itemIndex === index,
    playing: itemIndex === index,
  }));
  const item = mockState.playlist[index];
  Object.assign(mockState, {
    current_url: item.path,
    media_title: item.title,
    music_title: item.title,
    music_album: "",
    music_artist: "",
    duration_seconds: item.duration_seconds || 0,
    position_seconds: 0,
    paused: false,
    last_playback: { path: item.path, title: item.title, position_seconds: 0 },
    osd_message: `Opening ${item.title}`,
  });
}

function removeMockPlaylistIndex(index) {
  if (index < 0 || index >= mockState.playlist.length) return;
  const removedCurrent = mockState.playlist[index].current || mockState.playlist[index].playing;
  mockState.playlist.splice(index, 1);
  mockState.playlist = mockState.playlist.map((item, itemIndex) => ({ ...item, id: itemIndex + 1 }));
  if (!mockState.playlist.length) {
    Object.assign(mockState, {
      mode: "initial",
      current_url: null,
      media_title: "IINA",
      duration_seconds: 0,
      position_seconds: 0,
      paused: true,
      osd_message: "Removed from Playlist",
    });
    return;
  }
  if (removedCurrent || !mockState.playlist.some((item) => item.current || item.playing)) {
    selectMockPlaylistIndex(Math.min(index, mockState.playlist.length - 1));
  }
  mockState.osd_message = "Removed from Playlist";
}

function clearMockPlaylist() {
  const current = mockState.playlist.find((item) => item.current || item.playing);
  mockState.playlist = current ? [{ ...current, id: 1, current: true, playing: true }] : [];
  mockState.osd_message = "Cleared Playlist";
}

function moveMockPlaylistItems(indexes, destination) {
  const selected = normalizePlaylistIndexes(indexes, mockState.playlist.length);
  const reordered = reorderedPlaylistItems(mockState.playlist, selected, destination);
  if (!reordered.moved) return;
  mockState.playlist = reordered.items.map((item, index) => ({ ...item, id: index + 1 }));
  syncMockMpvProperties();
}

function cycleMockTrack(kind) {
  const tracks = mockState.tracks[kind] ?? [];
  if (tracks.length <= 1) return;
  const current = Math.max(0, tracks.findIndex((track) => track.selected));
  const next = (current + 1) % tracks.length;
  mockState.tracks[kind] = tracks.map((track, index) => ({ ...track, selected: index === next }));
}

function selectMockTrack(kind, id) {
  if (kind === "second-subtitles") {
    const trackId = Number(id) || 0;
    const valid = trackId === 0 || mockState.tracks.subtitles.some((track) => Number(track.id) === trackId);
    if (!valid || Number(mockState.second_subtitle_id) === trackId) return false;
    mockState.second_subtitle_id = trackId;
    return true;
  }
  const tracks = mockState.tracks[kind] ?? [];
  if (!tracks.some((track) => track.id === id)) return false;
  mockState.tracks[kind] = tracks.map((track) => ({ ...track, selected: track.id === id }));
  return true;
}

function selectedMockTrackId(tracks) {
  return tracks.find((track) => track.selected)?.id ?? 0;
}

function currentMockChapterIndex(timePos) {
  return [...mockState.chapters]
    .reverse()
    .find((chapter) => Number(chapter.time_seconds) <= timePos)?.index ?? 0;
}

const els = {
  app: document.querySelector("#app"),
  initial: document.querySelector("#initial-window"),
  player: document.querySelector("#player-window"),
  videoStage: document.querySelector(".video-stage"),
  mediaTitle: document.querySelector("#media-title"),
  surfaceTitle: document.querySelector("#surface-title"),
  surfaceSubtitle: document.querySelector("#surface-subtitle"),
  mediaDiagnostics: document.querySelector("#media-diagnostics"),
  mediaFormat: document.querySelector("#media-format"),
  mediaVideo: document.querySelector("#media-video"),
  mediaAudio: document.querySelector("#media-audio"),
  osc: document.querySelector(".osc"),
  osd: document.querySelector("#osd"),
  pipOverlay: document.querySelector("#pip-overlay"),
  fullscreenInfo: document.querySelector("#fullscreen-info"),
  fullscreenTitle: document.querySelector("#fullscreen-title"),
  fullscreenClock: document.querySelector("#fullscreen-clock"),
  fullscreenBattery: document.querySelector("#fullscreen-battery"),
  onTopButton: document.querySelector("#on-top-button"),
  backwardButton: document.querySelector("#backward-button"),
  forwardButton: document.querySelector("#forward-button"),
  leftArrowLabel: document.querySelector("#left-arrow-label"),
  rightArrowLabel: document.querySelector("#right-arrow-label"),
  playButton: document.querySelector("#play-button"),
  playSlider: document.querySelector("#play-slider"),
  playSliderTrack: document.querySelector("#play-slider-track"),
  chapterMarkers: document.querySelector("#chapter-markers"),
  abLoopA: document.querySelector("#ab-loop-a"),
  abLoopB: document.querySelector("#ab-loop-b"),
  volumeSlider: document.querySelector("#volume-slider"),
  muteButton: document.querySelector("#mute-button"),
  pipButton: document.querySelector("#pip-button"),
  playlistButton: document.querySelector("#playlist-button"),
  settingsButton: document.querySelector("#settings-button"),
  oscToolbarGroup: document.querySelector("#osc-toolbar-group"),
  musicModeButton: document.querySelector("#music-mode-button"),
  fullscreenButton: document.querySelector("#fullscreen-button"),
  subTrackButton: document.querySelector("#sub-track-button"),
  screenshotButton: document.querySelector("#screenshot-button"),
  thumbnailPeek: document.querySelector("#thumbnail-peek"),
  thumbnailImage: document.querySelector("#thumbnail-image"),
  thumbnailTime: document.querySelector("#thumbnail-time"),
  sidebar: document.querySelector("#sidebar"),
  sidebarTabs: document.querySelector(".sidebar-tabs"),
  sidebarContent: document.querySelector("#sidebar-content"),
  leftTime: document.querySelector("#left-time"),
  rightTime: document.querySelector("#right-time"),
  miniPlayerUi: document.querySelector("#mini-player-ui"),
  miniVideoRegion: document.querySelector("#mini-video-region"),
  miniDefaultAlbumArt: document.querySelector("#mini-default-album-art"),
  miniMediaInfo: document.querySelector("#mini-media-info"),
  miniTitle: document.querySelector("#mini-title"),
  miniArtistAlbum: document.querySelector("#mini-artist-album"),
  miniPlayButton: document.querySelector("#mini-play-button"),
  miniPreviousButton: document.querySelector("#mini-previous-button"),
  miniNextButton: document.querySelector("#mini-next-button"),
  miniVolumeButton: document.querySelector("#mini-volume-button"),
  miniPlaylistButton: document.querySelector("#mini-playlist-button"),
  miniAlbumArtButton: document.querySelector("#mini-album-art-button"),
  miniPlaySlider: document.querySelector("#mini-play-slider"),
  miniLeftTime: document.querySelector("#mini-left-time"),
  miniRightTime: document.querySelector("#mini-right-time"),
  miniVolumePopover: document.querySelector("#mini-volume-popover"),
  miniMuteButton: document.querySelector("#mini-mute-button"),
  miniVolumeSlider: document.querySelector("#mini-volume-slider"),
  miniVolumeLabel: document.querySelector("#mini-volume-label"),
  miniPlaylist: document.querySelector("#mini-playlist"),
  miniPlaylistList: document.querySelector("#mini-playlist-list"),
  miniCloseButton: document.querySelector("#mini-close-button"),
  miniBackButton: document.querySelector("#mini-back-button"),
  resumeButton: document.querySelector("#resume-button"),
  lastFileTitle: document.querySelector(".last-file-title"),
  lastFilePosition: document.querySelector(".last-file-position"),
  recentFiles: document.querySelector("#recent-files"),
  openUrlModal: document.querySelector("#open-url-modal"),
  urlHttpPrefix: document.querySelector("#url-http-prefix"),
  urlField: document.querySelector("#url-field"),
  urlError: document.querySelector("#url-error"),
  urlUsername: document.querySelector("#url-username"),
  urlPassword: document.querySelector("#url-password"),
  urlRemember: document.querySelector("#url-remember"),
  urlOpenButton: document.querySelector("#url-open-button"),
  urlCancelButton: document.querySelector("#url-cancel-button"),
  onlineSubtitlesAccessory: document.querySelector("#online-subtitles-accessory"),
  onlineSubtitlesList: document.querySelector("#online-subtitles-list"),
  onlineSubtitlesCancelButton: document.querySelector("#online-subtitles-cancel-button"),
  onlineSubtitlesDownloadButton: document.querySelector("#online-subtitles-download-button"),
  pluginGithubModal: document.querySelector("#plugin-github-modal"),
  pluginGithubSource: document.querySelector("#plugin-github-source"),
  pluginGithubDefaultList: document.querySelector("#plugin-github-default-list"),
  pluginGithubSpinner: document.querySelector("#plugin-github-spinner"),
  pluginGithubError: document.querySelector("#plugin-github-error"),
  pluginGithubCancelButton: document.querySelector("#plugin-github-cancel-button"),
  pluginGithubInstallButton: document.querySelector("#plugin-github-install-button"),
  filterModal: document.querySelector("#filter-modal"),
  filterWindowTitle: document.querySelector("#filter-window-title"),
  filterCloseButton: document.querySelector("#filter-close-button"),
  currentFilterList: document.querySelector("#current-filter-list"),
  savedFilterList: document.querySelector("#saved-filter-list"),
  filterAddCurrentButton: document.querySelector("#filter-add-current-button"),
  filterRemoveCurrentButton: document.querySelector("#filter-remove-current-button"),
  filterPresetSheet: document.querySelector("#filter-preset-sheet"),
  filterPresetList: document.querySelector("#filter-preset-list"),
  filterPresetSettings: document.querySelector("#filter-preset-settings"),
  filterPresetCancelButton: document.querySelector("#filter-preset-cancel-button"),
  filterPresetAddButton: document.querySelector("#filter-preset-add-button"),
  filterEditorSheet: document.querySelector("#filter-editor-sheet"),
  filterEditorTitle: document.querySelector("#filter-editor-title"),
  filterEditorDescription: document.querySelector("#filter-editor-description"),
  filterEditorStringRow: document.querySelector("#filter-editor-string-row"),
  filterEditorName: document.querySelector("#filter-editor-name"),
  filterEditorString: document.querySelector("#filter-editor-string"),
  filterEditorShortcut: document.querySelector("#filter-editor-shortcut"),
  filterEditorError: document.querySelector("#filter-editor-error"),
  filterEditorCancelButton: document.querySelector("#filter-editor-cancel-button"),
  filterEditorSubmitButton: document.querySelector("#filter-editor-submit-button"),
  preferencesModal: document.querySelector("#preferences-modal"),
  preferencesTabs: document.querySelector("#preferences-tabs"),
  preferencesContent: document.querySelector("#preferences-content"),
  preferencesSearch: document.querySelector("#preferences-search"),
  preferencesSearchCompletions: document.querySelector("#preferences-search-completions"),
  preferencesCloseButton: document.querySelector("#preferences-close-button"),
  preferenceSheetLayer: document.querySelector("#preference-sheet-layer"),
};

let osdTimer;
let osdHideTimer;
let osdPersistent = false;
let thumbnailSource;
let thumbnailSet;
let thumbnailRequestId = 0;
let thumbnailGenerationId = 0;
let openUrlAlternativeAction = false;
let openUrlEnqueueAction = false;
let openUrlCredentialLookupId = 0;
let openUrlCredentialLookupTimer;
let onlineSubtitleCandidates = [];
let selectedOnlineSubtitleId = null;
let onlineSubtitleBusy = false;
let onlineSubtitleRequestId = 0;
let onlineSubtitleFlowPhase = "idle";
let nextPluginSubtitleCandidateId = 0;
const pluginSubtitleCandidates = new Map();
let autoSubtitleSearchAttemptSource = null;
let surfaceClickTimer;
let surfaceOscHideTimer;
let surfaceOscVisibilityTask = Promise.resolve();
let surfaceOscDesiredVisible = null;
let surfaceOscVisibilityRunning = false;
let surfaceCursorHidden = false;
let playlistContextMenu;
let timeContextMenu;
let oscArrowSpeedIndex = OSC_NORMAL_SPEED_INDEX;
let oscArrowSpeedActive = false;
let playlistSelection = new Set();
let playlistSelectionAnchor = -1;
let playlistDragIndexes = [];
let playlistDropRevealTimer;
let playlistDropRevealPromise = Promise.resolve();
let playlistDropRevealGeneration = 0;
let playlistDropRevealHasPlayableFiles = false;
let playlistDropRevealAutoOpened = false;
let playlistDropRevealPointerX = Number.NaN;
let customCropEditor = null;
let activeFilterKind = "video";
let selectedCurrentFilterIndex = -1;
let selectedSavedFilterIndex = -1;
let selectedFilterPresetId = null;
let filterEditorContext = null;
let filterEditorShortcut = { key: "", modifiers: "" };
let nativeVideoRenderer = await invoke("get_native_video_renderer_status");
let pipOverlayClosing = false;
let stateEpoch = 0;
let renderedStateFingerprint = "";
let nativeMenuStateFingerprint = "";
let runtimeSnapshotInFlight = false;
let oscTimeSnapshotPosition = 0;
let oscTimeSnapshotDuration = 0;
let oscTimeSnapshotSpeed = 1;
let oscTimeSnapshotPaused = true;
let oscTimeSnapshotTimestamp = performance.now();
const pluginRuntimes = new Map();
const pluginDeveloperToolTargets = new Map();
let pluginRuntimeOrder = [];
let pluginRuntimeRefreshQueue = Promise.resolve();
let lastPreferenceChangeRevision = 0;
let pluginMpvEventSequence = { cursor: 0, path: null };
let pluginWindowWillCloseEmitted = false;
let activePluginSidebarId = null;
let selectedPluginPreferenceId = null;
let activePluginPreferenceTab = "permissions";
let pluginPreferencePageRequestId = 0;
let pluginGithubBusy = false;
let initialRecentItems = [];
let initialSelectedRecentIndex = -1;
let initialSelectionInitialized = false;
let wasInitialWindowVisible = false;
let lastWindowPresentationMode = null;
let windowFullscreenActive = false;
let windowAlwaysOnTopActive = false;
let fullscreenInfoTimer;
let fullscreenInfoVisible = false;
let nativeBatteryStatus;
let oscDragState;
let miniPlaylistVisible = false;
let miniVideoVisible = true;
let miniLayoutFingerprint = "";
let miniLayoutInFlight = false;
let miniLayoutQueued = false;
let miniPlaylistResizeTimer;
const nativeScrollGesture = new IinaScrollGestureState();
let nativeInputCommandQueue = Promise.resolve();
let forceTouchSecondStage = false;
let magnifyFullscreenHandled = false;
let surfaceWindowDragState;
let suppressSurfaceClickUntil = 0;
let firstMouseGate;
let oscToolbarLayoutFingerprint;

const RUNTIME_SNAPSHOT_POLL_MS = 100;
const OSC_TIME_PRECISE_REFRESH_MS = 40;

let preferences = await invoke("get_preferences");
firstMouseGate = new FirstMouseGate({
  active: document.hasFocus(),
  acceptsFirstMouse: () => Boolean(getPreferenceValue("videoViewAcceptsFirstMouse")),
  doubleClickInterval: DEFAULT_DOUBLE_CLICK_INTERVAL_MS,
});
await refreshKeyBindingProfiles({ loadCurrent: true });
miniPlaylistVisible = Boolean(getPreferenceValue("musicModeShowPlaylist"));
miniVideoVisible = Boolean(getPreferenceValue("musicModeShowAlbumArt"));
let state = await restoreLaunchMedia(await invoke("get_player_snapshot"));
setPlayerState(state, { force: true, forcePresentOsd: Boolean(state.osd_message) });
renderPreferences();
await installTauriMenuListeners();
await reconcilePreferencesAfterListenerInstall();
try {
  applyPlayerWindowStatus(await invoke("get_player_window_status"), { emitEvents: false });
} catch {
  // The initial browser-only surface has no native window status to synchronize.
}
await completeInitialLaunch();
if (!isAuxiliaryWindow) await restoreFilterPanelPreview();
if (!isAuxiliaryWindow) await refreshPluginRuntimes();
await activateAuxiliaryWindowSurface();
if (!isAuxiliaryWindow) emitPluginEvent("iina.window-loaded");
if (!isAuxiliaryWindow || isFilterAuxiliaryWindow) startRuntimeSnapshotPolling();
if (!isAuxiliaryWindow) startOscTimeDisplayTicker();
window.addEventListener("focus", () => {
  beginSurfaceFocusEpoch();
  refreshNativePlayerMenu(state, true);
});
window.matchMedia?.("(prefers-color-scheme: light)")?.addEventListener("change", () => {
  if (Number(getPreferenceValue("themeMaterial")) === 4) renderThemeMaterial();
});

document.querySelector("#open-file-button").addEventListener("click", async () => {
  await openMediaFromNativeDialog();
});

document.querySelector("#open-url-button").addEventListener("click", async () => {
  showOpenUrlPanel(false);
});

document.querySelector("#resume-button").addEventListener("click", async () => {
  const path = state.last_playback?.path;
  if (!path) return;
  setPlayerState(await invoke("open_media", { path }), { force: true, presentOsd: true });
});

els.playButton.addEventListener("click", () => void handleOscPlayButton());
els.backwardButton.addEventListener("click", () => void handleOscArrowButton(-1));
els.forwardButton.addEventListener("click", () => void handleOscArrowButton(1));
els.muteButton.addEventListener("click", () => command({ type: "toggle-mute" }));
els.pipButton.addEventListener("click", togglePictureInPicture);
els.musicModeButton.addEventListener("click", toggleMusicMode);
els.fullscreenButton.addEventListener("click", toggleFullscreenFromSurface);
els.subTrackButton.addEventListener("click", () => toggleSidebar("subtitles"));
els.screenshotButton.addEventListener("click", () => void captureScreenshot());
els.onTopButton.addEventListener("click", async () => {
  try {
    windowAlwaysOnTopActive = Boolean(await invoke("toggle_window_always_on_top"));
    renderOnTopIndicator();
  } catch {
    showOsd("Float on Top Failed");
  }
});
els.volumeSlider.addEventListener("input", (event) => {
  setRangeProgress(event.currentTarget);
  command({ type: "set-volume", volume: Number(event.currentTarget.value) });
});
els.playSlider.addEventListener("input", (event) => {
  setRangeProgress(event.currentTarget);
  command(iinaTimelineSeekPlan(
    event.currentTarget.value,
    event.currentTarget.max,
    getPreferenceValue("followGlobalSeekTypeWhenAdjustSlider"),
    getPreferenceValue("useExactSeek"),
  ));
});
els.playSlider.addEventListener("pointermove", showThumbnailPeek);
els.playSlider.addEventListener("pointerleave", hideThumbnailPeek);
els.playSlider.addEventListener("pointerdown", showThumbnailPeek);
els.playSlider.addEventListener("pointerup", hideThumbnailPeek);
els.thumbnailImage.addEventListener("load", () => {
  if (!els.thumbnailPeek.hidden) positionThumbnailPeek(els.playSlider.getBoundingClientRect());
});
installAbLoopKnob(els.abLoopA, "a");
installAbLoopKnob(els.abLoopB, "b");
els.videoStage.addEventListener("click", handleSurfaceClick);
els.videoStage.addEventListener("dblclick", handleSurfaceDoubleClick);
els.videoStage.addEventListener("auxclick", handleSurfaceAuxClick);
els.videoStage.addEventListener("contextmenu", handleSurfaceContextMenu);
els.videoStage.addEventListener("click", suppressFirstMouseSurfaceAction, { capture: true });
els.videoStage.addEventListener("dblclick", suppressFirstMouseSurfaceAction, { capture: true });
els.videoStage.addEventListener("auxclick", suppressFirstMouseSurfaceAction, { capture: true });
els.videoStage.addEventListener("contextmenu", suppressFirstMouseSurfaceAction, { capture: true });
els.videoStage.addEventListener("wheel", handleSurfaceWheel, { passive: false });
els.videoStage.addEventListener("pointerdown", suppressFirstMouseSurfacePointer, { capture: true });
els.videoStage.addEventListener("pointermove", suppressFirstMouseSurfacePointer, { capture: true });
els.videoStage.addEventListener("pointerup", suppressFirstMouseSurfacePointer, { capture: true });
els.videoStage.addEventListener("pointercancel", suppressFirstMouseSurfacePointer, { capture: true });
els.videoStage.addEventListener("pointerdown", handlePluginPointerDown);
els.videoStage.addEventListener("pointermove", handlePluginPointerMove);
els.videoStage.addEventListener("pointerdown", beginSurfaceWindowDrag);
els.videoStage.addEventListener("pointermove", updateSurfaceWindowDrag);
els.videoStage.addEventListener("pointerup", finishSurfaceWindowDrag);
els.videoStage.addEventListener("pointercancel", finishSurfaceWindowDrag);
els.player.addEventListener("pointerenter", handlePlayerPointerMovement);
els.player.addEventListener("pointermove", handlePlayerPointerMovement);
els.player.addEventListener("pointerleave", handlePlayerPointerLeave);
els.osc.addEventListener("pointerdown", beginOscDrag);
els.osc.addEventListener("pointermove", updateOscDrag);
els.osc.addEventListener("pointerup", finishOscDrag);
els.osc.addEventListener("pointercancel", finishOscDrag);
els.player.addEventListener("mousedown", preventPlayerChromeButtonMouseFocus, { capture: true });
els.player.addEventListener("contextmenu", (event) => {
  if (isInteractiveMouseTarget(event.target)) event.preventDefault();
});
els.playlistButton.addEventListener("click", () => toggleSidebar("playlist"));
els.settingsButton.addEventListener("click", () => toggleSidebar("video"));
els.rightTime.addEventListener("click", toggleRemainingTimeDisplay);
[els.leftTime, els.rightTime].forEach((label) => {
  label.addEventListener("contextmenu", showTimeContextMenu);
});
els.miniPlayButton.addEventListener("click", () => void handleOscPlayButton());
els.miniPreviousButton.addEventListener("click", () => void command({ type: "playlist-prev" }));
els.miniNextButton.addEventListener("click", () => void command({ type: "playlist-next" }));
els.miniVolumeButton.addEventListener("click", toggleMiniVolumePopover);
els.miniMuteButton.addEventListener("click", () => command({ type: "toggle-mute" }));
els.miniPlaylistButton.addEventListener("click", () => void toggleMiniPlaylist());
els.miniAlbumArtButton.addEventListener("click", () => void toggleMiniVideo());
els.miniBackButton.addEventListener("click", () => void toggleMusicMode());
els.miniCloseButton.addEventListener("click", () => void closeMiniPlayer());
els.miniPlaySlider.addEventListener("input", (event) => {
  setRangeProgress(event.currentTarget);
  void command(iinaTimelineSeekPlan(
    event.currentTarget.value,
    event.currentTarget.max,
    getPreferenceValue("followGlobalSeekTypeWhenAdjustSlider"),
    getPreferenceValue("useExactSeek"),
  ));
});
els.miniVolumeSlider.addEventListener("input", (event) => {
  setRangeProgress(event.currentTarget);
  void command({ type: "set-volume", volume: Number(event.currentTarget.value) });
});
els.miniRightTime.addEventListener("click", toggleRemainingTimeDisplay);
[els.miniLeftTime, els.miniRightTime].forEach((label) => {
  label.addEventListener("contextmenu", showTimeContextMenu);
});
els.urlField.addEventListener("input", () => updateOpenUrlValidation({ loadCredentials: true }));
els.urlUsername.addEventListener("input", () => updateOpenUrlValidation());
els.urlPassword.addEventListener("input", () => updateOpenUrlValidation());
els.urlOpenButton.addEventListener("click", submitOpenUrlPanel);
els.urlCancelButton.addEventListener("click", closeOpenUrlPanel);
els.onlineSubtitlesCancelButton.addEventListener("click", cancelOnlineSubtitleChooser);
els.onlineSubtitlesDownloadButton.addEventListener("click", () => void downloadSelectedOnlineSubtitles());
els.pluginGithubCancelButton.addEventListener("click", closePluginGithubPanel);
els.pluginGithubInstallButton.addEventListener("click", () => void submitPluginGithubPanel());
els.pluginGithubSource.addEventListener("input", updatePluginGithubInstallEnablement);
els.filterCloseButton.addEventListener("click", closeFilterPanel);
els.filterAddCurrentButton.addEventListener("click", () => showFilterPresetSheet());
els.filterRemoveCurrentButton.addEventListener("click", () => void removeSelectedCurrentFilter());
els.filterPresetCancelButton.addEventListener("click", closeFilterPresetSheet);
els.filterPresetSheet.addEventListener("submit", (event) => void submitFilterPreset(event));
els.filterEditorCancelButton.addEventListener("click", closeFilterEditor);
els.filterEditorSheet.addEventListener("submit", (event) => void submitFilterEditor(event));
els.filterEditorShortcut.addEventListener("keydown", recordFilterShortcut);
els.filterEditorShortcut.addEventListener("focus", () => els.filterEditorShortcut.select());
els.preferencesCloseButton.addEventListener("click", closePreferencesPanel);
els.preferencesSearch.addEventListener("input", () => {
  preferenceSearchQuery = normalizePreferenceSearchQuery(els.preferencesSearch.value);
  preferenceSearchCompletionIndex = -1;
  renderPreferenceSearchCompletions();
});
els.preferencesSearch.addEventListener("focus", () => {
  if (preferenceSearchQuery) renderPreferenceSearchCompletions();
});
els.preferencesSearch.addEventListener("keydown", handlePreferenceSearchKeydown);
els.preferencesModal.addEventListener("pointerdown", (event) => {
  if (event.target.closest("#preferences-search, #preferences-search-completions")) return;
  dismissPreferenceSearchCompletions();
});
document.addEventListener("dragenter", handleDragEnter);
document.addEventListener("dragover", handleDragOver);
document.addEventListener("dragleave", handleDragLeave);
document.addEventListener("drop", handleDrop);
document.addEventListener("copy", handlePlaylistCopyEvent);
document.addEventListener("cut", handlePlaylistCutEvent);
document.addEventListener("paste", handlePlaylistPasteEvent);
window.addEventListener("click", hideContextMenus);
window.addEventListener("click", (event) => {
  if (!isMiniPlayerWindow || els.miniVolumePopover.hidden) return;
  if (event.target.closest("#mini-volume-button, #mini-volume-popover")) return;
  els.miniVolumePopover.hidden = true;
});
window.addEventListener("resize", () => {
  hideContextMenus();
  layoutCustomCropEditor();
  scheduleMiniPlayerResizeCheck();
  if (!tauriInvoke) emitPluginEvent("iina.window-resized", pluginWindowRect());
});
window.addEventListener("blur", () => {
  firstMouseGate.blur();
  hideContextMenus();
  if (!tauriInvoke) emitPluginEvent("iina.window-main.changed", false);
});
window.addEventListener("focus", () => {
  if (!tauriInvoke) emitPluginEvent("iina.window-main.changed", true);
});
window.addEventListener("beforeunload", () => {
  if (!pluginWindowWillCloseEmitted) emitPluginEvent("iina.window-will-close");
});

document.querySelectorAll("[data-sidebar-tab]").forEach((button) => {
  button.addEventListener("click", () => {
    activePluginSidebarId = null;
    command({ type: "show-sidebar", tab: button.dataset.sidebarTab });
  });
});

window.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && customCropEditor) {
    event.preventDefault();
    closeCustomCropEditor();
    return;
  }

  if (event.key === "Escape" && (playlistContextMenu || timeContextMenu)) {
    event.preventDefault();
    hideContextMenus();
    return;
  }

  if ((event.metaKey || event.ctrlKey) && event.key === ",") {
    event.preventDefault();
    showPreferencesPanel();
    return;
  }

  if (!els.pluginGithubModal.hidden) {
    if (event.key === "Escape") {
      event.preventDefault();
      closePluginGithubPanel();
    } else if (event.key === "Enter") {
      event.preventDefault();
      void submitPluginGithubPanel();
    }
    return;
  }

  if (!els.filterModal.hidden) {
    if (event.key === "Escape") {
      event.preventDefault();
      if (!els.filterPresetSheet.hidden) closeFilterPresetSheet();
      else if (!els.filterEditorSheet.hidden) closeFilterEditor();
      else closeFilterPanel();
    }
    return;
  }

  if (!els.preferencesModal.hidden) {
    if (!els.preferenceSheetLayer.hidden) {
      if (event.key === "Escape") {
        event.preventDefault();
        dismissPreferenceSheet(null);
      } else if (event.key === "Enter" && !event.metaKey && !event.ctrlKey && !event.altKey) {
        event.preventDefault();
        els.preferenceSheetLayer.querySelector(".preference-sheet-primary")?.click();
      }
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      closePreferencesPanel();
      return;
    }
    return;
  }

  if (!els.openUrlModal.hidden) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeOpenUrlPanel();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      submitOpenUrlPanel();
      return;
    }
  }

  if (onlineSubtitleFlowPhase === "choosing" && event.key === "Escape") {
    event.preventDefault();
    cancelOnlineSubtitleChooser();
    return;
  }

  if (state.mode === "initial" && handleInitialWindowKeyDown(event)) {
    return;
  }

  if (isTypingTarget(event.target)) return;

  const handled = dispatchPluginInput(
    pluginKeyCodeFromEvent(event),
    "keyDown",
    pluginKeyEventArgs(event),
    () => {
      if (handlePlaylistSelectionShortcut(event)) return true;
      if (handlePlayerShortcut(event)) return true;
      if (event.key === "Escape" && state.sidebar.visible) {
        event.preventDefault();
        void command({ type: "hide-sidebar" });
        return true;
      }
      return false;
    },
  );
  if (handled) event.preventDefault();
});

window.addEventListener("keyup", (event) => {
  if (isTypingTarget(event.target)) return;
  if (dispatchPluginInput(pluginKeyCodeFromEvent(event), "keyUp", pluginKeyEventArgs(event))) {
    event.preventDefault();
  }
});

async function command(payload) {
  setPlayerState(await invoke("player_command", { command: payload }), { force: true, presentOsd: true });
}

async function handleOscPlayButton() {
  if (!state.paused && oscArrowSpeedActive) {
    oscArrowSpeedActive = false;
    oscArrowSpeedIndex = OSC_NORMAL_SPEED_INDEX;
    await command({ type: "set-speed", speed: 1 });
  }
  await command({ type: "toggle-pause" });
}

async function handleOscArrowButton(direction) {
  if (!state.current_url) return;
  const action = Number(getPreferenceValue("arrowBtnAction"));
  if (action === 1) {
    await command({ type: direction < 0 ? "playlist-prev" : "playlist-next" });
    return;
  }
  if (action === 2) {
    await command({
      type: "seek",
      seconds: (Number(state.position_seconds) || 0) + (direction < 0 ? -10 : 10),
    });
    return;
  }

  let index = oscArrowSpeedActive ? oscArrowSpeedIndex : nearestOscSpeedIndex(state.speed);
  if (direction < 0) {
    if (index > OSC_NORMAL_SPEED_INDEX) index = OSC_NORMAL_SPEED_INDEX;
    index = Math.max(0, index - 1);
  } else {
    if (index < OSC_NORMAL_SPEED_INDEX) index = OSC_NORMAL_SPEED_INDEX;
    index = Math.min(OSC_SPEED_VALUES.length - 1, index + 1);
  }
  oscArrowSpeedIndex = index;
  oscArrowSpeedActive = index !== OSC_NORMAL_SPEED_INDEX;
  if (state.paused) await command({ type: "toggle-pause" });
  await command({ type: "set-speed", speed: OSC_SPEED_VALUES[index] });
}

function nearestOscSpeedIndex(speed) {
  const value = Math.max(0.000001, Number(speed) || 1);
  let nearest = OSC_NORMAL_SPEED_INDEX;
  let distance = Number.POSITIVE_INFINITY;
  OSC_SPEED_VALUES.forEach((candidate, index) => {
    const nextDistance = Math.abs(Math.log(value / candidate));
    if (nextDistance < distance) {
      distance = nextDistance;
      nearest = index;
    }
  });
  return nearest;
}

async function toggleMusicMode() {
  try {
    setPlayerState(await invoke("toggle_music_mode"), { force: true, presentOsd: true });
  } catch {
    showOsd("Music Mode Failed");
  }
}

async function closeMiniPlayer() {
  try {
    setPlayerState(await invoke("close_mini_player"), { force: true, presentOsd: true });
  } catch {
    showOsd("Close Failed");
  }
}

async function togglePictureInPicture() {
  if (!state.current_url) return;
  const exiting = Boolean(state.pip_active);
  pipOverlayClosing = exiting;
  renderPipOverlay(state);
  try {
    setPlayerState(await invoke("toggle_picture_in_picture"), { force: true, presentOsd: true });
  } catch {
    pipOverlayClosing = false;
    renderPipOverlay(state);
    showOsd("Picture in Picture Failed");
  }
}

let drainingPluginInstallNotifications = false;
let pluginInstallDrainRequested = false;

async function drainPendingPluginInstallNotifications() {
  if (!isPreferencesAuxiliaryWindow) return;
  if (drainingPluginInstallNotifications) {
    pluginInstallDrainRequested = true;
    return;
  }
  drainingPluginInstallNotifications = true;
  try {
    do {
      pluginInstallDrainRequested = false;
      while (true) {
        let notification;
        try {
          notification = await invoke("claim_pending_plugin_install");
        } catch (error) {
          await showUtilityInformation(
            "Error",
            String(error?.message || error || "Unable to receive plugin installation"),
          );
          break;
        }
        if (!notification) break;
        if (notification.error) {
          await showUtilityInformation("Error", String(notification.error));
          continue;
        }
        const result = notification.result;
        const confirmationToken = ["permission-confirmation", "reinstall-confirmation"].includes(result?.status)
          ? result.confirmation?.token
          : null;
        const cancelConfirmationCommand = result?.status === "permission-confirmation"
          ? "cancel_plugin_permissions"
          : "cancel_plugin_reinstall";
        let record;
        try {
          record = await resolvePluginInstallResult(result);
        } catch (installError) {
          if (confirmationToken) {
            await invoke(cancelConfirmationCommand, { token: confirmationToken }).catch(() => {});
          }
          await showUtilityInformation(
            "Error",
            String(installError?.message || installError || "Plugin installation failed"),
          );
          continue;
        }
        if (!record) continue;
        try {
          activePreferencePane = "plugins";
          applyPluginPreferenceWindowContext(record.identifier);
          await refreshPlayerPluginRuntimes();
          renderPreferences();
        } catch (displayError) {
          await showUtilityInformation(
            "Error",
            String(displayError?.message || displayError || "Plugin installed"),
          );
        }
      }
    } while (pluginInstallDrainRequested);
  } finally {
    drainingPluginInstallNotifications = false;
    if (pluginInstallDrainRequested) void drainPendingPluginInstallNotifications();
  }
}

async function requestPendingPluginInstallDrain({ checkQueue = true } = {}) {
  if (!isPrimaryPlayerWindow) return;
  if (checkQueue) {
    try {
      if (!await invoke("has_pending_plugin_installs")) return;
    } catch (error) {
      showOsd(String(error?.message || error || "Unable to inspect plugin installations"));
      return;
    }
  }
  activePreferencePane = "plugins";
  await showPreferencesPanel({ drainPendingPluginInstalls: true });
}

async function installTauriMenuListeners() {
  if (!tauriListen) return;
  await tauriListen("iima-native-file-drag", (event) => {
    handleNativeFileDrag(event.payload);
  });
  await tauriListen("iima-native-file-drop", (event) => {
    void handleNativeFileDrop(event.payload);
  });
  await tauriListen("iima-player-state", (event) => {
    setPlayerState(event.payload, { force: true, presentOsd: true });
  });
  await tauriListen("iima-preference-changed", (event) => {
    applyBroadcastPreferenceChange(event.payload);
  });
  await tauriListen("iima-plugin-runtime-refresh", () => {
    if (!isAuxiliaryWindow && !isMiniPlayerWindow) void queuePluginRuntimeRefresh();
  });
  await tauriListen("iima-plugin-runtime-reload-all", () => {
    if (!isAuxiliaryWindow && !isMiniPlayerWindow) {
      void queuePluginRuntimeRefresh({ force: true });
    }
  });
  await tauriListen("iima-auxiliary-window-context", (event) => {
    void activateAuxiliaryWindowSurface(event.payload);
  });
  await tauriListen("iima-pip-will-close", () => {
    pipOverlayClosing = true;
    renderPipOverlay(state);
  });
  await tauriListen("iima-plugin-mpv-events", (event) => {
    dispatchPluginMpvEventBatch(event.payload);
  });
  await tauriListen("iima-plugin-host-event", (event) => {
    dispatchPluginHostEvent(event.payload);
  });
  await tauriListen("iima-player-window-status", (event) => {
    applyPlayerWindowStatus(event.payload);
  });
  await tauriListen("iima-native-player-input", (event) => {
    handleNativePlayerInput(event.payload);
  });
  await tauriListen("iima-native-mini-player-layout", (event) => {
    applyNativeMiniPlayerLayout(event.payload);
  });
  await tauriListen("iima-thumbnail-progress", (event) => {
    applyThumbnailProgressEvent(event.payload);
  });
  await tauriListen("iima-menu-request", async (event) => {
    const action = event.payload?.action;
    if (action === "open") {
      await openMediaFromNativeDialog();
    } else if (action === "open-recent" && typeof event.payload?.path === "string") {
      await openMediaForMenuAction(event.payload.path, false);
    } else if (action === "clear-recent") {
      setPlayerState(await invoke("clear_recent_documents"), { force: true });
    } else if (action === "open-new-window") {
      await openMediaInNewWindowFromNativeDialog();
    } else if (action === "load-external-audio") {
      await loadExternalTrackFromNativeDialog("audio");
    } else if (action === "load-external-subtitle") {
      await loadExternalTrackFromNativeDialog("subtitles");
    } else if (action === "custom-crop") {
      startCustomCropEditor();
    } else if (action === "delogo") {
      if (activeDelogoFilter()) {
        await command({ type: "remove-delogo" });
      } else {
        startCustomDelogoEditor();
      }
    } else if (action === "open-url" || action === "open-url-new-window") {
      showOpenUrlPanel(action === "open-url-new-window");
    } else if (action === "preferences") {
      await showPreferencesPanel();
    } else if (action === "check-updates") {
      try {
        await invoke("check_for_updates");
      } catch {
        showOsd("Unable to Check for Updates");
      }
    } else if (action === "video-filters") {
      await showFilterPanel("video");
    } else if (action === "audio-filters") {
      await showFilterPanel("audio");
    } else if (action === "manage-plugins") {
      activePreferencePane = "plugins";
      await showPreferencesPanel();
    } else if (action === "find-online-subtitles") {
      await showOnlineSubtitlePanel(event.payload?.providerId ?? null);
    } else if (action === "save-downloaded-subtitle") {
      await saveDownloadedSubtitle();
    } else if (action === "subtitle-font") {
      await chooseSubtitleFontFromNativeDialog();
    } else if (action === "jump-to") {
      try {
        const nextState = await invoke("jump_to_time_dialog");
        if (nextState) {
          setPlayerState(nextState, { force: true, presentOsd: true });
        }
      } catch (error) {
        console.error("Jump To failed", error);
        showOsd("Jump Failed");
      }
    } else if (action === "save-current-playlist") {
      try {
        await invoke("save_current_playlist");
      } catch (error) {
        console.error("Save Current Playlist failed", error);
        showOsd("Unable to Save Playlist");
      }
    } else if (action === "screenshot") {
      await captureScreenshot();
    } else if (action === "goto-screenshot-folder") {
      await revealScreenshotFolder();
    } else if (action === "music-mode") {
      await toggleMusicMode();
    } else if (action === "picture-in-picture") {
      await togglePictureInPicture();
    } else {
      showOsd(menuRequestLabel(action));
    }
  });
  await tauriListen("iima-plugin-menu-action", (event) => {
    const identifier = event.payload?.identifier;
    const itemId = event.payload?.itemId;
    const role = event.payload?.role;
    const runtime = pluginRuntimes.get(identifier);
    const item = runtime?.menuItemsById.get(pluginMenuItemKey(role, itemId));
    if (!item || typeof item.action !== "function") return;
    try {
      item.action(item);
    } catch (error) {
      console.error(`[${identifier}] plugin menu action failed`, error);
      showOsd("Plugin Menu Action Failed");
    }
  });
  await tauriListen("iima-plugin-developer-tool-opened", (event) => {
    const identifier = String(event.payload?.identifier || "");
    const role = String(event.payload?.role || "");
    const contextId = String(event.payload?.contextId || "");
    const targetLabel = String(event.payload?.windowLabel || "");
    const runtime = pluginRuntimes.get(identifier);
    const realm = pluginDeveloperToolRealmContext(runtime, role, contextId);
    if (!realm || !targetLabel || !["entry", "global"].includes(role)) return;
    pluginDeveloperToolTargets.set(contextId, {
      identifier,
      role,
      windowLabel: targetLabel,
    });
  });
  await tauriListen("iima-plugin-developer-tool-evaluate", async (event) => {
    const identifier = String(event.payload?.identifier || "");
    const role = String(event.payload?.role || "");
    const contextId = String(event.payload?.contextId || "");
    const requestId = Number(event.payload?.requestId);
    const source = String(event.payload?.source || "");
    const target = pluginDeveloperToolTargets.get(contextId);
    const runtime = pluginRuntimes.get(identifier);
    const realm = pluginDeveloperToolRealmContext(runtime, role, contextId);
    if (
      !target
      || target.identifier !== identifier
      || target.role !== role
      || !realm
      || !Number.isSafeInteger(requestId)
      || !source.trim()
      || !tauriEmitTo
    ) {
      return;
    }
    try {
      const value = source === "$global" ? realm.developerGlobal() : realm.evaluateDeveloper(source);
      await tauriEmitTo(target.windowLabel, "iima-plugin-developer-tool-result", {
        contextId,
        requestId,
        result: serializePluginDeveloperValue(value),
      });
    } catch (error) {
      await tauriEmitTo(target.windowLabel, "iima-plugin-developer-tool-result", {
        contextId,
        requestId,
        exception: {
          message: String(error?.message || error || "Unknown exception"),
          stack: String(error?.stack || "???"),
        },
      });
    }
  });
  await tauriListen("iima-plugin-websocket-state", (event) => {
    dispatchPluginWebSocketState(event.payload);
  });
  await tauriListen("iima-plugin-websocket-new-connection", (event) => {
    dispatchPluginWebSocketNewConnection(event.payload);
  });
  await tauriListen("iima-plugin-websocket-connection-state", (event) => {
    dispatchPluginWebSocketConnectionState(event.payload);
  });
  await tauriListen("iima-plugin-websocket-message", (event) => {
    dispatchPluginWebSocketMessage(event.payload);
  });
  await tauriListen("iima-plugin-utils-exec-output", (event) => {
    dispatchPluginUtilsExecOutput(event.payload);
  });
  await tauriListen("iima-plugin-webview-message", (event) => {
    dispatchPluginWebviewMessage(event.payload);
  });
  await tauriListen("iima-plugin-global-controller-message", (event) => {
    dispatchPluginGlobalControllerMessage(event.payload);
  });
  await tauriListen("iima-plugin-global-child-message", (event) => {
    dispatchPluginGlobalChildMessage(event.payload);
  });
  await tauriListen("iima-plugin-mpv-hook", (event) => {
    dispatchPluginMpvHook(event.payload);
  });
  if (isPrimaryPlayerWindow) {
    await tauriListen("iima-plugin-package", () => {
      void requestPendingPluginInstallDrain({ checkQueue: false });
    });
    await requestPendingPluginInstallDrain();
  }
}

function pluginWebSocketRuntimeForEvent(payload) {
  const runtime = pluginRuntimes.get(String(payload?.identifier || ""));
  const role = String(payload?.role || "");
  const controller = runtime?.websocket?.[role];
  if (!runtime || !controller || controller.disposed) return null;
  const generation = Number(payload?.generation);
  if (!Number.isSafeInteger(generation)) return null;
  if (controller.generation == null || generation > controller.generation) {
    controller.generation = generation;
  }
  return generation === controller.generation ? { runtime, controller } : null;
}

function callPluginWebSocketHandler(runtime, controller, field, args) {
  const handler = controller.handlers[field];
  if (typeof handler !== "function") return;
  try {
    handler(...args);
  } catch (error) {
    console.error(`[${runtime.spec.identifier}] WebSocket ${field} handler failed`, error);
  }
}

function dispatchPluginWebSocketState(payload) {
  const match = pluginWebSocketRuntimeForEvent(payload);
  if (!match) return;
  const args = [String(payload.state || "")];
  if (payload.error) args.push(payload.error);
  callPluginWebSocketHandler(match.runtime, match.controller, "state", args);
}

function dispatchPluginWebSocketNewConnection(payload) {
  const match = pluginWebSocketRuntimeForEvent(payload);
  if (!match) return;
  callPluginWebSocketHandler(match.runtime, match.controller, "newConnection", [
    String(payload.connectionId || ""),
    { path: payload.path ?? null },
  ]);
}

function dispatchPluginWebSocketConnectionState(payload) {
  const match = pluginWebSocketRuntimeForEvent(payload);
  if (!match) return;
  const args = [String(payload.connectionId || ""), String(payload.state || "")];
  if (payload.error) args.push(payload.error);
  callPluginWebSocketHandler(match.runtime, match.controller, "connectionState", args);
}

function dispatchPluginWebSocketMessage(payload) {
  const match = pluginWebSocketRuntimeForEvent(payload);
  if (!match) return;
  const data = Uint8Array.from(payload.data || []);
  let decoded = false;
  let text = null;
  const message = Object.freeze({
    text: () => {
      if (!decoded) {
        decoded = true;
        try {
          text = new TextDecoder("utf-8", { fatal: true }).decode(data);
        } catch {
          text = null;
        }
      }
      return text;
    },
    data: () => Uint8Array.from(data),
  });
  callPluginWebSocketHandler(
    match.runtime,
    match.controller,
    "message",
    [String(payload.connectionId || ""), message],
  );
}

function dispatchPluginUtilsExecOutput(payload) {
  const runtime = pluginRuntimes.get(String(payload?.identifier || ""));
  const hooks = runtime?.utilsExecHooks.get(String(payload?.requestId || ""));
  if (!hooks || hooks.role !== String(payload?.role || "")) return;
  const hook = payload?.stream === "stderr" ? hooks?.stderr : hooks?.stdout;
  if (typeof hook !== "function") return;
  try {
    hook(String(payload?.chunk || ""));
  } catch (error) {
    console.error(`[${runtime.spec.identifier}] process output hook failed`, error);
  }
}

function dispatchPluginWebviewMessage(payload) {
  const runtime = pluginRuntimes.get(String(payload?.identifier || ""));
  const role = String(payload?.role || "entry");
  const surface = runtime && ["entry", "global"].includes(role)
    ? pluginPageState(runtime, String(payload?.surface || ""), role)
    : null;
  if (!surface || surface.token !== payload?.token) return;
  const callback = surface.listeners.get(String(payload?.name ?? ""));
  if (typeof callback !== "function") return;
  try {
    if (Object.prototype.hasOwnProperty.call(payload, "data")) callback(payload.data);
    else callback();
  } catch (error) {
    console.error(`[${runtime.spec.identifier}] plugin page message callback failed`, error);
  }
}

function dispatchPluginEmbeddedPageMessage(event) {
  const payload = event.data;
  if (!payload || typeof payload.token !== "string") return;
  for (const runtime of pluginRuntimes.values()) {
    for (const surfaceName of ["overlay", "sidebar"]) {
      const surface = runtime.pluginPages?.[surfaceName];
      if (!surface || surface.token !== payload.token || event.source !== surface.frame?.contentWindow) continue;
      if (payload.__iimaPluginPageHitTest === true && surfaceName === "overlay") {
        if (!surface.clickableEnabled) return;
        const requestId = Number(payload.requestId) || 0;
        if (requestId > 0 && requestId < surface.hitTestRequestId) return;
        surface.container.style.pointerEvents = payload.clickable ? "auto" : "none";
        return;
      }
      if (payload.__iimaPluginPagePost !== true) return;
      dispatchPluginWebviewMessage({
        identifier: runtime.spec.identifier,
        surface: surfaceName,
        token: payload.token,
        name: payload.name,
        ...(Object.prototype.hasOwnProperty.call(payload, "data") ? { data: payload.data } : {}),
      });
      return;
    }
  }
}

window.addEventListener("message", dispatchPluginEmbeddedPageMessage);

let lastPluginPointerPosition = null;

function queryPluginOverlayHitTests(event) {
  lastPluginPointerPosition = { clientX: Number(event.clientX), clientY: Number(event.clientY) };
  for (const runtime of pluginRuntimes.values()) {
    const page = runtime.pluginPages?.overlay;
    if (!page?.clickableEnabled || !page.token || !page.loaded || !page.frame || page.container?.hidden) continue;
    const rect = page.frame.getBoundingClientRect();
    const x = Number(event.clientX) - rect.left;
    const y = Number(event.clientY) - rect.top;
    if (x < 0 || y < 0 || x >= rect.width || y >= rect.height) {
      page.container.style.pointerEvents = "none";
      continue;
    }
    const requestId = ++page.hitTestRequestId;
    page.frame.contentWindow?.postMessage({
      __iimaPluginBridge: true,
      token: page.token,
      control: "hit-test",
      requestId,
      x,
      y,
    }, "*");
  }
}

window.addEventListener("pointermove", queryPluginOverlayHitTests, true);

function dispatchPluginGlobalControllerMessage(payload) {
  const runtime = pluginRuntimes.get(String(payload?.identifier || ""));
  const callback = runtime?.globalControllerListeners.get(String(payload?.name ?? ""));
  if (typeof callback !== "function") return;
  try {
    callback(payload?.data ?? null, payload?.sender ?? null);
  } catch (error) {
    console.error(`[${runtime.spec.identifier}] global controller message callback failed`, error);
  }
}

function dispatchPluginGlobalChildMessage(payload) {
  const runtime = pluginRuntimes.get(String(payload?.identifier || ""));
  const callback = runtime?.globalChildListeners.get(String(payload?.name ?? ""));
  if (typeof callback !== "function") return;
  try {
    callback(payload?.data ?? null, null);
  } catch (error) {
    console.error(`[${runtime.spec.identifier}] global child message callback failed`, error);
  }
}

function continuePluginMpvHook(payload) {
  return invoke("plugin_mpv_continue_hook", {
    identifier: String(payload?.identifier || ""),
    callbackId: Number(payload?.callbackId),
    hookId: Number(payload?.hookId),
  });
}

function dispatchPluginMpvHook(payload) {
  const identifier = String(payload?.identifier || "");
  const runtime = pluginRuntimes.get(identifier);
  const callbackId = Number(payload?.callbackId);
  const callback = runtime?.mpvHookCallbacks.get(callbackId);
  if (typeof callback !== "function") {
    void continuePluginMpvHook(payload).catch((error) => {
      console.error(`[${identifier}] unable to release an orphaned mpv hook`, error);
    });
    return;
  }

  invokePluginMpvHook(callback, payload, continuePluginMpvHook, (phase, error) => {
    console.error(`[${identifier}] mpv hook ${payload?.name || ""} ${phase} failed`, error);
  });
}

async function refreshPluginRuntimes({ force = false } = {}) {
  if (isAuxiliaryWindow || isMiniPlayerWindow) {
    for (const runtime of pluginRuntimes.values()) await unloadPluginRuntime(runtime);
    pluginRuntimes.clear();
    pluginRuntimeOrder = [];
    return;
  }
  let specs = [];
  try {
    specs = Array.from(await invoke("get_plugin_runtime_specs"));
  } catch (error) {
    console.error("Unable to load IINA plugins", error);
    return;
  }
  if (isManagedPluginPlayerWindow && !managedPluginEnablesAll) {
    specs = specs.filter((spec) => spec.identifier === managedPluginIdentifier);
  }
  pluginRuntimeOrder = specs.map((spec) => spec.identifier);
  const incoming = new Map(specs.map((spec) => [spec.identifier, spec]));
  for (const [identifier, runtime] of pluginRuntimes) {
    const spec = incoming.get(identifier);
    if (force && spec) {
      try {
        await reloadPluginEntryRuntime(runtime, spec);
      } catch (error) {
        console.error(`[${spec.identifier}] unable to reload plugin entry instance`, error);
        showOsd(`Plugin Failed: ${spec.name || spec.identifier}`);
      }
      continue;
    }
    if (spec && pluginRuntimeFingerprint(spec) === runtime.fingerprint) continue;
    await unloadPluginRuntime(runtime);
    pluginRuntimes.delete(identifier);
  }
  for (const spec of specs) {
    if (pluginRuntimes.has(spec.identifier)) continue;
    let runtime = null;
    try {
      runtime = createPluginRuntime(spec);
      pluginRuntimes.set(spec.identifier, runtime);
      await runPluginRuntime(runtime);
      await syncPluginMenus(runtime);
    } catch (error) {
      if (runtime) await unloadPluginRuntime(runtime);
      pluginRuntimes.delete(spec.identifier);
      console.error(`[${spec.identifier}] unable to start plugin`, error);
      showOsd(`Plugin Failed: ${spec.name || spec.identifier}`);
    }
  }
  renderPluginSidebarTabs();
  renderPreferences();
}

function queuePluginRuntimeRefresh({ force = false } = {}) {
  pluginRuntimeRefreshQueue = pluginRuntimeRefreshQueue
    .catch(() => {})
    .then(() => refreshPluginRuntimes({ force }));
  return pluginRuntimeRefreshQueue;
}

async function refreshPlayerPluginRuntimes() {
  if (isPreferencesAuxiliaryWindow) {
    await invoke("request_player_plugin_runtime_refresh");
    return;
  }
  if (!isAuxiliaryWindow && !isMiniPlayerWindow) await queuePluginRuntimeRefresh();
}

function pluginRuntimeFingerprint(spec) {
  return JSON.stringify({
    identifier: spec.identifier,
    entry: spec.entry,
    globalEntry: spec.global_entry,
    scripts: spec.scripts,
    permissions: spec.permissions,
    allowedDomains: spec.allowed_domains,
  });
}

function createPluginWebSocketController() {
  return {
    generation: null,
    disposed: false,
    created: false,
    task: Promise.resolve(),
    handlers: {
      state: null,
      message: null,
      newConnection: null,
      connectionState: null,
    },
  };
}

function pluginDeveloperToolRealmContext(runtime, role, contextId) {
  if (!runtime || !contextId || !["entry", "global"].includes(role)) return null;
  if (runtime.realmContextIds[role] === contextId) {
    return role === "global" ? runtime.globalRealm : runtime.entryRealm;
  }
  const retired = runtime.retiredRealmContexts.get(contextId);
  return retired?.role === role ? retired.realm : null;
}

function createPluginRealmContextId() {
  const randomId = globalThis.crypto?.randomUUID?.()
    || `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}-${Math.random().toString(36).slice(2)}`;
  return String(randomId).toLocaleLowerCase();
}

async function createPluginRealmContext(runtime, role) {
  const contextId = createPluginRealmContextId();
  const realm = await createWebKitPluginRealm({
    identifier: runtime.spec.identifier,
    role,
  });
  Object.defineProperty(realm, "contextId", {
    value: contextId,
    enumerable: true,
  });
  return {
    contextId,
    realm,
    lease: {
      active: true,
      contextId,
      identifier: runtime.spec.identifier,
      role,
    },
  };
}

async function registerPluginDeveloperToolRealmContext(runtime, role) {
  const contextId = runtime.realmContextIds[role];
  if (!contextId) throw new Error(`Plugin ${runtime.spec.identifier} ${role} context is unavailable`);
  await invoke("set_plugin_developer_tool_realm_context", {
    identifier: runtime.spec.identifier,
    role,
    contextId,
  });
}

function createPluginRuntime(spec) {
  return {
    spec,
    fingerprint: pluginRuntimeFingerprint(spec),
    menuItems: { entry: [], global: [] },
    menuItemsById: new Map(),
    moduleExports: new Map(),
    globalModuleExports: new Map(),
    entryRealm: null,
    globalRealm: null,
    realmContextIds: { entry: null, global: null },
    realmLeases: { entry: null, global: null },
    retiredRealmContexts: new Map(),
    syncTransports: { entry: null, global: null },
    overlay: null,
    sidebar: null,
    pluginPages: {
      overlay: null,
      sidebar: null,
      standalone: { entry: null, global: null },
    },
    eventListeners: new Map(),
    inputListeners: new Map(),
    subtitleProviders: new Map(),
    playlistMenuItemBuilder: null,
    utilsExecHooks: new Map(),
    utilsExecRequestPrefix: globalThis.crypto?.randomUUID?.() || `${Date.now()}-${Math.random().toString(36).slice(2)}`,
    nextUtilsExecRequestId: 0,
    globalControllerRegistered: false,
    globalControllerListeners: new Map(),
    globalChildListeners: new Map(),
    globalControllerTask: Promise.resolve(),
    nextGlobalInstanceId: 0,
    mpvHookCallbacks: new Map(),
    mpvHookTask: Promise.resolve(),
    mpvHooksDisposed: false,
    nextMpvHookCallbackId: 1,
    websocket: {
      entry: createPluginWebSocketController(),
      global: createPluginWebSocketController(),
    },
    preferenceValues: {
      ...(spec.preference_defaults || {}),
      ...readPluginPreferenceValues(spec.identifier),
    },
    nextMenuItemId: 0,
    nextEventListenerId: 0,
  };
}

async function runPluginRuntime(runtime) {
  runtime.syncTransports.entry = await createPluginRoleSyncTransport(runtime, "entry");
  if (runtime.spec.global_entry && isPrimaryPlayerWindow) {
    runtime.syncTransports.global = await createPluginRoleSyncTransport(runtime, "global");
    await invoke("plugin_global_register_controller", { identifier: runtime.spec.identifier });
    runtime.globalControllerRegistered = true;
    const globalContext = await createPluginRealmContext(runtime, "global");
    runtime.globalRealm = globalContext.realm;
    runtime.realmContextIds.global = globalContext.contextId;
    runtime.realmLeases.global = globalContext.lease;
    const api = createIinaPluginApi(runtime, "controller");
    executePluginScript(
      runtime,
      runtime.spec.global_entry,
      api,
      false,
      runtime.globalModuleExports,
      runtime.globalRealm,
    );
    await registerPluginDeveloperToolRealmContext(runtime, "global");
  }
  await runPluginEntryRuntime(runtime);
}

function createPluginRoleSyncTransport(runtime, role) {
  return createPluginSyncTransport({
    identifier: runtime.spec.identifier,
    role,
    invoke,
    XMLHttpRequestClass: window.XMLHttpRequest,
  });
}

async function runPluginEntryRuntime(runtime) {
  const entryContext = await createPluginRealmContext(runtime, "entry");
  runtime.entryRealm = entryContext.realm;
  runtime.realmContextIds.entry = entryContext.contextId;
  runtime.realmLeases.entry = entryContext.lease;
  const api = createIinaPluginApi(runtime, "child");
  executePluginScript(runtime, runtime.spec.entry, api, false, runtime.moduleExports, runtime.entryRealm);
  await registerPluginDeveloperToolRealmContext(runtime, "entry");
}

function executePluginScript(
  runtime,
  path,
  api,
  asModule,
  moduleExports = runtime.moduleExports,
  realmOverride = null,
  scripts = runtime.spec.scripts,
) {
  const source = scripts?.[path];
  const realm = realmOverride || (moduleExports === runtime.globalModuleExports
    ? runtime.globalRealm
    : runtime.entryRealm);
  if (!realm) throw new Error(`Plugin ${runtime.spec.identifier} runtime realm is unavailable`);
  return realm.evaluate({
    path,
    source,
    api,
    asModule,
    moduleExports,
    requireModule: (request) => {
      const nextPath = resolvePluginModulePath(runtime, path, request, scripts);
      return executePluginScript(runtime, nextPath, api, true, moduleExports, realm, scripts);
    },
    pluginConsole: createPluginConsole(runtime, realm.role, realm.contextId),
  });
}

function resolvePluginModulePath(runtime, fromPath, request, scripts = runtime.spec.scripts) {
  if (typeof request !== "string" || !request) {
    throw new Error("Plugin require() expects a relative script path");
  }
  if (!request.startsWith(".")) {
    throw new Error(`Plugin module is outside its package: ${request}`);
  }
  const parts = fromPath.split("/");
  parts.pop();
  for (const part of request.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (!parts.length) throw new Error(`Plugin module escapes its package: ${request}`);
      parts.pop();
      continue;
    }
    parts.push(part);
  }
  const requested = parts.join("/");
  const candidates = [requested, `${requested}.js`, `${requested}.mjs`, `${requested}/index.js`];
  const resolved = candidates.find((candidate) => typeof scripts?.[candidate] === "string");
  if (!resolved) throw new Error(`Plugin module not found: ${request}`);
  return resolved;
}

function createPluginConsole(runtime, role = "entry", contextId = runtime.realmContextIds[role]) {
  const prefix = `[${runtime.spec.identifier}]`;
  const valueString = (value) => {
    if (value === null) return "<null>";
    if (value === undefined) return "<undefined>";
    if (typeof value === "symbol") return value.toString();
    if (typeof value === "string") return value;
    if (Object.prototype.toString.call(value) === "[object Date]" || ["number", "boolean"].includes(typeof value)) {
      return value.toString();
    }
    const encoded = JSON.stringify(value, null, 2);
    return encoded === undefined ? "undefined" : encoded;
  };
  const logMessage = (values) => {
    if (values.length === 1) return valueString(values[0]);
    return values.map((value) => `${valueString(value)}${typeof value === "object" && value !== null ? "\n" : " "}`).join("");
  };
  const emitDeveloperLog = (level, message) => {
    const target = pluginDeveloperToolTargets.get(contextId);
    if (
      !target
      || target.identifier !== runtime.spec.identifier
      || target.role !== role
      || !tauriEmitTo
    ) return;
    void tauriEmitTo(target.windowLabel, "iima-plugin-developer-tool-log", {
      contextId,
      level,
      message,
    });
  };
  return {
    log: (...values) => {
      const message = logMessage(values);
      console.log(prefix, message);
      emitDeveloperLog("debug", message);
    },
    warn: (value) => {
      const message = valueString(value);
      console.warn(prefix, message);
      emitDeveloperLog("warning", message);
    },
    error: (value) => {
      const message = valueString(value);
      console.error(prefix, message);
      emitDeveloperLog("error", message);
    },
  };
}

function serializePluginDeveloperValue(value) {
  if (value === undefined) return { kind: "undefined" };
  if (value === null) return { kind: "null" };
  if (typeof value === "number") return { kind: "number", value: String(value) };
  if (typeof value === "string") return { kind: "string", value };
  if (typeof value === "boolean") return { kind: "boolean", value };
  const stringify = (candidate) => {
    try { return String(candidate); } catch { return "<unprintable>"; }
  };
  if (Array.isArray(value)) {
    return {
      kind: "array",
      title: `Array (${value.length})`,
      entries: Array.from(value)
        .slice(0, 100)
        .map((entry, index) => [String(index), stringify(entry)]),
      remaining: Math.max(0, value.length - 100),
    };
  }
  if ((typeof value === "object" || typeof value === "function") && value !== null) {
    let keys = [];
    try { keys = Reflect.ownKeys(value).map(String); } catch { /* opaque host object */ }
    if (keys.length) {
      return {
        kind: "object",
        title: stringify(value),
        entries: keys.slice(0, 100).map((key) => {
          try { return [key, stringify(value[key])]; } catch { return [key, "<unavailable>"]; }
        }),
        remaining: Math.max(0, keys.length - 100),
      };
    }
    return { kind: "opaque", value: stringify(value) };
  }
  return { kind: "opaque", value: stringify(value) };
}

function bindPluginApiToRealmLease(api, lease) {
  if (!lease) throw new Error("Plugin API requires a realm-context lease");
  const guarded = new WeakMap();
  const unguarded = new WeakMap();
  const assertActive = () => {
    if (!lease.active) {
      throw new Error(`Plugin ${lease.identifier} ${lease.role} context is no longer active`);
    }
  };
  const canProxyObject = (value) => {
    if (!value || typeof value !== "object") return false;
    if (ArrayBuffer.isView(value) || value instanceof ArrayBuffer || value instanceof Promise) return false;
    const prototype = Object.getPrototypeOf(value);
    return prototype === Object.prototype || prototype === null || Array.isArray(value);
  };
  const wrap = (value, allowRetired = false) => {
    if (typeof value === "function") {
      const cache = allowRetired ? unguarded : guarded;
      if (cache.has(value)) return cache.get(value);
      const wrapped = function pluginRealmCapability(...args) {
        if (!allowRetired) assertActive();
        const result = Reflect.apply(value, this, args);
        if (result instanceof Promise) return result.then((resolved) => wrap(resolved, allowRetired));
        return wrap(result, allowRetired);
      };
      cache.set(value, wrapped);
      return wrapped;
    }
    if (!canProxyObject(value)) return value;
    const cache = allowRetired ? unguarded : guarded;
    if (cache.has(value)) return cache.get(value);
    const proxy = new Proxy(value, {
      get(target, property, receiver) {
        const nestedAllowsRetired = allowRetired || (target === api && property === "console");
        return wrap(Reflect.get(target, property, receiver), nestedAllowsRetired);
      },
      set(target, property, nextValue, receiver) {
        if (!allowRetired) assertActive();
        return Reflect.set(target, property, nextValue, receiver);
      },
      deleteProperty(target, property) {
        if (!allowRetired) assertActive();
        return Reflect.deleteProperty(target, property);
      },
      defineProperty(target, property, descriptor) {
        if (!allowRetired) assertActive();
        return Reflect.defineProperty(target, property, descriptor);
      },
    });
    cache.set(value, proxy);
    return proxy;
  };
  return wrap(api);
}

function createIinaPluginApi(runtime, globalRole = "child") {
  const hasPermission = (permission) => Array.from(runtime.spec.permissions || []).includes(permission);
  const requirePermission = (permission) => {
    if (!hasPermission(permission)) {
      throw new Error(`To call this API, the plugin must declare permission "${permission}" in its Info.json.`);
    }
  };
  const role = globalRole === "controller" ? "global" : "entry";
  const realmLease = runtime.realmLeases[role];
  if (!realmLease) throw new Error(`Plugin ${runtime.spec.identifier} ${role} lease is unavailable`);
  const syncTransport = runtime.syncTransports[role];
  if (!syncTransport) {
    throw new Error(`Plugin ${runtime.spec.identifier} synchronization transport is unavailable`);
  }
  const invokeSync = syncTransport.invokeSync;
  const pluginRealm = globalRole === "controller" ? runtime.globalRealm : runtime.entryRealm;
  if (!pluginRealm) throw new Error(`Plugin ${runtime.spec.identifier} realm is unavailable`);
  const applyPlayerCommand = (payload) => {
    void command(payload).catch((error) => console.error(`[${runtime.spec.identifier}] player command failed`, error));
  };
  const api = {
    console: createPluginConsole(runtime, role, realmLease.contextId),
    menu: createPluginMenuApi(runtime, role),
    standaloneWindow: createPluginStandaloneWindowApi(runtime, invokeSync, role),
    preferences: {
      get: (key) => {
        if (Object.prototype.hasOwnProperty.call(runtime.preferenceValues, key)) return runtime.preferenceValues[key];
        console.warn(`Trying to get preference value for undefined key ${key}`);
        return undefined;
      },
      set: (key, value) => {
        runtime.preferenceValues[key] = value;
      },
      sync: () => writePluginPreferenceValues(runtime.spec.identifier, runtime.preferenceValues),
    },
    utils: createPluginUtilsApi(runtime, hasPermission, invokeSync, globalRole !== "controller", role),
    http: createPluginHttpApi(runtime, hasPermission, globalRole !== "controller", invokeSync),
    file: createPluginFileApi(
      runtime,
      invokeSync,
      (bytes) => pluginRealm.createUint8Array(bytes),
      hasPermission,
      globalRole !== "controller",
    ),
    ws: createPluginWebSocketApi(runtime, invokeSync, role),
  };
  if (globalRole !== "controller") {
    Object.assign(api, {
      core: createPluginCoreApi(runtime, requirePermission, invokeSync, applyPlayerCommand),
      mpv: createPluginMpvApi(runtime, invokeSync),
      event: createPluginEventApi(runtime),
      input: createPluginInputApi(runtime),
      overlay: createPluginOverlayApi(runtime, hasPermission),
      sidebar: createPluginSidebarApi(runtime),
      playlist: createPluginPlaylistApi(runtime),
      subtitle: createPluginSubtitleApi(runtime),
    });
  }
  if (runtime.spec.global_entry) {
    api.global = globalRole === "controller"
      ? createPluginGlobalControllerApi(runtime)
      : createPluginGlobalChildApi(runtime);
  }
  return bindPluginApiToRealmLease(api, realmLease);
}

function createPluginCoreApi(runtime, requirePermission, invokeSync, applyPlayerCommand) {
  const pluginError = (message) => console.error(`[${runtime.spec.identifier}] ${message}`);
  const localFileUrl = (path, decode = false) => {
    const value = String(path || "");
    if (/^[a-z][a-z0-9+.-]*:/i.test(value)) {
      if (!decode) return value;
      try { return decodeURIComponent(value); } catch { return value; }
    }
    const encoded = encodeURI(`file://${value.startsWith("/") ? "" : "/"}${value}`);
    if (!decode) return encoded;
    try { return decodeURIComponent(encoded); } catch { return encoded; }
  };
  const rawTracks = (kind) => {
    if (!state.current_url) return [];
    return Array.from(state.tracks?.[kind] || []).filter((track) => {
      if (kind === "subtitles" && Number(track.id) === 0) return false;
      const metadata = track.metadata || {};
      const placeholder = /^Default (Video|Audio) Track$/.test(track.title || "")
        && metadata.source_id == null
        && metadata.source_title == null
        && metadata.codec == null
        && metadata.demux_width == null
        && metadata.demux_channel_count == null
        && !metadata.external;
      return !placeholder;
    });
  };
  const selectedTrack = (kind) => rawTracks(kind).find((track) => track.selected) || null;
  const serializeTrack = (track) => {
    const metadata = track?.metadata || {};
    return {
      id: Number(track?.id) || 0,
      title: metadata.source_title ?? null,
      formattedTitie: track?.title || metadata.source_title || "",
      lang: metadata.language ?? null,
      codec: metadata.codec ?? null,
      isDefault: Boolean(metadata.default_track),
      isForced: Boolean(metadata.forced),
      isSelected: Boolean(track?.selected),
      isExternal: Boolean(metadata.external),
      demuxW: metadata.demux_width ?? null,
      demuxH: metadata.demux_height ?? null,
      demuxChannelCount: metadata.demux_channel_count ?? null,
      demuxChannels: metadata.demux_channels ?? null,
      demuxSamplerate: metadata.demux_samplerate ?? null,
      demuxFPS: metadata.demux_fps ?? null,
    };
  };
  const tracks = (kind) => Array.from(rawTracks(kind), serializeTrack);
  const currentTrack = (kind) => {
    const track = selectedTrack(kind);
    return track ? serializeTrack(track) : null;
  };
  const setTrack = (kind, value, label) => {
    if (!Number.isInteger(value)) {
      pluginError(`core.${label}.id: Should be a number`);
      return;
    }
    applyPlayerCommand({ type: "select-track", kind, id: value });
  };
  const setDelay = (type, value) => {
    if (typeof value !== "number") {
      pluginError(`core.${type}.delay: Should be a number`);
      return;
    }
    applyPlayerCommand({ type: type === "audio" ? "set-audio-delay" : "set-sub-delay", seconds: value });
  };
  const loadTrack = (kind, track) => {
    if (typeof track !== "string") {
      pluginError("loadTrack: the url must be a string");
      return;
    }
    applyPlayerCommand({ type: "load-external-track", kind, path: track });
  };
  const core = {
    open: (path) => {
      const checkedPath = pluginPathForApi(String(path), {
        hasFileSystemPermission: Array.from(runtime.spec.permissions || []).includes("file-system"),
        playerAvailable: true,
        forceLocalPath: false,
      });
      let resolvedPath;
      try {
        resolvedPath = invokeSync("core.resolveopen", { path: checkedPath });
      } catch (error) {
        // parsePath logs and returns nil for @current when no media is loaded;
        // it does not expose a JavaScript exception in that one case.
        if (String(error?.message || error).includes("@current is unavailable")) return;
        throw error;
      }
      void invoke("open_media", { path: resolvedPath })
        .then((nextState) => setPlayerState(nextState, { force: true, presentOsd: true }))
        .catch((error) => pluginError(`open failed: ${error}`));
    },
    osd: (message) => {
      requirePermission("show-osd");
      showOsd(String(message || ""), {
        literal: true,
        detail: `From plugin ${runtime.spec.name}`,
      });
    },
    pause: () => applyPlayerCommand({ type: "pause" }),
    resume: () => applyPlayerCommand({ type: "resume" }),
    stop: () => applyPlayerCommand({ type: "stop" }),
    seek: (seconds, exact = false) => applyPlayerCommand({
      type: "seek-relative",
      seconds: Number(seconds),
      option: exact ? "exact" : "relative",
    }),
    seekTo: (seconds) => applyPlayerCommand({ type: "seek-absolute", seconds: Number(seconds) }),
    setSpeed: (speed) => applyPlayerCommand({ type: "set-speed", speed: Number(speed) }),
    getChapters: () => Array.from(state.chapters || []).map((chapter) => ({
      title: chapter.title,
      start: chapter.time_seconds,
    })),
    playChapter: (index) => {
      if (Number.isInteger(index)) applyPlayerCommand({ type: "select-chapter", index });
    },
    setUIVisibility: (visible) => {
      document.documentElement.classList.toggle("plugin-managed-ui-disabled", Boolean(visible));
    },
    getHistory: () => Array.from(invokeSync("core.history", {}) || [], (item) => ({
      name: item.name,
      url: localFileUrl(item.path),
      date: new Date(item.added_date),
      progress: item.progress_seconds ?? null,
      duration: Number(item.duration_seconds) || 0,
    })),
    getRecentDocuments: () => Array.from(state.recent_documents || [], (item) => ({
      name: item.title || String(item.path || "").split("/").pop() || "",
      url: localFileUrl(item.path),
    })),
    getVersion: () => ({
      ...invokeSync("core.version", {}),
      mpv: decodePluginMpvValue(invokeSync("mpv.get", { property: "mpv-version", kind: "string" })),
    }),
  };

  core.window = pluginStateProxy(
    (prop) => {
      const snapshot = invokeSync("core.window.snapshot", {}) || {};
      if (prop === "loaded") return Boolean(snapshot.loaded);
      if (prop === "frame") return snapshot.frame ?? pluginWindowRect();
      if (prop === "fullscreen") return Boolean(snapshot.fullscreen);
      if (prop === "pip") return Boolean(state.pip_active);
      if (prop === "ontop") return Boolean(snapshot.ontop);
      if (prop === "visible") return Boolean(snapshot.visible);
      if (prop === "sidebar") {
        return state.sidebar?.visible && ["video", "audio", "subtitles", "playlist", "chapters"].includes(state.sidebar.tab)
          ? state.sidebar.tab
          : null;
      }
      if (prop === "screens") return Array.from(snapshot.screens || []);
      return undefined;
    },
    (prop, value) => {
      if (prop === "frame") {
        const frame = value && typeof value === "object" ? value : {};
        const valid = ["x", "y", "width", "height"].every((key) => typeof frame[key] === "number" && Number.isFinite(frame[key]))
          && frame.width > 0 && frame.height > 0;
        if (!valid) {
          pluginError("core.window.frame: Invalid frame");
          return;
        }
        invokeSync("core.window.setframe", { frame });
      } else if (prop === "fullscreen" && typeof value === "boolean") {
        const current = Boolean(invokeSync("core.window.snapshot", {})?.fullscreen);
        if (value !== current) void invoke("set_window_fullscreen", { fullscreen: value }).then(applyFullscreenState).catch(pluginError);
      } else if (prop === "pip" && typeof value === "boolean") {
        if (value !== Boolean(state.pip_active)) {
          void invoke("toggle_picture_in_picture").then((nextState) => setPlayerState(nextState, { force: true })).catch(pluginError);
        }
      } else if (prop === "ontop" && typeof value === "boolean") {
        const current = Boolean(invokeSync("core.window.snapshot", {})?.ontop);
        if (value !== current) void invoke("toggle_window_always_on_top").catch(pluginError);
      } else if (prop === "sidebar") {
        if (typeof value === "string") {
          if (["video", "audio", "subtitles", "playlist", "chapters"].includes(value)) {
            applyPlayerCommand({ type: "show-sidebar", tab: value });
          } else {
            pluginError(`core.window.sidebar: Unknown sidebar name "${value}"`);
          }
        } else {
          applyPlayerCommand({ type: "hide-sidebar" });
        }
      } else {
        console.warn(`[${runtime.spec.identifier}] core.window: ${String(prop)} is not accessible`);
      }
    }
  );

  core.status = pluginStateProxy((prop) => {
    const video = selectedTrack("video")?.metadata || {};
    const source = state.current_url;
    const position = Number(state.position_seconds);
    const duration = Number(state.duration_seconds);
    if (prop === "paused") return Boolean(state.paused);
    if (prop === "idle") return Boolean(state.mpv_properties?.["idle-active"] ?? !source);
    if (prop === "position") return source && Number.isFinite(position) ? position : null;
    if (prop === "duration") return source && Number.isFinite(duration) ? duration : null;
    if (prop === "speed") return Number(state.speed) || 1;
    if (prop === "videoWidth") return video.demux_width ?? null;
    if (prop === "videoHeight") return video.demux_height ?? null;
    if (prop === "isNetworkResource") return typeof source === "string" && /^[a-z][a-z0-9+.-]*:\/\//i.test(source) && !source.startsWith("file://");
    if (prop === "url") return source ? localFileUrl(source, true) : null;
    if (prop === "title") return state.media_title || "";
    return undefined;
  }, (prop) => console.warn(`[${runtime.spec.identifier}] core.status: ${String(prop)} is not accessible`));

  core.audio = pluginStateProxy(
    (prop) => ({
      id: state.current_url ? (selectedTrack("audio")?.id ?? null) : null,
      delay: Number(state.quick_settings?.audio_delay) || 0,
      tracks: tracks("audio"),
      currentTrack: currentTrack("audio"),
      volume: Number(state.volume) || 0,
      muted: Boolean(state.muted),
    })[prop],
    (prop, value) => {
      if (prop === "id") setTrack("audio", value, "audio");
      else if (prop === "delay") setDelay("audio", value);
      else if (prop === "volume" && typeof value === "number") applyPlayerCommand({ type: "set-volume", volume: value });
      else if (prop === "muted" && typeof value === "boolean" && value !== Boolean(state.muted)) applyPlayerCommand({ type: "toggle-mute" });
      else if (prop === "volume") pluginError("core.audio.volume: Should be a number");
      else if (prop === "muted") pluginError("core.audio.muted: Should be a boolean value");
    },
    { loadTrack: (track) => loadTrack("audio", track) }
  );
  core.subtitle = pluginStateProxy(
    (prop) => ({
      id: state.current_url ? (selectedTrack("subtitles")?.id ?? null) : null,
      secondID: state.current_url ? (state.second_subtitle_id ?? null) : null,
      delay: Number(state.quick_settings?.sub_delay) || 0,
      tracks: tracks("subtitles"),
      currentTrack: currentTrack("subtitles"),
    })[prop],
    (prop, value) => {
      if (prop === "id") setTrack("subtitles", value, "subtitle");
      else if (prop === "secondID") setTrack("second-subtitles", value, "subtitle");
      else if (prop === "delay") setDelay("subtitle", value);
    },
    { loadTrack: (track) => loadTrack("subtitles", track) }
  );
  core.video = pluginStateProxy(
    (prop) => ({
      id: state.current_url ? (selectedTrack("video")?.id ?? null) : null,
      tracks: tracks("video"),
      currentTrack: currentTrack("video"),
    })[prop],
    (prop, value) => { if (prop === "id") setTrack("video", value, "video"); },
    { loadTrack: (track) => loadTrack("video", track) }
  );
  return core;
}

function createPluginGlobalControllerApi(runtime) {
  const identifier = runtime.spec.identifier;
  return {
    createPlayerInstance: (options = {}) => {
      if (!Number.isSafeInteger(runtime.nextGlobalInstanceId + 1)) {
        throw new Error("Plugin managed-player identifier overflow");
      }
      const instanceId = ++runtime.nextGlobalInstanceId;
      const operation = runtime.globalControllerTask.then(() => invoke("plugin_global_create_player_instance", {
        identifier,
        instanceId,
        options: options && typeof options === "object" ? options : {},
      }));
      runtime.globalControllerTask = operation.catch((error) => {
        console.error(`[${identifier}] unable to create managed player ${instanceId}`, error);
      });
      return instanceId;
    },
    postMessage: (target, name, data = null) => {
      const operation = runtime.globalControllerTask.then(() => invoke("plugin_global_post_to_child", {
        identifier,
        target: target ?? null,
        name: String(name ?? ""),
        data: data ?? null,
      }));
      runtime.globalControllerTask = operation.catch((error) => {
        console.error(`[${identifier}] unable to post global child message`, error);
      });
    },
    onMessage: (name, callback) => {
      runtime.globalControllerListeners.set(String(name ?? ""), callback);
    },
  };
}

function createPluginGlobalChildApi(runtime) {
  const identifier = runtime.spec.identifier;
  return {
    getLabel: () => managedPluginUserLabel,
    postMessage: (name, data = null) => {
      void invoke("plugin_global_post_to_controller", {
        identifier,
        name: String(name ?? ""),
        data: data ?? null,
      }).catch((error) => {
        console.error(`[${identifier}] unable to post global controller message`, error);
      });
    },
    onMessage: (name, callback) => {
      runtime.globalChildListeners.set(String(name ?? ""), callback);
    },
  };
}

function createPluginUtilsApi(runtime, hasPermission, invokeSync, playerAvailable = true, role = "entry") {
  const call = (command, args = {}) => invoke(command, {
    identifier: runtime.spec.identifier,
    ...args,
  });
  return {
    ERROR_BINARY_NOT_FOUND: -1,
    ERROR_RUNTIME: -2,
    fileInPath: (file) => withPluginFileSystemPermission(hasPermission, false, () => {
      const fileValue = String(file || "");
      const checkedFile = fileValue.includes("/") || fileValue.startsWith("@") || fileValue.startsWith("~")
        ? pluginPathForApi(fileValue, {
          hasFileSystemPermission: true,
          playerAvailable,
        })
        : fileValue;
      return Boolean(invokeSync("utils.fileinpath", { file: checkedFile }));
    }),
    resolvePath: (path) => withPluginFileSystemPermission(hasPermission, null, () => {
      const checkedPath = pluginPathForApi(String(path || ""), {
        hasFileSystemPermission: true,
        playerAvailable,
      });
      return invokeSync("utils.resolvepath", { path: checkedPath });
    }),
    exec: (file, args = [], cwd = null, stdoutHook = null, stderrHook = null) => withPluginFileSystemPermission(hasPermission, null, () => {
      const fileValue = String(file || "");
      const checkedFile = fileValue.includes("/")
        ? pluginPathForApi(fileValue, {
          hasFileSystemPermission: true,
          playerAvailable,
        })
        : fileValue;
      const checkedCwd = typeof cwd === "string"
        ? pluginPathForApi(cwd, {
          hasFileSystemPermission: true,
          playerAvailable,
        })
        : null;
      const argumentsList = Array.from(args || []).map(String);
      const hasHooks = typeof stdoutHook === "function" || typeof stderrHook === "function";
      const requestId = hasHooks
        ? `${runtime.utilsExecRequestPrefix}-${++runtime.nextUtilsExecRequestId}`
        : null;
      if (requestId) {
        runtime.utilsExecHooks.set(requestId, {
          role,
          stdout: typeof stdoutHook === "function" ? stdoutHook : null,
          stderr: typeof stderrHook === "function" ? stderrHook : null,
        });
      }
      return call("plugin_utils_exec", {
        file: checkedFile,
        args: argumentsList,
        cwd: checkedCwd,
        role,
        requestId,
      }).catch((error) => {
        const message = String(error?.message || error);
        if (message.includes("Cannot find the binary")) throw -1;
        if (message.includes("not executable, and execute permission cannot be added")) throw -2;
        throw error;
      }).finally(() => {
        if (requestId) runtime.utilsExecHooks.delete(requestId);
      });
    }),
    ask: (title) => Boolean(invokeSync("utils.ask", { title: String(title || "") })),
    prompt: (title) => invokeSync("utils.prompt", { title: String(title || "") }),
    chooseFile: (title, options = {}) => call("plugin_utils_choose_file", {
      title: String(title || ""),
      chooseDir: Boolean(options?.chooseDir),
      allowedFileTypes: Array.isArray(options?.allowedFileTypes)
        ? options.allowedFileTypes.map(String)
        : null,
    }).then((path) => path ?? new Promise(() => {})),
    keychainWrite: (service, name, password) => {
      try {
        return Boolean(invokeSync("utils.keychainwrite", {
          service: String(service || ""),
          name: String(name ?? ""),
          password: String(password ?? ""),
        }));
      } catch {
        return false;
      }
    },
    keychainRead: (service, name) => {
      try {
        return invokeSync("utils.keychainread", {
          service: String(service || ""),
          name: String(name ?? ""),
        }) ?? false;
      } catch {
        return false;
      }
    },
    open: (url) => {
      const value = String(url || "");
      let isWebUrl = false;
      try {
        isWebUrl = ["http:", "https:"].includes(new URL(value).protocol);
      } catch {
        // It may still be a local path handled by parsePath below.
      }
      const checkedValue = isWebUrl
        ? value
        : pluginPathForApi(value, {
          hasFileSystemPermission: hasPermission("file-system"),
          playerAvailable,
        });
      try {
        return Boolean(invokeSync("utils.open", { url: checkedValue }));
      } catch (error) {
        if (String(error?.message || error).includes("@current is unavailable")) return false;
        throw error;
      }
    },
  };
}

function createPluginPlaylistApi(runtime) {
  const isPlaying = () => Boolean(state.current_url) && state.mode !== "initial";
  const reportUnavailable = () => {
    console.error(`[${runtime.spec.identifier}] Playlist API is only available when playing files.`);
    return false;
  };
  const applyState = (promise) => {
    void promise
      .then((nextState) => setPlayerState(nextState, { force: true }))
      .catch((error) => console.error(`[${runtime.spec.identifier}] playlist operation failed`, error));
  };
  return {
    list: () => {
      if (!isPlaying()) {
        reportUnavailable();
        return [];
      }
      return Array.from(state.playlist || []).map((entry) => ({
        filename: entry.path,
        title: entry.title || null,
        isPlaying: Boolean(entry.playing),
        isCurrent: Boolean(entry.current),
      }));
    },
    count: () => Array.from(state.playlist || []).length,
    add: (url, at = -1) => {
      if (!isPlaying()) return reportUnavailable();
      const paths = typeof url === "string"
        ? [url]
        : Array.isArray(url) && url.every((path) => typeof path === "string")
          ? url
          : null;
      const count = state.playlist?.length ?? 0;
      const index = Number(at);
      if (!paths || !Number.isInteger(index) || index >= count) {
        console.error(`[${runtime.spec.identifier}] playlist.add: Invalid path or index.`);
        return false;
      }
      applyState(invoke("playlist_insert_items", {
        paths,
        destination: index < 0 ? count : index,
      }));
      return true;
    },
    remove: (index) => {
      if (!isPlaying()) return reportUnavailable();
      const indexes = Array.isArray(index) ? index : [index];
      const count = state.playlist?.length ?? 0;
      if (indexes.some((value) => !Number.isInteger(value) || value < 0 || value >= count)) {
        console.error(`[${runtime.spec.identifier}] playlist.remove: Invalid index.`);
        return false;
      }
      applyState(invoke("playlist_remove_items", { indexes }));
      return true;
    },
    move: (index, to) => {
      if (!isPlaying()) return reportUnavailable();
      const count = state.playlist?.length ?? 0;
      if (![index, to].every((value) => Number.isInteger(value) && value >= 0 && value < count) || index === to) {
        console.error(`[${runtime.spec.identifier}] playlist.move: Invalid index.`);
        return false;
      }
      const destination = index < to ? to + 1 : to;
      applyState(invoke("player_command", {
        command: { type: "move-playlist-items", indexes: [index], destination },
      }));
      return true;
    },
    play: (index) => {
      if (!isPlaying()) return;
      if (!Number.isInteger(index) || index < 0 || index >= (state.playlist?.length ?? 0)) {
        console.error(`[${runtime.spec.identifier}] playlist.play: Invalid index.`);
        return;
      }
      void command({ type: "select-playlist-item", index })
        .catch((error) => console.error(`[${runtime.spec.identifier}] playlist operation failed`, error));
    },
    playNext: () => {
      if (isPlaying()) void command({ type: "playlist-next" })
        .catch((error) => console.error(`[${runtime.spec.identifier}] playlist operation failed`, error));
      else reportUnavailable();
    },
    playPrevious: () => {
      if (isPlaying()) void command({ type: "playlist-prev" })
        .catch((error) => console.error(`[${runtime.spec.identifier}] playlist operation failed`, error));
      else reportUnavailable();
    },
    registerMenuItemBuilder: (builder) => {
      runtime.playlistMenuItemBuilder = typeof builder === "function" ? builder : null;
    },
  };
}

function pluginStateProxy(read, write = () => {}, methods = {}) {
  return new Proxy({}, {
    get: (_, key) => {
      if (key === "loadTrack" && typeof methods.loadTrack === "function") return methods.loadTrack.bind(methods);
      if (typeof key === "string" && key.startsWith("__")) return null;
      if (Object.prototype.hasOwnProperty.call(methods, key)) {
        const value = methods[key];
        return typeof value === "function" ? value.bind(methods) : value;
      }
      return read(key);
    },
    set: (_, key, value) => {
      write(key, value);
      // IINA's injected Proxy setter has no explicit return value. Preserve its false result
      // (and therefore strict-mode assignment behavior) after applying the native side effect.
      return false;
    },
  });
}

function pluginMenuItemKey(role, itemId) {
  return `${role}:${itemId}`;
}

function createPluginMenuApi(runtime, role) {
  const roleItems = runtime.menuItems[role];
  const item = (title, action, options = {}) => {
    let itemTitle = String(title);
    let itemSelected = Boolean(options?.selected);
    let itemEnabled = options?.enabled === undefined ? true : Boolean(options.enabled);
    let itemKeyBinding = options?.keyBinding === undefined ? null : String(options.keyBinding);
    const entry = {
      id: `item_${++runtime.nextMenuItemId}`,
      action: action === null ? null : action,
      items: [],
      addSubMenuItem(child) {
        this.items.push(child);
        return this;
      },
    };
    const liveUpdate = () => {
      if (runtime.menuItemsById.has(pluginMenuItemKey(role, entry.id))) {
        queuePluginMenuSync(runtime, role);
      }
    };
    Object.defineProperties(entry, {
      title: {
        enumerable: true,
        get: () => itemTitle,
        set: (value) => { itemTitle = String(value); liveUpdate(); },
      },
      selected: {
        enumerable: true,
        get: () => itemSelected,
        set: (value) => { itemSelected = Boolean(value); liveUpdate(); },
      },
      enabled: {
        enumerable: true,
        get: () => itemEnabled,
        set: (value) => { itemEnabled = Boolean(value); liveUpdate(); },
      },
      keyBinding: {
        enumerable: true,
        get: () => itemKeyBinding,
        set: (value) => { itemKeyBinding = value == null ? null : String(value); liveUpdate(); },
      },
    });
    return entry;
  };
  return {
    item,
    separator: () => {
      const entry = item("", null, { enabled: false, selected: false });
      entry.separator = true;
      return entry;
    },
    addItem: (entry) => roleItems.push(entry),
    items: () => Array.from(roleItems),
    removeAt: (index) => {
      if (!Number.isInteger(index) || index < 0 || index >= roleItems.length) return false;
      roleItems.splice(index, 1);
      return true;
    },
    removeAllItems: () => {
      roleItems.splice(0, roleItems.length);
      for (const key of runtime.menuItemsById.keys()) {
        if (key.startsWith(`${role}:`)) runtime.menuItemsById.delete(key);
      }
    },
    forceUpdate: () => {
      queuePluginMenuSync(runtime, role);
    },
  };
}

function queuePluginMenuSync(runtime, role) {
  runtime.menuSyncTask = (runtime.menuSyncTask || Promise.resolve())
    .catch(() => undefined)
    .then(() => syncPluginMenu(runtime, role));
  void runtime.menuSyncTask.catch((error) => {
    console.error(`[${runtime.spec.identifier}] plugin menu update failed`, error);
  });
}

function serializePluginMenuItem(runtime, role, item) {
  if (item?.separator) {
    return { id: String(item.id), title: "", separator: true, items: [] };
  }
  runtime.menuItemsById.set(pluginMenuItemKey(role, item.id), item);
  return {
    id: String(item.id),
    title: String(item.title || ""),
    enabled: item.enabled !== false,
    selected: Boolean(item.selected),
    key_binding: item.keyBinding || null,
    separator: false,
    items: Array.from(item.items || []).map((child) => serializePluginMenuItem(runtime, role, child)),
  };
}

async function syncPluginMenu(runtime, role) {
  for (const key of runtime.menuItemsById.keys()) {
    if (key.startsWith(`${role}:`)) runtime.menuItemsById.delete(key);
  }
  const items = runtime.menuItems[role].map((item) => serializePluginMenuItem(runtime, role, item));
  await invoke("set_plugin_menu_items", { identifier: runtime.spec.identifier, role, items });
}

async function syncPluginMenus(runtime) {
  if (runtime.globalRealm) await syncPluginMenu(runtime, "global");
  await syncPluginMenu(runtime, "entry");
}

function pluginPageState(runtime, surface, role = "entry") {
  return surface === "standalone"
    ? runtime.pluginPages.standalone[role]
    : runtime.pluginPages[surface];
}

function setPluginPageState(runtime, surface, role, page) {
  if (surface === "standalone") runtime.pluginPages.standalone[role] = page;
  else runtime.pluginPages[surface] = page;
}

function ensurePluginPageState(runtime, surface, container = null, role = "entry") {
  let page = pluginPageState(runtime, surface, role);
  if (!page) {
    page = {
      surface,
      role,
      container: null,
      frame: null,
      token: null,
      url: null,
      mode: null,
      loaded: false,
      listeners: new Map(),
      pendingStyle: "",
      pendingContent: "",
      clickableEnabled: false,
      hitTestRequestId: 0,
      generation: 0,
      task: Promise.resolve(),
    };
    setPluginPageState(runtime, surface, role, page);
  }
  if (container && !page.frame) {
    page.container = container;
    const frame = document.createElement("iframe");
    frame.className = "plugin-webview-frame";
    frame.title = `${runtime.spec.name || runtime.spec.identifier} ${surface}`;
    frame.referrerPolicy = "no-referrer";
    frame.setAttribute(
      "sandbox",
      "allow-scripts allow-same-origin allow-forms allow-modals allow-popups allow-downloads",
    );
    frame.addEventListener("load", () => {
      if (!page.token || frame.dataset.pluginPageToken !== page.token) return;
      page.loaded = true;
      if (page.mode === "simple") {
        sendPluginPageSimpleValue(runtime, page, "style", page.pendingStyle);
        sendPluginPageSimpleValue(runtime, page, "content", page.pendingContent);
      }
      if (page.surface === "overlay" || page.surface === "sidebar") {
        emitPluginEvent("iina.plugin-overlay-loaded");
      }
    });
    container.replaceChildren(frame);
    page.frame = frame;
  }
  return page;
}

function ensurePluginOverlay(runtime) {
  if (!runtime.overlay) {
    const overlay = document.createElement("section");
    overlay.className = "plugin-overlay";
    overlay.dataset.pluginIdentifier = runtime.spec.identifier;
    overlay.hidden = true;
    els.videoStage.append(overlay);
    runtime.overlay = overlay;
  }
  ensurePluginPageState(runtime, "overlay", runtime.overlay);
  return runtime.overlay;
}

function ensurePluginSidebar(runtime) {
  if (!runtime.spec.sidebar_tab_name) {
    throw new Error("Plugin did not declare a sidebarTab name");
  }
  if (!runtime.sidebar) {
    const sidebar = document.createElement("section");
    sidebar.className = "plugin-sidebar";
    sidebar.dataset.pluginIdentifier = runtime.spec.identifier;
    runtime.sidebar = sidebar;
  }
  ensurePluginPageState(runtime, "sidebar", runtime.sidebar);
  return runtime.sidebar;
}

function queuePluginPageLoad(
  runtime,
  surface,
  path,
  simpleMode,
  mode = simpleMode ? "simple" : "file",
  clearListeners = true,
  role = "entry",
) {
  const page = pluginPageState(runtime, surface, role)
    || ensurePluginPageState(runtime, surface, null, role);
  page.mode = mode;
  page.loaded = false;
  if (clearListeners) page.listeners.clear();
  const generation = ++page.generation;
  const operation = page.task
    .catch(() => undefined)
    .then(async () => {
      if (generation !== page.generation) return null;
      const descriptor = await invoke("plugin_webview_prepare_page", {
        identifier: runtime.spec.identifier,
        role,
        surface,
        path: path == null ? null : String(path),
        simpleMode: Boolean(simpleMode),
      });
      if (generation !== page.generation) return null;
      page.token = descriptor.token;
      page.url = descriptor.url;
      if (page.frame) {
        page.frame.dataset.pluginPageToken = descriptor.token;
        page.frame.src = descriptor.url;
      } else if (surface === "standalone") {
        await invoke("plugin_standalone_window_load", {
          identifier: runtime.spec.identifier,
          role,
          token: descriptor.token,
          url: descriptor.url,
        });
        if (generation !== page.generation) return null;
        if (page.mode === "simple") {
          await invoke("plugin_standalone_window_set_simple_value", {
            identifier: runtime.spec.identifier,
            role,
            token: page.token,
            target: "style",
            value: page.pendingStyle,
          });
          await invoke("plugin_standalone_window_set_simple_value", {
            identifier: runtime.spec.identifier,
            role,
            token: page.token,
            target: "content",
            value: page.pendingContent,
          });
        }
        page.loaded = true;
        if (page.mode === "simple") {
          sendPluginPageSimpleValue(runtime, page, "style", page.pendingStyle);
          sendPluginPageSimpleValue(runtime, page, "content", page.pendingContent);
        }
      }
      return descriptor;
    });
  page.task = operation;
  return operation;
}

function pluginPageMessageData(data) {
  try {
    const encoded = JSON.stringify(data);
    return encoded === undefined ? undefined : JSON.parse(encoded);
  } catch {
    return undefined;
  }
}

function postPluginPageMessage(runtime, page, name, data) {
  if (!page?.token) return Promise.resolve();
  const normalizedName = String(name);
  const normalizedData = pluginPageMessageData(data);
  if (page.surface === "standalone") {
    return invoke("plugin_standalone_window_post_message", {
      identifier: runtime.spec.identifier,
      role: page.role,
      token: page.token,
      name: normalizedName,
      data: normalizedData,
    });
  }
  page.frame?.contentWindow?.postMessage({
    __iimaPluginBridge: true,
    token: page.token,
    name: normalizedName,
    data: normalizedData,
  }, "*");
  return Promise.resolve();
}

function sendPluginPageSimpleValue(runtime, page, target, value) {
  if (!page?.token || !page.loaded || page.mode !== "simple") return;
  if (page.surface === "standalone") {
    void invoke("plugin_standalone_window_set_simple_value", {
      identifier: runtime.spec.identifier,
      role: page.role,
      token: page.token,
      target,
      value: String(value ?? ""),
    }).catch((error) => console.error(`[${runtime.spec.identifier}] standalone simple mode failed`, error));
    return;
  }
  page.frame?.contentWindow?.postMessage({
    __iimaPluginBridge: true,
    token: page.token,
    control: target,
    data: String(value ?? ""),
  }, "*");
}

function registerPluginPageListener(page, name, callback) {
  page.listeners.set(String(name), callback);
}

function pluginWindowFrameValue(value) {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function createPluginStandaloneWindowApi(runtime, invokeSync, role) {
  const ensurePage = () => {
    const page = pluginPageState(runtime, "standalone", role)
      || ensurePluginPageState(runtime, "standalone", null, role);
    if (!page.token && page.generation === 0) {
      return queuePluginPageLoad(runtime, "standalone", null, true, "blank", false, role).then(() => page);
    }
    return page.task.then(() => page);
  };
  const runVoid = (operation, action) => {
    void Promise.resolve(operation).catch((error) => {
      console.error(`[${runtime.spec.identifier}] standaloneWindow.${action} failed`, error);
    });
  };
  return {
    open: () => {
      runVoid(ensurePage().then(() => invoke("plugin_standalone_window_open", {
        identifier: runtime.spec.identifier,
        role,
      })), "open");
    },
    close: () => {
      // Accessing IINA's lazy standaloneWindow creates it before close().
      runVoid(ensurePage().then(() => invoke("plugin_standalone_window_close", {
        identifier: runtime.spec.identifier,
        role,
      })), "close");
    },
    isOpen: () => Boolean(invokeSync("standalone.isopen")),
    loadFile: (path) => {
      runVoid(queuePluginPageLoad(runtime, "standalone", String(path), false, "file", true, role), "loadFile");
    },
    simpleMode: () => {
      runVoid(queuePluginPageLoad(runtime, "standalone", null, true, "simple", true, role), "simpleMode");
    },
    setStyle: (style) => {
      const page = pluginPageState(runtime, "standalone", role);
      if (page?.mode !== "simple") {
        console.error(`[${runtime.spec.identifier}] standaloneWindow.setStyle is only available in simple mode.`);
        return;
      }
      page.pendingStyle = String(style ?? "");
      sendPluginPageSimpleValue(runtime, page, "style", page.pendingStyle);
    },
    setContent: (content) => {
      const page = pluginPageState(runtime, "standalone", role);
      if (page?.mode !== "simple") {
        console.error(`[${runtime.spec.identifier}] standaloneWindow.setContent is only available in simple mode.`);
        return;
      }
      page.pendingContent = String(content ?? "");
      sendPluginPageSimpleValue(runtime, page, "content", page.pendingContent);
    },
    postMessage: (name, data) => {
      runVoid(
        postPluginPageMessage(runtime, pluginPageState(runtime, "standalone", role), name, data),
        "postMessage",
      );
    },
    onMessage: (name, callback) => {
      const page = pluginPageState(runtime, "standalone", role)
        || ensurePluginPageState(runtime, "standalone", null, role);
      registerPluginPageListener(page, name, callback);
    },
    setProperty: (properties) => {
      // Unlike open/close/frame, the reference guards standaloneWindowCreated
      // and silently ignores properties before the first creation.
      const page = pluginPageState(runtime, "standalone", role);
      if (!page || page.generation === 0) return;
      runVoid(page.task.then(() => invoke("plugin_standalone_window_set_property", {
        identifier: runtime.spec.identifier,
        role,
        token: page.token,
        properties: properties && typeof properties === "object" ? properties : {},
      })), "setProperty");
    },
    setFrame: (width, height, x, y) => {
      runVoid(ensurePage().then((page) => invoke("plugin_standalone_window_set_frame", {
        identifier: runtime.spec.identifier,
        role,
        token: page.token,
        width: pluginWindowFrameValue(width),
        height: pluginWindowFrameValue(height),
        x: pluginWindowFrameValue(x),
        y: pluginWindowFrameValue(y),
      })), "setFrame");
    },
  };
}

function createPluginOverlayApi(runtime, hasPermission) {
  const permitted = () => hasPermission("video-overlay");
  const observe = (operation, action) => {
    void Promise.resolve(operation).catch((error) => {
      console.error(`[${runtime.spec.identifier}] overlay.${action} failed`, error);
    });
  };
  const requireOverlay = (action) => {
    if (!permitted()) {
      throw new Error(
        `overlay.${action} called when window is not available. Please call it after receiving the "iina.window-loaded" event.`,
      );
    }
    return ensurePluginOverlay(runtime);
  };
  return {
    show: () => {
      if (!permitted()) return;
      if (runtime.overlay && runtime.pluginPages.overlay?.mode) runtime.overlay.hidden = false;
    },
    hide: () => {
      if (!permitted()) return;
      if (runtime.overlay && runtime.pluginPages.overlay?.mode) runtime.overlay.hidden = true;
    },
    setOpacity: (opacity) => {
      if (!permitted()) return;
      if (runtime.overlay && runtime.pluginPages.overlay?.mode) {
        runtime.overlay.style.opacity = String(Math.max(0, Math.min(1, Number(opacity) || 0)));
      }
    },
    loadFile: (path) => {
      requireOverlay("loadFile");
      observe(queuePluginPageLoad(runtime, "overlay", String(path), false), "loadFile");
    },
    simpleMode: () => {
      requireOverlay("simpleMode");
      if (runtime.pluginPages.overlay?.mode === "simple") return;
      observe(queuePluginPageLoad(runtime, "overlay", null, true), "simpleMode");
    },
    setStyle: (style) => {
      if (!permitted()) return;
      const page = runtime.pluginPages.overlay;
      if (!page?.mode) return;
      if (page?.mode !== "simple") {
        console.error(`[${runtime.spec.identifier}] overlay.setStyle is only available in simple mode.`);
        return;
      }
      page.pendingStyle = String(style ?? "");
      sendPluginPageSimpleValue(runtime, page, "style", page.pendingStyle);
    },
    setContent: (content) => {
      if (!permitted()) return;
      const page = runtime.pluginPages.overlay;
      if (!page?.mode) return;
      if (page?.mode !== "simple") {
        console.error(`[${runtime.spec.identifier}] overlay.setContent is only available in simple mode.`);
        return;
      }
      page.pendingContent = String(content ?? "");
      sendPluginPageSimpleValue(runtime, page, "content", page.pendingContent);
    },
    setClickable: (clickable) => {
      // IINA 1.3.5 intentionally does not gate this setter on the
      // video-overlay permission. Loading or displaying content remains gated.
      const overlay = ensurePluginOverlay(runtime);
      const page = runtime.pluginPages.overlay;
      page.clickableEnabled = Boolean(clickable);
      page.hitTestRequestId += 1;
      overlay.style.pointerEvents = "none";
      if (page.clickableEnabled && lastPluginPointerPosition) {
        queryPluginOverlayHitTests(lastPluginPointerPosition);
      }
    },
    postMessage: (name, data) => {
      if (!permitted() || !runtime.pluginPages.overlay?.mode) return;
      observe(postPluginPageMessage(runtime, runtime.pluginPages.overlay, name, data), "postMessage");
    },
    onMessage: (name, callback) => {
      if (!permitted()) return;
      const page = runtime.pluginPages.overlay;
      if (page?.mode) registerPluginPageListener(page, name, callback);
    },
  };
}

function createPluginSidebarApi(runtime) {
  const observe = (operation, action) => {
    void Promise.resolve(operation).catch((error) => {
      console.error(`[${runtime.spec.identifier}] sidebar.${action} failed`, error);
    });
  };
  return {
    loadFile: (path) => {
      ensurePluginSidebar(runtime);
      observe(queuePluginPageLoad(runtime, "sidebar", String(path), false), "loadFile");
    },
    show: () => {
      ensurePluginSidebar(runtime);
      observe(showPluginSidebar(runtime), "show");
    },
    hide: () => {
      if (activePluginSidebarId === runtime.spec.identifier) {
        activePluginSidebarId = null;
        void command({ type: "hide-sidebar" });
      }
    },
    postMessage: (name, data) => {
      observe(postPluginPageMessage(runtime, runtime.pluginPages.sidebar, name, data), "postMessage");
    },
    onMessage: (name, callback) => {
      const page = runtime.pluginPages.sidebar
        || ensurePluginPageState(runtime, "sidebar");
      registerPluginPageListener(page, name, callback);
    },
  };
}

function renderPluginSidebarTabs() {
  if (!els.sidebarTabs) return;
  els.sidebarTabs.querySelectorAll("[data-plugin-sidebar]").forEach((button) => button.remove());
  for (const identifier of pluginRuntimeOrder) {
    const runtime = pluginRuntimes.get(identifier);
    if (!runtime) continue;
    if (!runtime.spec.sidebar_tab_name) continue;
    const button = document.createElement("button");
    button.type = "button";
    button.className = "sidebar-tab";
    button.dataset.pluginSidebar = runtime.spec.identifier;
    button.textContent = runtime.spec.sidebar_tab_name;
    button.title = runtime.spec.name;
    button.addEventListener("click", () => {
      ensurePluginSidebar(runtime);
      void showPluginSidebar(runtime);
    });
    els.sidebarTabs.append(button);
  }
  updatePluginSidebarTabVisibility(state);
}

async function showPluginSidebar(runtime) {
  ensurePluginSidebar(runtime);
  activePluginSidebarId = runtime.spec.identifier;
  if (!state.sidebar.visible) {
    await command({ type: "show-sidebar", tab: "video" });
  } else {
    renderSidebar(state);
  }
}

function updatePluginSidebarTabVisibility(nextState) {
  const quickSettingsVisible = Boolean(
    nextState?.sidebar?.visible
      && (activePluginSidebarId || ["video", "audio", "subtitles"].includes(nextState.sidebar.tab))
  );
  els.sidebarTabs?.querySelectorAll("[data-plugin-sidebar]").forEach((button) => {
    button.hidden = !quickSettingsVisible;
  });
}

function createPluginEventApi(runtime) {
  return {
    on: (name, callback) => {
      const normalizedName = normalizePluginEventName(name);
      const id = `event_${++runtime.nextEventListenerId}`;
      const listeners = runtime.eventListeners.get(normalizedName) || new Map();
      listeners.set(id, callback);
      runtime.eventListeners.set(normalizedName, listeners);
      const property = pluginChangedMpvProperty(normalizedName);
      if (property) {
        void invoke("plugin_mpv_observe_property", {
          identifier: runtime.spec.identifier,
          property,
        }).catch((error) => {
          console.error(`[${runtime.spec.identifier}] unable to observe mpv property ${property}`, error);
        });
      }
      return id;
    },
    off: (name, id) => {
      const listeners = runtime.eventListeners.get(normalizePluginEventName(name));
      if (!listeners) {
        console.warn(`[${runtime.spec.identifier}] Event listener not found for id ${id}`);
        return;
      }
      const removed = listeners.delete(String(id));
      if (!listeners.size) runtime.eventListeners.delete(normalizePluginEventName(name));
      if (!removed) console.warn(`[${runtime.spec.identifier}] Event listener not found for id ${id}`);
    },
  };
}

function createPluginInputApi(runtime) {
  const register = (event, input, callback, priority) => {
    if (callback === null || (typeof callback !== "object" && typeof callback !== "function")) return;
    const normalizedInput = event.startsWith("key")
      ? normalizePluginKeyCode(input)
      : normalizePluginMouseInput(input);
    const listeners = runtime.inputListeners.get(normalizedInput) || new Map();
    listeners.set(event, {
      callback,
      priority: typeof priority === "number" ? priority | 0 : PLUGIN_INPUT_PRIORITY_LOW,
    });
    runtime.inputListeners.set(normalizedInput, listeners);
  };
  return {
    PRIORITY_LOW: PLUGIN_INPUT_PRIORITY_LOW,
    PRIORITY_HIGH: PLUGIN_INPUT_PRIORITY_HIGH,
    MOUSE: PLUGIN_INPUT_MOUSE,
    RIGHT_MOUSE: PLUGIN_INPUT_RIGHT_MOUSE,
    OTHER_MOUSE: PLUGIN_INPUT_OTHER_MOUSE,
    normalizeKeyCode: (code) => normalizePluginKeyCode(code),
    getAllKeyBindings: () => Object.fromEntries(keyBindingRowsFromPreferences().map((binding) => [
      binding.key,
      {
        key: binding.key,
        action: binding.rawCommand || actionRawValue(binding.action),
        isIINACommand: Boolean(binding.isIINACommand),
      },
    ])),
    onMouseUp: (button, callback, priority) => register("mouseUp", button, callback, priority),
    onMouseDown: (button, callback, priority) => register("mouseDown", button, callback, priority),
    onMouseDrag: (button, callback, priority) => register("mouseDrag", button, callback, priority),
    onKeyDown: (key, callback, priority) => register("keyDown", key, callback, priority),
    onKeyUp: (key, callback, priority) => register("keyUp", key, callback, priority),
  };
}

function normalizePluginMouseInput(input) {
  const normalized = String(input || "");
  return [PLUGIN_INPUT_MOUSE, PLUGIN_INPUT_RIGHT_MOUSE, PLUGIN_INPUT_OTHER_MOUSE].includes(normalized)
    ? normalized
    : normalized;
}

function normalizePluginKeyCode(code) {
  const raw = String(code || "");
  if (!raw) return "";
  if (raw === "default-bindings" || [...raw].filter((character) => character === "-").length > 1) return raw;
  if (raw === "+") return "PLUS";

  const parts = raw.replaceAll("++", "+PLUS").split("+");
  let key = parts.pop() || "";
  if (key === "#") key = "SHARP";
  else if (key === "+") key = "PLUS";
  else if (key.length > 1) key = key.toUpperCase();

  const modifiers = new Set();
  for (const rawModifier of parts) {
    const modifier = rawModifier.toLowerCase();
    if (modifier === "shift") {
      if (PLUGIN_SHIFTED_KEY_MAP[key]) key = PLUGIN_SHIFTED_KEY_MAP[key];
      else if (key.length === 1 && key.toLocaleLowerCase() !== key.toLocaleUpperCase()) key = key.toLocaleUpperCase();
      else if (!PLUGIN_SHIFTED_KEY_CHARS.has(key)) modifiers.add("Shift");
    } else if (modifier === "ctrl" || modifier === "control") {
      modifiers.add("Ctrl");
    } else if (modifier === "alt" || modifier === "option") {
      modifiers.add("Alt");
    } else if (modifier === "meta" || modifier === "cmd" || modifier === "command") {
      modifiers.add("Meta");
    } else if (rawModifier) {
      modifiers.add(rawModifier.toUpperCase());
    }
  }
  return ["Ctrl", "Alt", "Shift", "Meta"].filter((modifier) => modifiers.has(modifier)).concat(key).join("+");
}

function pluginKeyCodeFromEvent(event) {
  const key = pluginMpvKeyFromEvent(event);
  const modifiers = [];
  if (event.ctrlKey) modifiers.push("Ctrl");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");
  if (event.metaKey) modifiers.push("Meta");
  return normalizePluginKeyCode([...modifiers, key].join("+"));
}

function pluginMpvKeyFromEvent(event) {
  return mpvKeyTokenFromKeyboardEvent(event);
}

function dispatchPluginInput(input, event, args, handler = null, defaultHandler = null) {
  const listeners = [];
  for (const runtime of pluginRuntimes.values()) {
    const listener = runtime.inputListeners.get(input)?.get(event);
    if (listener) listeners.push({ ...listener, runtime });
  }
  listeners.sort((left, right) => right.priority - left.priority);

  const callListeners = (predicate) => {
    for (const listener of listeners) {
      if (!predicate(listener)) continue;
      try {
        if (typeof listener.callback === "function" && Boolean(listener.callback(args))) return true;
      } catch (error) {
        console.error(`[${listener.runtime.spec.identifier}] input listener failed`, error);
      }
    }
    return false;
  };

  if (callListeners((listener) => listener.priority >= PLUGIN_INPUT_PRIORITY_HIGH)) return true;
  const eventHandled = Boolean(handler?.());
  if (callListeners((listener) => listener.priority < PLUGIN_INPUT_PRIORITY_HIGH)) return true;
  if (!eventHandled) defaultHandler?.();
  return eventHandled;
}

function pluginKeyEventArgs(event) {
  return {
    x: Number(event.clientX) || 0,
    y: Number(event.clientY) || 0,
    isRepeat: Boolean(event.repeat),
  };
}

function pluginMouseEventArgs(event) {
  const bounds = els.videoStage.getBoundingClientRect();
  return {
    x: Math.max(0, Number(event.clientX) - bounds.left),
    y: Math.max(0, Number(event.clientY) - bounds.top),
    clickCount: Number(event.detail) || 1,
    pressure: Number(event.pressure) || 0,
  };
}

function pluginMouseInputFromButton(button) {
  if (button === 2) return PLUGIN_INPUT_RIGHT_MOUSE;
  if (button === 1) return PLUGIN_INPUT_OTHER_MOUSE;
  return PLUGIN_INPUT_MOUSE;
}

function pluginMouseInputsFromButtons(buttons) {
  const inputs = [];
  if (buttons & 1) inputs.push(PLUGIN_INPUT_MOUSE);
  if (buttons & 2) inputs.push(PLUGIN_INPUT_RIGHT_MOUSE);
  if (buttons & 4) inputs.push(PLUGIN_INPUT_OTHER_MOUSE);
  return inputs;
}

function pluginWindowRect() {
  return {
    x: window.screenX,
    y: window.screenY,
    width: window.innerWidth,
    height: window.innerHeight,
  };
}

function createPluginMpvApi(runtime, invokeSync) {
  const get = (property, kind) => decodePluginMpvValue(invokeSync("mpv.get", {
    property: String(property),
    kind,
  }));
  return {
    getFlag: (property) => Boolean(get(property, "flag")),
    getNumber: (property) => Number(get(property, "number")),
    getString: (property) => get(property, "string"),
    getNative: (property) => get(property, "native"),
    set: (property, valueToSet) => {
      // JSCore reports null as an object but `toObject()` yields nil, so IINA performs no set.
      if (valueToSet === null) return;
      invokeSync("mpv.set", {
        property: String(property),
        value: encodePluginMpvValue(valueToSet),
      });
    },
    command: (commandName, args = []) => {
      invokeSync("mpv.command", {
        command: String(commandName),
        args: Array.from(args || []).map(String),
      });
    },
    addHook: (name, priority, callback) => {
      if (runtime.mpvHooksDisposed) {
        throw new Error("This plugin mpv API has already been disposed");
      }
      if (typeof callback !== "function") {
        throw new TypeError("mpv.addHook expects a callback function");
      }
      const hookName = String(name ?? "");
      const hookPriority = Math.trunc(Number(priority));
      if (!Number.isFinite(hookPriority) || hookPriority < -2147483648 || hookPriority > 2147483647) {
        throw new RangeError("mpv.addHook priority must be a signed 32-bit integer");
      }
      const callbackId = runtime.nextMpvHookCallbackId++;
      runtime.mpvHookCallbacks.set(callbackId, callback);
      runtime.mpvHookTask = runtime.mpvHookTask
        .catch(() => {})
        .then(async () => {
          if (runtime.mpvHooksDisposed || !runtime.mpvHookCallbacks.has(callbackId)) return;
          await invoke("plugin_mpv_add_hook", {
            identifier: runtime.spec.identifier,
            name: hookName,
            priority: hookPriority,
            callbackId,
          });
        })
        .catch((error) => {
          runtime.mpvHookCallbacks.delete(callbackId);
          console.error(`[${runtime.spec.identifier}] unable to register mpv hook ${hookName}`, error);
        });
    },
  };
}

function createPluginHttpApi(
  runtime,
  hasPermission = () => true,
  playerAvailable = true,
  invokeSync = runtime.syncTransports?.entry?.invokeSync,
) {
  const requireNetworkPermission = () => {
    if (!hasPermission("network-request")) {
      throw new Error('To call this API, the plugin must declare permission "network-request" in its Info.json.');
    }
  };
  const request = (method, url, options = {}, permissionRequired = true) => {
    if (permissionRequired) requireNetworkPermission();
    const validatedUrl = pluginHttpValidatedUrl(runtime, url);
    return invoke("plugin_http_request", {
      identifier: runtime.spec.identifier,
      method,
      url: validatedUrl,
      options,
      permissionRequired,
    }).then((value) => {
      const response = pluginHttpResponse(value);
      return pluginHttpResponseIsOk(response) ? response : Promise.reject(response);
    });
  };
  return {
    get: (url, options) => request("GET", url, options),
    post: (url, options) => request("POST", url, options),
    put: (url, options) => request("PUT", url, options),
    patch: (url, options) => request("PATCH", url, options),
    delete: (url, options) => request("DELETE", url, options),
    xmlrpc: (location) => {
      const url = pluginHttpValidatedUrl(runtime, location);
      return {
        call: async (method, args = []) => {
          const methodName = String(method ?? "");
          let response;
          try {
            response = await request("POST", url, {
              headers: { "Content-Type": "text/xml; charset=utf-8" },
              data: xmlRpcEncodeCall(methodName, args),
            }, false);
          } catch (error) {
            const httpResponse = pluginHttpResponse(error);
            const httpCode = Number(httpResponse.statusCode) || 0;
            const reason = httpResponse.reason || "Unknown";
            throw {
              httpCode,
              reason,
              description: `${methodName}: [${httpCode}] ${reason}`,
            };
          }
          let decoded;
          try {
            decoded = xmlRpcDecodeResponse(response.text);
          } catch {
            const httpCode = Number(response.statusCode) || 0;
            throw {
              httpCode,
              reason: "Bad response",
              description: `${methodName}: [${httpCode}] Bad response`,
            };
          }
          if (decoded.fault) return Promise.reject();
          return decoded.value;
        },
      };
    },
    download: (url, destination, options = {}) => {
      requireNetworkPermission();
      const validatedUrl = pluginHttpValidatedUrl(runtime, url);
      pluginHttpDownloadMethod(options);
      const checkedDestination = pluginPathForApi(String(destination), {
        hasFileSystemPermission: hasPermission("file-system"),
        playerAvailable,
      });
      try {
        // JavascriptAPIHttp resolves parsePath before constructing its Promise.
        // Use the synchronous resolver here so invalid track/private/current
        // destinations cannot turn into asynchronous Promise rejections.
        invokeSync("core.resolveopen", { path: checkedDestination });
      } catch (error) {
        if (String(error?.message || error).includes("@current is unavailable")) {
          throw new Error("Not allowed to write to the destination.");
        }
        throw error;
      }
      return invoke("plugin_http_download", {
        identifier: runtime.spec.identifier,
        url: validatedUrl,
        destination: checkedDestination,
        options,
      }).then((value) => {
        if (value?.response) {
          const response = pluginHttpResponse(value.response);
          if (!pluginHttpResponseIsOk(response)) return Promise.reject(response);
        }
        return undefined;
      });
    },
  };
}

function pluginHttpResponse(value) {
  const source = value && typeof value === "object" ? value : {};
  const rawStatusCode = source.statusCode ?? source.status_code ?? null;
  const numericStatusCode = rawStatusCode == null ? null : Number(rawStatusCode);
  return {
    statusCode: Number.isInteger(numericStatusCode) ? numericStatusCode : null,
    reason: source.reason == null ? "Unknown" : String(source.reason),
    data: Object.prototype.hasOwnProperty.call(source, "data") ? source.data : null,
    text: Object.prototype.hasOwnProperty.call(source, "text") ? source.text : null,
  };
}

function pluginHttpResponseIsOk(response) {
  const statusCode = response?.statusCode;
  return Number.isInteger(statusCode) && !(statusCode >= 400 && statusCode < 600);
}

function pluginHttpValidatedUrl(runtime, value) {
  const rawUrl = String(value);
  try {
    new URL(rawUrl);
  } catch {
    throw new Error(`URL ${rawUrl} is invalid.`);
  }
  if (!pluginHttpUrlAllowed(runtime, rawUrl)) {
    throw new Error(`URL ${rawUrl} is not allowed.`);
  }
  return rawUrl;
}

function pluginHttpDownloadMethod(options) {
  const method = typeof options?.method === "string" ? options.method : "GET";
  if (!["DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT"].includes(method)) {
    throw new Error("method is invalid.");
  }
  return method;
}

function pluginHttpUrlAllowed(runtime, rawUrl) {
  let url;
  try {
    url = new URL(rawUrl);
  } catch {
    return false;
  }
  if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) return false;
  const host = url.hostname.toLocaleLowerCase();
  return Array.from(runtime.spec.allowed_domains || []).some((rule) => {
    const normalized = String(rule || "").trim().toLocaleLowerCase();
    if (normalized === "*" || normalized === host) return true;
    const suffix = normalized.startsWith("*.") ? normalized.slice(2) : null;
    return Boolean(suffix && host.endsWith(`.${suffix}`));
  });
}

function createPluginWebSocketApi(runtime, invokeSync, role) {
  const controller = runtime.websocket[role];
  const setHandler = (field, handler) => {
    controller.handlers[field] = pluginWebSocketHandlerValue(controller.handlers[field], handler);
  };
  return {
    createServer: (options = {}) => {
      const port = pluginWebSocketPort(options?.port);
      controller.disposed = false;
      const created = invokeSync("ws.createserver", { port });
      const generation = Number(created?.generation);
      controller.generation = Number.isSafeInteger(generation) ? generation : null;
      controller.created = true;
    },
    startServer: () => {
      if (!controller.created) throw new Error("ws.startServer: server not created");
      invokeSync("ws.startserver");
    },
    onStateUpdate: (handler) => setHandler("state", handler),
    onMessage: (handler) => setHandler("message", handler),
    onNewConnection: (handler) => setHandler("newConnection", handler),
    onConnectionStateUpdate: (handler) => setHandler("connectionState", handler),
    sendText: (connection, text) => controller.task.then(() => invoke("plugin_websocket_send_text", {
      identifier: runtime.spec.identifier,
      role,
      connectionId: String(connection ?? ""),
      text: String(text ?? ""),
    })),
  };
}

function createPluginFileApi(
  runtime,
  invokeSync,
  createUint8Array,
  hasPermission = () => false,
  playerAvailable = true,
) {
  const pathForFileApi = (path) => pluginPathForApi(String(path), {
    hasFileSystemPermission: hasPermission("file-system"),
    playerAvailable,
  });
  const handle = (path, mode) => {
    const token = invokeSync("file.handle.open", {
      path: pathForFileApi(path),
      mode: String(mode),
    });
    if (typeof token !== "string" || !token) return null;
    let closed = false;
    const call = (method, args = {}) => {
      if (closed) throw new Error("Plugin file handle is closed");
      return invokeSync(method, { token, ...args });
    };
    return {
      offset: () => call("file.handle.offset"),
      seekTo: (offset) => {
        call("file.handle.seek", { offset: Math.max(0, Math.trunc(Number(offset) || 0)) });
      },
      seekToEnd: () => {
        call("file.handle.seektoend");
      },
      read: (length) => pluginFileHandleReadValue(call("file.handle.read", {
        length: Math.max(0, Math.trunc(Number(length) || 0)),
      }), createUint8Array),
      readToEnd: () => pluginFileHandleReadValue(call("file.handle.readtoend"), createUint8Array),
      write: (data) => {
        call("file.handle.write", {
          data: typeof data === "string" ? data : Array.from(data || []),
        });
      },
      close: () => {
        if (closed) return;
        try {
          call("file.handle.close");
        } finally {
          closed = true;
        }
      },
    };
  };
  return {
    exists: (path) => Boolean(invokeSync("file.exists", { path: pathForFileApi(path) })),
    list: (path, options = {}) => invokeSync("file.list", {
      path: pathForFileApi(path),
      includeSubDir: Boolean(options?.includeSubDir),
    }),
    read: (path, options = {}) => invokeSync("file.read", {
      path: pathForFileApi(path),
      encoding: typeof options?.encoding === "string" ? options.encoding : null,
    }),
    write: (path, content) => {
      invokeSync("file.write", { path: pathForFileApi(path), content: String(content ?? "") });
    },
    delete: (path) => {
      invokeSync("file.delete", { path: pathForFileApi(path) });
    },
    trash: (path) => {
      invokeSync("file.trash", { path: pathForFileApi(path) });
    },
    showInFinder: (path) => {
      invokeSync("file.showinfinder", { path: pathForFileApi(path) });
    },
    handle,
  };
}

const pluginSubtitleItemStates = new WeakMap();

function createPluginSubtitleItem(data, desc) {
  const item = {};
  const managed = { data, desc, download: null };
  Object.defineProperties(item, {
    data: {
      enumerable: true,
      get: () => managed.data,
    },
    desc: {
      enumerable: true,
      get: () => managed.desc,
      set: (value) => { managed.desc = value; },
    },
    __setDownloadCallback: {
      enumerable: true,
      value: (callback) => { managed.download = callback; },
    },
  });
  pluginSubtitleItemStates.set(item, managed);
  return item;
}

function downloadPluginSubtitleItem(item) {
  const managed = pluginSubtitleItemStates.get(item);
  if (!managed) {
    return Promise.reject(new Error("provider.search should return an array of subtitle items."));
  }
  if (typeof managed.download !== "function") return Promise.resolve([]);
  return new Promise((resolve, reject) => {
    managed.download(
      (urls) => {
        if (!Array.isArray(urls)) {
          reject(new Error("provider.download should return an array of strings."));
          return;
        }
        resolve(urls);
      },
      (error) => reject(new Error(String(error))),
    );
  });
}

function pluginSubtitleDownloadUrlsForNativeBoundary(urls) {
  if (!Array.isArray(urls) || urls.some((url) => typeof url !== "string")) {
    throw new Error("provider.download should return an array of strings.");
  }
  return urls;
}

function createPluginSubtitleApi(runtime) {
  const providers = {};
  const api = {
    CUSTOM_IMPLEMENTATION: "custom-implementation",
    __providers: providers,
    __invokeSearch: (id, complete, fail) => {
      const provider = providers[id];
      if (typeof provider !== "object") {
        fail(`The provider with id "${id}" is not registered.`);
        return;
      }
      const checkAsync = (name) => {
        if (provider[name]) return true;
        fail(`provider.${name} doesn't exist or is not an async function.`);
        return false;
      };
      for (const name of ["search", "download"]) {
        if (!checkAsync(name)) return;
      }
      const createDownloadCallback = (subtitle) => (downloadComplete, downloadFail) => {
        provider.download(subtitle).then(
          (urls) => {
            if (!Array.isArray(urls)) {
              downloadFail("provider.download should return an array of strings.");
              return;
            }
            downloadComplete(urls);
          },
          (error) => downloadFail(error.toString()),
        );
      };
      provider.search().then(
        (subtitles) => {
          createPluginConsole(runtime).log(subtitles);
          if (subtitles === api.CUSTOM_IMPLEMENTATION) {
            complete(null);
            return;
          }
          if (!Array.isArray(subtitles)) {
            fail("provider.search should return an array of subtitle items.");
            return;
          }
          const hasDescription = typeof provider.description === "function";
          for (const subtitle of subtitles) {
            if (hasDescription && !subtitle.desc) subtitle.desc = provider.description(subtitle);
            subtitle.__setDownloadCallback(createDownloadCallback(subtitle));
          }
          complete(subtitles);
        },
        (error) => fail(error.toString()),
      );
    },
    registerProvider: (id, provider) => {
      if (typeof id !== "string") throw new Error("A subtitle provider should have an id.");
      providers[id] = provider;
      runtime.subtitleProviders.set(id, provider);
    },
    item: (data, desc) => createPluginSubtitleItem(data, desc),
  };
  runtime.subtitleApi = api;
  return api;
}

function readPluginPreferenceValues(identifier) {
  try {
    return JSON.parse(localStorage.getItem(`iima.plugin.${identifier}.preferences`) || "{}") || {};
  } catch {
    return {};
  }
}

function writePluginPreferenceValues(identifier, values) {
  localStorage.setItem(`iima.plugin.${identifier}.preferences`, JSON.stringify(values));
}

function emitPluginPlayerState(nextState) {
  emitPluginEvent("iina.player-state", nextState);
}

function emitPluginEvent(name, ...args) {
  for (const runtime of pluginRuntimes.values()) {
    for (const callback of runtime.eventListeners.get(name)?.values() || []) {
      try {
        if (typeof callback === "function") callback(...args);
      } catch (error) {
        console.error(`[${runtime.spec.identifier}] event listener failed`, error);
      }
    }
  }
}

function hasNativeMpvEventSequence(nextState) {
  return Number.isSafeInteger(Number(nextState?.mpv_event_cursor));
}

function dispatchPluginMpvEventBatch(
  batch,
  currentUrl = batch?.currentUrl ?? batch?.current_url ?? state?.current_url,
) {
  const droppedEventCount = Number(batch?.droppedEventCount ?? batch?.dropped_event_count) || 0;
  if (droppedEventCount > 0) {
    console.warn(`Plugin mpv event log dropped ${droppedEventCount} event(s) before delivery`);
  }
  pluginMpvEventSequence = consumePluginMpvEventBatch(
    pluginMpvEventSequence,
    batch,
    emitPluginEvent,
    () => currentUrl || null,
  );
}

function dispatchPluginMpvEventsFromState(nextState) {
  if (!hasNativeMpvEventSequence(nextState)) return false;
  dispatchPluginMpvEventBatch({
    cursor: Number(nextState.mpv_event_cursor),
    events: Array.from(nextState.mpv_events || []),
  }, nextState.current_url);
  return true;
}

function dispatchPluginHostEvent(payload) {
  let name;
  try {
    name = normalizePluginEventName(payload?.name);
  } catch (error) {
    console.error("Ignoring malformed native plugin event", error);
    return;
  }
  let args = Array.isArray(payload?.args) ? payload.args : [];
  if (
    ["iina.window-size-adjusted", "iina.window-moved", "iina.window-resized"].includes(name)
    && !args.length
  ) {
    args = [pluginWindowRect()];
  }
  if (name === "iina.window-will-close") pluginWindowWillCloseEmitted = true;
  emitPluginEvent(name, ...args);
}

async function unloadPluginEntryRuntime(runtime) {
  // IINA's Reload all plugins replaces each PlayerCore-owned entry instance only.
  // The global realm, controller, module cache, menu, retained Developer Tool,
  // synchronization transport, and global-owned shared services stay alive.
  const entryContextId = runtime.realmContextIds.entry;
  const entryRealm = runtime.entryRealm;
  const entryModuleExports = runtime.moduleExports;
  if (runtime.realmLeases.entry) runtime.realmLeases.entry.active = false;
  runtime.realmLeases.entry = null;
  runtime.realmContextIds.entry = null;
  runtime.entryRealm = null;
  runtime.moduleExports = new Map();
  await runtime.menuSyncTask?.catch(() => {});
  const entrySyncTransport = runtime.syncTransports.entry;
  runtime.syncTransports.entry = null;
  if (entrySyncTransport) {
    try {
      await entrySyncTransport.revoke();
    } catch {
      // A full owner-window teardown also revokes the entry grant and its file handles.
    }
  }
  runtime.mpvHooksDisposed = true;
  await runtime.mpvHookTask.catch(() => {});
  try {
    await invoke("plugin_mpv_remove_hooks", { identifier: runtime.spec.identifier });
  } catch {
    // Player teardown also releases pending hooks.
  }
  runtime.mpvHookCallbacks.clear();
  runtime.mpvHookTask = Promise.resolve();

  const entryWebSocket = runtime.websocket.entry;
  entryWebSocket.disposed = true;
  entryWebSocket.handlers.state = null;
  entryWebSocket.handlers.message = null;
  entryWebSocket.handlers.newConnection = null;
  entryWebSocket.handlers.connectionState = null;
  try {
    await invoke("plugin_websocket_stop", {
      identifier: runtime.spec.identifier,
      role: "entry",
    });
  } catch {
    // Owner-window teardown and plugin disable independently stop entry servers.
  }
  runtime.websocket.entry = createPluginWebSocketController();

  for (const surface of ["overlay", "sidebar", "standalone"]) {
    const page = pluginPageState(runtime, surface, "entry");
    if (!page) continue;
    page.generation += 1;
    page.listeners.clear();
    page.token = null;
    page.url = null;
    page.mode = null;
    page.loaded = false;
    if (page.frame) {
      page.frame.dataset.pluginPageToken = "";
      page.frame.src = "about:blank";
    }
    setPluginPageState(runtime, surface, "entry", null);
  }
  runtime.overlay?.remove();
  runtime.sidebar?.remove();
  runtime.overlay = null;
  runtime.sidebar = null;
  if (activePluginSidebarId === runtime.spec.identifier) activePluginSidebarId = null;
  try {
    await invoke("plugin_webview_cleanup_role", {
      identifier: runtime.spec.identifier,
      role: "entry",
    });
  } catch {
    // Owner-window teardown and full plugin cleanup independently release entry pages.
  }

  runtime.eventListeners.clear();
  runtime.inputListeners.clear();
  runtime.globalChildListeners.clear();
  for (const [requestId, hooks] of runtime.utilsExecHooks) {
    if (hooks.role === "entry") runtime.utilsExecHooks.delete(requestId);
  }
  runtime.subtitleProviders.clear();
  runtime.subtitleApi = null;
  runtime.playlistMenuItemBuilder = null;
  runtime.menuItems.entry.splice(0, runtime.menuItems.entry.length);
  for (const key of runtime.menuItemsById.keys()) {
    if (key.startsWith("entry:")) runtime.menuItemsById.delete(key);
  }
  if (entryRealm) {
    if (entryContextId && pluginDeveloperToolTargets.has(entryContextId)) {
      runtime.retiredRealmContexts.set(entryContextId, {
        role: "entry",
        realm: entryRealm,
        moduleExports: entryModuleExports,
      });
    } else {
      entryModuleExports.clear();
      entryRealm.destroy();
      if (entryContextId) pluginDeveloperToolTargets.delete(entryContextId);
    }
  }
  try {
    await invoke("set_plugin_menu_items", {
      identifier: runtime.spec.identifier,
      role: "entry",
      items: [],
    });
  } catch {
    // A disabled or removed plugin no longer accepts entry-menu updates.
  }
}

async function reloadPluginEntryRuntime(runtime, spec) {
  await unloadPluginEntryRuntime(runtime);
  runtime.spec = spec;
  runtime.mpvHooksDisposed = false;
  try {
    runtime.syncTransports.entry = await createPluginRoleSyncTransport(runtime, "entry");
    await runPluginEntryRuntime(runtime);
    await syncPluginMenu(runtime, "entry");
    runtime.fingerprint = pluginRuntimeFingerprint(spec);
  } catch (error) {
    await unloadPluginEntryRuntime(runtime);
    throw error;
  }
}

async function unloadPluginRuntime(runtime) {
  runtime.mpvHooksDisposed = true;
  for (const role of ["entry", "global"]) {
    if (runtime.realmLeases[role]) runtime.realmLeases[role].active = false;
    runtime.realmLeases[role] = null;
  }
  const syncTransports = Object.values(runtime.syncTransports);
  runtime.syncTransports.entry = null;
  runtime.syncTransports.global = null;
  for (const syncTransport of syncTransports) {
    if (syncTransport) {
      try {
        await syncTransport.revoke();
      } catch {
        // Owner-window teardown and application exit independently revoke every
        // synchronization grant and close its plugin-instance-owned file handles.
      }
    }
  }
  await runtime.mpvHookTask.catch(() => {});
  try {
    await invoke("plugin_mpv_remove_hooks", { identifier: runtime.spec.identifier });
  } catch {
    // Disable, removal, player teardown, and app exit also release pending hooks.
  }
  runtime.mpvHookCallbacks.clear();
  if (runtime.globalControllerRegistered) {
    await runtime.globalControllerTask.catch(() => {});
    try {
      await invoke("plugin_global_unregister_controller", { identifier: runtime.spec.identifier });
    } catch {
      // Disable, removal, main-window teardown, and app exit also release managed players.
    }
    runtime.globalControllerRegistered = false;
  }
  for (const role of ["entry", "global"]) {
    const controller = runtime.websocket[role];
    controller.disposed = true;
    controller.handlers.state = null;
    controller.handlers.message = null;
    controller.handlers.newConnection = null;
    controller.handlers.connectionState = null;
    try {
      await invoke("plugin_websocket_stop", { identifier: runtime.spec.identifier, role });
    } catch {
      // The backend also stops servers on disable, removal, window close, and app exit.
    }
  }
  try {
    await invoke("plugin_webview_cleanup", { identifier: runtime.spec.identifier });
  } catch {
    // Owner-window teardown and app exit also revoke page grants and destroy standalone windows.
  }
  const pluginPages = [
    runtime.pluginPages.overlay,
    runtime.pluginPages.sidebar,
    ...Object.values(runtime.pluginPages.standalone),
  ];
  for (const page of pluginPages) {
    if (!page) continue;
    page.generation += 1;
    page.listeners.clear();
    if (page.frame) page.frame.src = "about:blank";
  }
  runtime.overlay?.remove();
  runtime.sidebar?.remove();
  if (activePluginSidebarId === runtime.spec.identifier) {
    activePluginSidebarId = null;
  }
  runtime.eventListeners.clear();
  runtime.inputListeners.clear();
  runtime.globalControllerListeners.clear();
  runtime.globalChildListeners.clear();
  runtime.utilsExecHooks.clear();
  runtime.subtitleProviders.clear();
  runtime.subtitleApi = null;
  runtime.playlistMenuItemBuilder = null;
  runtime.menuItems.entry.splice(0, runtime.menuItems.entry.length);
  runtime.menuItems.global.splice(0, runtime.menuItems.global.length);
  runtime.menuItemsById.clear();
  runtime.moduleExports.clear();
  runtime.globalModuleExports.clear();
  for (const contextId of Object.values(runtime.realmContextIds)) {
    if (contextId) pluginDeveloperToolTargets.delete(contextId);
  }
  for (const [contextId, retired] of runtime.retiredRealmContexts) {
    retired.moduleExports.clear();
    retired.realm.destroy();
    pluginDeveloperToolTargets.delete(contextId);
  }
  runtime.retiredRealmContexts.clear();
  runtime.entryRealm?.destroy();
  runtime.globalRealm?.destroy();
  runtime.entryRealm = null;
  runtime.globalRealm = null;
  runtime.realmContextIds.entry = null;
  runtime.realmContextIds.global = null;
  try {
    await invoke("set_plugin_menu_items", {
      identifier: runtime.spec.identifier,
      role: "entry",
      items: [],
    });
    if (runtime.spec.global_entry && isPrimaryPlayerWindow) {
      await invoke("set_plugin_menu_items", {
        identifier: runtime.spec.identifier,
        role: "global",
        items: [],
      });
    }
  } catch {
    // A disabled or removed plugin no longer accepts menu updates.
  }
}

async function openMediaFromNativeDialog() {
  const nextState = await invoke("open_media_dialog");
  if (!nextState) return;
  setPlayerState(nextState, { force: true, presentOsd: true });
}

function shouldOpenInNewPlayerForMenuAction(isAlternativeAction) {
  if (!state.current_url) return false;
  return Boolean(getPreferenceValue("alwaysOpenInNewWindow")) !== Boolean(isAlternativeAction);
}

async function openMediaForMenuAction(path, isAlternativeAction) {
  if (shouldOpenInNewPlayerForMenuAction(isAlternativeAction)) {
    await invoke("open_media_in_new_window", { path });
    return;
  }
  setPlayerState(await invoke("open_media", { path }), { force: true, presentOsd: true });
}

async function completeInitialLaunch() {
  const query = new URLSearchParams(window.location.search);
  if (isAuxiliaryWindow || isMiniPlayerWindow || query.has("player-session")) return;
  const action = await invoke("complete_initial_launch");
  if (action === "open-panel") await openMediaFromNativeDialog();
}

async function activateAuxiliaryWindowSurface(context = null) {
  if (!isAuxiliaryWindow) return;
  let resolvedContext = context;
  if (!resolvedContext) {
    try {
      resolvedContext = await invoke("get_auxiliary_window_context");
    } catch (error) {
      console.error("Unable to restore the auxiliary window context", error);
      resolvedContext = { role: auxiliaryWindowRole };
    }
  }
  if (resolvedContext?.role && resolvedContext.role !== auxiliaryWindowRole) return;

  if (isOpenUrlAuxiliaryWindow) {
    showOpenUrlPanel(Boolean(resolvedContext?.isAlternativeAction), {
      enqueue: Boolean(resolvedContext?.enqueue),
    });
    return;
  }
  if (isFilterAuxiliaryWindow) {
    try {
      setPlayerState(await invoke("get_player_snapshot"), { force: true });
    } catch (error) {
      console.error("Unable to refresh the filter window player", error);
    }
    await showFilterPanel(auxiliaryWindowRole === "audio-filter" ? "audio" : "video");
    return;
  }
  if (isPreferencesAuxiliaryWindow) {
    const pane = String(resolvedContext?.pane || "");
    if (PREFERENCE_PANES.some((candidate) => candidate.id === pane)) activePreferencePane = pane;
    if (resolvedContext?.selectedPluginIdentifier) {
      applyPluginPreferenceWindowContext(resolvedContext.selectedPluginIdentifier);
    }
    await showPreferencesPanel();
    if (resolvedContext?.drainPendingPluginInstalls) {
      void drainPendingPluginInstallNotifications();
    }
  }
}

async function openMediaInNewWindowFromNativeDialog() {
  try {
    await invoke("open_media_dialog_new_window");
  } catch {
    showOsd("Open Failed");
  }
}

async function addMediaToPlaylistFromNativeDialog() {
  const nextState = await invoke("enqueue_media_dialog");
  if (!nextState) return;
  setPlayerState(nextState, { force: true, presentOsd: true });
}

async function loadExternalTrackFromNativeDialog(kind) {
  const nextState = await invoke("load_external_track_dialog", { kind });
  if (!nextState) return;
  setPlayerState(nextState, { force: true, presentOsd: true });
}

async function chooseSubtitleFontFromNativeDialog() {
  const nextState = await invoke("choose_subtitle_font_dialog");
  if (!nextState) return;
  setPlayerState(nextState, { force: true });
}

async function restoreLaunchMedia(snapshot) {
  const launchPath = new URLSearchParams(window.location.search).get("open");
  if (!launchPath) return snapshot;
  return invoke("open_media", { path: launchPath });
}

async function restoreFilterPanelPreview() {
  if (tauriInvoke) return;
  const params = new URLSearchParams(window.location.search);
  const kind = params.get("filterPanel");
  if (!matchesFilterKind(kind)) return;
  const current = params.get("currentFilter");
  if (current && parseFilterRawForUi(current)) {
    const key = kind === "audio" ? "audio_filters" : "video_filters";
    mockState[key] = [parseFilterRawForUi(current)];
    setPlayerState(structuredClone(mockState), { force: true });
  }
  await showFilterPanel(kind);
  const previewWidth = Number(params.get("filterPreviewWidth"));
  if (Number.isFinite(previewWidth) && previewWidth >= 320 && previewWidth <= 480) {
    document.querySelector(".filter-window").style.width = `${previewWidth}px`;
  }
  const preset = params.get("filterPreset");
  if (preset) showFilterPresetSheet(preset);
}

function matchesFilterKind(kind) {
  return kind === "video" || kind === "audio";
}

function setPlayerState(nextState, options = {}) {
  if (!nextState) return false;
  const nativeMpvEvents = dispatchPluginMpvEventsFromState(nextState);
  const previousState = state;
  const nextOsdMessageId = Number(nextState.osd_message_id);
  const hasNativeOsdMessageId = Number.isFinite(nextOsdMessageId);
  const shouldPresentOsd = Boolean(
    nextState.osd_message &&
      (hasNativeOsdMessageId
        ? options.forcePresentOsd || nextOsdMessageId !== Number(previousState?.osd_message_id)
        : options.presentOsd || nextState.osd_message !== previousState?.osd_message)
  );
  normalizePlaylistSelection(nextState.playlist?.length ?? 0);
  const fingerprint = playerStateFingerprint(nextState);
  state = nextState;
  stateEpoch += 1;
  refreshNativePlayerMenu(nextState);
  if (!els.filterModal.hidden) renderFilterPanel();
  if (!options.force && fingerprint === renderedStateFingerprint) {
    if (shouldPresentOsd) showPlayerOsd(nextState.osd_message, nextState);
    return false;
  }
  renderedStateFingerprint = fingerprint;
  render(nextState);
  if (shouldPresentOsd) showPlayerOsd(nextState.osd_message, nextState);
  maybeAutoSearchOnlineSubtitles(nextState);
  emitPluginPlayerState(nextState);
  emitPluginStateEvents(previousState, nextState, { nativeMpvEvents });
  return true;
}

function emitPluginStateEvents(previousState, nextState, { nativeMpvEvents = false } = {}) {
  const previousUrl = previousState?.current_url;
  const nextUrl = nextState.current_url;
  if (!nativeMpvEvents && nextUrl && nextUrl !== previousUrl) {
    emitPluginEvent("iina.file-started");
    emitPluginEvent("mpv.file-start");
  }
  if (
    !nativeMpvEvents &&
    nextUrl &&
    !Boolean(nextState.file_loading) &&
    (nextUrl !== previousUrl || Boolean(previousState?.file_loading))
  ) {
    emitPluginEvent("iina.file-loaded", nextUrl);
    emitPluginEvent("mpv.file-loaded");
  }
  if (
    nextState.playback_error &&
    (nextState.playback_error.code !== previousState?.playback_error?.code ||
      nextState.playback_error.message !== previousState?.playback_error?.message)
  ) {
    setTimeout(() => window.alert(tr("Cannot open file or stream!")), 0);
  }
  if (Boolean(previousState?.pip_active) !== Boolean(nextState.pip_active)) {
    emitPluginEvent("iina.pip.changed", Boolean(nextState.pip_active));
  }
  const wasMusicMode = previousState?.mode === "mini-player";
  const isMusicMode = nextState.mode === "mini-player";
  if (wasMusicMode !== isMusicMode) {
    emitPluginEvent("iina.music-mode.changed", isMusicMode);
  }
  const previousProperties = previousState?.mpv_properties || {};
  const nextProperties = nextState.mpv_properties || {};
  if (!nativeMpvEvents) {
    for (const [property, value] of Object.entries(nextProperties)) {
      if (previousProperties[property] !== value) {
        emitPluginEvent(`mpv.${property}.changed`, value);
      }
    }
  }
}

function maybeAutoSearchOnlineSubtitles(nextState) {
  const source = nextState.current_url;
  if (!source) {
    autoSubtitleSearchAttemptSource = null;
    return;
  }
  if (source === autoSubtitleSearchAttemptSource) return;
  if (!Boolean(getPreferenceValue("autoSearchOnlineSub"))) return;

  const minimumDurationSeconds = Math.max(
    1,
    Number(getPreferenceValue("autoSearchThreshold")) || 20
  ) * 60;
  const hasVideo = Boolean(nextState.media_info?.video_summary);
  const hasSubtitle = (nextState.tracks?.subtitles || []).some(
    (track) => Number(track.id) !== 0
  );
  const duration = Number(nextState.duration_seconds) || 0;
  if (!hasVideo || hasSubtitle || duration < minimumDurationSeconds || onlineSubtitleBusy) return;

  // Mark first so the runtime snapshot poll cannot start duplicate requests.
  autoSubtitleSearchAttemptSource = source;
  void showOnlineSubtitlePanel();
}

function playerStateFingerprint(nextState) {
  return JSON.stringify({
    mode: nextState.mode,
    current_url: nextState.current_url,
    file_loading: nextState.file_loading,
    playback_error: nextState.playback_error,
    media_title: nextState.media_title,
    music_title: nextState.music_title,
    music_album: nextState.music_album,
    music_artist: nextState.music_artist,
    duration_seconds: nextState.duration_seconds,
    position_seconds: nextState.position_seconds,
    volume: nextState.volume,
    speed: nextState.speed,
    muted: nextState.muted,
    paused: nextState.paused,
    loop_mode: nextState.loop_mode,
    ab_loop: nextState.ab_loop,
    audio_devices: nextState.audio_devices,
    audio_device: nextState.audio_device,
    video_filters: nextState.video_filters,
    audio_filters: nextState.audio_filters,
    playlist: nextState.playlist,
    playlist_cache: nextState.playlist_cache,
    recent_documents: nextState.recent_documents,
    last_playback: nextState.last_playback,
    chapters: nextState.chapters,
    tracks: nextState.tracks,
    sidebar: nextState.sidebar,
    quick_settings: nextState.quick_settings,
    osc_visible: nextState.osc_visible,
    pip_active: nextState.pip_active,
    osd_message: nextState.osd_message,
    osd_message_id: nextState.osd_message_id,
    mpv_properties: nextState.mpv_properties,
  });
}

function nativePlayerMenuFingerprint(nextState) {
  const position = Number(nextState.position_seconds) || 0;
  const activeChapter = (nextState.chapters || []).reduce(
    (selected, chapter, index) => Number(chapter.time_seconds) <= position ? index : selected,
    -1
  );
  return JSON.stringify({
    mode: nextState.mode,
    current_url: nextState.current_url,
    paused: nextState.paused,
    speed: nextState.speed,
    volume: nextState.volume,
    muted: nextState.muted,
    loop_mode: nextState.loop_mode,
    ab_loop: nextState.ab_loop,
    audio_devices: nextState.audio_devices,
    audio_device: nextState.audio_device,
    video_filters: nextState.video_filters,
    audio_filters: nextState.audio_filters,
    playlist: (nextState.playlist || []).map((item) => [item.title, item.current]),
    chapters: (nextState.chapters || []).map((chapter) => [chapter.title, chapter.time_seconds]),
    activeChapter,
    tracks: nextState.tracks,
    second_subtitle_id: nextState.second_subtitle_id,
    sidebar: nextState.sidebar,
    deinterlace: nextState.quick_settings?.deinterlace,
    video_aspect: nextState.quick_settings?.video_aspect,
    video_crop: nextState.quick_settings?.video_crop,
    custom_crop: nextState.quick_settings?.custom_crop,
    video_rotate: nextState.quick_settings?.video_rotate,
    video_flipped: nextState.quick_settings?.video_flipped,
    video_mirrored: nextState.quick_settings?.video_mirrored,
    audio_delay: nextState.quick_settings?.audio_delay,
    sub_delay: nextState.quick_settings?.sub_delay,
    pip_active: nextState.pip_active,
  });
}

function refreshNativePlayerMenu(nextState, force = false) {
  if (!tauriInvoke || !document.hasFocus()) return;
  const fingerprint = nativePlayerMenuFingerprint(nextState);
  if (!force && fingerprint === nativeMenuStateFingerprint) return;
  nativeMenuStateFingerprint = fingerprint;
  void invoke("refresh_player_menu").catch(() => {});
}

function startRuntimeSnapshotPolling() {
  const poll = () => {
    if (shouldPollRuntimeSnapshot()) void refreshRuntimeSnapshot();
    window.setTimeout(poll, RUNTIME_SNAPSHOT_POLL_MS);
  };
  window.setTimeout(poll, RUNTIME_SNAPSHOT_POLL_MS);
}

function startOscTimeDisplayTicker() {
  const tick = () => {
    if (!document.hidden && state.mode !== "initial") {
      const position = estimatedOscPosition();
      renderOscTimeLabels(position, oscTimeSnapshotDuration);
      renderMiniTimeLabels(position, oscTimeSnapshotDuration);
    }
    const interval = timeDisplayPrecision() >= 2 ? OSC_TIME_PRECISE_REFRESH_MS : RUNTIME_SNAPSHOT_POLL_MS;
    window.setTimeout(tick, interval);
  };
  window.setTimeout(tick, RUNTIME_SNAPSHOT_POLL_MS);
}

function shouldPollRuntimeSnapshot() {
  if (document.hidden || runtimeSnapshotInFlight) return false;
  return state.mode !== "initial" || Boolean(state.current_url);
}

async function refreshRuntimeSnapshot() {
  runtimeSnapshotInFlight = true;
  const requestEpoch = stateEpoch;
  try {
    const snapshot = await invoke("get_player_snapshot");
    if (requestEpoch === stateEpoch) {
      setPlayerState(snapshot);
    }
  } catch {
    // Keep UI responsive if a transient native sync fails; explicit commands still surface errors.
  } finally {
    runtimeSnapshotInFlight = false;
  }
}

function toggleSidebar(tab) {
  if (state.sidebar.visible && state.sidebar.tab === tab) {
    command({ type: "hide-sidebar" });
  } else {
    command({ type: "show-sidebar", tab });
  }
}

function beginSurfaceFocusEpoch() {
  const epoch = firstMouseGate.beginFocus();
  window.setTimeout(() => {
    if (document.hasFocus()) firstMouseGate.commitFocus(epoch);
  }, 0);
}

function suppressFirstMouseSurfacePointer(event) {
  const phase = event.type === "pointerdown"
    ? "down"
    : event.type === "pointerup"
      ? "up"
      : event.type === "pointercancel"
        ? "cancel"
        : "move";
  if (!firstMouseGate.shouldSuppressPointer(event, phase)) return;
  event.preventDefault();
  event.stopImmediatePropagation();
}

function suppressFirstMouseSurfaceAction(event) {
  if (!firstMouseGate.shouldSuppressAction(event)) return;
  clearTimeout(surfaceClickTimer);
  surfaceClickTimer = undefined;
  surfaceWindowDragState = undefined;
  event.preventDefault();
  event.stopImmediatePropagation();
}

function handleSurfaceClick(event) {
  if (performance.now() < suppressSurfaceClickUntil) {
    event.preventDefault();
    suppressSurfaceClickUntil = 0;
    return;
  }
  if (event.button !== 0 || event.detail > 1 || isInteractiveMouseTarget(event.target)) return;
  const stopped = dispatchPluginInput(PLUGIN_INPUT_MOUSE, "mouseUp", pluginMouseEventArgs(event), null, () => {
    clearTimeout(surfaceClickTimer);
    if (state.sidebar.visible) {
      void command({ type: "hide-sidebar" });
      return;
    }
    const action = mouseClickAction("singleClickAction");
    if (mouseClickAction("doubleClickAction") === MouseClickAction.none) {
      void performMouseClickAction(action);
      return;
    }
    surfaceClickTimer = setTimeout(() => {
      void performMouseClickAction(action);
    }, DEFAULT_DOUBLE_CLICK_INTERVAL_MS);
  });
  if (stopped) event.preventDefault();
}

function beginSurfaceWindowDrag(event) {
  if (
    event.defaultPrevented
    || event.button !== 0
    || windowFullscreenActive
    || customCropEditor
    || isInteractiveMouseTarget(event.target)
    || els.osc.classList.contains("is-dragging")
  ) {
    surfaceWindowDragState = undefined;
    return;
  }
  surfaceWindowDragState = {
    pointerId: event.pointerId,
    x: event.clientX,
    y: event.clientY,
    active: false,
  };
}

function updateSurfaceWindowDrag(event) {
  const drag = surfaceWindowDragState;
  if (!drag || drag.pointerId !== event.pointerId || drag.active || !(event.buttons & 1)) return;
  if (!exceedsWindowDragThreshold(drag, { x: event.clientX, y: event.clientY })) return;
  drag.active = true;
  clearTimeout(surfaceClickTimer);
  surfaceClickTimer = undefined;
  suppressSurfaceClickUntil = performance.now() + 750;
  void invoke("start_player_window_drag").catch((error) => {
    console.error("Unable to begin native player-window drag", error);
  });
}

function finishSurfaceWindowDrag(event) {
  const drag = surfaceWindowDragState;
  if (!drag || drag.pointerId !== event.pointerId) return;
  if (drag.active) suppressSurfaceClickUntil = performance.now() + 750;
  surfaceWindowDragState = undefined;
}

async function handleSurfaceDoubleClick(event) {
  if (event.button !== 0 || isInteractiveMouseTarget(event.target)) return;
  const stopped = dispatchPluginInput(PLUGIN_INPUT_MOUSE, "mouseUp", pluginMouseEventArgs(event), null, () => {
    event.preventDefault();
    clearTimeout(surfaceClickTimer);
    void performMouseClickAction(mouseClickAction("doubleClickAction"));
  });
  if (stopped) event.preventDefault();
}

function handleSurfaceAuxClick(event) {
  if (event.button !== 1) return;
  event.preventDefault();
  if (isInteractiveMouseTarget(event.target)) return;
  const stopped = dispatchPluginInput(PLUGIN_INPUT_OTHER_MOUSE, "mouseUp", pluginMouseEventArgs(event), null, () => {
    void performMouseClickAction(mouseClickAction("middleClickAction"));
  });
  if (stopped) event.preventDefault();
}

function handleSurfaceContextMenu(event) {
  event.preventDefault();
  if (isInteractiveMouseTarget(event.target)) return;
  const stopped = dispatchPluginInput(PLUGIN_INPUT_RIGHT_MOUSE, "mouseUp", pluginMouseEventArgs(event), null, () => {
    void performMouseClickAction(mouseClickAction("rightClickAction"));
  });
  if (stopped) event.preventDefault();
}

function floatingOscCoordinatesFromPointer(event) {
  const parent = els.player.getBoundingClientRect();
  const rect = els.osc.getBoundingClientRect();
  let horizontal = (event.clientX - parent.left - (oscDragState?.offsetX || rect.width / 2) + rect.width / 2) / parent.width;
  let vertical = (parent.bottom - event.clientY - (oscDragState?.offsetBottom ?? 0)) / parent.height;
  horizontal = Math.max(rect.width / parent.width / 2, Math.min(1 - rect.width / parent.width / 2, horizontal));
  vertical = Math.max(0, Math.min(1 - rect.height / parent.height, vertical));
  if (Boolean(getPreferenceValue("controlBarStickToCenter")) && Math.abs(horizontal - 0.5) <= 0.035) {
    horizontal = 0.5;
  }
  return { horizontal, vertical };
}

function beginOscDrag(event) {
  if (Number(getPreferenceValue("oscPosition")) !== 0) return;
  if (event.target instanceof Element && event.target.closest("button,input,select,a")) return;
  const rect = els.osc.getBoundingClientRect();
  oscDragState = {
    pointerId: event.pointerId,
    offsetX: event.clientX - rect.left,
    offsetBottom: rect.bottom - event.clientY,
  };
  els.osc.setPointerCapture(event.pointerId);
  els.osc.classList.add("is-dragging");
  event.preventDefault();
}

function updateOscDrag(event) {
  if (!oscDragState || oscDragState.pointerId !== event.pointerId) return;
  const { horizontal, vertical } = floatingOscCoordinatesFromPointer(event);
  els.osc.style.left = `${horizontal * 100}%`;
  els.osc.style.bottom = `${vertical * 100}%`;
  event.preventDefault();
}

function finishOscDrag(event) {
  if (!oscDragState || oscDragState.pointerId !== event.pointerId) return;
  const { horizontal, vertical } = floatingOscCoordinatesFromPointer(event);
  els.osc.releasePointerCapture?.(event.pointerId);
  els.osc.classList.remove("is-dragging");
  oscDragState = undefined;
  void setPreferenceValues([
    ["controlBarPositionHorizontal", horizontal],
    ["controlBarPositionVertical", vertical],
  ]).then(() => render(state));
}

function handlePlayerPointerMovement(event) {
  setSurfaceCursorHidden(false);
  void setSurfaceOscVisible(true);
  if (isOscAutoHideSuspendedTarget(event.target)) {
    clearTimeout(surfaceOscHideTimer);
  } else {
    scheduleSurfaceOscHide();
  }
}

function handlePlayerPointerLeave() {
  clearTimeout(surfaceOscHideTimer);
  void setSurfaceOscVisible(false);
}

function isOscAutoHideSuspendedTarget(target) {
  return target instanceof Element && Boolean(target.closest(".osc,.top-bar"));
}

function scheduleSurfaceOscHide() {
  clearTimeout(surfaceOscHideTimer);
  if (state.mode === "initial" || state.mode === "mini-player" || state.pip_active) return;
  const configured = Number(getPreferenceValue("controlBarAutoHideTimeout"));
  const seconds = Number.isFinite(configured) && configured >= 0 ? configured : 2.5;
  surfaceOscHideTimer = setTimeout(() => {
    void setSurfaceOscVisible(false, { hideCursor: true });
  }, seconds * 1000);
}

function handlePluginPointerDown(event) {
  if (isInteractiveMouseTarget(event.target)) return;
  const input = pluginMouseInputFromButton(event.button);
  if (dispatchPluginInput(input, "mouseDown", pluginMouseEventArgs(event))) event.preventDefault();
}

function handlePluginPointerMove(event) {
  if (isInteractiveMouseTarget(event.target) || !event.buttons) return;
  for (const input of pluginMouseInputsFromButtons(event.buttons)) {
    if (dispatchPluginInput(input, "mouseDrag", pluginMouseEventArgs(event))) event.preventDefault();
  }
}

function handleSurfaceWheel(event) {
  const precise = event.deltaMode === WheelEvent.DOM_DELTA_PIXEL;
  const payload = {
    kind: "scroll",
    x: event.clientX,
    y: event.clientY,
    delta_x: event.deltaX,
    delta_y: -event.deltaY,
    precise,
    natural: Boolean(event.webkitDirectionInvertedFromDevice),
    phase: NSEventPhase.none,
    momentum_phase: NSEventPhase.none,
  };
  if (routeScrollInput(payload, event.target)) event.preventDefault();
}

function handleNativePlayerInput(payload) {
  if (!payload || state.mode === "initial") return;
  const target = document.elementFromPoint(Number(payload.x) || 0, Number(payload.y) || 0);
  if (payload.kind === "scroll") {
    routeScrollInput(payload, target);
  } else if (payload.kind === "pressure") {
    handleNativePressureInput(payload);
  } else if (payload.kind === "magnify") {
    handleNativeMagnifyInput(payload, target);
  }
}

function routeScrollInput(payload, target) {
  const plan = nativeScrollGesture.advance(
    payload,
    (direction) => scrollActionForTarget(target, direction),
    !state.paused,
  );
  // PlayerWindowController consumes the NSEvent even for None/Pass to mpv;
  // only Volume and Seek currently have an IINA-side action in 1.3.5.
  const consumes = true;
  if (plan.pause) enqueueNativeInputCommand(() => command({ type: "pause" }));
  if (plan.action === ScrollAction.volume) {
    const amount = iinaScrollAmount(
      plan.action,
      payload,
      plan.direction,
      getPreferenceValue("volumeScrollAmount"),
    );
    if (amount) enqueueNativeInputCommand(() => command({
      type: "set-volume",
      volume: (Number(state.volume) || 0) + amount,
    }));
  } else if (plan.action === ScrollAction.seek) {
    const amount = iinaScrollAmount(
      plan.action,
      payload,
      plan.direction,
      getPreferenceValue("relativeSeekAmount"),
    );
    if (amount) enqueueNativeInputCommand(() => command({
      type: "seek-relative",
      seconds: amount,
      option: ["relative", "exact", "auto"][clampedPreferenceIndex("useExactSeek", 0, 2)] || "relative",
    }));
  }
  if (plan.resume) enqueueNativeInputCommand(() => command({ type: "resume" }));
  return consumes;
}

function scrollActionForTarget(target, direction) {
  if (!(target instanceof Element)) return ScrollAction.none;
  if (state.mode === "mini-player") {
    if (target === els.miniPlaySlider && !els.miniPlaySlider.disabled) return ScrollAction.seek;
    if (target === els.miniVolumeSlider && !els.miniVolumeSlider.disabled) return ScrollAction.volume;
    if (target.closest(".mini-player-controls,.mini-volume-popover,.mini-playlist")) return ScrollAction.none;
  } else {
    if (target.closest(".sidebar,.top-bar,.subtitle-popover,.url-window,.online-subtitles-accessory")) {
      return ScrollAction.none;
    }
    if (target.closest(".play-slider-track") && !els.playSlider.disabled) return ScrollAction.seek;
    if (target.closest(".osc-volume-group") && !els.volumeSlider.disabled) return ScrollAction.volume;
    if (target.closest(".osc")) return ScrollAction.none;
  }
  return normalizeScrollAction(scrollAction(
    direction === "horizontal" ? "horizontalScrollAction" : "verticalScrollAction",
  ));
}

function enqueueNativeInputCommand(task) {
  nativeInputCommandQueue = nativeInputCommandQueue
    .then(task)
    .catch((error) => console.error("Native player input failed", error));
  return nativeInputCommandQueue;
}

function handleNativePressureInput(payload) {
  const stage = Number(payload.stage) || 0;
  if (!forceTouchSecondStage && stage === 2) {
    forceTouchSecondStage = true;
    enqueueNativeInputCommand(() => performMouseClickAction(mouseClickAction("forceTouchAction")));
  } else if (stage === 1 || stage === 0) {
    forceTouchSecondStage = false;
  }
}

function handleNativeMagnifyInput(payload, _target) {
  if (state.mode !== "player" || customCropEditor) return;
  const phase = Number(payload.phase) || 0;
  const began = phaseContains(phase, NSEventPhase.began);
  const ended = phaseContains(phase, NSEventPhase.ended | NSEventPhase.cancelled);
  if (began) magnifyFullscreenHandled = false;
  const magnification = Number(payload.magnification) || 0;
  const action = normalizePinchAction(getPreferenceValue("pinchAction"));
  if (action === PinchAction.fullscreen && !magnifyFullscreenHandled && magnification !== 0) {
    const enlarge = magnification > 0;
    magnifyFullscreenHandled = true;
    if (enlarge !== windowFullscreenActive) enqueueNativeInputCommand(toggleFullscreenFromSurface);
  } else if (
    action === PinchAction.windowSize
    && !windowFullscreenActive
    && !began
    && !ended
    && magnification !== 0
  ) {
    enqueueNativeInputCommand(async () => {
      await invoke("resize_player_window_by_magnification", { magnification });
    });
  }
  if (ended) magnifyFullscreenHandled = false;
}

function mouseClickAction(key) {
  return normalizeMouseClickAction(getPreferenceValue(key));
}

function scrollAction(key) {
  const value = Number(getPreferenceValue(key));
  return Number.isFinite(value) ? value : 2;
}

async function performMouseClickAction(action) {
  await dispatchMouseClickAction(action, {
    fullscreen: toggleFullscreenFromSurface,
    pause: () => command({ type: "toggle-pause" }),
    hideOsc: () => setSurfaceOscVisible(false, { hideCursor: true }),
    togglePip: togglePictureInPicture,
  });
}

function setSurfaceCursorHidden(hidden) {
  surfaceCursorHidden = Boolean(hidden);
  els.player.classList.toggle("player-window--cursor-hidden", surfaceCursorHidden);
}

function setSurfaceOscVisible(visible, { hideCursor = false } = {}) {
  if (hideCursor) {
    clearTimeout(surfaceOscHideTimer);
    setSurfaceCursorHidden(true);
  }
  surfaceOscDesiredVisible = Boolean(visible);
  if (!surfaceOscVisibilityRunning) {
    surfaceOscVisibilityTask = drainSurfaceOscVisibility();
  }
  return surfaceOscVisibilityTask;
}

async function drainSurfaceOscVisibility() {
  surfaceOscVisibilityRunning = true;
  try {
    while (surfaceOscDesiredVisible !== null) {
      const visible = surfaceOscDesiredVisible;
      surfaceOscDesiredVisible = null;
      // IINA refuses to start an OSC hide animation while the main window is in PIP.
      if (!visible && state.pip_active && state.osc_visible) continue;
      if (Boolean(state.osc_visible) !== visible) {
        await command({ type: "toggle-osc" });
      }
    }
  } catch (error) {
    console.error("Unable to update OSC visibility", error);
  } finally {
    surfaceOscVisibilityRunning = false;
    if (surfaceOscDesiredVisible !== null) {
      surfaceOscVisibilityTask = drainSurfaceOscVisibility();
    }
  }
}

function clampedPreferenceIndex(key, min, max) {
  const value = Number(getPreferenceValue(key));
  if (!Number.isFinite(value)) return min;
  return Math.max(min, Math.min(max, Math.round(value)));
}

async function toggleFullscreenFromSurface() {
  const fullscreen = await invoke("toggle_window_fullscreen");
  applyFullscreenState(Boolean(fullscreen));
  showOsd(fullscreen ? "Full Screen" : "Exit Full Screen");
}

async function captureScreenshot() {
  if (!state.current_url || Number(state.mpv_properties?.vid) <= 0) return;

  try {
    const result = await invoke("capture_current_screenshot");
    showScreenshotOsd(result);
  } catch {
    window.alert(tr("Cannot take a screenshot!"));
  }
}

async function revealScreenshotFolder() {
  try {
    await invoke("reveal_screenshot_folder");
    showOsd("Screenshot Folder");
  } catch {
    showOsd("Screenshot Folder Failed");
  }
}

function showScreenshotOsd(result) {
  if (!result?.show_preview || !result?.path) {
    showOsd("Screenshot Captured");
    return;
  }

  showOsd("Screenshot Captured", {
    timeout: 5000,
    previewPath: result.path,
    savedToFile: result.saved_to_file,
  });
}


const DEFAULT_KEY_BINDING_ROWS = parseKeyMappingInputConf(IINA_DEFAULT_INPUT_CONF)
  .map(inputMappingToKeyBindingRow)
  .filter(Boolean);

const KEYBINDING_MODIFIERS = [
  ["meta", "Cmd"],
  ["ctrl", "Ctrl"],
  ["alt", "Alt"],
  ["shift", "Shift"],
];

function storedRawAction(row) {
  if (typeof row?.rawAction === "string") return row.rawAction;
  if (typeof row?.rawCommand === "string") {
    return row.isIINACommand && row.rawCommand.startsWith("#@iina ")
      ? row.rawCommand.slice("#@iina ".length).trim()
      : row.rawCommand;
  }
  return actionRawValue(row?.action);
}

function inputMappingToKeyBindingRow(mapping, index = 0) {
  const model = normalizeKeyMappingModel({
    ...mapping,
    rawAction: storedRawAction(mapping),
  }, index);
  if (!model) return null;
  const runtimeAction = model.effectiveRawAction ?? model.normalizedRawAction;
  return {
    ...model,
    action: inputActionToRuntimeAction(runtimeAction, model.isIINACommand, model.runtimeEligible, model.inactiveReason),
    rawCommand: model.isIINACommand
      ? `#@iina ${model.normalizedRawAction}`
      : model.normalizedRawAction,
  };
}

function activeKeyBindings() {
  return activeKeyMappingsLastWins(keyBindingRowsFromPreferences())
    .map(inputMappingToKeyBindingRow)
    .filter((row) => row && isExecutableKeyBindingAction(row.action));
}

function keyBindingRowsFromPreferences() {
  const configured = preferences?.values && Object.hasOwn(preferences.values, "modeledKeyBindings")
    ? preferences.values.modeledKeyBindings
    : mockPreferences.values.modeledKeyBindings;
  return keyMappingPreferenceRows(configured, DEFAULT_KEY_BINDING_ROWS)
    .map(normalizeKeyBindingRow)
    .filter(Boolean);
}

function normalizeKeyBindingRow(row, index = 0) {
  return inputMappingToKeyBindingRow(row, index);
}

function serializeKeyBindingRow(row) {
  return serializeKeyMapping(row);
}

function generateInputConf(rows) {
  return generateKeyMappingInputConf(rows);
}

function keyBindingRowsFromInputConf(source) {
  return parseKeyMappingInputConf(source).map(inputMappingToKeyBindingRow).filter(Boolean);
}

function isExecutableKeyBindingAction(action) {
  if (!action?.type) return false;
  if (action.type === "player") return Boolean(action.command?.type);
  return [
    "iina-command",
    "mpv-command",
    "seek-relative",
    "volume-relative",
    "sidebar",
    "screenshot",
    "fullscreen-toggle",
    "fullscreen-set",
    "music-mode",
    "picture-in-picture",
    "osd",
  ].includes(action.type);
}

function isExecutableKeyBindingRow(row) {
  return Boolean(row?.runtimeEligible) && isExecutableKeyBindingAction(row.action);
}

function inputActionToRuntimeAction(rawAction, isIINACommand, runtimeEligible = true, inactiveReason = null) {
  const action = String(rawAction ?? "").trim();
  if (!runtimeEligible) return { type: "inactive", rawAction: action, reason: inactiveReason };
  if (!isIINACommand) return { type: "mpv-command", action };
  return iinaPrivateActionToRuntimeAction(action.split(/\s+/).filter(Boolean), action);
}

function iinaPrivateActionToRuntimeAction(parts, rawAction) {
  if (parts.length === 1 && IINA_PRIVATE_COMMANDS.has(parts[0])) {
    return { type: "iina-command", action: parts[0] };
  }
  return unsupportedInputAction(rawAction, true);
}

function unsupportedInputAction(rawAction, isIINACommand) {
  return {
    type: "unsupported",
    rawAction,
    isIINACommand: Boolean(isIINACommand),
  };
}

function newKeyBindingRow() {
  return inputMappingToKeyBindingRow({
    id: `custom-${Date.now()}-${Math.random().toString(16).slice(2)}`,
    rawKey: "F13",
    rawAction: "ignore",
    isIINACommand: false,
  });
}

function modifiersFromEvent(event) {
  return {
    alt: event.altKey,
    ctrl: event.ctrlKey,
    meta: event.metaKey,
    shift: event.shiftKey,
  };
}

function isModifierOnlyKey(event) {
  return isModifierOnlyKeyboardEvent(event);
}

function modifiersLabel(modifiers) {
  const active = KEYBINDING_MODIFIERS.filter(([key]) => Boolean(modifiers?.[key])).map(([, label]) => label);
  return active.length ? active.join("+") : "-";
}

function keyBindingSignature(row) {
  return normalizedKeyMappingSignature(row);
}

function keyBindingRowsForDisplay(rows, duplicateSignatures) {
  return rows
    .map((row, index) => ({ row, index }))
    .filter(({ row }) => {
      if (keyBindingFilterMode === "conflicts") return duplicateSignatures.has(keyBindingSignature(row));
      if (keyBindingFilterMode === "unsupported") return !isExecutableKeyBindingRow(row);
      return true;
    });
}

function actionLabel(action) {
  if (action.type === "iina-command") return IINA_PRIVATE_COMMAND_LABELS[action.action] || action.action;
  if (action.type === "mpv-command") return action.action;
  if (action.type === "inactive") return `${action.rawAction} (${action.reason || "inactive"})`;
  if (action.type === "seek-relative") return action.seconds >= 0 ? `Seek Forward ${action.seconds}s` : `Seek Back ${Math.abs(action.seconds)}s`;
  if (action.type === "volume-relative") return action.amount >= 0 ? `Volume Up ${action.amount}` : `Volume Down ${Math.abs(action.amount)}`;
  if (action.type === "screenshot") return "Take Screenshot";
  if (action.type === "fullscreen-toggle") return "Toggle Full Screen";
  if (action.type === "fullscreen-set") return action.fullscreen ? "Enter Full Screen" : "Exit Full Screen";
  if (action.type === "sidebar") return `Show ${sidebarTabTitle(action.tab)}`;
  if (action.type === "music-mode") return "Toggle Music Mode";
  if (action.type === "picture-in-picture") return "Toggle Picture-in-Picture";
  if (action.type === "osd") return action.message;
  if (action.type === "unsupported") return action.rawAction || "Unsupported Action";
  if (action.type !== "player") return action.type;

  const command = action.command ?? {};
  if (command.type === "toggle-pause") return "Toggle Pause";
  if (command.type === "toggle-mute") return "Toggle Mute";
  if (command.type === "cycle-ab-loop") return "A-B Loop";
  if (command.type === "stop") return "Stop";
  if (command.type === "frame-step") return command.backwards ? "Frame Back Step" : "Frame Step";
  if (command.type === "playlist-next") return "Playlist Next";
  if (command.type === "playlist-prev") return "Playlist Previous";
  if (command.type === "cycle-track") return `Cycle ${trackKindTitle(command.kind)}`;
  if (command.type === "multiply-speed") return `Speed x${Number(command.factor).toFixed(2)}`;
  if (command.type === "set-speed") return `Set Speed ${Number(command.speed).toFixed(2)}x`;
  return command.type;
}

function actionRawValue(action) {
  if (!action) return "";
  if (action.type === "iina-command") return `#@iina ${action.action}`;
  if (action.type === "mpv-command") return action.action;
  if (action.type === "inactive") return action.rawAction;
  if (action.type === "music-mode") return "#@iina toggle-music-mode";
  if (action.type === "picture-in-picture") return "#@iina toggle-pip";
  if (action.type === "seek-relative") return `seek ${action.seconds}`;
  if (action.type === "volume-relative") return `add volume ${action.amount}`;
  if (action.type === "screenshot") return "screenshot";
  if (action.type === "fullscreen-toggle") return "cycle fullscreen";
  if (action.type === "fullscreen-set") return `set fullscreen ${action.fullscreen ? "yes" : "no"}`;
  if (action.type === "sidebar") return `#@iina ${action.tab}-panel`;
  if (action.type === "osd") return action.message;
  if (action.type === "unsupported") return `${action.isIINACommand ? "#@iina " : ""}${action.rawAction || ""}`.trim();
  if (action.type !== "player") return action.type;

  const command = action.command ?? {};
  if (command.type === "toggle-pause") return "cycle pause";
  if (command.type === "toggle-mute") return "cycle mute";
  if (command.type === "cycle-ab-loop") return "ab-loop";
  if (command.type === "stop") return "stop";
  if (command.type === "frame-step") return command.backwards ? "frame-back-step" : "frame-step";
  if (command.type === "playlist-next") return "playlist-next";
  if (command.type === "playlist-prev") return "playlist-prev";
  if (command.type === "cycle-track") return `cycle ${trackKindRawValue(command.kind)}`;
  if (command.type === "multiply-speed") return `multiply speed ${command.factor}`;
  if (command.type === "set-speed") return `set speed ${command.speed}`;
  return command.type;
}

function sidebarTabTitle(tab) {
  return {
    video: "Video Panel",
    audio: "Audio Panel",
    subtitles: "Subtitle Panel",
    playlist: "Playlist",
    chapters: "Chapters",
  }[tab] ?? tab;
}

function trackKindTitle(kind) {
  return {
    video: "Video",
    audio: "Audio",
    subtitles: "Subtitles",
    "second-subtitles": "Second Subtitle",
  }[kind] ?? kind;
}

function localizedTrackKindTitle(kind) {
  if (kind === "video") {
    return trKey("QuickSettingViewController", "CYP-el-A6A.label", "Video");
  }
  if (kind === "audio") {
    return trKey("QuickSettingViewController", "bzk-c2-LH5.label", "Audio");
  }
  return tr(trackKindTitle(kind));
}

function localizedQuickSettingsValue(value) {
  if (value === "Default") {
    return trKey("Localizable", "quicksetting.item_default", "Default");
  }
  if (value === "None") {
    return trKey("Localizable", "quicksetting.item_none", "None");
  }
  return tr(value);
}

function localizedTrackBadge(badge) {
  return badge === "Default"
    ? trKey("Localizable", "quicksetting.item_default", "Default")
    : tr(badge);
}

function trackKindRawValue(kind) {
  return kind === "subtitles" ? "sub" : kind;
}

function handlePlayerShortcut(event) {
  const binding = keyBindingForEvent(event);
  if (!binding) {
    return false;
  }

  event.preventDefault();
  void executeKeyBinding(binding);
  return true;
}

function keyBindingForEvent(event) {
  const normalizedKey = normalizeMpvKey(pluginKeyCodeFromEvent(event));
  return activeKeyBindings().find((binding) => binding.normalizedMpvKey === normalizedKey);
}

function eventKeyToken(event) {
  return mpvKeyTokenFromKeyboardEvent(event);
}

async function executeKeyBinding(binding) {
  const action = binding.action;
  if (action.type === "iina-command") {
    await executeIinaPrivateCommand(action.action);
  } else if (action.type === "mpv-command") {
    await command({ type: "key-binding-mpv-command", action: action.action });
  } else if (action.type === "player") {
    await command(action.command);
  } else if (action.type === "seek-relative") {
    await command({ type: "seek", seconds: (Number(state.position_seconds) || 0) + action.seconds });
  } else if (action.type === "volume-relative") {
    await command({ type: "set-volume", volume: (Number(state.volume) || 0) + action.amount });
  } else if (action.type === "sidebar") {
    await command({ type: "show-sidebar", tab: action.tab });
  } else if (action.type === "screenshot") {
    await captureScreenshot();
  } else if (action.type === "fullscreen-toggle") {
    await toggleFullscreenFromSurface();
  } else if (action.type === "fullscreen-set") {
    await setFullscreenFromShortcut(action.fullscreen);
  } else if (action.type === "music-mode") {
    await toggleMusicMode();
  } else if (action.type === "picture-in-picture") {
    await togglePictureInPicture();
  } else if (action.type === "osd") {
    showOsd(action.message);
  }
}

async function executeIinaPrivateCommand(action) {
  try {
    const execution = await invoke("execute_iina_command", { action });
    if (execution?.player) {
      setPlayerState(execution.player, { force: true, presentOsd: true });
    }
    if (execution?.frontendAction === "open") {
      await openMediaFromNativeDialog();
    } else if (execution?.frontendAction === "open-url") {
      showOpenUrlPanel(false);
    } else if (execution?.frontendAction === "find-online-subtitles") {
      await showOnlineSubtitlePanel();
    } else if (execution?.frontendAction) {
      throw new Error(`Unsupported IINA frontend action: ${execution.frontendAction}`);
    }
  } catch (error) {
    console.error(`IINA command failed: ${action}`, error);
    showOsd("Command Failed");
  }
}

async function setFullscreenFromShortcut(fullscreen) {
  try {
    const nextFullscreen = await invoke("set_window_fullscreen", { fullscreen });
    applyFullscreenState(Boolean(nextFullscreen));
    showOsd(nextFullscreen ? "Full Screen" : "Exit Full Screen");
  } catch {
    showOsd("Full Screen Failed");
  }
}

function isTypingTarget(target) {
  if (!(target instanceof Element)) return false;
  const tagName = target.tagName.toLowerCase();
  return target.isContentEditable || ["input", "textarea", "select"].includes(tagName);
}

function isInteractiveMouseTarget(target) {
  if (!(target instanceof Element)) return false;
  return Boolean(target.closest("button,input,textarea,select,a,[role='button'],.osc,.top-bar,.sidebar,.thumbnail-peek,.url-window,.online-subtitles-accessory"));
}

function preventPlayerChromeButtonMouseFocus(event) {
  if (event.button !== 0 || !(event.target instanceof Element)) return;
  if (!event.target.closest(".osc button,.top-bar button")) return;
  // IINA's OSC and title-bar buttons refuse first responder. Preventing WebKit's mousedown
  // default keeps mouse clicks functional without leaking the browser focus ring.
  event.preventDefault();
}

function setButtonIcon(button, source, label) {
  const icon = button.querySelector(".button-icon");
  if (icon && icon.getAttribute("src") !== source) {
    icon.setAttribute("src", source);
  }
  const localizedLabel = tr(label);
  button.title = localizedLabel;
  button.setAttribute("aria-label", localizedLabel);
}

function setRangeProgress(input) {
  const min = Number(input.min) || 0;
  const max = Number(input.max) || 1;
  const value = Math.max(min, Math.min(max, Number(input.value) || 0));
  const progress = max > min ? ((value - min) / (max - min)) * 100 : 0;
  input.style.setProperty("--range-progress", `${progress}%`);
}

function miniPlayerVideoAspect(nextState) {
  const track = (nextState.tracks?.video || []).find((item) => item.selected && Number(item.id) !== 0);
  const metadata = track?.metadata || {};
  let width = Number(metadata.demux_width);
  let height = Number(metadata.demux_height);
  const rotation = Math.abs(Number(metadata.demux_rotation) || 0) % 180;
  if (rotation === 90) [width, height] = [height, width];
  return width > 0 && height > 0 ? width / height : 1;
}

function miniPlayerArtistAlbum(nextState) {
  const artist = String(nextState.music_artist || "").trim();
  const album = String(nextState.music_album || "").trim();
  if (artist && album) return `${artist} - ${album}`;
  return artist || album;
}

function setMiniButtonIcon(button, source, label) {
  const icon = button.querySelector("img");
  if (icon && icon.getAttribute("src") !== source) icon.setAttribute("src", source);
  const localizedLabel = tr(label);
  button.title = localizedLabel;
  button.setAttribute("aria-label", localizedLabel);
}

function renderMiniPlayer(nextState, position, duration, hasMedia, hasAudio, hasVideo) {
  els.miniPlayerUi.hidden = !isMiniPlayerWindow;
  if (!isMiniPlayerWindow) return;

  const aspect = miniPlayerVideoAspect(nextState);
  const expectedVideoHeight = miniVideoVisible ? Math.round(Math.max(300, window.innerWidth) / aspect) : 0;
  els.miniPlayerUi.style.setProperty("--mini-video-height", `${expectedVideoHeight}px`);
  els.miniPlayerUi.classList.toggle("mini-player-ui--video-hidden", !miniVideoVisible);
  els.miniVideoRegion.hidden = !miniVideoVisible;
  els.miniDefaultAlbumArt.hidden = !miniVideoVisible || hasVideo;
  // The reference playlist wrapper remains below the control bar while folded, so a live
  // vertical resize reveals it before AppKit commits the new visibility state at gesture end.
  els.miniPlaylist.hidden = false;
  els.miniPlaylist.setAttribute("aria-hidden", String(!miniPlaylistVisible));
  els.miniPlaylistButton.classList.toggle("is-active", miniPlaylistVisible);
  els.miniAlbumArtButton.classList.toggle("is-active", miniVideoVisible);

  const title = String(nextState.music_title || nextState.media_title || "");
  const artistAlbum = miniPlayerArtistAlbum(nextState);
  els.miniTitle.textContent = title;
  els.miniArtistAlbum.textContent = artistAlbum;
  els.miniArtistAlbum.hidden = !artistAlbum;
  els.miniMediaInfo.classList.toggle("mini-media-info--title-only", !artistAlbum);

  setMiniButtonIcon(
    els.miniPlayButton,
    nextState.paused ? "assets/iina/icons/play.png" : "assets/iina/icons/pause.png",
    nextState.paused ? "Play" : "Pause",
  );
  setMiniButtonIcon(
    els.miniVolumeButton,
    nextState.muted ? "assets/iina/icons/mute.png" : "assets/iina/icons/volume.png",
    "Volume",
  );
  setMiniButtonIcon(
    els.miniMuteButton,
    nextState.muted ? "assets/iina/icons/mute.png" : "assets/iina/icons/volume.png",
    nextState.muted ? "Unmute" : "Mute",
  );
  els.miniPlayButton.disabled = !hasMedia;
  els.miniPreviousButton.disabled = !hasMedia || (nextState.playlist || []).length <= 1;
  els.miniNextButton.disabled = !hasMedia || (nextState.playlist || []).length <= 1;
  els.miniVolumeButton.disabled = !hasAudio;
  els.miniMuteButton.disabled = !hasAudio;
  els.miniPlaylistButton.disabled = !hasMedia;

  els.miniPlaySlider.max = String(duration > 0 ? Math.max(1, Math.ceil(duration)) : 1000);
  els.miniPlaySlider.value = String(Math.min(position, Number(els.miniPlaySlider.max)));
  els.miniPlaySlider.disabled = !hasMedia || duration <= 0 || Boolean(nextState.file_loading);
  setRangeProgress(els.miniPlaySlider);
  els.miniVolumeSlider.max = String(Math.max(100, Number(getPreferenceValue("maxVolume")) || 100));
  els.miniVolumeSlider.value = String(nextState.volume);
  els.miniVolumeSlider.disabled = !hasAudio;
  els.miniVolumeLabel.value = String(Math.round(Number(nextState.volume) || 0));
  els.miniVolumeLabel.textContent = els.miniVolumeLabel.value;
  setRangeProgress(els.miniVolumeSlider);
  renderMiniTimeLabels(position, duration);
  renderMiniPlaylist(nextState);
  requestMiniPlayerLayout(aspect);
}

function renderMiniTimeLabels(position, duration) {
  if (!isMiniPlayerWindow) return;
  const remaining = Math.max(0, duration - position);
  const showRemaining = Boolean(getPreferenceValue("showRemainingTime"));
  els.miniLeftTime.textContent = formatTimeWithPrecision(position);
  els.miniRightTime.textContent = showRemaining
    ? `-${formatTimeWithPrecision(remaining)}`
    : formatTimeWithPrecision(duration);
  els.miniRightTime.title = showRemaining ? "Show total duration" : "Show remaining time";
  els.miniRightTime.setAttribute("aria-label", showRemaining ? "Remaining time" : "Total duration");
}

function renderMiniPlaylist(nextState) {
  if (!isMiniPlayerWindow) return;
  const items = nextState.playlist || [];
  const details = playlistCacheDetails(nextState, items);
  els.miniPlaylistList.replaceChildren();
  if (!items.length) {
    const empty = document.createElement("div");
    empty.className = "mini-playlist-empty";
    empty.textContent = "Playlist is empty";
    els.miniPlaylistList.append(empty);
    return;
  }
  items.forEach((item, index) => {
    const detail = details[index];
    const metadata = playlistMetadata(detail, preferences.values, true);
    const row = document.createElement("button");
    row.type = "button";
    row.className = `mini-playlist-row${item.current || item.playing ? " is-current" : ""}`;
    row.title = item.path || item.title || "";
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", String(Boolean(item.current || item.playing)));
    const title = document.createElement("span");
    title.className = "mini-playlist-row-title";
    title.textContent = metadata?.title || item.title || item.path || "";
    const artist = document.createElement("span");
    artist.className = "mini-playlist-row-artist";
    artist.textContent = metadata?.artist || "";
    artist.hidden = !metadata;
    const time = document.createElement("span");
    time.className = "mini-playlist-row-time";
    const duration = Number(detail?.duration_seconds ?? item.duration_seconds);
    time.textContent = duration > 0 ? formatTime(duration) : "";
    const progress = document.createElement("span");
    progress.className = "mini-playlist-row-progress";
    progress.style.width = `${playlistProgressFraction(detail) * 100}%`;
    row.append(title, artist, time, progress);
    row.addEventListener("dblclick", () => void command({ type: "select-playlist-item", index }));
    els.miniPlaylistList.append(row);
  });
}

function requestMiniPlayerLayout(aspect = miniPlayerVideoAspect(state)) {
  if (!isMiniPlayerWindow) return;
  const fingerprint = [
    Math.round(window.innerWidth),
    miniVideoVisible,
    miniPlaylistVisible,
    Number(aspect).toFixed(5),
  ].join(":");
  if (fingerprint === miniLayoutFingerprint) return;
  miniLayoutFingerprint = fingerprint;
  if (miniLayoutInFlight) {
    miniLayoutQueued = true;
    return;
  }
  miniLayoutInFlight = true;
  void invoke("set_mini_player_layout", {
    videoVisible: miniVideoVisible,
    playlistVisible: miniPlaylistVisible,
    videoAspect: aspect,
  })
    .then((layout) => {
      const videoHeight = Math.max(0, Number(layout?.video_height) || 0);
      els.miniPlayerUi.style.setProperty("--mini-video-height", `${videoHeight}px`);
    })
    .catch(() => {})
    .finally(() => {
      miniLayoutInFlight = false;
      if (miniLayoutQueued) {
        miniLayoutQueued = false;
        miniLayoutFingerprint = "";
        requestMiniPlayerLayout();
      }
    });
}

function applyNativeMiniPlayerLayout(layout) {
  if (!isMiniPlayerWindow || !layout) return;
  miniPlaylistVisible = Boolean(layout.playlist_visible);
  const videoHeight = Math.max(0, Number(layout.video_height) || 0);
  els.miniPlayerUi.style.setProperty("--mini-video-height", `${videoHeight}px`);
  els.miniPlaylist.hidden = false;
  els.miniPlaylist.setAttribute("aria-hidden", String(!miniPlaylistVisible));
  els.miniPlaylistButton.classList.toggle("is-active", miniPlaylistVisible);
  miniLayoutFingerprint = "";
}

async function toggleMiniPlaylist(visible = !miniPlaylistVisible) {
  miniPlaylistVisible = Boolean(visible);
  await setPreferenceValue("musicModeShowPlaylist", miniPlaylistVisible);
  miniLayoutFingerprint = "";
  render(state);
}

async function toggleMiniVideo(visible = !miniVideoVisible) {
  miniVideoVisible = Boolean(visible);
  await setPreferenceValue("musicModeShowAlbumArt", miniVideoVisible);
  miniLayoutFingerprint = "";
  render(state);
}

function toggleMiniVolumePopover(event) {
  event?.preventDefault();
  event?.stopPropagation();
  if (els.miniVolumeButton.disabled) return;
  els.miniVolumePopover.hidden = !els.miniVolumePopover.hidden;
}

function scheduleMiniPlayerResizeCheck() {
  if (!isMiniPlayerWindow) return;
  clearTimeout(miniPlaylistResizeTimer);
  miniPlaylistResizeTimer = setTimeout(() => {
    // Native AppKit notifications own the exact live-resize boundary in the packaged app.
    // Keep a browser-only equivalent so static interaction checks exercise the same threshold.
    if (tauriInvoke) return;
    const videoHeight = miniVideoVisible ? Math.round(Math.max(300, window.innerWidth) / miniPlayerVideoAspect(state)) : 0;
    const normalHeight = MINI_PLAYER_CONTROL_HEIGHT + videoHeight;
    miniPlaylistVisible = window.innerHeight >= normalHeight + MINI_PLAYER_AUTO_HIDE_PLAYLIST_HEIGHT;
    applyNativeMiniPlayerLayout({
      playlist_visible: miniPlaylistVisible,
      video_height: videoHeight,
    });
    miniLayoutFingerprint = "";
    requestMiniPlayerLayout();
  }, 180);
}

function renderThemeMaterial() {
  const theme = Number(getPreferenceValue("themeMaterial"));
  const matchesLightSystem = window.matchMedia?.("(prefers-color-scheme: light)")?.matches;
  const light = theme === 2 || (theme === 4 && matchesLightSystem);
  els.app.classList.toggle("theme-material--light", Boolean(light));
  els.app.classList.toggle("theme-material--dark", !light);
  els.app.dataset.themeMaterial = String([0, 2, 4].includes(theme) ? theme : 0);
}

function renderOscPosition() {
  const position = Math.max(0, Math.min(2, Number(getPreferenceValue("oscPosition")) || 0));
  els.player.classList.toggle("player-window--osc-floating", position === 0);
  els.player.classList.toggle("player-window--osc-top", position === 1);
  els.player.classList.toggle("player-window--osc-bottom", position === 2);
  if (position === 0 && !oscDragState) {
    const storedHorizontal = Number(getPreferenceValue("controlBarPositionHorizontal"));
    const storedVertical = Number(getPreferenceValue("controlBarPositionVertical"));
    const horizontal = Math.max(0, Math.min(1, Number.isFinite(storedHorizontal) ? storedHorizontal : 0.5));
    const vertical = Math.max(0, Math.min(1, Number.isFinite(storedVertical) ? storedVertical : 0.1));
    els.osc.style.left = `${horizontal * 100}%`;
    els.osc.style.bottom = `${vertical * 100}%`;
  } else if (position !== 0) {
    els.osc.style.removeProperty("left");
    els.osc.style.removeProperty("bottom");
  }
}

function renderOscToolbar(nextState, { hasMedia, hasAudio, hasVideo }) {
  const configured = normalizeIinaOscToolbarButtons(getPreferenceValue("controlBarToolbarButtons"));
  const buttons = new Map([
    [0, els.settingsButton],
    [1, els.playlistButton],
    [2, els.pipButton],
    [3, els.fullscreenButton],
    [4, els.musicModeButton],
    [5, els.subTrackButton],
    [6, els.screenshotButton],
  ]);
  oscToolbarLayoutFingerprint = reconcileOscToolbarLayout({
    container: els.oscToolbarGroup,
    buttons,
    configured,
    previousFingerprint: oscToolbarLayoutFingerprint,
  });
  els.settingsButton.disabled = !hasMedia;
  els.playlistButton.disabled = !hasMedia;
  els.pipButton.disabled = !hasVideo || nativeVideoRenderer?.pip_available === false;
  els.fullscreenButton.disabled = !hasMedia;
  els.musicModeButton.disabled = !hasAudio && !hasVideo;
  els.subTrackButton.disabled = !hasMedia;
  els.screenshotButton.disabled = !hasVideo;
  setButtonIcon(
    els.musicModeButton,
    "assets/iina/icons/music-mode.png",
    nextState.mode === "mini-player" ? "Exit Music Mode" : "Enter Music Mode",
  );
}

function renderChapterMarkers(nextState, duration) {
  els.chapterMarkers.replaceChildren();
  if (!Boolean(getPreferenceValue("showChapterPos")) || duration <= 0) return;
  for (const chapter of nextState.chapters || []) {
    const seconds = Number(chapter.time_seconds);
    if (!Number.isFinite(seconds) || seconds <= 0 || seconds >= duration) continue;
    const marker = document.createElement("span");
    marker.className = "chapter-marker";
    marker.style.left = `${Math.max(0, Math.min(100, (seconds / duration) * 100))}%`;
    els.chapterMarkers.append(marker);
  }
}

function updateFullscreenClock() {
  els.fullscreenClock.textContent = new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date());
}

function applyFullscreenState(fullscreen, { emitEvents = true } = {}) {
  const nextFullscreen = Boolean(fullscreen);
  const changed = nextFullscreen !== windowFullscreenActive;
  windowFullscreenActive = nextFullscreen;
  renderFullscreenInfo();
  renderOnTopIndicator(state);
  if (changed && emitEvents) {
    emitPluginEvent("iina.window-fs.changed", nextFullscreen);
    if (!hasNativeMpvEventSequence(state)) {
      emitPluginEvent("mpv.fullscreen.changed", nextFullscreen);
    }
  }
}

function applyPlayerWindowStatus(status, { emitEvents = true } = {}) {
  if (!status || typeof status !== "object") return;
  if (typeof status.alwaysOnTop === "boolean") windowAlwaysOnTopActive = status.alwaysOnTop;
  const capacity = Number(status.batteryCapacity);
  nativeBatteryStatus = status.batteryCapacity !== null
    && status.batteryCapacity !== undefined
    && Number.isFinite(capacity)
    ? { capacity: Math.max(0, Math.min(100, Math.round(capacity))), charging: Boolean(status.batteryCharging) }
    : undefined;
  applyFullscreenState(Boolean(status.fullscreen), { emitEvents });
}

function renderFullscreenBattery() {
  const capacity = Number(nativeBatteryStatus?.capacity);
  const available = Number.isFinite(capacity);
  els.fullscreenBattery.hidden = !available;
  els.fullscreenBattery.textContent = available ? `${Math.max(0, Math.min(100, Math.round(capacity)))}%` : "";
}

async function refreshFullscreenBattery() {
  try {
    applyPlayerWindowStatus(await invoke("get_player_window_status"));
  } catch {
    // Fall through to the Web Battery API when no native bridge is available.
  }
  if (nativeBatteryStatus || typeof navigator.getBattery !== "function") {
    renderFullscreenBattery();
    return;
  }
  try {
    const battery = await navigator.getBattery();
    nativeBatteryStatus = {
      capacity: Math.round(battery.level * 100),
      charging: Boolean(battery.charging),
    };
  } catch {
    nativeBatteryStatus = undefined;
  }
  renderFullscreenBattery();
}

function fullscreenTitleForState(nextState) {
  const current = String(nextState?.current_url || "");
  if (current.includes("://")) {
    try {
      return titleFromPath(decodeURIComponent(new URL(current).pathname)) || String(nextState?.media_title || "");
    } catch {
      // Preserve malformed or non-standard URLs literally below.
    }
  }
  return titleFromPath(current || String(nextState?.media_title || ""));
}

function renderFullscreenInfo(nextState = state) {
  const visible = windowFullscreenActive
    && Boolean(getPreferenceValue("enableOSD"))
    && Boolean(getPreferenceValue("displayTimeAndBatteryInFullScreen"));
  els.fullscreenTitle.textContent = fullscreenTitleForState(nextState);
  renderFullscreenBattery();
  const visibilityChanged = visible !== fullscreenInfoVisible;
  fullscreenInfoVisible = visible;
  els.fullscreenInfo.hidden = !visible;
  if (!visibilityChanged) return;
  clearInterval(fullscreenInfoTimer);
  fullscreenInfoTimer = undefined;
  if (!visible) return;
  updateFullscreenClock();
  void refreshFullscreenBattery();
  fullscreenInfoTimer = window.setInterval(() => {
    updateFullscreenClock();
    void refreshFullscreenBattery();
  }, 30_000);
}

function renderOnTopIndicator(nextState = state) {
  const preferenceEnabled = Boolean(getPreferenceValue("alwaysFloatOnTop"));
  if (preferenceEnabled && nextState?.current_url) {
    windowAlwaysOnTopActive = !nextState.paused && !windowFullscreenActive;
  }
  els.onTopButton.hidden = !Boolean(getPreferenceValue("alwaysShowOnTopIcon")) && !windowAlwaysOnTopActive;
  els.onTopButton.classList.toggle("is-active", windowAlwaysOnTopActive);
  const icon = els.onTopButton.querySelector("img");
  if (icon) icon.src = windowAlwaysOnTopActive
    ? "assets/iina/icons/ontop.png"
    : "assets/iina/icons/ontop-off.png";
  const label = windowAlwaysOnTopActive ? "Disable Float on Top" : "Float on Top";
  els.onTopButton.title = tr(label);
  els.onTopButton.setAttribute("aria-label", tr(label));
}

function renderPipOverlay(nextState = state) {
  const visible = Boolean(nextState?.pip_active) && !pipOverlayClosing;
  els.pipOverlay.hidden = !visible;
  els.pipOverlay.setAttribute("aria-hidden", String(!visible));
}

function render(nextState) {
  const isInitial = nextState.mode === "initial" && !isMiniPlayerWindow;
  if (isInitial && !wasInitialWindowVisible) initialSelectionInitialized = false;
  wasInitialWindowVisible = isInitial;
  syncWindowPresentationMode(isInitial ? "initial" : "player");
  els.app.classList.toggle("app-shell--initial", isInitial);
  els.app.classList.toggle("app-shell--player", !isInitial);
  els.app.classList.toggle("app-shell--mini-player", isMiniPlayerWindow);
  els.app.classList.toggle("app-shell--native-video", !isInitial && Boolean(nativeVideoRenderer?.installed));
  els.player.classList.toggle("player-window--osc-hidden", !nextState.osc_visible);
  renderThemeMaterial();
  renderOscPosition();
  els.initial.hidden = !isInitial;
  els.player.hidden = isInitial;
  els.mediaTitle.textContent = nextState.media_title;
  const duration = Number(nextState.duration_seconds) || 0;
  const hasMedia = Boolean(nextState.current_url);
  const hasAudio = hasSelectedTrack(nextState, "audio", "aid");
  const hasVideo = hasSelectedTrack(nextState, "video", "vid");
  els.pipButton.classList.toggle("is-active", Boolean(nextState.pip_active));
  setButtonIcon(
    els.pipButton,
    "assets/iina/icons/pip.png",
    nextState.pip_active ? "Exit Picture in Picture" : "Picture in Picture",
  );
  if (!nextState.pip_active) pipOverlayClosing = false;
  renderPipOverlay(nextState);
  renderOscToolbar(nextState, { hasMedia, hasAudio, hasVideo });
  renderOnTopIndicator(nextState);
  renderFullscreenInfo();
  setButtonIcon(
    els.playButton,
    nextState.paused ? "assets/iina/icons/play.png" : "assets/iina/icons/pause.png",
    nextState.paused ? "Play" : "Pause",
  );
  els.playButton.disabled = !hasMedia;
  renderOscArrowControls(nextState, duration, hasMedia);
  els.volumeSlider.value = String(nextState.volume);
  els.volumeSlider.disabled = !hasAudio;
  els.muteButton.classList.toggle("is-active", nextState.muted);
  setButtonIcon(
    els.muteButton,
    nextState.muted ? "assets/iina/icons/mute.png" : "assets/iina/icons/volume.png",
    nextState.muted ? "Unmute" : "Mute",
  );
  els.muteButton.disabled = !hasAudio;
  setRangeProgress(els.volumeSlider);
  els.playSlider.max = String(duration > 0 ? Math.max(1, Math.ceil(duration)) : 1000);
  els.playSlider.value = String(Math.min(Number(nextState.position_seconds) || 0, Number(els.playSlider.max)));
  els.playSlider.disabled = !hasMedia || duration <= 0 || Boolean(nextState.file_loading);
  setRangeProgress(els.playSlider);
  renderAbLoopKnobs(nextState, duration);
  renderChapterMarkers(nextState, duration);
  const position = Math.max(0, Number(nextState.position_seconds) || 0);
  updateOscTimeSnapshot(nextState, position, duration);
  renderOscTimeLabels(position, duration);
  renderMiniPlayer(nextState, position, duration, hasMedia, hasAudio, hasVideo);
  els.sidebar.hidden = !nextState.sidebar.visible;
  renderMediaInfo(nextState);
  renderSidebar(nextState);
  renderLastPlayback(nextState);
  renderRecent(nextState);
  prepareThumbnails(nextState);
}

function updateOscTimeSnapshot(nextState, position, duration) {
  oscTimeSnapshotPosition = position;
  oscTimeSnapshotDuration = duration;
  oscTimeSnapshotSpeed = Math.max(0, Number(nextState.speed) || 1);
  oscTimeSnapshotPaused = Boolean(nextState.paused);
  oscTimeSnapshotTimestamp = performance.now();
}

function estimatedOscPosition() {
  if (oscTimeSnapshotPaused) return oscTimeSnapshotPosition;
  const elapsed = Math.max(0, performance.now() - oscTimeSnapshotTimestamp) / 1000;
  const estimated = oscTimeSnapshotPosition + elapsed * oscTimeSnapshotSpeed;
  return oscTimeSnapshotDuration > 0 ? Math.min(oscTimeSnapshotDuration, estimated) : estimated;
}

function renderOscTimeLabels(position, duration) {
  const precision = timeDisplayPrecision();
  const showRemainingTime = Boolean(getPreferenceValue("showRemainingTime"));
  els.leftTime.textContent = formatTimeWithPrecision(position, precision);
  els.rightTime.textContent = duration
    ? showRemainingTime
      ? `-${formatTimeWithPrecision(Math.max(0, duration - position), precision)}`
      : formatTimeWithPrecision(duration, precision)
    : "--:--";
  els.rightTime.title = showRemainingTime ? "Show total duration" : "Show remaining time";
  els.rightTime.setAttribute("aria-label", showRemainingTime ? "Remaining time" : "Total duration");
}

function hasSelectedTrack(nextState, kind, property) {
  const selectedProperty = Number(nextState.mpv_properties?.[property]);
  if (Number.isFinite(selectedProperty)) return selectedProperty > 0;
  return (nextState.tracks?.[kind] ?? []).some((track) => track.selected && Number(track.id) > 0);
}

function renderOscArrowControls(nextState, duration, hasMedia) {
  const action = Number(getPreferenceValue("arrowBtnAction"));
  const playlistMode = action === 1;
  const seekMode = action === 2;
  setButtonIcon(
    els.backwardButton,
    playlistMode ? "assets/iina/icons/previous.png" : "assets/iina/icons/speed-backward.png",
    playlistMode ? "Previous Media" : seekMode ? "Rewind 10 Seconds" : "Decrease Speed",
  );
  setButtonIcon(
    els.forwardButton,
    playlistMode ? "assets/iina/icons/next.png" : "assets/iina/icons/speed-forward.png",
    playlistMode ? "Next Media" : seekMode ? "Fast Forward 10 Seconds" : "Increase Speed",
  );
  const arrowsEnabled = hasMedia && (!playlistMode || (nextState.playlist?.length ?? 0) > 1) && (!seekMode || duration > 0);
  els.backwardButton.disabled = !arrowsEnabled;
  els.forwardButton.disabled = !arrowsEnabled;

  if (action !== 0 || nextState.paused) {
    oscArrowSpeedActive = false;
    oscArrowSpeedIndex = OSC_NORMAL_SPEED_INDEX;
  }
  const speed = OSC_SPEED_VALUES[oscArrowSpeedIndex] ?? 1;
  const showLeft = oscArrowSpeedActive && oscArrowSpeedIndex < OSC_NORMAL_SPEED_INDEX;
  const showRight = oscArrowSpeedActive && oscArrowSpeedIndex > OSC_NORMAL_SPEED_INDEX;
  els.leftArrowLabel.hidden = !showLeft;
  els.rightArrowLabel.hidden = !showRight;
  els.leftArrowLabel.textContent = showLeft ? `${speed.toFixed(2)}x` : "";
  els.rightArrowLabel.textContent = showRight ? `${speed.toFixed(0)}x` : "";
}

function renderAbLoopKnobs(nextState, duration) {
  const loop = nextState.ab_loop || {};
  const hasDuration = duration > 0;
  const showA = hasDuration && loop.status !== "cleared" && Number(loop.a_seconds) > 0;
  const showB = hasDuration && loop.status === "b-set" && Number(loop.b_seconds) > 0;
  positionAbLoopKnob(els.abLoopA, loop.a_seconds, duration, showA);
  positionAbLoopKnob(els.abLoopB, loop.b_seconds, duration, showB);
}

function positionAbLoopKnob(knob, seconds, duration, visible) {
  knob.hidden = !visible;
  if (!visible) return;
  const percentage = Math.max(0, Math.min(100, (Number(seconds) / duration) * 100));
  knob.style.left = `${percentage}%`;
}

function installAbLoopKnob(knob, point) {
  let dragging = false;
  let queuedSeconds;
  let commandInFlight = false;

  const flush = async () => {
    if (commandInFlight || queuedSeconds === undefined) return;
    commandInFlight = true;
    while (queuedSeconds !== undefined) {
      const seconds = queuedSeconds;
      queuedSeconds = undefined;
      await command({ type: "set-ab-loop-point", point, seconds });
    }
    commandInFlight = false;
  };

  const update = (event) => {
    const duration = Number(state.duration_seconds) || 0;
    if (!dragging || duration <= 0) return;
    const rect = els.playSliderTrack.getBoundingClientRect();
    if (rect.width <= 0) return;
    const ratio = Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width));
    const seconds = Math.max(0.000001, ratio * duration);
    knob.style.left = `${ratio * 100}%`;
    queuedSeconds = seconds;
    void flush();
  };

  knob.addEventListener("pointerdown", (event) => {
    event.preventDefault();
    event.stopPropagation();
    dragging = true;
    knob.setPointerCapture(event.pointerId);
  });
  knob.addEventListener("pointermove", update);
  knob.addEventListener("pointerup", (event) => {
    update(event);
    dragging = false;
    if (knob.hasPointerCapture(event.pointerId)) knob.releasePointerCapture(event.pointerId);
  });
  knob.addEventListener("pointercancel", () => {
    dragging = false;
  });
}

function prepareThumbnails(nextState) {
  const source = nextState.current_url;
  const duration = Number(nextState.duration_seconds) || 0;
  const enabled = Boolean(getPreferenceValue("enableThumbnailPreview"));
  const hasVideoTrack = Array.from(nextState.tracks?.video || [])
    .some((track) => track.selected && !track.metadata?.albumart);
  if (!enabled || !hasVideoTrack || !source || duration <= 0 || source.startsWith("http://") || source.startsWith("https://")) {
    if (thumbnailSource !== undefined) void invoke("cancel_media_thumbnails").catch(() => {});
    thumbnailSource = undefined;
    thumbnailSet = undefined;
    thumbnailGenerationId = 0;
    hideThumbnailPeek();
    return;
  }
  if (thumbnailSource === source) return;

  thumbnailSource = source;
  thumbnailSet = undefined;
  thumbnailGenerationId = 0;
  const requestId = ++thumbnailRequestId;
  invoke("generate_media_thumbnails", { path: source })
    .then((result) => {
      if (requestId === thumbnailRequestId && thumbnailSource === source && !result.cancelled) {
        thumbnailSet = result;
      }
    })
    .catch(() => {
      if (requestId === thumbnailRequestId) {
        thumbnailSet = { thumbnails: [] };
      }
    });
}

function applyThumbnailProgressEvent(payload) {
  const progress = payload?.progress;
  const generationId = Number(payload?.generation_id) || 0;
  if (!progress || progress.source_path !== thumbnailSource || generationId < thumbnailGenerationId) return;
  const alreadyReady = generationId === thumbnailGenerationId && Boolean(thumbnailSet?.ready);
  thumbnailGenerationId = generationId;
  if (progress.cancelled) return;

  const thumbnails = progress.complete
    ? Array.from(progress.thumbnails || [])
    : mergeThumbnailBatches(thumbnailSet?.thumbnails, progress.thumbnails);
  thumbnailSet = {
    source_path: progress.source_path,
    width: progress.width,
    requested_count: progress.requested_count,
    thumbnails,
    progress: Number(progress.progress) || 0,
    ready: Boolean(progress.complete),
    cache_hit: Boolean(progress.cache_hit),
    cancelled: false,
  };
  if (progress.complete && !alreadyReady) emitPluginEvent("iina.thumbnails-ready");
}

function mergeThumbnailBatches(existing, incoming) {
  const byIndex = new Map(Array.from(existing || []).map((thumbnail) => [Number(thumbnail.index), thumbnail]));
  for (const thumbnail of incoming || []) byIndex.set(Number(thumbnail.index), thumbnail);
  return [...byIndex.values()].sort((left, right) => Number(left.index) - Number(right.index));
}

function showThumbnailPeek(event) {
  const duration = Number(state.duration_seconds) || 0;
  if (!duration) return;

  const rect = els.playSlider.getBoundingClientRect();
  const ratio = Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width));
  const time = ratio * duration;
  const thumbnail = thumbnailSet?.ready ? nearestThumbnail(time) : undefined;
  els.thumbnailTime.textContent = formatTime(time);
  els.thumbnailImage.hidden = !thumbnail;
  if (thumbnail) {
    els.thumbnailImage.src = localFileSrc(thumbnail.path);
  }

  const halfWidth = 60;
  const left = Math.max(halfWidth + 8, Math.min(window.innerWidth - halfWidth - 8, rect.left + ratio * rect.width));
  els.thumbnailPeek.style.left = `${left}px`;
  els.thumbnailPeek.hidden = false;
  positionThumbnailPeek(rect);
}

function hideThumbnailPeek() {
  els.thumbnailPeek.hidden = true;
}

function nearestThumbnail(timeSeconds) {
  const thumbnails = thumbnailSet?.thumbnails ?? [];
  if (!thumbnails.length) return undefined;
  let thumbnail = thumbnails.at(-1);
  for (let index = 0; index < thumbnails.length; index += 1) {
    if (Number(thumbnails[index].time_seconds) >= timeSeconds) {
      thumbnail = thumbnails[index === 0 ? 0 : index - 1];
      break;
    }
  }
  return thumbnail;
}

function positionThumbnailPeek(sliderRect) {
  const peekRect = els.thumbnailPeek.getBoundingClientRect();
  const top = sliderRect.top - peekRect.height >= 22
    ? sliderRect.top - peekRect.height
    : sliderRect.bottom;
  els.thumbnailPeek.style.top = `${Math.round(top)}px`;
}

function localFileSrc(path) {
  if (tauriConvertFileSrc) return tauriConvertFileSrc(path);
  return path?.startsWith("/") ? `file://${path}` : path;
}

function renderMediaInfo(nextState) {
  const info = nextState.media_info;
  if (!info) {
    els.surfaceTitle.textContent = nextState.media_title || "";
    els.surfaceSubtitle.textContent = "";
    els.mediaDiagnostics.hidden = true;
    return;
  }

  els.surfaceTitle.textContent = info.video_summary || info.audio_summary || nextState.media_title || "";
  const metadataParts = [info.format, info.video_summary, info.audio_summary].filter(Boolean);
  els.surfaceSubtitle.textContent =
    info.probe_status === "probed" && metadataParts.length
      ? metadataParts.join(" | ")
      : info.probe_message || "Metadata unavailable";

  els.mediaFormat.textContent = info.format ? `Format: ${info.format}` : "Format: --";
  els.mediaVideo.textContent = info.video_summary ? `Video: ${info.video_summary}` : "Video: --";
  els.mediaAudio.textContent = info.audio_summary ? `Audio: ${info.audio_summary}` : "Audio: --";
  els.mediaDiagnostics.hidden = info.probe_status !== "probed";
}

function renderRecent(nextState) {
  els.recentFiles.replaceChildren();
  const lastPath = nextState.last_playback?.path;
  initialRecentItems = (nextState.recent_documents ?? []).filter((item) => item.path !== lastPath);
  if (!initialSelectionInitialized) {
    initialSelectedRecentIndex = nextState.last_playback ? -1 : initialRecentItems.length ? 0 : -1;
    initialSelectionInitialized = true;
  } else if (initialSelectedRecentIndex >= initialRecentItems.length) {
    initialSelectedRecentIndex = initialRecentItems.length - 1;
  } else if (initialSelectedRecentIndex < 0 && !nextState.last_playback && initialRecentItems.length) {
    initialSelectedRecentIndex = 0;
  }

  initialRecentItems.forEach((item, index) => {
    const row = document.createElement("button");
    row.type = "button";
    row.className = `recent-row${index === initialSelectedRecentIndex ? " selected" : ""}`;
    row.role = "option";
    row.tabIndex = -1;
    row.ariaSelected = String(index === initialSelectedRecentIndex);
    row.dataset.recentIndex = String(index);

    const icon = document.createElement("img");
    icon.className = "recent-row-icon";
    icon.src = recentDocumentIcon(item.path);
    icon.alt = "";
    icon.draggable = false;
    const title = document.createElement("span");
    title.className = "recent-row-title";
    title.textContent = item.title || titleFromPath(item.path);
    row.append(icon, title);
    row.addEventListener("click", async () => {
      initialSelectedRecentIndex = index;
      updateInitialSelectionStyles();
      setPlayerState(await invoke("open_media", { path: item.path }), { force: true, presentOsd: true });
    });
    els.recentFiles.append(row);
  });
  updateInitialSelectionStyles();
}

function renderLastPlayback(nextState) {
  const lastPlayback = nextState.last_playback;
  els.resumeButton.hidden = !lastPlayback;
  els.initial.classList.toggle("initial-window--has-last-playback", Boolean(lastPlayback));
  if (!lastPlayback) {
    els.resumeButton.classList.remove("selected");
    els.lastFileTitle.textContent = "";
    els.lastFilePosition.textContent = "";
    return;
  }
  els.lastFileTitle.textContent = lastPlayback.title || titleFromPath(lastPlayback.path);
  els.lastFilePosition.textContent = formatTime(lastPlayback.position_seconds || 0);
}

function syncWindowPresentationMode(mode) {
  if (isMiniPlayerWindow || lastWindowPresentationMode === mode) return;
  lastWindowPresentationMode = mode;
  void invoke("set_window_presentation_mode", { mode }).catch(() => {});
}

function handleInitialWindowKeyDown(event) {
  if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return false;
  const hasLastPlayback = Boolean(state.last_playback);
  if (event.key === "Enter") {
    event.preventDefault();
    void openInitialSelection();
    return true;
  }
  if (event.key === "ArrowDown") {
    event.preventDefault();
    if (initialRecentItems.length) {
      initialSelectedRecentIndex = Math.min(initialRecentItems.length - 1, initialSelectedRecentIndex + 1);
      updateInitialSelectionStyles();
    }
    return true;
  }
  if (event.key === "ArrowUp") {
    event.preventDefault();
    if (initialSelectedRecentIndex > 0) {
      initialSelectedRecentIndex -= 1;
    } else if (initialSelectedRecentIndex === 0 && hasLastPlayback) {
      initialSelectedRecentIndex = -1;
    }
    updateInitialSelectionStyles();
    return true;
  }
  return false;
}

async function openInitialSelection() {
  const selected = initialSelectedRecentIndex < 0 ? state.last_playback : initialRecentItems[initialSelectedRecentIndex];
  if (!selected?.path) return;
  setPlayerState(await invoke("open_media", { path: selected.path }), { force: true, presentOsd: true });
}

function updateInitialSelectionStyles() {
  const resumeSelected = Boolean(state.last_playback) && initialSelectedRecentIndex < 0;
  els.resumeButton.classList.toggle("selected", resumeSelected);
  els.recentFiles.querySelectorAll(".recent-row").forEach((row) => {
    const selected = Number(row.dataset.recentIndex) === initialSelectedRecentIndex;
    row.classList.toggle("selected", selected);
    row.ariaSelected = String(selected);
  });
}

function recentDocumentIcon(path) {
  const extension = String(path || "").split(/[?#]/, 1)[0].split(".").pop()?.toLowerCase() || "";
  const exact = new Set([
    "3gp", "aac", "asf", "avi", "flac", "flv", "gif", "m4a", "mkv", "mp3", "mp4", "ogg", "rm", "ts", "wav", "webm", "wmv",
  ]);
  let icon = exact.has(extension) ? extension : "other_v";
  if (["m3u", "m3u8", "pls", "cue"].includes(extension)) icon = "list";
  else if (["mov", "qt"].includes(extension)) icon = "qt";
  else if (["m4v", "mpg", "mpeg", "vob", "m2ts", "mts"].includes(extension)) icon = "other_v";
  else if (["aiff", "aif", "alac", "ape", "mka", "opus", "wma"].includes(extension)) icon = "other_a";
  return `assets/iina/doc-icons/doc_${icon}.png`;
}

function filterPreferenceKey(kind = activeFilterKind) {
  return kind === "audio" ? "savedAudioFilters" : "savedVideoFilters";
}

function activeFilters(kind = activeFilterKind) {
  return kind === "audio" ? state.audio_filters || [] : state.video_filters || [];
}

function savedFilters(kind = activeFilterKind) {
  const filters = preferences?.values?.[filterPreferenceKey(kind)];
  if (!Array.isArray(filters)) return [];
  return filters.filter((filter) =>
    filter &&
    typeof filter.name === "string" &&
    typeof filter.filterString === "string" &&
    typeof filter.shortcutKey === "string" &&
    typeof filter.shortcutKeyModifiers === "string"
  );
}

function parseFilterRawForUi(raw) {
  const stringFormat = String(raw || "").trim();
  if (!stringFormat || stringFormat.length > 8192 || stringFormat.includes("\0")) return null;
  const equalsIndex = stringFormat.indexOf("=");
  let head = equalsIndex >= 0 ? stringFormat.slice(0, equalsIndex) : stringFormat;
  let label = null;
  if (head.startsWith("@")) {
    const separator = head.indexOf(":");
    if (separator <= 1 || separator === head.length - 1) return null;
    label = head.slice(1, separator);
    head = head.slice(separator + 1);
  }
  if (!/^[A-Za-z0-9_-]+$/.test(head)) return null;
  return { name: head, label, params: {}, string_format: stringFormat };
}

function normalizedFilterRaw(raw) {
  const filter = parseFilterRawForUi(raw);
  if (!filter) return null;
  const equalsIndex = filter.string_format.indexOf("=");
  if (equalsIndex < 0) return `${filter.label ? `@${filter.label}:` : ""}${filter.name}`;
  const params = filter.string_format.slice(equalsIndex + 1);
  const pairs = params.split(":");
  if (filter.name !== "lavfi" && pairs.length > 1 && pairs.every((pair) => pair.includes("="))) {
    const normalized = pairs
      .map((pair) => {
        const separator = pair.indexOf("=");
        const key = pair.slice(0, separator);
        const value = pair.slice(separator + 1).replace(/^%\d+%/, "");
        return `${key}=${value}`;
      })
      .sort()
      .join(":");
    return `${filter.label ? `@${filter.label}:` : ""}${filter.name}=${normalized}`;
  }
  return filter.string_format;
}

function filterMatchesRaw(active, raw) {
  const activeRaw = active?.string_format || active?.stringFormat || "";
  return normalizedFilterRaw(activeRaw) === normalizedFilterRaw(raw);
}

async function showFilterPanel(kind) {
  const normalizedKind = kind === "audio" ? "audio" : "video";
  if (!isFilterAuxiliaryWindow && tauriInvoke) {
    await invoke("show_filter_window", { kind: normalizedKind });
    return;
  }
  activeFilterKind = isFilterAuxiliaryWindow
    ? (auxiliaryWindowRole === "audio-filter" ? "audio" : "video")
    : normalizedKind;
  selectedCurrentFilterIndex = -1;
  selectedSavedFilterIndex = -1;
  filterEditorContext = null;
  selectedFilterPresetId = null;
  preferences = await invoke("get_preferences");
  els.filterWindowTitle.textContent = tr(activeFilterKind === "audio" ? "Audio Filters" : "Video Filters");
  els.filterPresetSheet.hidden = true;
  els.filterEditorSheet.hidden = true;
  els.filterModal.hidden = false;
  renderFilterPanel();
}

function closeFilterPanel() {
  closeFilterPresetSheet();
  closeFilterEditor();
  els.filterModal.hidden = true;
  selectedCurrentFilterIndex = -1;
  selectedSavedFilterIndex = -1;
  if (isFilterAuxiliaryWindow) {
    void invoke("hide_auxiliary_window").catch((error) => {
      console.error("Unable to hide the filter window", error);
    });
  }
}

function renderFilterPanel() {
  renderCurrentFilterList();
  renderSavedFilterList();
}

function renderCurrentFilterList() {
  const filters = activeFilters();
  if (selectedCurrentFilterIndex >= filters.length) selectedCurrentFilterIndex = -1;
  els.currentFilterList.replaceChildren();
  if (!filters.length) {
    const empty = document.createElement("div");
    empty.className = "filter-empty";
    empty.textContent = "No filters";
    els.currentFilterList.append(empty);
  }
  const definitions = savedFilters();
  filters.forEach((filter, index) => {
    const row = document.createElement("div");
    row.className = `filter-current-row${index === selectedCurrentFilterIndex ? " selected" : ""}`;
    row.role = "option";
    row.tabIndex = 0;
    row.ariaSelected = String(index === selectedCurrentFilterIndex);
    row.addEventListener("click", () => {
      selectedCurrentFilterIndex = index;
      renderCurrentFilterList();
    });
    row.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        selectedCurrentFilterIndex = index;
        renderCurrentFilterList();
      }
    });

    const number = document.createElement("span");
    number.className = "filter-current-index";
    number.textContent = String(index);
    const string = document.createElement("span");
    string.className = "filter-current-string";
    string.textContent = filter.string_format || "";
    string.title = filter.string_format || "";
    const save = document.createElement("button");
    save.type = "button";
    save.className = "filter-row-save";
    save.textContent = trKey("FilterWindowController", "02L-0u-zae.title", "Save");
    save.disabled = definitions.some((saved) => filterMatchesRaw(filter, saved.filterString));
    save.addEventListener("click", (event) => {
      event.stopPropagation();
      showSaveFilterEditor(index);
    });
    row.append(number, string, save);
    els.currentFilterList.append(row);
  });
  els.filterRemoveCurrentButton.disabled = selectedCurrentFilterIndex < 0;
}

function renderSavedFilterList() {
  const definitions = savedFilters();
  if (selectedSavedFilterIndex >= definitions.length) selectedSavedFilterIndex = -1;
  els.savedFilterList.replaceChildren();
  if (!definitions.length) {
    const empty = document.createElement("div");
    empty.className = "filter-empty";
    empty.textContent = "No saved filters";
    els.savedFilterList.append(empty);
    return;
  }
  const current = activeFilters();
  definitions.forEach((definition, index) => {
    const row = document.createElement("div");
    row.className = `saved-filter-row${index === selectedSavedFilterIndex ? " selected" : ""}`;
    row.role = "option";
    row.ariaSelected = String(index === selectedSavedFilterIndex);
    row.addEventListener("click", () => {
      selectedSavedFilterIndex = index;
      renderSavedFilterList();
    });

    const toggle = document.createElement("input");
    toggle.type = "checkbox";
    toggle.className = "saved-filter-toggle";
    toggle.checked = current.some((filter) => filterMatchesRaw(filter, definition.filterString));
    toggle.title = tr(toggle.checked ? "Disable Filter" : "Enable Filter");
    toggle.addEventListener("click", (event) => event.stopPropagation());
    toggle.addEventListener("change", () => void toggleSavedFilter(definition));

    const copy = document.createElement("div");
    copy.className = "saved-filter-copy";
    const name = document.createElement("div");
    name.className = "saved-filter-name";
    name.textContent = definition.name;
    const details = document.createElement("div");
    details.className = "saved-filter-details";
    const shortcut = formatSavedFilterShortcut(definition.shortcutKey, definition.shortcutKeyModifiers);
    details.textContent = shortcut ? `${shortcut} | ${definition.filterString}` : definition.filterString;
    details.title = definition.filterString;
    copy.append(name, details);

    const actions = document.createElement("div");
    actions.className = "saved-filter-actions";
    const edit = document.createElement("button");
    edit.type = "button";
    edit.className = "saved-filter-action";
    edit.textContent = tr("Edit");
    edit.addEventListener("click", (event) => {
      event.stopPropagation();
      showEditSavedFilterEditor(index);
    });
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "saved-filter-action";
    remove.textContent = tr("Delete");
    remove.addEventListener("click", (event) => {
      event.stopPropagation();
      void deleteSavedFilter(index);
    });
    actions.append(edit, remove);
    row.append(toggle, copy, actions);
    els.savedFilterList.append(row);
  });
}

async function toggleSavedFilter(definition) {
  await command({
    type: "toggle-saved-filter",
    kind: activeFilterKind,
    name: definition.name,
    filter: definition.filterString,
  });
}

function filterPresets(kind = activeFilterKind) {
  return FILTER_PRESETS[kind === "audio" ? "audio" : "video"];
}

function showFilterPresetSheet(initialPresetId = null) {
  closeFilterEditor();
  const presets = filterPresets();
  selectedFilterPresetId = presets.some((preset) => preset.id === initialPresetId) ? initialPresetId : null;
  els.filterPresetSheet.hidden = false;
  renderFilterPresetSheet();
}

function closeFilterPresetSheet() {
  els.filterPresetSheet.hidden = true;
  selectedFilterPresetId = null;
  els.filterPresetList.replaceChildren();
  els.filterPresetSettings.replaceChildren();
}

function renderFilterPresetSheet() {
  els.filterPresetList.replaceChildren();
  for (const preset of filterPresets()) {
    const row = document.createElement("div");
    row.className = `filter-preset-row${preset.id === selectedFilterPresetId ? " selected" : ""}`;
    row.role = "option";
    row.tabIndex = 0;
    row.ariaSelected = String(preset.id === selectedFilterPresetId);
    row.dataset.presetId = preset.id;
    row.textContent = trKey("FilterPresets", preset.id, preset.label);
    const select = () => {
      selectedFilterPresetId = preset.id;
      renderFilterPresetSheet();
      requestAnimationFrame(() => els.filterPresetSettings.querySelector("input, select")?.focus());
    };
    row.addEventListener("click", select);
    row.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        select();
      }
    });
    els.filterPresetList.append(row);
  }
  renderFilterPresetSettings();
}

function renderFilterPresetSettings() {
  els.filterPresetSettings.replaceChildren();
  const preset = filterPresets().find((candidate) => candidate.id === selectedFilterPresetId);
  els.filterPresetAddButton.disabled = false;
  if (!preset) return;
  for (const param of preset.params) {
    const label = document.createElement("label");
    label.className = "filter-preset-field";
    const caption = document.createElement("span");
    caption.textContent = trKey("FilterPresets", `${preset.id}.${param.name}`, param.label);
    const input = createFilterPresetInput(preset, param);
    label.append(caption, input);
    els.filterPresetSettings.append(label);
  }
  updateFilterPresetAddButton();
}

function createFilterPresetInput(preset, param) {
  let input;
  if (param.type === "choose") {
    input = document.createElement("select");
    for (const choice of param.choices) {
      const option = document.createElement("option");
      option.value = choice;
      option.textContent = tr(choice);
      input.append(option);
    }
  } else {
    input = document.createElement("input");
    input.type = param.type === "int" || param.type === "float" ? "range" : "text";
    input.autocomplete = "off";
    if (input.type === "range") {
      input.min = String(param.min);
      input.max = String(param.max);
      input.step = String(param.type === "int" ? param.step : 0.01);
    }
  }
  input.value = String(param.defaultValue);
  input.dataset.filterParam = param.name;
  input.id = `filter-preset-${preset.id}-${param.name}`;
  input.addEventListener("input", updateFilterPresetAddButton);
  return input;
}

function selectedFilterPresetValues() {
  return Object.fromEntries(
    [...els.filterPresetSettings.querySelectorAll("[data-filter-param]")].map((input) => [input.dataset.filterParam, input.value])
  );
}

function updateFilterPresetAddButton() {
  const preset = filterPresets().find((candidate) => candidate.id === selectedFilterPresetId);
  if (!preset || !preset.id.startsWith("custom_")) {
    els.filterPresetAddButton.disabled = false;
    return;
  }
  const name = selectedFilterPresetValues().name?.trim() || "";
  els.filterPresetAddButton.disabled = !name || (preset.id === "custom_mpv" && !/^[A-Za-z0-9_@:-]+$/.test(name));
}

function swiftFloatDescription(value) {
  const number = Number(value);
  if (!Number.isFinite(number)) return "0.0";
  return Number.isInteger(number) ? number.toFixed(1) : String(number);
}

function buildFilterPresetString(preset, values) {
  switch (preset.id) {
    case "crop":
      return `crop=${["w", "h", "x", "y"].map((name) => values[name] || "").join(":")}`;
    case "expand":
      return `expand=${["w", "h", "x", "y", "aspect", "round"].map((name) => values[name] || "").join(":")}`;
    case "sharpen": {
      const msize = String(Number.parseInt(values.msize, 10) || 5);
      const amount = swiftFloatDescription(values.amount);
      return `lavfi=[unsharp=${msize}:${msize}:${amount}:${msize}:${msize}:${amount}]`;
    }
    case "blur": {
      const msize = String(Number.parseInt(values.msize, 10) || 5);
      const amountValue = Number(values.amount);
      const amount = amountValue === 0 ? "-0.0" : swiftFloatDescription(-amountValue);
      return `lavfi=[unsharp=${msize}:${msize}:${amount}:${msize}:${msize}:${amount}]`;
    }
    case "delogo":
      return `lavfi=[delogo=${["x", "y", "w", "h"].map((name) => `${name}=${values[name] || ""}`).join(":")}]`;
    case "negative":
      return "lavfi=[lutrgb=r=negval:g=negval:b=negval]";
    case "vflip":
      return "vflip";
    case "hflip":
      return "hflip";
    case "lut3d":
      return `lavfi=[lut3d=file=${values.file || ""}:interp=${values.interp || "nearest"}]`;
    case "custom_mpv":
      return `${values.name?.trim() || ""}${values.string ? `=${values.string}` : ""}`;
    case "custom_ffmpeg":
      return `lavfi=[${values.name?.trim() || ""}=${values.string || ""}]`;
    default:
      return "";
  }
}

async function submitFilterPreset(event) {
  event.preventDefault();
  const preset = filterPresets().find((candidate) => candidate.id === selectedFilterPresetId);
  if (!preset) return;
  const filter = buildFilterPresetString(preset, selectedFilterPresetValues());
  if (!parseFilterRawForUi(filter)) return;
  await command({ type: "add-filter", kind: activeFilterKind, filter });
  closeFilterPresetSheet();
}

function showSaveFilterEditor(index) {
  const filter = activeFilters()[index];
  if (!filter) return;
  filterEditorContext = { mode: "save", index };
  filterEditorShortcut = { key: "", modifiers: "" };
  els.filterEditorTitle.textContent = tr("Save Filter");
  els.filterEditorDescription.textContent = tr("By saving a filter, you can enable or disable it conveniently and even assign a shortcut key to it.");
  els.filterEditorName.value = "";
  els.filterEditorString.value = filter.string_format || "";
  els.filterEditorName.closest("label").hidden = false;
  els.filterEditorStringRow.hidden = true;
  els.filterEditorShortcut.closest("label").hidden = false;
  els.filterEditorShortcut.value = "";
  els.filterEditorSubmitButton.textContent = trKey("FilterWindowController", "w2g-wR-hPu.title", "Add");
  openFilterEditor(els.filterEditorName);
}

function showEditSavedFilterEditor(index) {
  const definition = savedFilters()[index];
  if (!definition) return;
  filterEditorContext = { mode: "edit", index };
  filterEditorShortcut = {
    key: definition.shortcutKey || "",
    modifiers: definition.shortcutKeyModifiers || "",
  };
  els.filterEditorTitle.textContent = tr("Edit Filter");
  els.filterEditorDescription.textContent = "Edit the saved filter name, filter string, or shortcut key.";
  els.filterEditorName.value = definition.name;
  els.filterEditorString.value = definition.filterString;
  els.filterEditorName.closest("label").hidden = false;
  els.filterEditorStringRow.hidden = false;
  els.filterEditorShortcut.closest("label").hidden = false;
  els.filterEditorShortcut.value = formatSavedFilterShortcut(filterEditorShortcut.key, filterEditorShortcut.modifiers);
  els.filterEditorSubmitButton.textContent = trKey("Localizable", "general.ok", "OK");
  openFilterEditor(els.filterEditorName);
}

function openFilterEditor(focusTarget) {
  closeFilterPresetSheet();
  els.filterEditorError.hidden = true;
  els.filterEditorError.textContent = "";
  els.filterEditorSheet.hidden = false;
  requestAnimationFrame(() => focusTarget.focus());
}

function closeFilterEditor() {
  els.filterEditorSheet.hidden = true;
  filterEditorContext = null;
  filterEditorShortcut = { key: "", modifiers: "" };
  els.filterEditorError.hidden = true;
  els.filterEditorError.textContent = "";
}

function recordFilterShortcut(event) {
  event.preventDefault();
  event.stopPropagation();
  if (isModifierOnlyKeyboardEvent(event)) return;
  const token = mpvKeyTokenFromKeyboardEvent(event);
  if (!token) return;
  const key = [...String(event.key || "")].length === 1 && event.key !== " "
    ? event.key.toLocaleLowerCase()
    : token;
  const modifiers = `${event.ctrlKey ? "c" : ""}${event.altKey ? "o" : ""}${event.shiftKey ? "s" : ""}${event.metaKey ? "m" : ""}`;
  filterEditorShortcut = { key, modifiers };
  els.filterEditorShortcut.value = macOSReadableKeyboardEvent(event);
}

function formatSavedFilterShortcut(key, modifiers) {
  if (!key) return "";
  return macOSReadableSavedFilterShortcut(key, modifiers);
}

async function submitFilterEditor(event) {
  event.preventDefault();
  if (!filterEditorContext) return;
  const filterString = els.filterEditorString.value.trim();
  if (!parseFilterRawForUi(filterString)) {
    showFilterEditorError("The filter string is invalid.");
    return;
  }
  try {
    const name = els.filterEditorName.value.trim();
    if (!name || name.includes("\0")) {
      showFilterEditorError("Please enter a name.");
      return;
    }
    const definitions = savedFilters().map((filter) => ({ ...filter }));
    const definition = {
      name,
      filterString,
      shortcutKey: filterEditorShortcut.key,
      shortcutKeyModifiers: filterEditorShortcut.modifiers,
    };
    if (filterEditorContext.mode === "edit") definitions[filterEditorContext.index] = definition;
    else definitions.push(definition);
    await persistSavedFilters(definitions);
    closeFilterEditor();
    renderFilterPanel();
  } catch (error) {
    showFilterEditorError(String(error));
  }
}

function showFilterEditorError(message) {
  els.filterEditorError.textContent = tr(message);
  els.filterEditorError.hidden = false;
}

async function removeSelectedCurrentFilter() {
  if (selectedCurrentFilterIndex < 0) return;
  const index = selectedCurrentFilterIndex;
  selectedCurrentFilterIndex = -1;
  await command({ type: "remove-filter", kind: activeFilterKind, index });
}

async function deleteSavedFilter(index) {
  const definitions = savedFilters().map((filter) => ({ ...filter }));
  if (index < 0 || index >= definitions.length) return;
  definitions.splice(index, 1);
  selectedSavedFilterIndex = -1;
  await persistSavedFilters(definitions);
  renderFilterPanel();
}

async function persistSavedFilters(definitions) {
  preferences = await invoke("set_preference", {
    change: { key: filterPreferenceKey(), value: definitions },
  });
  nativeMenuStateFingerprint = "";
  refreshNativePlayerMenu(state, true);
}

async function showPreferencesPanel({
  selectedPluginIdentifier = null,
  drainPendingPluginInstalls = false,
} = {}) {
  if (!isPreferencesAuxiliaryWindow && tauriInvoke) {
    await invoke("show_preferences_window", {
      pane: activePreferencePane,
      selectedPluginIdentifier,
      drainPendingPluginInstalls,
    });
    return;
  }
  await flushKeyBindingProfileSaves();
  preferences = await invoke("get_preferences");
  els.preferencesModal.hidden = false;
  renderPreferences();
  await refreshKeyBindingProfiles({ loadCurrent: true });
  requestAnimationFrame(() => {
    els.preferencesSearch.focus({ preventScroll: true });
    if (preferenceSearchQuery) renderPreferenceSearchCompletions();
  });
}

function closePreferencesPanel() {
  void flushKeyBindingProfileSaves();
  dismissPreferenceSheet(null);
  els.preferencesModal.hidden = true;
  preferenceSearchQuery = "";
  preferenceSearchCompletionIndex = -1;
  els.preferencesSearch.value = "";
  dismissPreferenceSearchCompletions({ clear: true });
  preferenceSearchHighlightGeneration += 1;
  window.clearTimeout(preferenceSearchHighlightTimer);
  if (isPreferencesAuxiliaryWindow) {
    void invoke("hide_auxiliary_window").catch((error) => {
      console.error("Unable to hide the preferences window", error);
    });
  }
}

async function showPluginGithubPanel() {
  els.pluginGithubSource.value = "";
  els.pluginGithubError.hidden = true;
  els.pluginGithubError.textContent = "";
  els.pluginGithubSource.classList.remove("is-invalid");
  els.pluginGithubDefaultList.replaceChildren();
  setPluginGithubBusy(false);
  updatePluginGithubInstallEnablement();
  els.pluginGithubModal.hidden = false;
  try {
    renderPluginGithubDefaultRepositories(await invoke("get_plugins"));
  } catch (error) {
    renderPluginGithubDefaultRepositories([]);
    els.pluginGithubError.textContent = String(error?.message || error || "Unable to load plugins");
    els.pluginGithubError.hidden = false;
  }
  requestAnimationFrame(() => els.pluginGithubSource.focus());
}

function closePluginGithubPanel() {
  if (pluginGithubBusy) return;
  els.pluginGithubModal.hidden = true;
}

function setPluginGithubBusy(busy) {
  pluginGithubBusy = Boolean(busy);
  els.pluginGithubSource.disabled = pluginGithubBusy;
  els.pluginGithubCancelButton.disabled = pluginGithubBusy;
  els.pluginGithubSpinner.hidden = !pluginGithubBusy;
  for (const row of els.pluginGithubDefaultList.querySelectorAll(".plugin-github-default-row")) {
    row.disabled = pluginGithubBusy || row.dataset.installed === "true";
  }
  updatePluginGithubInstallEnablement();
}

function updatePluginGithubInstallEnablement() {
  const source = els.pluginGithubSource.value.trim();
  els.pluginGithubInstallButton.disabled = pluginGithubBusy || !source;
  for (const row of els.pluginGithubDefaultList.querySelectorAll(".plugin-github-default-row")) {
    row.classList.toggle("is-selected", !row.disabled && row.dataset.repository === source);
    row.setAttribute("aria-selected", String(!row.disabled && row.dataset.repository === source));
  }
}

function renderPluginGithubDefaultRepositories(installedPlugins) {
  els.pluginGithubDefaultList.replaceChildren();
  for (const repository of defaultPluginRepositoryRows(installedPlugins)) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "plugin-github-default-row";
    row.dataset.repository = repository.repository;
    row.dataset.installed = String(repository.installed);
    row.disabled = pluginGithubBusy || repository.installed;
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", "false");
    const source = document.createElement("span");
    source.className = "plugin-github-default-repository";
    source.textContent = repository.repository;
    const state = document.createElement("span");
    state.className = "plugin-github-default-state";
    state.textContent = tr(repository.installed ? "Installed" : "Not Installed");
    row.append(source, state);
    row.addEventListener("click", () => {
      els.pluginGithubSource.value = repository.repository;
      els.pluginGithubSource.classList.remove("is-invalid");
      els.pluginGithubError.hidden = true;
      updatePluginGithubInstallEnablement();
      els.pluginGithubSource.focus();
    });
    els.pluginGithubDefaultList.append(row);
  }
}

function renderPluginPreferencesPage(container, plugin, html) {
  if (typeof html !== "string") {
    renderPluginPageEmpty(container, "This plugin has no preferences page.");
    return;
  }
  const page = document.createElement("section");
  page.className = "plugin-preferences-page";
  page.innerHTML = html;
  bindPluginPreferencePage(page, plugin);
  container.replaceChildren(page);
}

function renderPluginHelpPage(container, html, url) {
  if (typeof html === "string") {
    const page = document.createElement("section");
    page.className = "plugin-help-page";
    page.innerHTML = html;
    container.append(page);
    return;
  }
  if (typeof url === "string" && /^https?:\/\//i.test(url)) {
    const frame = document.createElement("iframe");
    frame.src = url;
    frame.title = "Plugin Help";
    frame.referrerPolicy = "no-referrer";
    container.append(frame);
    return;
  }
  const empty = document.createElement("div");
  empty.className = "plugin-page-empty";
  empty.textContent = tr("This plugin has no help page.");
  container.append(empty);
}

function renderPluginPageEmpty(container, message) {
  const empty = document.createElement("div");
  empty.className = "plugin-page-empty";
  empty.textContent = tr(message);
  container.replaceChildren(empty);
}

function bindPluginPreferencePage(page, plugin) {
  const values = {
    ...(plugin.preference_defaults || {}),
    ...readPluginPreferenceValues(plugin.identifier),
  };
  const inputs = Array.from(page.querySelectorAll("input[data-pref-key], input[type=radio][name]"));
  const radios = new Map();
  for (const input of inputs) {
    const key = input.dataset.prefKey || (input.type === "radio" ? input.name : "");
    if (!key) continue;
    if (input.type === "radio") {
      const group = radios.get(key) || [];
      group.push(input);
      radios.set(key, group);
      continue;
    }
    applyPluginPreferenceInput(input, values[key]);
    input.addEventListener("change", () => writePluginPagePreference(plugin, key, pluginPreferenceInputValue(input)));
  }
  for (const [key, group] of radios) {
    for (const input of group) {
      input.checked = String(values[key] ?? "") === input.value;
      input.addEventListener("change", () => {
        if (input.checked) writePluginPagePreference(plugin, key, input.value);
      });
    }
  }
}

function applyPluginPreferenceInput(input, value) {
  if (value === undefined || value === null) return;
  if (input.type === "checkbox") {
    input.checked = Boolean(value);
  } else {
    input.value = String(value);
  }
}

function pluginPreferenceInputValue(input) {
  if (input.type === "checkbox") return Boolean(input.checked);
  if (input.dataset.type === "int") return Number.parseInt(input.value, 10) || 0;
  if (input.dataset.type === "float") return Number.parseFloat(input.value) || 0;
  return input.value;
}

function writePluginPagePreference(plugin, key, value) {
  const values = {
    ...(plugin.preference_defaults || {}),
    ...readPluginPreferenceValues(plugin.identifier),
    [key]: value,
  };
  writePluginPreferenceValues(plugin.identifier, values);
  const runtime = pluginRuntimes.get(plugin.identifier);
  if (runtime) runtime.preferenceValues[key] = value;
}

function pluginPermissionPresentation(permission, plugin) {
  const id = String(permission?.id || "").trim();
  if (id === "network-request") {
    const domains = Array.from(plugin?.allowed_domains || []).map(String).filter(Boolean);
    return {
      title: tr("Network Request"),
      description: `${tr("This plugin can make requests from or upload data to these websites:")} ${domains.length ? domains.join(", ") : tr("Any site")}`,
    };
  }
  if (id === "show-osd") {
    return {
      title: tr("Show OSD"),
      description: tr("This plugin can show OSD (On Screen Display) messages."),
    };
  }
  if (id === "show-alert") {
    return {
      title: tr("Show Alerts"),
      description: tr("This plugin can show pop-up alerts."),
    };
  }
  if (id === "video-overlay") {
    return {
      title: tr("Add overlays on videos"),
      description: tr("This plugin can render additional content on the top of the video."),
    };
  }
  if (id === "file-system") {
    return {
      title: tr("Access the file system"),
      description: tr("This plugin can read or write files on your disk, which may include sensitive information. It can also execute other programs or applications that can harm your computer. So be sure to verify the source."),
    };
  }
  return {
    title: id || tr("Unknown permission"),
    description: tr("This plugin requests an additional capability."),
  };
}

function showPluginPermissionConfirmation(confirmation) {
  const plugin = confirmation?.plugin || {};
  const permissions = Array.from(confirmation?.permissions || []);
  const sheet = document.createElement("form");
  sheet.className = "preference-sheet preference-plugin-permission-sheet";
  const heading = document.createElement("h2");
  heading.className = "preference-alert-title";
  heading.textContent = trKey("Localizable", "alert.title_warning", "Warning");
  const description = document.createElement("p");
  description.className = "preference-alert-message";
  description.textContent = confirmation?.only_added
    ? tr("This update requires additional permissions. Please review them before proceeding.")
    : tr("This plugin requires the following permissions. Please review them before proceeding.");
  const identity = document.createElement("p");
  identity.className = "plugin-permission-identity";
  identity.textContent = [plugin.name || plugin.identifier, plugin.version ? `v${plugin.version}` : ""]
    .filter(Boolean)
    .join(" ");
  const list = document.createElement("div");
  list.className = "plugin-permission-list";
  if (!permissions.length) {
    const empty = document.createElement("p");
    empty.className = "plugin-permission-empty";
    empty.textContent = tr("No permissions requested");
    list.append(empty);
  } else {
    for (const permission of permissions) {
      const presentation = pluginPermissionPresentation(permission, plugin);
      const row = document.createElement("article");
      row.className = permission?.dangerous
        ? "plugin-permission-row dangerous"
        : "plugin-permission-row";
      const title = document.createElement("strong");
      title.textContent = presentation.title;
      const detail = document.createElement("span");
      detail.textContent = presentation.description;
      row.append(title, detail);
      list.append(row);
    }
  }
  const actions = document.createElement("div");
  actions.className = "preference-sheet-actions";
  const cancel = sheetButton(trKey("Localizable", "general.cancel", "Cancel"));
  const confirm = sheetButton(tr("Install"), true);
  cancel.addEventListener("click", () => dismissPreferenceSheet(false));
  actions.append(cancel, confirm);
  sheet.append(heading, description, identity, list, actions);
  sheet.addEventListener("submit", (event) => {
    event.preventDefault();
    dismissPreferenceSheet(true);
  });
  return presentPreferenceSheet(sheet);
}

async function resolvePluginInstallResult(result) {
  if (!result) return null;
  if (result.status === "installed") return result.record || null;
  if (result.status === "permission-confirmation" && result.confirmation?.token) {
    const confirmation = result.confirmation;
    let confirmed = false;
    try {
      confirmed = await showPluginPermissionConfirmation(confirmation);
    } catch (error) {
      await invoke("cancel_plugin_permissions", { token: confirmation.token }).catch(() => {});
      throw error;
    }
    if (!confirmed) {
      await invoke("cancel_plugin_permissions", { token: confirmation.token });
      return null;
    }
    const nextResult = await invoke("confirm_plugin_permissions", { token: confirmation.token });
    return resolvePluginInstallResult(nextResult);
  }
  if (result.status !== "reinstall-confirmation" || !result.confirmation?.token) {
    throw new Error("Plugin installation returned an invalid confirmation");
  }
  const confirmation = result.confirmation;
  const plugin = confirmation.plugin || {};
  let confirmed = false;
  try {
    confirmed = await showUtilityConfirmation(
      trFormat("Plugin %@ already exists", plugin.name || plugin.identifier || ""),
      tr("Do you want to reinstall the plugin?"),
    );
  } catch (error) {
    await invoke("cancel_plugin_reinstall", { token: confirmation.token }).catch(() => {});
    throw error;
  }
  if (!confirmed) {
    await invoke("cancel_plugin_reinstall", { token: confirmation.token });
    return null;
  }
  return invoke("confirm_plugin_reinstall", { token: confirmation.token });
}

async function submitPluginGithubPanel() {
  if (pluginGithubBusy) return;
  const source = els.pluginGithubSource.value.trim();
  if (!source) {
    els.pluginGithubError.textContent = "Enter a GitHub owner/repository source.";
    els.pluginGithubError.hidden = false;
    els.pluginGithubSource.classList.add("is-invalid");
    els.pluginGithubSource.focus();
    return;
  }
  els.pluginGithubError.hidden = true;
  els.pluginGithubSource.classList.remove("is-invalid");
  setPluginGithubBusy(true);
  try {
    const result = await invoke("install_plugin_from_github", { source });
    els.pluginGithubModal.hidden = true;
    const plugin = await resolvePluginInstallResult(result);
    if (!plugin) return;
    selectedPluginPreferenceId = plugin.identifier;
    activePluginPreferenceTab = "permissions";
    await refreshPlayerPluginRuntimes();
    renderPreferences();
    showOsd(`Plugin Installed: ${plugin.name || plugin.identifier}`);
  } catch (error) {
    els.pluginGithubModal.hidden = false;
    els.pluginGithubError.textContent = String(error?.message || error || "Plugin installation failed");
    els.pluginGithubError.hidden = false;
    els.pluginGithubSource.classList.add("is-invalid");
  } finally {
    setPluginGithubBusy(false);
  }
}

function trReference(value, context) {
  if (context?.table && context?.key) return trKey(context.table, context.key, value);
  return tr(value);
}

function preferenceEntryContext(entry) {
  if (!entry?.label) return entry?.section?.l10n;
  return entry?.l10n || entry?.item?.l10n || entry?.control?.l10n;
}

function renderPreferences() {
  renderPreferenceTabs();
  disposePluginPreferencesPane();
  els.preferencesContent.replaceChildren();
  const panes = [PREFERENCE_PANES.find((pane) => pane.id === activePreferencePane)];
  const visiblePanes = panes.filter(Boolean);
  if (!visiblePanes.length) {
    const empty = document.createElement("div");
    empty.className = "preferences-empty";
    empty.textContent = "No Results";
    els.preferencesContent.append(empty);
    return;
  }

  for (const pane of visiblePanes) {
    const paneEl = document.createElement("section");
    paneEl.className = "pref-pane";
    paneEl.dataset.pane = pane.id;
    if (pane.id === "plugins") {
      const pluginControl = pane.sections
        .flatMap((section) => section.controls)
        .find((control) => control.type === "plugins");
      if (pluginControl) paneEl.append(renderPluginManager(pluginControl));
      els.preferencesContent.append(paneEl);
      continue;
    }
    for (const section of pane.sections) {
      if (!section.controls.length) continue;
      paneEl.append(renderPreferenceSection(section));
    }

    els.preferencesContent.append(paneEl);
  }
}

function renderPreferenceTabs() {
  els.preferencesTabs.replaceChildren();
  for (const pane of PREFERENCE_PANES) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = pane.id === activePreferencePane ? "preferences-tab active" : "preferences-tab";
    button.textContent = trReference(pane.title, pane.l10n);
    button.setAttribute("role", "tab");
    button.setAttribute("aria-selected", pane.id === activePreferencePane ? "true" : "false");
    button.addEventListener("click", () => {
      activePreferencePane = pane.id;
      preferenceSearchQuery = "";
      preferenceSearchCompletionIndex = -1;
      els.preferencesSearch.value = "";
      dismissPreferenceSearchCompletions({ clear: true });
      renderPreferences();
      els.preferencesContent.scrollTop = 0;
    });
    els.preferencesTabs.append(button);
  }
}

function preferenceSearchEntries() {
  return buildPreferenceSearchEntries(PREFERENCE_PANES);
}

function localizedPreferenceSearchCandidates(entry) {
  return [
    trReference(entry.pane.title, entry.pane.l10n),
    trReference(entry.section.title, entry.section.l10n),
    entry.label ? trReference(entry.label, preferenceEntryContext(entry)) : null,
  ];
}

function currentPreferenceSearchCompletions() {
  if (!preferenceSearchQuery) return [];
  return filterPreferenceSearchEntries(
    preferenceSearchEntries(),
    preferenceSearchQuery,
    localizedPreferenceSearchCandidates,
  );
}

function dismissPreferenceSearchCompletions({ clear = false } = {}) {
  els.preferencesSearchCompletions.hidden = true;
  els.preferencesSearch.setAttribute("aria-expanded", "false");
  els.preferencesSearch.removeAttribute("aria-activedescendant");
  if (clear) els.preferencesSearchCompletions.replaceChildren();
}

function renderPreferenceSearchCompletions() {
  const results = currentPreferenceSearchCompletions();
  els.preferencesSearchCompletions.replaceChildren();
  if (!preferenceSearchQuery) {
    dismissPreferenceSearchCompletions();
    return;
  }
  els.preferencesSearchCompletions.hidden = false;
  els.preferencesSearch.setAttribute("aria-expanded", "true");
  if (!results.length) {
    const empty = document.createElement("div");
    empty.className = "preferences-search-empty";
    empty.setAttribute("role", "status");
    empty.textContent = tr("No Result");
    els.preferencesSearchCompletions.append(empty);
    els.preferencesSearch.removeAttribute("aria-activedescendant");
    return;
  }
  if (preferenceSearchCompletionIndex >= results.length) preferenceSearchCompletionIndex = results.length - 1;
  results.forEach((entry, index) => {
    const button = document.createElement("button");
    button.type = "button";
    button.id = `preferences-search-result-${index}`;
    button.className = index === preferenceSearchCompletionIndex
      ? "preferences-search-completion is-selected"
      : "preferences-search-completion";
    button.setAttribute("role", "option");
    button.setAttribute("aria-selected", String(index === preferenceSearchCompletionIndex));
    const label = document.createElement("span");
    label.className = "preferences-search-completion-label";
    label.textContent = trReference(
      entry.label || entry.section.title,
      preferenceEntryContext(entry),
    ).replace(/:\s*$/u, "");
    const path = document.createElement("span");
    path.className = "preferences-search-completion-path";
    const tab = document.createElement("span");
    tab.className = "preferences-search-completion-tab";
    tab.textContent = trReference(entry.pane.title, entry.pane.l10n);
    path.append(tab);
    if (entry.label) {
      const separator = document.createElement("span");
      separator.className = "preferences-search-completion-separator";
      separator.textContent = "»";
      const section = document.createElement("span");
      section.className = "preferences-search-completion-section";
      section.textContent = trReference(entry.section.title, entry.section.l10n).replace(/:\s*$/u, "");
      path.append(separator, section);
    }
    button.append(label, path);
    button.addEventListener("mouseenter", () => {
      if (preferenceSearchCompletionIndex === index) return;
      preferenceSearchCompletionIndex = index;
      renderPreferenceSearchCompletions();
    });
    button.addEventListener("mousedown", (event) => event.preventDefault());
    button.addEventListener("click", () => selectPreferenceSearchCompletion(entry));
    els.preferencesSearchCompletions.append(button);
  });
  if (preferenceSearchCompletionIndex >= 0) {
    els.preferencesSearch.setAttribute(
      "aria-activedescendant",
      `preferences-search-result-${preferenceSearchCompletionIndex}`,
    );
  } else {
    els.preferencesSearch.removeAttribute("aria-activedescendant");
  }
}

function handlePreferenceSearchKeydown(event) {
  const results = currentPreferenceSearchCompletions();
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    preferenceSearchCompletionIndex = -1;
    dismissPreferenceSearchCompletions();
    return;
  }
  if (!results.length) return;
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    event.stopPropagation();
    const direction = event.key === "ArrowDown" ? 1 : -1;
    preferenceSearchCompletionIndex = nextPreferenceSearchIndex(
      preferenceSearchCompletionIndex,
      results.length,
      direction,
    );
    renderPreferenceSearchCompletions();
    els.preferencesSearchCompletions
      .querySelector(".is-selected")
      ?.scrollIntoView({ block: "nearest" });
  } else if (event.key === "Enter") {
    event.preventDefault();
    event.stopPropagation();
    selectPreferenceSearchCompletion(results[Math.max(0, preferenceSearchCompletionIndex)]);
  }
}

function selectPreferenceSearchCompletion(entry) {
  if (!entry) return;
  const selectionStart = els.preferencesSearch.selectionStart;
  const selectionEnd = els.preferencesSearch.selectionEnd;
  const generation = ++preferenceSearchHighlightGeneration;
  activePreferencePane = entry.pane.id;
  preferenceSearchCompletionIndex = -1;
  dismissPreferenceSearchCompletions();
  renderPreferences();
  els.preferencesSearch.focus({ preventScroll: true });
  if (selectionStart !== null && selectionEnd !== null) {
    els.preferencesSearch.setSelectionRange(selectionStart, selectionEnd);
  }
  window.clearTimeout(preferenceSearchHighlightTimer);
  preferenceSearchHighlightTimer = window.setTimeout(() => {
    if (generation !== preferenceSearchHighlightGeneration) return;
    let target = null;
    const targetKeys = preferenceSearchTargetKeys(entry);
    for (const key of targetKeys) {
      target = els.preferencesContent.querySelector(`[data-key="${CSS.escape(key)}"]`);
      if (target) break;
    }
    if (!target) {
      target = [...els.preferencesContent.querySelectorAll(".pref-section-title")]
        .find((heading) => normalizePreferenceSearchTerm(heading.textContent)
          === normalizePreferenceSearchTerm(trReference(entry.section.title, entry.section.l10n)));
    }
    if (!target) return;
    expandPreferenceCollapseForSearch(target);
    const highlightTarget = entry.label ? target : target.closest(".pref-section") || target;
    highlightTarget.scrollIntoView({ block: "nearest" });
    highlightTarget.classList.remove("pref-search-highlight");
    void highlightTarget.offsetWidth;
    highlightTarget.classList.add("pref-search-highlight");
    preferenceSearchHighlightTimer = window.setTimeout(() => {
      highlightTarget.classList.remove("pref-search-highlight");
    }, 1600);
  }, 250);
}

function renderPreferenceSection(sectionSpec) {
  const { title, controls } = sectionSpec;
  const section = document.createElement("section");
  section.className = "pref-section";
  if (controls.some((control) => String(control.type || "").startsWith("utility-"))) {
    section.classList.add("pref-section--stacked");
  }
  const heading = document.createElement("h3");
  heading.className = "pref-section-title";
  heading.textContent = trReference(title, sectionSpec.l10n);
  section.append(heading);

  const rows = document.createElement("div");
  rows.className = "pref-rows";
  for (const control of preferenceTopLevelControls(controls)) {
    if (!preferenceControlVisible(control)) continue;
    if (control.type === "disclosure") {
      rows.append(renderPreferenceDisclosure(
        control,
        preferenceDisclosureChildren(controls, control.disclosureId),
      ));
    } else {
      rows.append(renderPreferenceControl(control));
    }
  }
  section.append(rows);
  return section;
}

function renderPreferenceDisclosure(control, childControls) {
  const disclosure = document.createElement("div");
  disclosure.className = "pref-disclosure";
  disclosure.dataset.key = control.key;
  disclosure.dataset.prefCollapse = control.disclosureId;

  const trigger = document.createElement("button");
  trigger.type = "button";
  trigger.className = "pref-disclosure-trigger";
  trigger.dataset.prefCollapseTrigger = "true";
  const contentId = `pref-disclosure-${control.disclosureId}`;
  trigger.setAttribute("aria-controls", contentId);
  const marker = document.createElement("span");
  marker.className = "pref-disclosure-marker";
  marker.setAttribute("aria-hidden", "true");
  const title = document.createElement("span");
  title.textContent = trReference(control.label, control.l10n);
  trigger.append(marker, title);

  const content = document.createElement("div");
  content.id = contentId;
  content.className = "pref-disclosure-content";
  content.dataset.prefCollapseContent = "true";
  content.setAttribute("role", "group");
  content.setAttribute("aria-label", trReference(control.label, control.l10n));
  for (const childControl of childControls) {
    if (preferenceControlVisible(childControl)) content.append(renderPreferenceControl(childControl));
  }
  disclosure.append(trigger, content);
  setPreferenceCollapseOpen(trigger, content, control.defaultOpen === true);
  trigger.addEventListener("click", () => togglePreferenceCollapse(trigger, content));
  return disclosure;
}

function renderPreferenceControl(control) {
  if (control.type === "keybindings") return renderPreferenceKeyBindings(control);
  if (control.type === "keybinding-profile") return renderKeyBindingProfileControl(control);
  if (control.type === "plugins") return renderPluginManager(control);
  if (control.type === "utility-default-application") return renderDefaultApplicationControl(control);
  if (control.type === "utility-restore-alerts") return renderRestoreAlertsControl(control);
  if (control.type === "utility-clear-cache") return renderUtilityCacheControl(control);
  if (control.type === "utility-browser-extensions") return renderBrowserExtensionsControl(control);
  if (control.type === "updater-check") return renderUpdaterCheckControl(control);
  if (control.type === "opensubtitles-account") return renderOpenSubtitlesAccountControl(control);
  if (control.type === "advanced-actions") return renderAdvancedPreferenceActions(control);
  if (control.type === "advanced-options") return renderAdvancedUserOptions(control);
  if (control.type === "advanced-config-directory") return renderAdvancedConfigDirectory(control);
  if (control.type === "window-geometry") return renderPreferenceWindowGeometry(control);
  if (control.type === "radio-group") return renderPreferenceRadioGroup(control);
  if (control.type === "osc-layout") return renderPreferenceOscLayout(control);
  if (control.type === "osc-toolbar") return renderPreferenceOscToolbar(control);
  if (control.type === "checkbox") return renderPreferenceCheckbox(control);
  if (control.type === "checkbox-group") return renderPreferenceCheckboxGroup(control);
  if (control.type === "checkbox-number") return renderPreferenceCheckboxNumber(control);
  if (control.type === "hint") return renderPreferenceStaticHint(control);
  if (control.type === "group-heading") return renderPreferenceGroupHeading(control);

  const row = document.createElement("label");
  row.className = "pref-row";
  row.dataset.key = control.key;
  const disabled = !preferenceControlEnabled(control);

  const label = document.createElement("span");
  label.className = "pref-label";
  label.textContent = trReference(control.label, control.l10n);
  row.append(label);

  const value = getPreferenceValue(control.key);
  const fieldWrap = document.createElement("span");
  fieldWrap.className = "pref-field";

  if (control.type === "select" || control.type === "audio-device") {
    const select = document.createElement("select");
    select.className = "pref-select";
    select.disabled = disabled;
    if (control.ariaLabel) select.setAttribute("aria-label", trReference(control.ariaLabel, control.ariaLabelL10n));
    if (control.type === "audio-device") {
      const options = preferenceAudioDeviceOptions(
        state.audio_devices,
        value,
        getPreferenceValue(control.descriptionKey),
      );
      for (const item of options) {
        const option = document.createElement("option");
        option.value = item.value;
        option.textContent = item.label;
        option.dataset.description = item.description;
        if (item.missing) option.dataset.missing = "true";
        select.append(option);
      }
      select.value = String(value || "auto");
      select.addEventListener("change", async () => {
        const selected = select.selectedOptions[0];
        await setPreferenceValues([
          [control.key, select.value],
          [control.descriptionKey, selected?.dataset.description || select.value],
        ]);
      });
    } else {
      const options = preferenceOptions(control);
      for (const [optionValue, optionLabel, optionContext] of options) {
        const option = document.createElement("option");
        option.value = String(optionValue);
        option.textContent = trReference(optionLabel, optionContext);
        select.append(option);
      }
      const selectedValue = options.some(([optionValue]) => String(optionValue) === String(value))
        ? value
        : control.defaultValue ?? value;
      select.value = String(selectedValue);
      select.addEventListener("change", () => setPreferenceValue(control.key, readPreferenceSelectValue(control, select.value)));
    }
    fieldWrap.append(select);
  } else if (control.type === "font") {
    const fontName = document.createElement("span");
    fontName.className = disabled ? "pref-font-name disabled" : "pref-font-name";
    fontName.textContent = String(value || control.defaultValue || "sans-serif");
    const chooseButton = document.createElement("button");
    chooseButton.className = "pref-action-button pref-font-button";
    chooseButton.type = "button";
    chooseButton.textContent = tr("Choose...");
    chooseButton.disabled = disabled || !control.pickerCommand;
    chooseButton.addEventListener("click", async (event) => {
      event.preventDefault();
      const result = await invoke(control.pickerCommand, control.pickerArgs || {});
      const selectedFont = typeof result === "string"
        ? result
        : result?.quick_settings?.sub_text_font;
      if (result?.mode) setPlayerState(result, { force: true, presentOsd: false });
      if (selectedFont) await setPreferenceValue(control.key, selectedFont);
    });
    fieldWrap.append(fontName, chooseButton);
  } else if (control.type === "color") {
    fieldWrap.classList.add("pref-color-field");
    const colorState = preferenceColorInputState(value, control.defaultValue);
    const colorInput = document.createElement("input");
    colorInput.className = "pref-color-input";
    colorInput.type = "color";
    colorInput.value = colorState.hex;
    colorInput.disabled = disabled;
    colorInput.setAttribute("aria-label", `${trReference(control.label, control.l10n)} ${tr("Color")}`);
    const alphaInput = document.createElement("input");
    alphaInput.className = "pref-color-alpha-input";
    alphaInput.type = "range";
    alphaInput.min = "0";
    alphaInput.max = "1";
    alphaInput.step = "0.01";
    alphaInput.value = String(colorState.alpha);
    alphaInput.disabled = disabled;
    alphaInput.setAttribute("aria-label", `${trReference(control.label, control.l10n)} ${tr("Opacity")}`);
    const alphaLabel = document.createElement("span");
    alphaLabel.className = "pref-color-alpha-label";
    const updateAlphaLabel = () => {
      alphaLabel.textContent = `${Math.round(Number(alphaInput.value) * 100)}%`;
    };
    const saveColor = () => {
      const nextValue = preferenceColorValue(colorInput.value, alphaInput.value);
      if (nextValue) void setPreferenceValue(control.key, nextValue);
    };
    updateAlphaLabel();
    colorInput.addEventListener("change", saveColor);
    alphaInput.addEventListener("input", updateAlphaLabel);
    alphaInput.addEventListener("change", saveColor);
    fieldWrap.append(colorInput, alphaInput, alphaLabel);
    if (colorState.preservedRaw) {
      const preservedHint = document.createElement("small");
      preservedHint.className = "pref-hint";
      preservedHint.textContent = tr("Imported IINA color data is preserved until this color is edited.");
      fieldWrap.append(preservedHint);
    }
  } else if (control.type === "slider") {
    const input = document.createElement("input");
    input.className = "pref-slider";
    input.type = "range";
    input.min = String(control.min ?? 0);
    input.max = String(control.max ?? 100);
    input.step = String(control.step ?? 1);
    input.value = String(normalizePreferenceNumber(control, value));
    input.disabled = disabled;
    input.setAttribute("aria-label", trReference(control.label, control.l10n));
    const valueLabel = document.createElement("span");
    valueLabel.className = "pref-slider-value";
    const updateValueLabel = () => {
      const option = preferenceOptions(control)
        ?.find(([optionValue]) => String(optionValue) === String(input.value));
      valueLabel.textContent = option
        ? trReference(option[1], option[2])
        : String(input.value);
    };
    input.addEventListener("input", updateValueLabel);
    input.addEventListener("change", () => {
      const nextValue = normalizePreferenceNumber(control, input.value);
      input.value = String(nextValue);
      updateValueLabel();
      void setPreferenceValue(control.key, nextValue);
    });
    updateValueLabel();
    fieldWrap.append(input, valueLabel);
  } else if (control.type === "tokens") {
    fieldWrap.classList.add("pref-token-field");
    fieldWrap.append(renderPreferenceLanguageTokenField(control, value, disabled));
  } else if (control.type === "folder") {
    const path = document.createElement("span");
    path.className = disabled ? "pref-folder disabled" : "pref-folder";
    path.textContent = String(value || "");
    const chooseButton = document.createElement("button");
    chooseButton.className = "pref-reveal-button";
    chooseButton.type = "button";
    chooseButton.title = tr("Choose...");
    chooseButton.textContent = "...";
    chooseButton.disabled = disabled || !control.pickerCommand;
    chooseButton.addEventListener("click", async (event) => {
      event.preventDefault();
      const selected = await invoke(control.pickerCommand, control.pickerArgs || {});
      if (selected) await setPreferenceValue(control.key, selected);
    });
    fieldWrap.append(path, chooseButton);
  } else {
    if (control.prefix) {
      const prefix = document.createElement("span");
      prefix.className = "pref-prefix";
      prefix.textContent = control.prefix;
      fieldWrap.append(prefix);
    }
    const input = document.createElement("input");
    input.className = "pref-input";
    input.type = control.type === "number" ? "number" : "text";
    input.disabled = disabled;
    input.value = String(value ?? "");
    if (control.placeholder) input.placeholder = control.placeholder;
    if (control.type === "number") {
      if (control.min !== undefined) input.min = String(control.min);
      if (control.max !== undefined) input.max = String(control.max);
      if (control.step !== undefined) input.step = String(control.step);
    }
    input.addEventListener("change", () => {
      const nextValue = control.type === "number" ? normalizePreferenceNumber(control, input.value) : input.value;
      if (control.type === "number") input.value = String(nextValue);
      setPreferenceValue(control.key, nextValue);
    });
    fieldWrap.append(input);
    if (control.suffix) {
      const suffix = document.createElement("span");
      suffix.className = "pref-suffix";
      suffix.textContent = control.suffix;
      fieldWrap.append(suffix);
    }
  }

  const hintText = control.descriptions?.[String(value)] || control.hint;
  if (hintText) {
    const hint = document.createElement("small");
    hint.className = "pref-hint";
    hint.textContent = trReference(hintText, control.hintL10n);
    fieldWrap.append(hint);
  }
  if (control.helpUrl) fieldWrap.append(preferenceHelpButton(control.helpUrl, control.helpCommand));

  row.append(fieldWrap);
  return row;
}

function persistPreferenceLanguageTokens(key, tokens) {
  const value = serializeLanguageTokens(tokens);
  preferences = {
    ...(preferences || {}),
    values: {
      ...(preferences?.values || {}),
      [key]: value,
    },
  };
  preferenceTokenSaveQueue = preferenceTokenSaveQueue
    .catch(() => {})
    .then(async () => {
      await invoke("set_preference", { change: { key, value } });
    });
}

function renderPreferenceLanguageTokenField(control, rawValue, disabled) {
  const editor = document.createElement("div");
  editor.className = disabled ? "pref-language-token-editor disabled" : "pref-language-token-editor";
  const tokenList = document.createElement("span");
  tokenList.className = "pref-language-token-list";
  const input = document.createElement("input");
  input.className = "pref-language-token-input";
  input.type = "text";
  input.disabled = disabled;
  input.autocomplete = "off";
  input.spellcheck = false;
  input.setAttribute("role", "combobox");
  input.setAttribute("aria-autocomplete", "list");
  input.setAttribute("aria-expanded", "false");
  input.setAttribute("aria-label", trReference(control.label, control.l10n));
  const completionList = document.createElement("div");
  completionList.id = `preference-language-completions-${control.key}`;
  completionList.className = "pref-language-token-completions";
  completionList.setAttribute("role", "listbox");
  completionList.hidden = true;
  input.setAttribute("aria-controls", completionList.id);
  editor.append(tokenList, input, completionList);

  let languages = preferenceLanguageCatalog;
  let tokens = languageTokensFromCsv(rawValue, languages);
  let completions = [];
  let completionIndex = -1;
  let selectedTokenIndex = -1;

  const save = () => persistPreferenceLanguageTokens(control.key, tokens);
  const removeToken = (index, { edit = false } = {}) => {
    const token = tokens[index];
    if (!token) return;
    tokens.splice(index, 1);
    selectedTokenIndex = -1;
    if (edit) input.value = token.editingString;
    renderTokens();
    renderCompletions();
    save();
    input.focus({ preventScroll: true });
  };
  const renderTokens = () => {
    tokenList.replaceChildren();
    tokens.forEach((token, index) => {
      const chip = document.createElement("button");
      chip.type = "button";
      chip.className = index === selectedTokenIndex
        ? "pref-language-token-chip is-selected"
        : "pref-language-token-chip";
      chip.textContent = token.identifier;
      chip.title = token.editingString;
      chip.disabled = disabled;
      chip.setAttribute("aria-label", token.identifier);
      chip.addEventListener("click", () => {
        selectedTokenIndex = index;
        renderTokens();
        chip.focus({ preventScroll: true });
      });
      chip.addEventListener("dblclick", () => removeToken(index, { edit: true }));
      chip.addEventListener("keydown", (event) => {
        if (event.key === "Backspace" || event.key === "Delete") {
          event.preventDefault();
          removeToken(index);
        } else if (event.key === "Enter") {
          event.preventDefault();
          removeToken(index, { edit: true });
        } else if (event.key === "ArrowLeft" && index > 0) {
          event.preventDefault();
          tokenList.children[index - 1]?.focus();
        } else if (event.key === "ArrowRight") {
          event.preventDefault();
          if (index + 1 < tokens.length) tokenList.children[index + 1]?.focus();
          else input.focus({ preventScroll: true });
        }
      });
      tokenList.append(chip);
    });
  };
  const renderCompletions = () => {
    completions = iinaLanguageTokenCompletions(languages, input.value, tokens);
    completionList.replaceChildren();
    if (!completions.length || document.activeElement !== input) {
      completionList.hidden = true;
      input.setAttribute("aria-expanded", "false");
      input.removeAttribute("aria-activedescendant");
      completionIndex = -1;
      return;
    }
    if (completionIndex >= completions.length) completionIndex = completions.length - 1;
    completionList.hidden = false;
    input.setAttribute("aria-expanded", "true");
    completions.forEach((completion, index) => {
      const option = document.createElement("button");
      option.type = "button";
      option.id = `${completionList.id}-${index}`;
      option.className = index === completionIndex
        ? "pref-language-token-completion is-selected"
        : "pref-language-token-completion";
      option.setAttribute("role", "option");
      option.setAttribute("aria-selected", String(index === completionIndex));
      option.textContent = completion.editingString;
      option.addEventListener("mousedown", (event) => event.preventDefault());
      option.addEventListener("mouseenter", () => {
        completionIndex = index;
        renderCompletions();
      });
      option.addEventListener("click", () => commitToken(completion));
      completionList.append(option);
    });
    if (completionIndex >= 0) {
      input.setAttribute("aria-activedescendant", `${completionList.id}-${completionIndex}`);
    } else {
      input.removeAttribute("aria-activedescendant");
    }
  };
  const commitToken = (completion = null) => {
    const token = completion || languageTokenFromEditingString(input.value, languages);
    if (!token) return false;
    const nextTokens = appendUniqueLanguageTokens(tokens, [token]);
    const changed = nextTokens.length !== tokens.length;
    tokens = nextTokens;
    selectedTokenIndex = -1;
    completionIndex = -1;
    input.value = "";
    renderTokens();
    renderCompletions();
    if (changed) save();
    return true;
  };

  input.addEventListener("focus", () => {
    selectedTokenIndex = -1;
    renderTokens();
    renderCompletions();
  });
  input.addEventListener("input", () => {
    selectedTokenIndex = -1;
    completionIndex = -1;
    renderTokens();
    renderCompletions();
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      if (!completions.length) return;
      event.preventDefault();
      completionIndex = nextLanguageCompletionIndex(
        completionIndex,
        completions.length,
        event.key === "ArrowDown" ? 1 : -1,
      );
      renderCompletions();
      completionList.querySelector(".is-selected")?.scrollIntoView({ block: "nearest" });
    } else if (event.key === "Enter") {
      event.preventDefault();
      commitToken(completions[Math.max(0, completionIndex)] || null);
    } else if (event.key === "Tab") {
      if (completionIndex >= 0) {
        event.preventDefault();
        commitToken(completions[completionIndex]);
      } else {
        commitToken();
      }
    } else if (event.key === "Escape") {
      event.preventDefault();
      completionIndex = -1;
      completionList.hidden = true;
      input.setAttribute("aria-expanded", "false");
      input.removeAttribute("aria-activedescendant");
    } else if (event.key === "Backspace" && !input.value && tokens.length) {
      event.preventDefault();
      const lastIndex = tokens.length - 1;
      if (selectedTokenIndex === lastIndex) removeToken(lastIndex);
      else {
        selectedTokenIndex = lastIndex;
        renderTokens();
      }
    }
  });
  input.addEventListener("paste", (event) => {
    const pasted = event.clipboardData?.getData("text/plain") || "";
    if (!/[\n,]/u.test(pasted)) return;
    event.preventDefault();
    const pastedTokens = languageTokensFromCsv(pasted.replace(/\r?\n/gu, ","), languages);
    const nextTokens = appendUniqueLanguageTokens(tokens, pastedTokens);
    const changed = nextTokens.length !== tokens.length;
    tokens = nextTokens;
    input.value = "";
    renderTokens();
    renderCompletions();
    if (changed) save();
  });
  input.addEventListener("blur", () => {
    commitToken();
    completionList.hidden = true;
    input.setAttribute("aria-expanded", "false");
    input.removeAttribute("aria-activedescendant");
  });

  void preferenceLanguageCatalogReady.then((catalog) => {
    if (!editor.isConnected || !catalog.length) return;
    languages = catalog;
    tokens = languageTokensFromCsv(serializeLanguageTokens(tokens), languages);
    renderTokens();
    renderCompletions();
  });
  renderTokens();
  return editor;
}

function preferenceRowLabel(control) {
  const label = document.createElement("span");
  label.className = "pref-label";
  label.textContent = trReference(control.label, control.l10n);
  return label;
}

function renderPreferenceRadioGroup(control) {
  const row = document.createElement("div");
  row.className = "pref-row pref-radio-row";
  row.dataset.key = control.key;
  row.append(preferenceRowLabel(control));
  const field = document.createElement("div");
  field.className = "pref-field pref-radio-group";
  if (control.secondaryLabel) {
    const secondaryLabel = document.createElement("span");
    secondaryLabel.className = "pref-radio-secondary-label";
    secondaryLabel.textContent = trReference(control.secondaryLabel, control.secondaryL10n);
    field.append(secondaryLabel);
  }
  const current = getPreferenceValue(control.key);
  const name = `preference-${control.key}`;
  for (const [optionValue, optionLabel, optionContext] of control.options || []) {
    const option = document.createElement("label");
    option.className = "pref-radio-option";
    const input = document.createElement("input");
    input.type = "radio";
    input.name = name;
    input.value = String(optionValue);
    input.checked = String(current) === String(optionValue);
    input.disabled = !preferenceControlEnabled(control);
    input.addEventListener("change", () => {
      if (input.checked) void setPreferenceValue(control.key, readPreferenceSelectValue(control, input.value));
    });
    const text = document.createElement("span");
    text.textContent = trReference(optionLabel, optionContext);
    option.append(input, text);
    field.append(option);
  }
  row.append(field);
  return row;
}

function renderPreferenceOscLayout(control) {
  const row = document.createElement("div");
  row.className = "pref-row pref-osc-layout-row";
  row.dataset.key = control.key;
  row.append(preferenceRowLabel(control));
  const field = document.createElement("div");
  field.className = "pref-field pref-osc-layout-field";
  const select = document.createElement("select");
  select.className = "pref-select";
  for (const [optionValue, optionLabel, optionContext] of control.options || []) {
    const option = document.createElement("option");
    option.value = String(optionValue);
    option.textContent = trReference(optionLabel, optionContext);
    select.append(option);
  }
  select.value = String(getPreferenceValue(control.key) ?? control.defaultValue);
  select.addEventListener("change", () => void setPreferenceValue(
    control.key,
    readPreferenceSelectValue(control, select.value),
  ));
  const preview = document.createElement("span");
  preview.className = `pref-osc-layout-preview pref-osc-layout-preview--${select.value}`;
  preview.setAttribute("aria-label", tr("On Screen Controller layout preview"));
  const previewBar = document.createElement("span");
  previewBar.className = "pref-osc-layout-preview-bar";
  preview.append(previewBar);
  field.append(select, preview);
  row.append(field);
  return row;
}

function geometrySelect(options, value, ariaLabel) {
  const select = document.createElement("select");
  select.className = "pref-select pref-geometry-select";
  select.setAttribute("aria-label", tr(ariaLabel));
  for (const [optionValue, optionLabel, optionContext] of options) {
    const option = document.createElement("option");
    option.value = optionValue;
    option.textContent = trReference(optionLabel, optionContext);
    select.append(option);
  }
  select.value = value;
  return select;
}

function geometryNumber(value, ariaLabel) {
  const input = document.createElement("input");
  input.className = "pref-input pref-geometry-number";
  input.type = "number";
  input.min = "0";
  input.step = "1";
  input.value = value;
  input.setAttribute("aria-label", tr(ariaLabel));
  return input;
}

function renderPreferenceWindowGeometry(control) {
  const group = document.createElement("div");
  group.className = "pref-window-geometry";
  group.dataset.key = control.key;
  const initial = parseIinaWindowGeometry(getPreferenceValue(control.key));

  const sizeToggle = document.createElement("input");
  sizeToggle.type = "checkbox";
  sizeToggle.checked = initial.sizeEnabled;
  sizeToggle.dataset.prefCollapseTrigger = "true";
  sizeToggle.setAttribute("aria-controls", "pref-window-geometry-size-fields");
  const sizeLabel = document.createElement("label");
  sizeLabel.className = "pref-window-geometry-toggle";
  const sizeText = document.createElement("span");
  sizeText.textContent = trKey("PrefUIViewController", "zOq-Em-wUe.title", "Initial window size:");
  sizeLabel.append(sizeToggle, sizeText);
  const sizeFields = document.createElement("div");
  sizeFields.id = "pref-window-geometry-size-fields";
  sizeFields.className = "pref-window-geometry-fields";
  sizeFields.dataset.prefCollapseContent = "true";
  sizeFields.dataset.prefCollapseDisableContents = "true";
  const sizeDimension = geometrySelect([["width", "Width:"], ["height", "Height:"]], initial.sizeDimension, "Initial size dimension");
  const sizeValue = geometryNumber(initial.sizeValue, "Initial size value");
  const sizeUnit = geometrySelect([["point", "point"], ["percent", "% of screen"]], initial.sizeUnit, "Initial size unit");
  sizeFields.append(sizeDimension, sizeValue, sizeUnit);

  const positionToggle = document.createElement("input");
  positionToggle.type = "checkbox";
  positionToggle.checked = initial.positionEnabled;
  positionToggle.dataset.prefCollapseTrigger = "true";
  positionToggle.setAttribute("aria-controls", "pref-window-geometry-position-fields");
  const positionLabel = document.createElement("label");
  positionLabel.className = "pref-window-geometry-toggle";
  const positionText = document.createElement("span");
  positionText.textContent = trKey("PrefUIViewController", "Ofm-qE-RgQ.title", "Initial window position:");
  positionLabel.append(positionToggle, positionText);
  const positionFields = document.createElement("div");
  positionFields.id = "pref-window-geometry-position-fields";
  positionFields.className = "pref-window-geometry-position";
  positionFields.dataset.prefCollapseContent = "true";
  positionFields.dataset.prefCollapseDisableContents = "true";
  const xLabel = document.createElement("span");
  xLabel.textContent = tr("X offset:");
  const xOffset = geometryNumber(initial.xOffset, "X offset");
  const xUnit = geometrySelect([["point", "point"], ["percent", "% of screen"]], initial.xUnit, "X offset unit");
  const xAnchor = geometrySelect([["left", "left"], ["right", "right"]], initial.xAnchor, "X anchor");
  const xSuffix = document.createElement("span");
  xSuffix.textContent = tr("side of the screen");
  const yLabel = document.createElement("span");
  yLabel.textContent = tr("Y offset:");
  const yOffset = geometryNumber(initial.yOffset, "Y offset");
  const yUnit = geometrySelect([["point", "point"], ["percent", "% of screen"]], initial.yUnit, "Y offset unit");
  const yAnchor = geometrySelect([["top", "top"], ["bottom", "bottom"]], initial.yAnchor, "Y anchor");
  const ySuffix = document.createElement("span");
  ySuffix.textContent = tr("of the screen");
  positionFields.append(
    xLabel,
    xOffset,
    xUnit,
    document.createTextNode(trKey("PrefUIViewController", "qbc-CY-bdK.title", "to the")),
    xAnchor,
    xSuffix,
  );
  positionFields.append(
    yLabel,
    yOffset,
    yUnit,
    document.createTextNode(trKey("PrefUIViewController", "Ydi-yB-FMe.title", "to the")),
    yAnchor,
    ySuffix,
  );

  const allFields = [sizeDimension, sizeValue, sizeUnit, xOffset, xUnit, xAnchor, yOffset, yUnit, yAnchor];
  const syncVisibility = () => {
    for (const field of [sizeDimension, sizeValue, sizeUnit]) field.disabled = !sizeToggle.checked;
    for (const field of [xOffset, xUnit, xAnchor, yOffset, yUnit, yAnchor]) field.disabled = !positionToggle.checked;
    setPreferenceCollapseOpen(sizeToggle, sizeFields, sizeToggle.checked);
    setPreferenceCollapseOpen(positionToggle, positionFields, positionToggle.checked);
  };
  const save = () => {
    const next = buildIinaWindowGeometry({
      sizeEnabled: sizeToggle.checked,
      sizeDimension: sizeDimension.value,
      sizeValue: sizeValue.value,
      sizeUnit: sizeUnit.value,
      positionEnabled: positionToggle.checked,
      xOffset: xOffset.value,
      xUnit: xUnit.value,
      xAnchor: xAnchor.value,
      yOffset: yOffset.value,
      yUnit: yUnit.value,
      yAnchor: yAnchor.value,
    });
    void setPreferenceValue(control.key, next);
  };
  sizeToggle.addEventListener("change", () => { syncVisibility(); save(); });
  positionToggle.addEventListener("change", () => { syncVisibility(); save(); });
  for (const field of allFields) field.addEventListener("change", save);
  const sizeCollapse = document.createElement("div");
  sizeCollapse.className = "pref-window-geometry-collapse";
  sizeCollapse.dataset.key = `${control.key}:size`;
  sizeCollapse.dataset.prefCollapse = "window-geometry-size";
  sizeCollapse.append(sizeLabel, sizeFields);
  const positionCollapse = document.createElement("div");
  positionCollapse.className = "pref-window-geometry-collapse";
  positionCollapse.dataset.key = `${control.key}:position`;
  positionCollapse.dataset.prefCollapse = "window-geometry-position";
  positionCollapse.append(positionLabel, positionFields);
  group.append(sizeCollapse, positionCollapse);
  syncVisibility();
  return group;
}

const OSC_TOOLBAR_ICON_PATHS = Object.freeze({
  0: "assets/iina/icons/settings.png",
  1: "assets/iina/icons/playlist.png",
  2: "assets/iina/icons/pip.png",
  3: "assets/iina/icons/fullscreen.png",
  4: "assets/iina/icons/toggle-album-art.png",
  5: "assets/iina/icons/sub-track.png",
  6: "assets/iina/icons/screenshot.png",
});

function oscToolbarButtonItem(buttonValue, { compact = false } = {}) {
  const spec = IINA_OSC_TOOLBAR_BUTTONS.find(({ value }) => value === buttonValue);
  const item = document.createElement("span");
  item.className = compact ? "pref-osc-toolbar-item is-compact" : "pref-osc-toolbar-item";
  item.dataset.button = String(buttonValue);
  const icon = document.createElement("img");
  icon.src = OSC_TOOLBAR_ICON_PATHS[buttonValue];
  icon.alt = "";
  icon.draggable = false;
  const label = document.createElement("span");
  label.textContent = spec
    ? trReference(spec.label, spec.l10n)
    : tr("Unknown");
  item.append(icon, label);
  return item;
}

function renderPreferenceOscToolbar(control) {
  const row = document.createElement("div");
  row.className = "pref-row pref-osc-toolbar-row";
  row.dataset.key = control.key;
  row.append(preferenceRowLabel(control));
  const field = document.createElement("div");
  field.className = "pref-field pref-osc-toolbar-field";
  const preview = document.createElement("span");
  preview.className = "pref-osc-toolbar-preview";
  for (const button of normalizeIinaOscToolbarButtons(getPreferenceValue(control.key))) {
    preview.append(oscToolbarButtonItem(button, { compact: true }));
  }
  const customize = document.createElement("button");
  customize.className = "pref-action-button pref-osc-toolbar-customize";
  customize.type = "button";
  customize.textContent = tr("Customize…");
  customize.addEventListener("click", async () => {
    const next = await showOscToolbarCustomization(getPreferenceValue(control.key));
    if (next) await setPreferenceValue(control.key, next);
  });
  field.append(preview, customize);
  row.append(field);
  return row;
}

async function showOscToolbarCustomization(rawValue) {
  let draft = normalizeIinaOscToolbarButtons(rawValue);
  const sheet = document.createElement("form");
  sheet.className = "preference-sheet preference-osc-toolbar-sheet";
  const title = document.createElement("h3");
  title.className = "preference-alert-title";
  title.textContent = tr("Customize On Screen Controller Toolbar");
  const hint = document.createElement("p");
  hint.className = "preference-osc-toolbar-hint";
  hint.textContent = tr("Drag items to add, reorder, or remove them. Up to 5 items can be shown.");
  const columns = document.createElement("div");
  columns.className = "preference-osc-toolbar-columns";
  const currentColumn = document.createElement("section");
  const availableColumn = document.createElement("section");
  const currentTitle = document.createElement("h4");
  currentTitle.textContent = tr("Current Items");
  const availableTitle = document.createElement("h4");
  availableTitle.textContent = tr("Available Items");
  const currentList = document.createElement("div");
  currentList.className = "preference-osc-toolbar-list is-current";
  const availableList = document.createElement("div");
  availableList.className = "preference-osc-toolbar-list is-available";
  currentColumn.append(currentTitle, currentList);
  availableColumn.append(availableTitle, availableList);
  columns.append(currentColumn, availableColumn);

  const readDraggedButton = (event) => Number(event.dataTransfer?.getData("text/plain"));
  const renderLists = () => {
    currentList.replaceChildren();
    availableList.replaceChildren();
    draft.forEach((button, index) => {
      const item = oscToolbarButtonItem(button);
      item.draggable = true;
      item.addEventListener("dragstart", (event) => event.dataTransfer?.setData("text/plain", String(button)));
      item.addEventListener("dragover", (event) => event.preventDefault());
      item.addEventListener("drop", (event) => {
        event.preventDefault();
        const dragged = readDraggedButton(event);
        if (!Number.isInteger(dragged) || dragged === button) return;
        const next = draft.filter((value) => value !== dragged);
        next.splice(next.indexOf(button), 0, dragged);
        draft = normalizeIinaOscToolbarButtons(next);
        renderLists();
      });
      const actions = document.createElement("span");
      actions.className = "preference-osc-toolbar-item-actions";
      for (const [glyph, label, delta] of [["‹", "Move left", -1], ["›", "Move right", 1]]) {
        const move = document.createElement("button");
        move.type = "button";
        move.textContent = glyph;
        move.title = tr(label);
        move.disabled = index + delta < 0 || index + delta >= draft.length;
        move.addEventListener("click", () => {
          const next = [...draft];
          [next[index], next[index + delta]] = [next[index + delta], next[index]];
          draft = next;
          renderLists();
        });
        actions.append(move);
      }
      const remove = document.createElement("button");
      remove.type = "button";
      remove.textContent = "×";
      remove.title = tr("Remove");
      remove.addEventListener("click", () => { draft = draft.filter((value) => value !== button); renderLists(); });
      actions.append(remove);
      item.append(actions);
      currentList.append(item);
    });
    for (const spec of IINA_OSC_TOOLBAR_BUTTONS) {
      const item = oscToolbarButtonItem(spec.value);
      item.draggable = !draft.includes(spec.value) && draft.length < 5;
      const add = document.createElement("button");
      add.type = "button";
      add.textContent = draft.includes(spec.value)
        ? tr("Added")
        : trKey("FilterWindowController", "w2g-wR-hPu.title", "Add");
      add.disabled = draft.includes(spec.value) || draft.length >= 5;
      add.addEventListener("click", () => { draft = normalizeIinaOscToolbarButtons([...draft, spec.value]); renderLists(); });
      item.addEventListener("dragstart", (event) => event.dataTransfer?.setData("text/plain", String(spec.value)));
      item.append(add);
      availableList.append(item);
    }
  };
  currentList.addEventListener("dragover", (event) => event.preventDefault());
  currentList.addEventListener("drop", (event) => {
    event.preventDefault();
    const button = readDraggedButton(event);
    if (!Number.isInteger(button) || draft.includes(button) || draft.length >= 5) return;
    draft = normalizeIinaOscToolbarButtons([...draft, button]);
    renderLists();
  });
  availableList.addEventListener("dragover", (event) => event.preventDefault());
  availableList.addEventListener("drop", (event) => {
    event.preventDefault();
    const button = readDraggedButton(event);
    if (!Number.isInteger(button) || !draft.includes(button)) return;
    draft = draft.filter((value) => value !== button);
    renderLists();
  });
  renderLists();

  const restore = document.createElement("button");
  restore.type = "button";
  restore.className = "dialog-button preference-osc-toolbar-restore";
  restore.textContent = tr("Restore Default");
  restore.addEventListener("click", () => { draft = [...IINA_DEFAULT_OSC_TOOLBAR_BUTTONS]; renderLists(); });
  const actions = document.createElement("div");
  actions.className = "preference-sheet-actions";
  const cancel = sheetButton(trKey("PrefOSCToolbarSettingsSheetController", "Jrf-II-Cfc.title", "Cancel"));
  cancel.addEventListener("click", () => dismissPreferenceSheet(null));
  const done = sheetButton(trKey("PrefOSCToolbarSettingsSheetController", "0wJ-C7-Gds.title", "Done"), true);
  sheet.addEventListener("submit", (event) => { event.preventDefault(); dismissPreferenceSheet([...draft]); });
  actions.append(restore, cancel, done);
  sheet.append(title, hint, columns, actions);
  return presentPreferenceSheet(sheet);
}

function renderPreferenceStaticHint(control) {
  const row = document.createElement("div");
  row.className = "pref-row pref-row--hint";
  row.dataset.key = control.key;
  const spacer = document.createElement("span");
  spacer.setAttribute("aria-hidden", "true");
  const hint = document.createElement("small");
  hint.className = "pref-hint pref-static-hint";
  hint.textContent = trReference(control.label, control.l10n);
  row.append(spacer, hint);
  return row;
}

function renderPreferenceGroupHeading(control) {
  const heading = document.createElement("div");
  heading.className = "pref-group-heading";
  heading.dataset.key = control.key;
  heading.textContent = trReference(control.label, control.l10n);
  return heading;
}

function renderAdvancedPreferenceActions(control) {
  const row = document.createElement("div");
  row.className = "pref-row pref-advanced-action-row";
  row.dataset.key = control.key;
  const spacer = document.createElement("span");
  spacer.setAttribute("aria-hidden", "true");
  const actions = document.createElement("span");
  actions.className = "pref-field pref-advanced-actions";
  const disabled = !preferenceControlEnabled(control);
  const status = document.createElement("small");
  status.className = "pref-advanced-action-status";
  status.setAttribute("aria-live", "polite");
  for (const action of control.actions || []) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "pref-action-button";
    button.textContent = tr(action.label);
    button.disabled = disabled || !action.command;
    button.addEventListener("click", async () => {
      button.disabled = true;
      status.textContent = "";
      try {
        await invoke(action.command, action.args || {});
      } catch (error) {
        status.textContent = String(error?.message || error);
      } finally {
        button.disabled = disabled;
      }
    });
    actions.append(button);
  }
  actions.append(status);
  row.append(spacer, actions);
  return row;
}

function renderAdvancedUserOptions(control) {
  const group = document.createElement("div");
  group.className = "pref-advanced-options";
  group.dataset.key = control.key;
  const title = document.createElement("span");
  title.className = "pref-advanced-options-title";
  title.textContent = trReference(control.label, control.l10n);
  const enabled = preferenceControlEnabled(control);
  const options = normalizeAdvancedUserOptions(getPreferenceValue(control.key));
  if (advancedOptionSelectedIndex >= options.length) advancedOptionSelectedIndex = -1;

  const tableWrap = document.createElement("div");
  tableWrap.className = "pref-advanced-options-table-wrap";
  const table = document.createElement("table");
  table.className = "pref-advanced-options-table";
  table.setAttribute("aria-label", trReference(control.label, control.l10n));
  const head = document.createElement("thead");
  const headRow = document.createElement("tr");
  for (const headingText of ["Name", "Value"]) {
    const heading = document.createElement("th");
    heading.scope = "col";
    heading.textContent = tr(headingText);
    headRow.append(heading);
  }
  head.append(headRow);
  const body = document.createElement("tbody");
  const error = document.createElement("small");
  error.className = "pref-advanced-options-error";
  error.setAttribute("aria-live", "polite");
  error.textContent = advancedOptionError;
  let removeButton;

  const updateSelection = (selectedIndex) => {
    advancedOptionSelectedIndex = selectedIndex;
    Array.from(body.rows).forEach((row, index) => {
      const selected = index === advancedOptionSelectedIndex;
      row.classList.toggle("is-selected", selected);
      row.setAttribute("aria-selected", String(selected));
    });
    if (removeButton) removeButton.disabled = !enabled || advancedOptionSelectedIndex < 0;
  };

  options.forEach((option, rowIndex) => {
    const row = document.createElement("tr");
    row.className = rowIndex === advancedOptionSelectedIndex ? "is-selected" : "";
    row.setAttribute("aria-selected", String(rowIndex === advancedOptionSelectedIndex));
    row.addEventListener("click", () => updateSelection(rowIndex));
    option.forEach((cellValue, columnIndex) => {
      const cell = document.createElement("td");
      const input = document.createElement("input");
      input.type = "text";
      input.className = "pref-advanced-option-input";
      input.value = cellValue;
      input.disabled = !enabled;
      input.setAttribute("aria-label", `${tr(columnIndex === 0 ? "Name" : "Value")} ${rowIndex + 1}`);
      input.addEventListener("change", async () => {
        const next = advancedUserOptionsWithEdit(options, rowIndex, columnIndex, input.value);
        if (!next) {
          input.value = cellValue;
          advancedOptionError = tr("The option name and value cannot be empty.");
          error.textContent = advancedOptionError;
          return;
        }
        advancedOptionError = "";
        await setPreferenceValue(control.key, next);
      });
      cell.append(input);
      row.append(cell);
    });
    body.append(row);
  });
  table.append(head, body);
  tableWrap.append(table);

  const toolbar = document.createElement("div");
  toolbar.className = "pref-advanced-options-toolbar";
  const addButton = document.createElement("button");
  addButton.type = "button";
  addButton.className = "pref-advanced-option-button";
  addButton.textContent = "+";
  addButton.title = trKey("FilterWindowController", "w2g-wR-hPu.title", "Add");
  addButton.setAttribute("aria-label", trKey("FilterWindowController", "w2g-wR-hPu.title", "Add"));
  addButton.disabled = !enabled;
  addButton.addEventListener("click", async () => {
    const next = advancedUserOptionsWithAdded(options);
    advancedOptionSelectedIndex = next.length - 1;
    advancedOptionError = "";
    await setPreferenceValue(control.key, next);
  });
  removeButton = document.createElement("button");
  removeButton.type = "button";
  removeButton.className = "pref-advanced-option-button";
  removeButton.textContent = "−";
  removeButton.title = tr("Remove");
  removeButton.setAttribute("aria-label", tr("Remove"));
  removeButton.disabled = !enabled || advancedOptionSelectedIndex < 0;
  removeButton.addEventListener("click", async () => {
    if (advancedOptionSelectedIndex < 0) return;
    const next = advancedUserOptionsWithRemoved(options, advancedOptionSelectedIndex);
    advancedOptionSelectedIndex = Math.min(advancedOptionSelectedIndex, next.length - 1);
    advancedOptionError = "";
    await setPreferenceValue(control.key, next);
  });
  toolbar.append(addButton, removeButton);
  group.append(title, tableWrap, toolbar, error);
  return group;
}

function renderAdvancedConfigDirectory(control) {
  const row = document.createElement("label");
  row.className = "pref-row pref-advanced-config-directory";
  row.dataset.key = control.key;
  const label = document.createElement("span");
  label.className = "pref-label";
  label.textContent = trReference(control.label, control.l10n);
  const field = document.createElement("span");
  field.className = "pref-field pref-advanced-config-field";
  const disabled = !preferenceControlEnabled(control);
  const input = document.createElement("input");
  input.type = "text";
  input.className = "pref-input";
  input.value = String(getPreferenceValue(control.key) ?? "");
  input.placeholder = control.placeholder || "~/.config/mpv/";
  input.disabled = disabled;
  input.addEventListener("change", () => setPreferenceValue(control.key, input.value));
  const choose = document.createElement("button");
  choose.type = "button";
  choose.className = "pref-action-button";
  choose.textContent = tr("Choose directory…");
  choose.disabled = disabled || !control.pickerCommand;
  choose.addEventListener("click", async (event) => {
    event.preventDefault();
    const selected = await invoke(control.pickerCommand, control.pickerArgs || {});
    if (selected) await setPreferenceValue(control.key, selected);
  });
  field.append(input, choose);
  row.append(label, field);
  return row;
}

function renderUpdaterCheckControl(control) {
  const row = document.createElement("label");
  row.className = "pref-update-check";
  row.dataset.key = control.key;

  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = Boolean(getPreferenceValue(control.key));

  const text = document.createElement("span");
  text.textContent = trReference(control.label, control.l10n);

  const select = document.createElement("select");
  select.className = "pref-select pref-update-interval";
  select.disabled = !input.checked;
  for (const [optionValue, optionLabel, optionContext] of control.options) {
    const option = document.createElement("option");
    option.value = String(optionValue);
    option.textContent = trReference(optionLabel, optionContext);
    select.append(option);
  }
  select.value = String(getPreferenceValue(control.intervalKey));

  input.addEventListener("change", async () => {
    select.disabled = !input.checked;
    await setPreferenceValue(control.key, input.checked);
  });
  select.addEventListener("change", () => {
    setPreferenceValue(control.intervalKey, readPreferenceSelectValue(control, select.value));
  });
  row.append(input, text, select);
  return row;
}

function renderOpenSubtitlesAccountControl(control) {
  const row = document.createElement("div");
  row.className = "pref-row";
  row.dataset.key = control.key;

  const label = document.createElement("span");
  label.className = "pref-label";
  label.textContent = trReference(control.label, control.l10n);

  const field = document.createElement("span");
  field.className = "pref-field pref-account-field";
  const username = String(getPreferenceValue("openSubUsername") || "");
  const status = document.createElement("span");
  status.className = "pref-account-status";
  status.textContent = username
    ? trFormat("Logged in as %@", username)
    : trKey("Localizable", "preference.not_logged_in", "Not logged in");
  const spinner = document.createElement("span");
  spinner.className = "pref-account-spinner";
  spinner.hidden = true;
  spinner.setAttribute("aria-hidden", "true");
  const action = document.createElement("button");
  action.type = "button";
  action.className = "pref-account-button";
  action.textContent = tr(username ? "Logout" : "Login");
  action.addEventListener("click", async () => {
    if (username) {
      action.disabled = true;
      try {
        preferences = await invoke("logout_opensubtitles_account");
      } catch (error) {
        await showUtilityInformation("Error", String(error?.message || error));
      } finally {
        renderPreferences();
      }
      return;
    }
    const credentials = await showOpenSubtitlesLoginPanel();
    if (!credentials) return;
    action.disabled = true;
    spinner.hidden = false;
    try {
      preferences = await invoke("login_opensubtitles_account", credentials);
    } catch (error) {
      await showUtilityInformation("Error", openSubtitlesAccountErrorMessage(error));
    } finally {
      renderPreferences();
    }
  });
  field.append(status, spinner, action);
  if (control.helpUrl) field.append(preferenceHelpButton(control.helpUrl, control.helpCommand));
  row.append(label, field);
  return row;
}

function renderPreferenceCheckbox(control) {
  const label = document.createElement("label");
  label.className = "pref-check";
  label.dataset.key = control.key;
  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = Boolean(getPreferenceValue(control.key));
  input.disabled = !preferenceControlEnabled(control);
  input.addEventListener("change", () => setPreferenceValue(control.key, input.checked));
  const text = document.createElement("span");
  text.textContent = trReference(control.label, control.l10n);
  label.append(input, text);
  if (control.helpUrl) label.append(preferenceHelpButton(control.helpUrl, control.helpCommand));
  if (control.hint) {
    const hint = document.createElement("small");
    hint.className = "pref-check-hint";
    hint.textContent = trReference(control.hint, control.hintL10n);
    label.append(hint);
  }
  return label;
}

function preferenceHelpButton(url, command) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "pref-help-button";
  button.title = trKey("MainMenu", "wpr-3q-Mcd.title", "Help");
  button.setAttribute("aria-label", trKey("MainMenu", "wpr-3q-Mcd.title", "Help"));
  button.textContent = "?";
  button.addEventListener("click", async (event) => {
    event.preventDefault();
    event.stopPropagation();
    if (command) {
      await invoke(command);
    } else {
      window.open(url, "_blank", "noopener,noreferrer");
    }
  });
  return button;
}

function renderPreferenceCheckboxGroup(control) {
  const row = document.createElement("div");
  row.className = "pref-row";
  row.dataset.key = control.key;

  const heading = document.createElement("span");
  heading.className = "pref-label";
  heading.textContent = trReference(control.label, control.l10n);

  const field = document.createElement("span");
  field.className = "pref-field pref-checkbox-group";
  for (const item of control.items || []) {
    const label = document.createElement("label");
    label.className = "pref-inline-check";
    label.dataset.key = item.key;
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = Boolean(getPreferenceValue(item.key));
    input.disabled = !preferenceControlEnabled(control);
    input.addEventListener("change", () => setPreferenceValue(item.key, input.checked));
    const text = document.createElement("span");
    text.textContent = trReference(item.label, item.l10n);
    label.append(input, text);
    field.append(label);
  }
  row.append(heading, field);
  return row;
}

function renderPreferenceCheckboxNumber(control) {
  const row = document.createElement("div");
  row.className = "pref-row";
  row.dataset.key = control.key;

  const toggleLabel = document.createElement("label");
  toggleLabel.className = "pref-check pref-label pref-compound-check";
  const toggle = document.createElement("input");
  toggle.type = "checkbox";
  toggle.checked = Boolean(getPreferenceValue(control.key));
  toggle.disabled = !preferenceControlEnabled(control);
  toggle.addEventListener("change", () => setPreferenceValue(control.key, toggle.checked));
  const text = document.createElement("span");
  text.textContent = trReference(control.label, control.l10n);
  toggleLabel.append(toggle, text);

  const field = document.createElement("span");
  field.className = "pref-field";
  const input = document.createElement("input");
  input.className = "pref-input pref-number-compact";
  input.type = "number";
  input.disabled = !toggle.checked || toggle.disabled;
  input.value = String(getPreferenceValue(control.valueKey) ?? control.valueDefault ?? "");
  if (control.min !== undefined) input.min = String(control.min);
  if (control.max !== undefined) input.max = String(control.max);
  if (control.step !== undefined) input.step = String(control.step);
  input.addEventListener("change", () => {
    const nextValue = normalizePreferenceNumber(
      { ...control, defaultValue: control.valueDefault },
      input.value,
    );
    input.value = String(nextValue);
    setPreferenceValue(control.valueKey, nextValue);
  });
  field.append(input);
  row.append(toggleLabel, field);
  return row;
}

function utilityActionButton(id, label) {
  const button = document.createElement("button");
  button.id = id;
  button.type = "button";
  button.className = "pref-action-button pref-utility-button";
  button.textContent = tr(label);
  return button;
}

function utilityDescription(text) {
  const description = document.createElement("p");
  description.className = "pref-utility-description";
  description.textContent = tr(text);
  return description;
}

function utilityActionRow(button, statusText, statusId) {
  const row = document.createElement("div");
  row.className = "pref-utility-action-row";
  row.append(button);
  if (statusText) {
    const status = document.createElement("span");
    status.id = statusId;
    status.className = "pref-utility-result";
    status.textContent = tr(statusText);
    status.hidden = true;
    row.append(status);
  }
  return row;
}

function renderDefaultApplicationControl(control) {
  const group = document.createElement("div");
  group.className = "pref-utility-control";
  group.dataset.key = control.key;
  const button = utilityActionButton("utility-default-application-button", "Set IINA as the Default Application…");
  button.addEventListener("click", async () => {
    button.disabled = true;
    try {
      const selected = await chooseDefaultApplicationTypes();
      if (!selected) return;
      const result = await invoke("set_default_application", selected);
      await showUtilityInformation(
        "Setting IINA as Default App",
        trFormat("Finished with %d success and %d failed.", result.success_count, result.failed_count),
      );
    } catch (error) {
      await showUtilityInformation("Setting IINA as Default App", String(error?.message || error));
    } finally {
      if (button.isConnected) button.disabled = false;
    }
  });
  group.append(button);
  return group;
}

function renderRestoreAlertsControl(control) {
  const group = document.createElement("div");
  group.className = "pref-utility-control";
  group.dataset.key = control.key;
  const button = utilityActionButton("utility-restore-alerts-button", "Restore Suppressed Alerts…");
  const action = utilityActionRow(button, "Restored.", "utility-restore-alerts-result");
  const result = action.querySelector(".pref-utility-result");
  button.addEventListener("click", async () => {
    const confirmed = await showUtilityConfirmation(
      "Restore Suppressed Alerts",
      "Are you sure you want to restore suppressed alerts?",
    );
    if (!confirmed) return;
    button.disabled = true;
    try {
      preferences = await invoke("restore_suppressed_alerts");
      result.hidden = false;
    } catch (error) {
      await showUtilityInformation("Restore Suppressed Alerts", String(error?.message || error));
    } finally {
      if (button.isConnected) button.disabled = false;
    }
  });
  group.append(
    utilityDescription('The button below will restore all alerts that have been suppressed using the "Do not show this message again" checkbox.'),
    action,
  );
  return group;
}

function renderUtilityCacheControl(control) {
  const group = document.createElement("div");
  group.className = "pref-utility-control pref-utility-cache";
  group.dataset.key = control.key;

  const clearProgress = utilityActionButton("utility-clear-watch-later-button", "Clear Saved Playback Progress…");
  const progressAction = utilityActionRow(clearProgress, "Cleared.", "utility-clear-watch-later-result");
  const progressResult = progressAction.querySelector(".pref-utility-result");
  clearProgress.addEventListener("click", async () => {
    const confirmed = await showUtilityConfirmation(
      "Clear Saved Playback Progress",
      'Are you sure to delete all saved playback progress ("watch later" data)?',
    );
    if (!confirmed) return;
    clearProgress.disabled = true;
    try {
      await invoke("clear_saved_playback_progress");
      progressResult.hidden = false;
    } catch (error) {
      await showUtilityInformation("Clear Saved Playback Progress", String(error?.message || error));
    } finally {
      if (clearProgress.isConnected) clearProgress.disabled = false;
    }
  });

  const clearHistory = utilityActionButton("utility-clear-history-button", "Clear Playback History…");
  const historyAction = utilityActionRow(clearHistory, "Cleared.", "utility-clear-history-result");
  const historyResult = historyAction.querySelector(".pref-utility-result");
  clearHistory.addEventListener("click", async () => {
    const confirmed = await showUtilityConfirmation(
      "Clear Playback History",
      "Are you sure to delete all playback history?",
    );
    if (!confirmed) return;
    clearHistory.disabled = true;
    try {
      setPlayerState(await invoke("clear_playback_history"), { force: true });
      historyResult.hidden = false;
    } catch (error) {
      await showUtilityInformation("Clear Playback History", String(error?.message || error));
    } finally {
      if (clearHistory.isConnected) clearHistory.disabled = false;
    }
  });

  const cacheStatus = document.createElement("div");
  cacheStatus.className = "pref-cache-status";
  const cacheLabel = document.createElement("span");
  cacheLabel.textContent = tr("Current thumbnail cache:");
  const cacheSize = document.createElement("span");
  cacheSize.id = "utility-thumbnail-cache-size";
  cacheSize.className = "pref-cache-size";
  cacheSize.textContent = tr("Calculating…");
  cacheStatus.append(cacheLabel, cacheSize);
  const clearThumbnail = utilityActionButton("utility-clear-thumbnail-button", "Clear Thumbnail Cache…");
  clearThumbnail.addEventListener("click", async () => {
    const confirmed = await showUtilityConfirmation(
      "Clear Thumbnail Cache",
      "Are you sure to clear all thumbnail Cache?",
    );
    if (!confirmed) return;
    clearThumbnail.disabled = true;
    try {
      const stats = await invoke("clear_thumbnail_cache");
      cacheSize.textContent = formatBinaryByteCount(stats.size_bytes);
    } catch (error) {
      await showUtilityInformation("Clear Thumbnail Cache", String(error?.message || error));
    } finally {
      if (clearThumbnail.isConnected) clearThumbnail.disabled = false;
    }
  });

  group.append(
    utilityDescription('The button below will delete all files in the "watch later" folder. These files contain all saved playback progress and settings applied during playback.'),
    progressAction,
    utilityDescription('The button below will delete all playback histories and entries in "Open Recent".'),
    historyAction,
    utilityDescription("The button below will delete all cached thumbnails."),
    cacheStatus,
    clearThumbnail,
  );
  void invoke("get_thumbnail_cache_stats")
    .then((stats) => {
      if (cacheSize.isConnected) cacheSize.textContent = formatBinaryByteCount(stats.size_bytes);
    })
    .catch(() => {
      if (cacheSize.isConnected) cacheSize.textContent = "Unavailable";
    });
  return group;
}

function renderBrowserExtensionsControl(control) {
  const group = document.createElement("div");
  group.className = "pref-utility-control";
  group.dataset.key = control.key;
  const links = document.createElement("div");
  links.className = "pref-browser-links";
  for (const [browser, label] of [["chrome", "Chrome"], ["firefox", "Firefox"]]) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "pref-browser-link";
    button.dataset.browser = browser;
    const icon = document.createElement("span");
    icon.className = "pref-browser-link-icon";
    icon.setAttribute("aria-hidden", "true");
    icon.textContent = "↗";
    const text = document.createElement("span");
    text.textContent = label;
    button.append(icon, text);
    button.addEventListener("click", async () => {
      try {
        await invoke("open_browser_extension", { browser });
      } catch (error) {
        await showUtilityInformation("Browser Extension", String(error?.message || error));
      }
    });
    links.append(button);
  }
  group.append(
    utilityDescription("Open links or current webpage in IINA with one click. The website must be supported by youtube-dl."),
    links,
  );
  return group;
}

function sheetButton(label, primary = false) {
  const button = document.createElement("button");
  button.type = primary ? "submit" : "button";
  button.className = primary
    ? "dialog-button dialog-button--primary preference-sheet-primary"
    : "dialog-button";
  button.textContent = label;
  return button;
}

function presentPreferenceSheet(sheet) {
  dismissPreferenceSheet(null);
  els.preferenceSheetLayer.replaceChildren(sheet);
  els.preferenceSheetLayer.hidden = false;
  return new Promise((resolve) => {
    preferenceSheetResolver = resolve;
    requestAnimationFrame(() => sheet.querySelector("input, .preference-sheet-primary")?.focus());
  });
}

function dismissPreferenceSheet(result) {
  if (!els.preferenceSheetLayer || els.preferenceSheetLayer.hidden) return false;
  els.preferenceSheetLayer.hidden = true;
  els.preferenceSheetLayer.replaceChildren();
  const resolve = preferenceSheetResolver;
  preferenceSheetResolver = null;
  resolve?.(result);
  return true;
}

function chooseDefaultApplicationTypes() {
  const sheet = document.createElement("form");
  sheet.id = "default-application-sheet";
  sheet.className = "preference-sheet preference-default-sheet";
  const description = document.createElement("p");
  description.className = "preference-default-description";
  description.textContent = trKey(
    "PrefUtilsViewController",
    "uvK-Y1-dZr.title",
    "Please select the media types that you want to make IINA as the default Application for.",
  );
  const choices = document.createElement("div");
  choices.className = "preference-default-choices";
  const inputs = {};
  for (const [key, label, contextKey] of [
    ["video", "Video", "8wH-Gi-mRV.title"],
    ["audio", "Audio", "jFt-2a-nmc.title"],
    ["playlist", "Playlist", "MGo-J8-zWE.title"],
  ]) {
    const choice = document.createElement("label");
    choice.className = "preference-default-choice";
    const input = document.createElement("input");
    input.id = `default-application-${key}`;
    input.type = "checkbox";
    input.checked = true;
    inputs[key] = input;
    const text = document.createElement("span");
    text.textContent = trKey("PrefUtilsViewController", contextKey, label);
    choice.append(input, text);
    choices.append(choice);
  }
  const actions = document.createElement("div");
  actions.className = "preference-sheet-actions";
  const cancel = sheetButton(trKey("PrefUtilsViewController", "kbP-VI-dmT.title", "Cancel"));
  const confirm = sheetButton(trKey("PrefUtilsViewController", "IkF-qG-s4j.title", "OK"), true);
  cancel.addEventListener("click", () => dismissPreferenceSheet(null));
  actions.append(cancel, confirm);
  sheet.append(description, choices, actions);
  sheet.addEventListener("submit", (event) => {
    event.preventDefault();
    dismissPreferenceSheet({
      video: inputs.video.checked,
      audio: inputs.audio.checked,
      playlist: inputs.playlist.checked,
    });
  });
  return presentPreferenceSheet(sheet);
}

function showUtilityConfirmation(title, message) {
  const sheet = document.createElement("form");
  sheet.className = "preference-sheet preference-alert-sheet";
  const heading = document.createElement("h2");
  heading.className = "preference-alert-title";
  heading.textContent = tr(title);
  const description = document.createElement("p");
  description.className = "preference-alert-message";
  description.textContent = tr(message);
  const actions = document.createElement("div");
  actions.className = "preference-sheet-actions";
  const cancel = sheetButton("Cancel");
  const confirm = sheetButton("OK", true);
  cancel.addEventListener("click", () => dismissPreferenceSheet(false));
  actions.append(cancel, confirm);
  sheet.append(heading, description, actions);
  sheet.addEventListener("submit", (event) => {
    event.preventDefault();
    dismissPreferenceSheet(true);
  });
  return presentPreferenceSheet(sheet);
}

function showOpenSubtitlesLoginPanel() {
  const sheet = document.createElement("form");
  sheet.className = "preference-sheet preference-account-sheet";
  const heading = document.createElement("h2");
  heading.className = "preference-alert-title";
  heading.textContent = tr("OpenSubtitles Login");
  const description = document.createElement("p");
  description.className = "preference-alert-message";
  description.textContent = tr("Please enter your username and password");
  const fields = document.createElement("div");
  fields.className = "preference-account-fields";
  const usernameLabel = document.createElement("label");
  usernameLabel.textContent = `${trKey("Localizable", "general.username", "Username")}:`;
  const username = document.createElement("input");
  username.className = "pref-input";
  username.type = "text";
  username.autocomplete = "username";
  usernameLabel.append(username);
  const passwordLabel = document.createElement("label");
  passwordLabel.textContent = `${trKey("Localizable", "general.password", "Password")}:`;
  const password = document.createElement("input");
  password.className = "pref-input";
  password.type = "password";
  password.autocomplete = "current-password";
  passwordLabel.append(password);
  fields.append(usernameLabel, passwordLabel);
  const actions = document.createElement("div");
  actions.className = "preference-sheet-actions";
  const cancel = sheetButton("Cancel");
  const confirm = sheetButton("OK", true);
  cancel.addEventListener("click", () => dismissPreferenceSheet(null));
  actions.append(cancel, confirm);
  sheet.append(heading, description, fields, actions);
  sheet.addEventListener("submit", (event) => {
    event.preventDefault();
    dismissPreferenceSheet({ username: username.value, password: password.value });
  });
  const result = presentPreferenceSheet(sheet);
  queueMicrotask(() => username.focus());
  return result;
}

function openSubtitlesAccountErrorMessage(error) {
  const raw = String(error?.message || error || "Unknown error");
  const keychainPrefix = "OPEN_SUBTITLES_KEYCHAIN:";
  if (raw.startsWith(keychainPrefix)) {
    return trFormat("Cannot save your password to Keychain: %@", raw.slice(keychainPrefix.length));
  }
  const loginPrefix = "OPEN_SUBTITLES_LOGIN:";
  const detail = raw.startsWith(loginPrefix) ? raw.slice(loginPrefix.length) : raw;
  return trFormat(
    "Cannot login. Please check your username, password and network status.\n\n%@",
    detail,
  );
}

function showUtilityInformation(title, message) {
  const sheet = document.createElement("form");
  sheet.className = "preference-sheet preference-alert-sheet";
  const heading = document.createElement("h2");
  heading.className = "preference-alert-title";
  heading.textContent = tr(title);
  const description = document.createElement("p");
  description.className = "preference-alert-message";
  description.textContent = tr(message);
  const actions = document.createElement("div");
  actions.className = "preference-sheet-actions";
  actions.append(sheetButton("OK", true));
  sheet.append(heading, description, actions);
  sheet.addEventListener("submit", (event) => {
    event.preventDefault();
    dismissPreferenceSheet(true);
  });
  return presentPreferenceSheet(sheet);
}

function formatBinaryByteCount(value) {
  let bytes = Math.max(0, Number(value) || 0);
  const units = ["B", "KiB", "MiB", "GiB"];
  let unit = 0;
  while (bytes >= 1024 && unit < units.length - 1) {
    bytes /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || bytes >= 10 ? 0 : 1;
  return `${bytes.toFixed(digits)} ${units[unit]}`;
}

function renderPluginManager(control) {
  const root = document.createElement("section");
  root.className = "plugin-manager";
  root.dataset.key = control.key;
  const title = document.createElement("h2");
  title.className = "plugin-manager-title";
  title.textContent = tr("Plugins");
  const systemToggle = document.createElement("label");
  systemToggle.className = "plugin-manager-system-toggle";
  systemToggle.dataset.key = "iinaEnablePluginSystem";
  const systemToggleInput = document.createElement("input");
  systemToggleInput.type = "checkbox";
  systemToggleInput.checked = Boolean(getPreferenceValue("iinaEnablePluginSystem"));
  systemToggle.append(systemToggleInput, document.createTextNode(tr("Enable plugin system")));
  const description = document.createElement("p");
  description.className = "plugin-manager-description";
  description.textContent = tr("You can install a new plugin from GitHub or a local .iinaplgz package. Plugins installed from GitHub can be updated automatically.");
  const install = document.createElement("button");
  install.type = "button";
  install.className = "plugin-manager-install";
  install.textContent = tr("Install from a local package…");
  install.disabled = !systemToggleInput.checked;
  const github = document.createElement("button");
  github.type = "button";
  github.className = "plugin-manager-github";
  github.textContent = tr("Install from GitHub...");
  github.disabled = !systemToggleInput.checked;
  const headerActions = document.createElement("div");
  headerActions.className = "plugin-manager-install-actions";
  headerActions.append(github, install);
  const separator = document.createElement("div");
  separator.className = "plugin-manager-separator";
  const installedTitle = document.createElement("h3");
  installedTitle.className = "plugin-manager-installed-title";
  installedTitle.textContent = tr("Installed plugins:");

  const status = document.createElement("div");
  status.className = "plugin-manager-status";
  const list = document.createElement("div");
  list.className = "plugin-manager-list";
  list.setAttribute("role", "listbox");
  list.setAttribute("aria-label", tr("Installed plugins"));
  const detail = document.createElement("section");
  detail.className = "plugin-manager-detail";
  detail.hidden = true;
  const workspace = document.createElement("div");
  workspace.className = "plugin-manager-workspace";
  workspace.append(list, detail);

  let plugins = [];
  let refreshGeneration = 0;

  const refresh = async () => {
    const generation = ++refreshGeneration;
    status.textContent = "";
    try {
      const nextPlugins = Array.from(await invoke("get_plugins"));
      if (generation !== refreshGeneration) return;
      plugins = nextPlugins;
      selectedPluginPreferenceId = retainedPluginPreferenceSelection(
        selectedPluginPreferenceId,
        plugins,
      );
      renderPluginManagerMasterList({
        plugins,
        list,
        refresh,
        reorderPlugin,
        selectPlugin,
        systemEnabled: systemToggleInput.checked,
      });
      renderPluginManagerDetail({
        plugin: plugins.find((plugin) => plugin.identifier === selectedPluginPreferenceId),
        detail,
        refresh,
      });
    } catch (error) {
      status.textContent = String(error?.message || error || "Unable to load plugins");
    }
  };

  const selectPlugin = (identifier) => {
    if (selectedPluginPreferenceId === identifier) return;
    selectedPluginPreferenceId = identifier;
    activePluginPreferenceTab = "permissions";
    renderPluginManagerMasterList({
      plugins,
      list,
      refresh,
      reorderPlugin,
      selectPlugin,
      systemEnabled: systemToggleInput.checked,
    });
    renderPluginManagerDetail({
      plugin: plugins.find((plugin) => plugin.identifier === selectedPluginPreferenceId),
      detail,
      refresh,
    });
  };

  const reorderPlugin = async (identifier, destinationIndex) => {
    list.setAttribute("aria-busy", "true");
    status.textContent = "";
    try {
      await invoke("reorder_plugin", { identifier, destinationIndex });
      selectedPluginPreferenceId = identifier;
      await refreshPlayerPluginRuntimes();
      await refresh();
    } catch (error) {
      status.textContent = String(error?.message || error || "Unable to reorder plugins");
    } finally {
      list.removeAttribute("aria-busy");
    }
  };

  systemToggleInput.addEventListener("change", async () => {
    systemToggleInput.disabled = true;
    try {
      preferences = await invoke("set_preference", {
        change: { key: "iinaEnablePluginSystem", value: systemToggleInput.checked },
      });
      install.disabled = !systemToggleInput.checked;
      github.disabled = !systemToggleInput.checked;
      await refresh();
    } catch (error) {
      systemToggleInput.checked = !systemToggleInput.checked;
      status.textContent = String(error?.message || error || "Unable to update plugin system");
    } finally {
      systemToggleInput.disabled = false;
    }
  });

  install.addEventListener("click", async () => {
    try {
      const result = await invoke("install_plugin_dialog");
      const plugin = await resolvePluginInstallResult(result);
      if (plugin) {
        selectedPluginPreferenceId = plugin.identifier;
        activePluginPreferenceTab = "permissions";
        await refreshPlayerPluginRuntimes();
        await refresh();
      }
    } catch (error) {
      status.textContent = String(error?.message || error || "Plugin installation failed");
    }
  });
  github.addEventListener("click", () => void showPluginGithubPanel());

  root.append(
    title,
    systemToggle,
    description,
    headerActions,
    separator,
    installedTitle,
    status,
    workspace,
  );
  void refresh();
  return root;
}

function renderPluginManagerMasterList({
  plugins,
  list,
  refresh,
  reorderPlugin,
  selectPlugin,
  systemEnabled,
}) {
  list.replaceChildren();
  if (!plugins.length) {
    const empty = document.createElement("div");
    empty.className = "plugin-manager-empty";
    empty.textContent = tr("No Plugins Installed");
    list.append(empty);
    return;
  }
  for (const [index, plugin] of plugins.entries()) {
    const row = document.createElement("div");
    row.className = plugin.identifier === selectedPluginPreferenceId
      ? "plugin-manager-row is-selected"
      : "plugin-manager-row";
    row.tabIndex = 0;
    row.draggable = plugins.length > 1;
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", String(plugin.identifier === selectedPluginPreferenceId));
    const enabled = document.createElement("input");
    enabled.type = "checkbox";
    enabled.checked = Boolean(plugin.enabled);
    enabled.disabled = !systemEnabled;
    enabled.title = tr("Enable plugin");
    enabled.setAttribute("aria-label", trFormat("Enable %@", plugin.name || plugin.identifier));
    const name = document.createElement("span");
    name.className = "plugin-manager-name";
    name.textContent = plugin.name || plugin.identifier;
    row.append(enabled, name);
    row.addEventListener("click", () => selectPlugin(plugin.identifier));
    row.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      selectPlugin(plugin.identifier);
    });
    row.addEventListener("dragstart", (event) => {
      event.stopPropagation();
      if (!event.dataTransfer) return;
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData("application/x-iina-plugin-id", plugin.identifier);
      event.dataTransfer.setData("text/plain", plugin.identifier);
      row.classList.add("is-dragging");
    });
    row.addEventListener("dragenter", (event) => {
      event.preventDefault();
      event.stopPropagation();
    });
    row.addEventListener("dragover", (event) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
      for (const candidate of list.querySelectorAll(".plugin-manager-row")) {
        candidate.classList.remove("is-drop-before", "is-drop-after");
      }
      const bounds = row.getBoundingClientRect();
      row.classList.add(event.clientY >= bounds.top + bounds.height / 2
        ? "is-drop-after"
        : "is-drop-before");
    });
    row.addEventListener("drop", (event) => {
      event.preventDefault();
      event.stopPropagation();
      const identifier = event.dataTransfer?.getData("application/x-iima-plugin-id")
        || event.dataTransfer?.getData("text/plain")
        || "";
      const sourceIndex = plugins.findIndex((candidate) => candidate.identifier === identifier);
      const insertAfter = row.classList.contains("is-drop-after");
      const destinationIndex = pluginReorderFinalIndex(
        sourceIndex,
        index + (insertAfter ? 1 : 0),
        plugins.length,
      );
      for (const candidate of list.querySelectorAll(".plugin-manager-row")) {
        candidate.classList.remove("is-dragging", "is-drop-before", "is-drop-after");
      }
      if (destinationIndex == null || destinationIndex === sourceIndex) return;
      void reorderPlugin(identifier, destinationIndex);
    });
    row.addEventListener("dragend", (event) => {
      event.stopPropagation();
      for (const candidate of list.querySelectorAll(".plugin-manager-row")) {
        candidate.classList.remove("is-dragging", "is-drop-before", "is-drop-after");
      }
    });
    enabled.addEventListener("click", (event) => event.stopPropagation());
    enabled.addEventListener("change", async () => {
      enabled.disabled = true;
      try {
        await invoke("set_plugin_enabled", {
          identifier: plugin.identifier,
          enabled: enabled.checked,
        });
        await refreshPlayerPluginRuntimes();
        await refresh();
      } catch {
        enabled.checked = !enabled.checked;
        enabled.disabled = !systemEnabled;
      }
    });
    list.append(row);
  }
}

function unloadEmbeddedPluginPreferencePages(container) {
  pluginPreferencePageRequestId += 1;
  for (const frame of container.querySelectorAll("iframe")) {
    frame.src = "about:blank";
    frame.removeAttribute("srcdoc");
  }
  container.replaceChildren();
}

function disposePluginPreferencesPane() {
  const detail = els.preferencesContent.querySelector(".plugin-manager-detail");
  if (detail) unloadEmbeddedPluginPreferencePages(detail);
}

function applyPluginPreferenceWindowContext(identifier) {
  const normalizedIdentifier = String(identifier || "").trim();
  selectedPluginPreferenceId = normalizedIdentifier || null;
  activePluginPreferenceTab = "permissions";
  pluginPreferencePageRequestId += 1;
}

function renderPluginManagerDetail({ plugin, detail, refresh }) {
  unloadEmbeddedPluginPreferencePages(detail);
  detail.hidden = !plugin;
  if (!plugin) return;

  const heading = document.createElement("header");
  heading.className = "plugin-manager-detail-header";
  const title = document.createElement("div");
  title.className = "plugin-manager-detail-title";
  const name = document.createElement("strong");
  name.textContent = plugin.name || plugin.identifier;
  const version = document.createElement("span");
  version.textContent = plugin.version || "";
  title.append(name, version);
  const description = document.createElement("p");
  description.className = "plugin-manager-detail-description";
  description.textContent = plugin.description || tr("No Description");
  const actions = document.createElement("div");
  actions.className = "plugin-manager-detail-actions";
  const reveal = document.createElement("button");
  reveal.type = "button";
  reveal.className = "plugin-manager-reveal";
  reveal.textContent = trKey("Localizable", "pl_menu.show_in_finder", "Show in Finder");
  reveal.addEventListener("click", () => void revealPluginFromPreferences(plugin, reveal));
  actions.append(reveal);
  const remove = document.createElement("button");
  remove.type = "button";
  remove.className = "plugin-manager-remove";
  remove.textContent = tr(plugin.is_external ? "Unlink" : "Uninstall");
  remove.addEventListener("click", () => void removePluginFromPreferences(plugin, remove, refresh));
  actions.append(remove);
  heading.append(title, description, actions);

  const segments = document.createElement("div");
  segments.className = "plugin-manager-segments";
  segments.setAttribute("role", "tablist");
  const tabContent = document.createElement("div");
  tabContent.className = "plugin-manager-tab-content";
  for (const tab of [
    ["permissions", "Permissions"],
    ["about", "About"],
    ["preferences", "Preferences"],
  ]) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = activePluginPreferenceTab === tab[0] ? "is-selected" : "";
    button.textContent = tr(tab[1]);
    button.setAttribute("role", "tab");
    button.setAttribute("aria-selected", String(activePluginPreferenceTab === tab[0]));
    button.addEventListener("click", () => {
      if (activePluginPreferenceTab === tab[0]) return;
      activePluginPreferenceTab = tab[0];
      renderPluginManagerDetail({ plugin, detail, refresh });
    });
    segments.append(button);
  }
  detail.append(heading, segments, tabContent);
  void renderPluginManagerTab(plugin, tabContent, refresh);
}

async function renderPluginManagerTab(plugin, content, refresh) {
  const requestId = pluginPreferencePageRequestId;
  if (activePluginPreferenceTab === "permissions") {
    renderPluginPermissionsPage(content, plugin);
    return;
  }
  const loading = document.createElement("div");
  loading.className = "plugin-manager-tab-loading";
  loading.textContent = tr("Loading…");
  content.replaceChildren(loading);
  try {
    const contents = await invoke("get_plugin_page_contents", { identifier: plugin.identifier });
    if (
      requestId !== pluginPreferencePageRequestId
      || selectedPluginPreferenceId !== plugin.identifier
      || !content.isConnected
    ) return;
    if (activePluginPreferenceTab === "about") {
      renderPluginAboutPage(content, plugin, contents, refresh);
    } else if (activePluginPreferenceTab === "preferences") {
      renderPluginPreferencesPage(content, plugin, contents?.preference_html);
    }
  } catch (error) {
    if (requestId !== pluginPreferencePageRequestId || !content.isConnected) return;
    renderPluginPageEmpty(content, String(error?.message || error || "Unable to load plugin page"));
  }
}

function renderPluginPermissionsPage(content, plugin) {
  const permissions = Array.from(plugin.permissions || []);
  const list = document.createElement("div");
  list.className = "plugin-manager-permissions";
  if (!permissions.length) {
    const empty = document.createElement("div");
    empty.className = "plugin-page-empty";
    empty.textContent = tr("No permissions requested");
    list.append(empty);
  } else {
    for (const permission of permissions) {
      const presentation = pluginPermissionPresentation(permission, plugin);
      const row = document.createElement("article");
      row.className = permission?.dangerous
        ? "plugin-permission-row dangerous"
        : "plugin-permission-row";
      const title = document.createElement("strong");
      title.textContent = presentation.title;
      const description = document.createElement("span");
      description.textContent = presentation.description;
      row.append(title, description);
      list.append(row);
    }
  }
  content.replaceChildren(list);
}

function renderPluginAboutPage(content, plugin, contents, refresh) {
  const about = document.createElement("div");
  about.className = "plugin-manager-about";
  const metadata = document.createElement("dl");
  metadata.className = "plugin-manager-metadata";
  for (const [label, value] of [
    ["Identifier", plugin.identifier],
    ["Author", plugin.author_name],
    ["Installation Source", plugin.github_repo ? `https://github.com/${plugin.github_repo}` : tr("Local")],
  ]) {
    if (!value) continue;
    const term = document.createElement("dt");
    term.textContent = tr(label);
    const definition = document.createElement("dd");
    definition.textContent = value;
    metadata.append(term, definition);
  }
  about.append(metadata);
  const support = document.createElement("div");
  support.className = "plugin-manager-support";
  for (const [label, url] of [
    ["Website", plugin.author_url],
    ["Email", plugin.author_email ? `mailto:${plugin.author_email}` : null],
  ]) {
    if (!url) continue;
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = tr(label);
    button.addEventListener("click", () => window.open(url, "_blank", "noopener,noreferrer"));
    support.append(button);
  }
  if (support.childElementCount) about.append(support);
  if (!plugin.is_external && plugin.github_repo && Number.isInteger(plugin.github_version)) {
    const update = document.createElement("button");
    update.type = "button";
    update.className = "plugin-manager-update";
    update.textContent = tr("Check for Updates");
    update.addEventListener("click", () => void runPluginUpdate(plugin, update, refresh));
    about.append(update);
  }
  const help = document.createElement("section");
  help.className = "plugin-manager-help";
  renderPluginHelpPage(help, contents?.help_html, contents?.help_url);
  about.append(help);
  content.replaceChildren(about);
}

async function revealPluginFromPreferences(plugin, button) {
  button.disabled = true;
  try {
    await invoke("reveal_plugin_in_finder", { identifier: plugin.identifier });
  } catch (error) {
    await showUtilityInformation(
      "Error",
      String(error?.message || error || "Unable to show plugin in Finder"),
    );
  } finally {
    if (button.isConnected) button.disabled = false;
  }
}

async function runPluginUpdate(plugin, button, refresh) {
  button.disabled = true;
  try {
    const availableUpdate = await invoke("check_plugin_github_update", {
      identifier: plugin.identifier,
    });
    if (!availableUpdate) {
      await showUtilityInformation("Information", "No update found.");
      return;
    }
    const confirmed = await showUtilityConfirmation(
      trFormat("Update found for %@", plugin.name || plugin.identifier),
      trFormat(
        "The latest version is %@. You have %@. Would you like to update?",
        availableUpdate.version,
        plugin.version,
      ),
    );
    if (!confirmed) return;
    button.textContent = tr("Downloading update…");
    const result = await invoke("update_plugin_from_github", { identifier: plugin.identifier });
    const updatedPlugin = await resolvePluginInstallResult(result);
    if (!updatedPlugin) return;
    selectedPluginPreferenceId = updatedPlugin.identifier;
    await refreshPlayerPluginRuntimes();
    await refresh();
  } catch (error) {
    await showUtilityInformation("Error", String(error?.message || error || "Plugin Update Failed"));
  } finally {
    if (button.isConnected) {
      button.textContent = tr("Check for Updates");
      button.disabled = false;
    }
  }
}

async function removePluginFromPreferences(plugin, button, refresh) {
  const confirmed = await showUtilityConfirmation(
    trFormat("Are you sure to uninstall %@?", plugin.name || plugin.identifier),
    "The plugin will be deleted from your disk.",
  );
  if (!confirmed) return;
  button.disabled = true;
  try {
    await invoke("remove_plugin", { identifier: plugin.identifier });
    selectedPluginPreferenceId = null;
    activePluginPreferenceTab = "permissions";
    await refreshPlayerPluginRuntimes();
    await refresh();
  } catch {
    if (button.isConnected) button.disabled = false;
  }
}

function currentKeyBindingProfileName() {
  return String(preferences?.values?.currentInputConfigName || "IINA Default");
}

function findKeyBindingProfile(name) {
  const normalized = String(name || "").toLocaleLowerCase();
  return keyBindingProfiles.find((profile) => profile.name.toLocaleLowerCase() === normalized) ?? null;
}

function currentKeyBindingProfile() {
  return findKeyBindingProfile(currentKeyBindingProfileName());
}

function currentKeyBindingProfileIsReadOnly() {
  return currentKeyBindingProfile()?.readOnly !== false;
}

function keyBindingProfileErrorMessage(error) {
  return String(error?.message || error || "Unable to update key binding configuration");
}

function renderKeyBindingPreferencesIfReady() {
  if (els?.preferencesContent) renderPreferences();
}

async function flushKeyBindingProfileSaves() {
  await keyBindingProfileSaveQueue;
}

async function persistKeyBindingProfileSelection(name, rows) {
  const serializedRows = rows.map(serializeKeyBindingRow);
  const hadModeledRows = Boolean(preferences?.values && Object.hasOwn(preferences.values, "modeledKeyBindings"));
  const previousRows = hadModeledRows ? preferences.values.modeledKeyBindings : null;
  let nextPreferences = await invoke("set_preference", {
    change: { key: "modeledKeyBindings", value: serializedRows },
  });
  try {
    nextPreferences = await invoke("set_preference", {
      change: { key: "currentInputConfigName", value: name },
    });
  } catch (error) {
    try {
      preferences = await invoke("set_preference", {
        change: { key: "modeledKeyBindings", value: previousRows },
      });
    } catch {
      preferences = nextPreferences;
    }
    throw error;
  }
  preferences = nextPreferences;
}

async function applyKeyBindingProfileDocument(document) {
  const profile = document?.profile;
  if (!profile?.name || typeof document.contents !== "string") {
    throw new Error("Key binding profile response is invalid");
  }
  const rows = keyBindingRowsFromInputConf(document.contents);
  await persistKeyBindingProfileSelection(profile.name, rows);
  const index = keyBindingProfiles.findIndex((candidate) => candidate.name === profile.name);
  if (index >= 0) keyBindingProfiles[index] = profile;
  else keyBindingProfiles.push(profile);
}

async function refreshKeyBindingProfiles({ loadCurrent = false, preferredName = null } = {}) {
  await flushKeyBindingProfileSaves();
  const requestId = ++keyBindingProfileLoadRequest;
  keyBindingProfilesLoading = true;
  keyBindingProfileBusy = loadCurrent;
  keyBindingProfileError = "";
  renderKeyBindingPreferencesIfReady();
  try {
    const profiles = await invoke("list_key_binding_profiles");
    if (requestId !== keyBindingProfileLoadRequest) return;
    keyBindingProfiles = Array.isArray(profiles) ? profiles : [];
    if (!loadCurrent) return;
    const desiredName = preferredName || currentKeyBindingProfileName();
    const selected = findKeyBindingProfile(desiredName)
      || findKeyBindingProfile("IINA Default")
      || keyBindingProfiles[0];
    if (!selected) throw new Error("No key binding configurations are available");
    const document = await invoke("read_key_binding_profile", { name: selected.name });
    if (requestId !== keyBindingProfileLoadRequest) return;
    await applyKeyBindingProfileDocument(document);
  } catch (error) {
    if (requestId === keyBindingProfileLoadRequest) {
      keyBindingProfileError = keyBindingProfileErrorMessage(error);
    }
  } finally {
    if (requestId === keyBindingProfileLoadRequest) {
      keyBindingProfilesLoading = false;
      keyBindingProfileBusy = false;
      renderKeyBindingPreferencesIfReady();
    }
  }
}

async function selectKeyBindingProfile(name) {
  await flushKeyBindingProfileSaves();
  const requestId = ++keyBindingProfileLoadRequest;
  keyBindingProfileBusy = true;
  keyBindingProfileError = "";
  renderKeyBindingPreferencesIfReady();
  try {
    const document = await invoke("read_key_binding_profile", { name });
    if (requestId !== keyBindingProfileLoadRequest) return;
    await applyKeyBindingProfileDocument(document);
  } catch (error) {
    if (requestId === keyBindingProfileLoadRequest) {
      keyBindingProfileError = keyBindingProfileErrorMessage(error);
    }
  } finally {
    if (requestId === keyBindingProfileLoadRequest) {
      keyBindingProfileBusy = false;
      renderKeyBindingPreferencesIfReady();
    }
  }
}

async function performKeyBindingProfileAction(action) {
  await flushKeyBindingProfileSaves();
  keyBindingProfileBusy = true;
  keyBindingProfileError = "";
  renderKeyBindingPreferencesIfReady();
  try {
    await action();
  } catch (error) {
    keyBindingProfileError = keyBindingProfileErrorMessage(error);
  } finally {
    keyBindingProfileBusy = false;
    renderKeyBindingPreferencesIfReady();
  }
}

async function adoptKeyBindingProfileDocument(document) {
  const profiles = await invoke("list_key_binding_profiles");
  keyBindingProfiles = Array.isArray(profiles) ? profiles : [];
  await applyKeyBindingProfileDocument(document);
}

function showKeyBindingProfileNamePrompt(title, suggestedName = "") {
  const sheet = document.createElement("form");
  sheet.className = "preference-sheet preference-profile-name-sheet";
  const heading = document.createElement("h2");
  heading.className = "preference-alert-title";
  heading.textContent = tr(title);
  const field = document.createElement("label");
  field.className = "preference-profile-name-field";
  field.textContent = `${tr("Name")}:`;
  const input = document.createElement("input");
  input.className = "pref-input";
  input.type = "text";
  input.maxLength = 250;
  input.autocomplete = "off";
  input.value = suggestedName;
  input.addEventListener("input", () => input.classList.remove("is-invalid"));
  field.append(input);
  const actions = document.createElement("div");
  actions.className = "preference-sheet-actions";
  const cancel = sheetButton("Cancel");
  const confirm = sheetButton("OK", true);
  cancel.addEventListener("click", () => dismissPreferenceSheet(null));
  actions.append(cancel, confirm);
  sheet.append(heading, field, actions);
  sheet.addEventListener("submit", (event) => {
    event.preventDefault();
    const name = input.value.trim();
    if (!name) {
      input.classList.add("is-invalid");
      input.focus();
      return;
    }
    dismissPreferenceSheet(name);
  });
  const result = presentPreferenceSheet(sheet);
  queueMicrotask(() => {
    input.focus();
    input.select();
  });
  return result;
}

async function createKeyBindingProfile() {
  const name = await showKeyBindingProfileNamePrompt("New Configuration");
  if (!name) return;
  await performKeyBindingProfileAction(async () => {
    const document = await invoke("create_key_binding_profile", { name });
    await adoptKeyBindingProfileDocument(document);
  });
}

async function duplicateCurrentKeyBindingProfile() {
  const source = currentKeyBindingProfile();
  if (!source) return;
  const name = await showKeyBindingProfileNamePrompt("Duplicate Configuration", `${source.name} Copy`);
  if (!name) return;
  await performKeyBindingProfileAction(async () => {
    const document = await invoke("duplicate_key_binding_profile", {
      sourceName: source.name,
      newName: name,
    });
    await adoptKeyBindingProfileDocument(document);
  });
}

async function importKeyBindingProfileFromFile(file) {
  if (!file) return;
  const fileName = String(file.name || "");
  if (!fileName.toLocaleLowerCase().endsWith(".conf")) {
    keyBindingProfileError = "Input configuration files must use the .conf extension";
    renderKeyBindingPreferencesIfReady();
    return;
  }
  const name = fileName.slice(0, -".conf".length) || "Imported";
  await performKeyBindingProfileAction(async () => {
    const source = await file.text();
    let created = false;
    try {
      await invoke("create_key_binding_profile", { name });
      created = true;
      const document = await invoke("save_key_binding_profile", {
        name,
        contents: source,
      });
      await adoptKeyBindingProfileDocument(document);
      showOsd("Key Bindings Imported");
    } catch (error) {
      if (created) {
        await invoke("delete_key_binding_profile", { name }).catch(() => {});
      }
      throw error;
    }
  });
}

async function deleteCurrentKeyBindingProfile() {
  const profile = currentKeyBindingProfile();
  if (!profile || profile.readOnly) return;
  const confirmed = await showUtilityConfirmation(
    "Delete Configuration",
    trFormat("Are you sure you want to delete %@?", profile.name),
  );
  if (!confirmed) return;
  await performKeyBindingProfileAction(async () => {
    await invoke("delete_key_binding_profile", { name: profile.name });
    const profiles = await invoke("list_key_binding_profiles");
    keyBindingProfiles = Array.isArray(profiles) ? profiles : [];
    const fallback = findKeyBindingProfile("IINA Default") || keyBindingProfiles[0];
    if (!fallback) throw new Error("No key binding configurations are available");
    await applyKeyBindingProfileDocument(
      await invoke("read_key_binding_profile", { name: fallback.name }),
    );
  });
}

async function revealCurrentKeyBindingProfile() {
  const profile = currentKeyBindingProfile();
  if (!profile || profile.readOnly) return;
  await performKeyBindingProfileAction(async () => {
    let path;
    try {
      path = await invoke("reveal_key_binding_profile", { name: profile.name });
    } catch {
      path = await invoke("get_key_binding_profile_path", { name: profile.name });
    }
    if (path) showOsd(`Key Bindings: ${path}`);
  });
}

function keyBindingProfileButton(label, handler) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "keybinding-button keybinding-profile-button";
  button.textContent = tr(label);
  button.addEventListener("click", () => void handler());
  return button;
}

function renderKeyBindingProfileControl(control) {
  const row = document.createElement("div");
  row.className = "pref-row pref-keybinding-profile-row";
  row.dataset.key = control.key;
  const label = document.createElement("span");
  label.className = "pref-label";
  label.textContent = trReference(control.label, control.l10n);

  const field = document.createElement("div");
  field.className = "pref-field keybinding-profile-field";
  const select = document.createElement("select");
  select.className = "pref-select keybinding-profile-select";
  select.disabled = keyBindingProfilesLoading || keyBindingProfileBusy || !keyBindingProfiles.length;
  if (!keyBindingProfiles.length) {
    const option = document.createElement("option");
    option.textContent = tr(keyBindingProfilesLoading ? "Loading…" : "No Configurations");
    select.append(option);
  } else {
    for (const profile of keyBindingProfiles) {
      const option = document.createElement("option");
      option.value = profile.name;
      option.textContent = profile.readOnly ? `${profile.name} — ${tr("Built-in")}` : profile.name;
      select.append(option);
    }
    select.value = currentKeyBindingProfile()?.name || keyBindingProfiles[0].name;
  }
  select.addEventListener("change", () => void selectKeyBindingProfile(select.value));

  const actions = document.createElement("div");
  actions.className = "keybinding-profile-actions";
  const newButton = keyBindingProfileButton("New", createKeyBindingProfile);
  const duplicateButton = keyBindingProfileButton("Duplicate", duplicateCurrentKeyBindingProfile);
  const importInput = document.createElement("input");
  importInput.type = "file";
  importInput.accept = ".conf";
  importInput.className = "keybinding-import-input";
  importInput.addEventListener("change", () => {
    const file = importInput.files?.[0];
    if (file) void importKeyBindingProfileFromFile(file);
  });
  const importButton = keyBindingProfileButton("Import", async () => {
    importInput.value = "";
    importInput.click();
  });
  const deleteButton = keyBindingProfileButton("Delete", deleteCurrentKeyBindingProfile);
  const revealButton = keyBindingProfileButton("Reveal", revealCurrentKeyBindingProfile);
  const profile = currentKeyBindingProfile();
  const actionDisabled = keyBindingProfilesLoading || keyBindingProfileBusy;
  newButton.disabled = actionDisabled;
  importButton.disabled = actionDisabled;
  duplicateButton.disabled = actionDisabled || !profile;
  deleteButton.disabled = actionDisabled || !profile || profile.readOnly;
  revealButton.disabled = actionDisabled || !profile || profile.readOnly;
  actions.append(importInput, newButton, duplicateButton, importButton, deleteButton, revealButton);

  const status = document.createElement("small");
  status.className = keyBindingProfileError
    ? "keybinding-profile-status is-error"
    : "keybinding-profile-status";
  status.textContent = keyBindingProfileError
    || (keyBindingProfilesLoading || keyBindingProfileBusy
      ? tr("Loading…")
      : profile?.readOnly
        ? tr("Built-in configurations are read-only. Duplicate one to edit it.")
        : profile?.path || "");
  field.append(select, actions, status);
  row.append(label, field);
  return row;
}

function persistCurrentKeyBindingRows(rows) {
  const profile = currentKeyBindingProfile();
  if (!profile || profile.readOnly || keyBindingProfileBusy) return Promise.resolve();
  const serializedRows = rows.map(serializeKeyBindingRow);
  const contents = generateInputConf(rows);
  const profileName = profile.name;
  const revision = ++keyBindingProfileSaveRevision;
  preferences = {
    ...preferences,
    values: {
      ...(preferences?.values || {}),
      modeledKeyBindings: serializedRows,
    },
  };
  renderKeyBindingPreferencesIfReady();
  keyBindingProfileSaveQueue = keyBindingProfileSaveQueue
    .then(async () => {
      await invoke("save_key_binding_profile", {
        name: profileName,
        contents,
      });
      const nextPreferences = await invoke("set_preference", {
        change: { key: "modeledKeyBindings", value: serializedRows },
      });
      if (revision === keyBindingProfileSaveRevision && currentKeyBindingProfileName() === profileName) {
        preferences = nextPreferences;
        keyBindingProfileError = "";
        renderKeyBindingPreferencesIfReady();
      }
    })
    .catch((error) => {
      if (revision === keyBindingProfileSaveRevision) {
        keyBindingProfileError = keyBindingProfileErrorMessage(error);
        renderKeyBindingPreferencesIfReady();
      }
    });
  return keyBindingProfileSaveQueue;
}

function renderPreferenceKeyBindings(control) {
  const rows = keyBindingRowsFromPreferences();
  const readOnly = currentKeyBindingProfileIsReadOnly();
  const conflictState = keyMappingConflictState(rows);
  const duplicates = conflictState.duplicateSignatures;
  const conflictCount = conflictState.activeIndexes.size + conflictState.shadowedIndexes.size;
  const unsupportedCount = rows.filter((row) => !isExecutableKeyBindingRow(row)).length;
  const displayRawValues = Boolean(getPreferenceValue("displayKeyBindingRawValues"));
  const displayRows = keyBindingRowsForDisplay(rows, duplicates);

  const wrapper = document.createElement("div");
  wrapper.className = readOnly ? "pref-keybindings is-read-only" : "pref-keybindings";
  wrapper.dataset.key = control.key;

  const toolbar = document.createElement("div");
  toolbar.className = "keybinding-toolbar";

  const summary = document.createElement("span");
  summary.className = "keybinding-summary";
  const visibleSummary = displayRows.length === rows.length ? "" : ` | ${displayRows.length} Shown`;
  const conflictSummary = conflictCount ? ` | ${conflictCount} Conflicts` : "";
  summary.textContent = `${rows.length} Shortcuts${conflictSummary} | ${unsupportedCount} Raw${visibleSummary}`;

  const actions = document.createElement("span");
  actions.className = "keybinding-actions";

  const addButton = document.createElement("button");
  addButton.type = "button";
  addButton.className = "keybinding-button";
  addButton.textContent = trKey("FilterWindowController", "w2g-wR-hPu.title", "Add");
  addButton.disabled = readOnly || keyBindingProfileBusy;
  addButton.addEventListener("click", () => {
    addKeyBindingRow(control.key);
  });

  const exportButton = document.createElement("button");
  exportButton.type = "button";
  exportButton.className = "keybinding-button";
  exportButton.textContent = tr("Export");
  exportButton.addEventListener("click", () => {
    void exportKeyBindings(rows);
  });

  const resolveButton = document.createElement("button");
  resolveButton.type = "button";
  resolveButton.className = "keybinding-button";
  resolveButton.textContent = tr("Keep Last");
  resolveButton.title = tr("Remove Shadowed");
  resolveButton.disabled = readOnly || keyBindingProfileBusy || conflictCount === 0;
  resolveButton.addEventListener("click", () => {
    resolveKeyBindingConflicts(control.key);
  });

  const resetButton = document.createElement("button");
  resetButton.type = "button";
  resetButton.className = "keybinding-button";
  resetButton.textContent = tr("Reload");
  resetButton.disabled = keyBindingProfileBusy || !currentKeyBindingProfile();
  resetButton.addEventListener("click", () => {
    void selectKeyBindingProfile(currentKeyBindingProfileName());
  });
  actions.append(addButton, exportButton, resolveButton, resetButton);
  toolbar.append(summary, actions);

  const filterBar = document.createElement("div");
  filterBar.className = "keybinding-filterbar";
  const filterOptions = [
    ["all", "All", rows.length],
    ["conflicts", "Conflicts", conflictCount],
    ["unsupported", "Raw", unsupportedCount],
  ];
  for (const [mode, label, count] of filterOptions) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = keyBindingFilterMode === mode ? "keybinding-filter active" : "keybinding-filter";
    button.textContent = `${tr(label)} ${count}`;
    button.addEventListener("click", () => {
      keyBindingFilterMode = mode;
      renderPreferences();
    });
    filterBar.append(button);
  }

  const table = document.createElement("div");
  table.className = "keybinding-table";
  table.setAttribute("role", "table");

  const header = document.createElement("div");
  header.className = "keybinding-row keybinding-row--header";
  header.setAttribute("role", "row");
  for (const title of ["Key", "Modifiers", "Action", ""]) {
    const cell = document.createElement("span");
    cell.setAttribute("role", "columnheader");
    cell.textContent = tr(title);
    header.append(cell);
  }
  table.append(header);

  displayRows.forEach(({ row, index }) => {
    const rowEl = document.createElement("div");
    rowEl.className = "keybinding-row";
    rowEl.setAttribute("role", "row");
    if (duplicates.has(keyBindingSignature(row))) {
      rowEl.classList.add("is-conflict");
      if (conflictState.activeIndexes.has(index)) rowEl.classList.add("is-conflict-active");
      if (conflictState.shadowedIndexes.has(index)) rowEl.classList.add("is-conflict-shadowed");
    }
    if (!isExecutableKeyBindingRow(row)) {
      rowEl.classList.add("is-unsupported");
    }

    const keyCell = document.createElement("span");
    keyCell.className = "keybinding-key-cell";
    keyCell.setAttribute("role", "cell");
    const keyInput = document.createElement("input");
    keyInput.className = "keybinding-key-input";
    keyInput.value = displayRawValues ? row.key : macOSReadableKey(row.key);
    keyInput.title = displayRawValues ? row.rawKey : `${macOSModifierSymbols(row.modifiers, row.key)}${macOSReadableKey(row.key)}`;
    keyInput.classList.toggle("is-localized", !displayRawValues);
    keyInput.disabled = readOnly || keyBindingProfileBusy;
    keyInput.setAttribute("aria-label", `${actionLabel(row.action)} key`);
    keyInput.addEventListener("keydown", (event) => {
      if (isModifierOnlyKey(event)) return;
      event.preventDefault();
      event.stopPropagation();
      const key = eventKeyToken(event);
      keyInput.value = key;
      updateKeyBindingRow(index, { key, modifiers: modifiersFromEvent(event) });
    });
    keyInput.addEventListener("change", () => {
      updateKeyBindingRow(index, { key: keyInput.value.trim() });
    });
    keyCell.append(keyInput);

    const modifierCell = document.createElement("span");
    modifierCell.className = "keybinding-modifiers";
    modifierCell.setAttribute("role", "cell");
    modifierCell.setAttribute("aria-label", modifiersLabel(row.modifiers));
    if (displayRawValues) {
      for (const [modifierKey, modifierLabel] of KEYBINDING_MODIFIERS) {
        const label = document.createElement("label");
        label.className = "keybinding-modifier";
        const input = document.createElement("input");
        input.type = "checkbox";
        input.checked = Boolean(row.modifiers[modifierKey]);
        input.disabled = readOnly || keyBindingProfileBusy;
        input.addEventListener("change", () => {
          updateKeyBindingRow(index, {
            modifiers: {
              ...row.modifiers,
              [modifierKey]: input.checked,
            },
          });
        });
        const text = document.createElement("span");
        text.textContent = tr(modifierLabel);
        label.append(input, text);
        modifierCell.append(label);
      }
    } else {
      const symbols = document.createElement("span");
      symbols.className = "keybinding-modifier-symbols";
      symbols.textContent = macOSModifierSymbols(row.modifiers, row.key) || "—";
      modifierCell.append(symbols);
    }

    const actionCell = document.createElement("span");
    actionCell.className = "keybinding-action";
    actionCell.setAttribute("role", "cell");
    const label = document.createElement("span");
    label.textContent = actionLabel(row.action);
    actionCell.append(label);
    if (duplicates.has(keyBindingSignature(row))) {
      const status = document.createElement("small");
      status.className = "keybinding-conflict-status";
      status.textContent = conflictState.activeIndexes.has(index) ? tr("Active (last wins)") : tr("Shadowed");
      actionCell.append(status);
    }
    if (displayRawValues) {
      const raw = document.createElement("code");
      raw.textContent = row.rawCommand || actionRawValue(row.action);
      actionCell.append(raw);
    }

    const operationCell = document.createElement("span");
    operationCell.className = "keybinding-row-actions";
    operationCell.setAttribute("role", "cell");
    const duplicateButton = document.createElement("button");
    duplicateButton.type = "button";
    duplicateButton.className = "keybinding-row-button";
    duplicateButton.textContent = "+";
    duplicateButton.title = tr("Duplicate");
    duplicateButton.disabled = readOnly || keyBindingProfileBusy;
    duplicateButton.addEventListener("click", () => {
      duplicateKeyBindingRow(index);
    });
    const deleteButton = document.createElement("button");
    deleteButton.type = "button";
    deleteButton.className = "keybinding-row-button";
    deleteButton.textContent = "-";
    deleteButton.title = tr("Delete");
    deleteButton.disabled = readOnly || keyBindingProfileBusy;
    deleteButton.addEventListener("click", () => {
      deleteKeyBindingRow(index);
    });
    operationCell.append(duplicateButton, deleteButton);

    rowEl.append(keyCell, modifierCell, actionCell, operationCell);
    table.append(rowEl);
  });

  wrapper.append(toolbar, filterBar, table);
  return wrapper;
}

async function exportKeyBindings(rows) {
  const source = generateInputConf(rows);
  try {
    const path = await invoke("export_key_bindings_config", {
      filename: `${sanitizedConfigFileName(getPreferenceValue("currentInputConfigName"))}.conf`,
      contents: source,
    });
    if (path) showOsd("Key Bindings Exported");
  } catch {
    showOsd("Key Bindings Export Failed");
  }
}

function sanitizedConfigFileName(name) {
  const baseName = String(name || "input")
    .trim()
    .replace(/[^A-Za-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return baseName || "input";
}

function updateKeyBindingRow(index, patch) {
  const rows = keyBindingRowsFromPreferences();
  const current = rows[index];
  if (!current) return;
  const draft = {
    ...current,
    ...patch,
    modifiers: patch.modifiers ? normalizeModifiers(patch.modifiers) : current.modifiers,
  };
  if (patch.key !== undefined || patch.modifiers) {
    draft.rawKey = mpvKeyFromParts(draft.key, draft.modifiers);
  }
  const next = normalizeKeyBindingRow(
    draft,
    index,
  );
  if (!next) return;
  rows[index] = next;
  void persistCurrentKeyBindingRows(rows);
}

function addKeyBindingRow(key = "modeledKeyBindings") {
  const rows = keyBindingRowsFromPreferences();
  rows.push(newKeyBindingRow());
  void persistCurrentKeyBindingRows(rows);
}

function duplicateKeyBindingRow(index) {
  const rows = keyBindingRowsFromPreferences();
  const current = rows[index];
  if (!current) return;
  rows.splice(index + 1, 0, {
    ...serializeKeyBindingRow(current),
    id: `duplicate-${Date.now()}-${Math.random().toString(16).slice(2)}`,
  });
  void persistCurrentKeyBindingRows(rows);
}

function deleteKeyBindingRow(index) {
  const rows = keyBindingRowsFromPreferences();
  if (!rows[index]) return;
  rows.splice(index, 1);
  void persistCurrentKeyBindingRows(rows);
}

function resolveKeyBindingConflicts(key = "modeledKeyBindings") {
  const rows = keyBindingRowsFromPreferences();
  const resolved = removeShadowedKeyMappings(rows);
  void persistCurrentKeyBindingRows(resolved);
}

function getPreferenceValue(key) {
  return preferences?.values?.[key] ?? mockPreferences.values[key] ?? "";
}

function preferenceControlEnabled(control) {
  return preferenceControlEnabledForValues(control, {
    ...mockPreferences.values,
    ...(preferences?.values || {}),
  });
}

function preferenceControlVisible(control) {
  if (!control.visibleWhen) return true;
  const [key, value] = control.visibleWhen;
  return String(getPreferenceValue(key)) === String(value);
}

function readPreferenceSelectValue(control, value) {
  const typedOption = preferenceOptions(control).find(([optionValue]) => String(optionValue) === value)?.[0];
  return typedOption ?? value;
}

function preferenceOptions(control) {
  if (control.key !== "onlineSubProvider") return control.options;
  const options = Array.from(control.options || []);
  for (const runtime of pluginRuntimes.values()) {
    for (const provider of runtime.spec.subtitle_providers || []) {
      if (!provider?.id || !provider?.name) continue;
      options.push([
        `plugin:${runtime.spec.identifier}:${provider.id}`,
        `${provider.name} - ${runtime.spec.name}`,
      ]);
    }
  }
  return options;
}

async function setPreferenceValue(key, value) {
  const nextPreferences = await invoke("set_preference", { change: { key, value } });
  if (!tauriInvoke) applyFrontendPreferenceChange(nextPreferences, key, value);
}

function applyBroadcastPreferenceChange(payload) {
  const revision = Number(payload?.revision);
  if (!Number.isSafeInteger(revision) || revision <= lastPreferenceChangeRevision) return;
  if (!payload?.preferences?.values || typeof payload.key !== "string") return;
  lastPreferenceChangeRevision = revision;
  applyFrontendPreferenceChange(payload.preferences, payload.key, payload.value);
}

function preferenceSnapshotChangedKeys(previousPreferences, nextPreferences) {
  const previousValues = previousPreferences?.values || {};
  const nextValues = nextPreferences?.values || {};
  return [...new Set([...Object.keys(previousValues), ...Object.keys(nextValues)])]
    .filter((key) => JSON.stringify(previousValues[key]) !== JSON.stringify(nextValues[key]));
}

async function reconcilePreferencesAfterListenerInstall() {
  const snapshot = await invoke("get_preference_snapshot");
  const revision = Number(snapshot?.revision);
  if (!Number.isSafeInteger(revision) || revision < lastPreferenceChangeRevision) return;
  if (!snapshot?.preferences?.values) return;
  const changedKeys = preferenceSnapshotChangedKeys(preferences, snapshot.preferences);
  preferences = snapshot.preferences;
  lastPreferenceChangeRevision = Math.max(lastPreferenceChangeRevision, revision);
  for (const key of changedKeys) {
    applyFrontendPreferenceChange(preferences, key, preferences.values[key], {
      refreshPluginRuntime: false,
    });
  }
  if (changedKeys.some((key) => ["currentInputConfigName", "modeledKeyBindings"].includes(key))) {
    await refreshKeyBindingProfiles({ loadCurrent: true });
  }
}

function applyFrontendPreferenceChange(
  nextPreferences,
  key,
  value,
  { refreshPluginRuntime = true } = {},
) {
  preferences = nextPreferences;
  if (key === "enableOSD" && !value) hideOsd(true);
  if (key === "alwaysFloatOnTop" && !value) windowAlwaysOnTopActive = false;
  if (key === "arrowBtnAction") {
    oscArrowSpeedActive = false;
    oscArrowSpeedIndex = OSC_NORMAL_SPEED_INDEX;
  }
  if (key === "musicModeShowPlaylist") miniPlaylistVisible = Boolean(value);
  if (key === "musicModeShowAlbumArt") miniVideoVisible = Boolean(value);
  if (["enableThumbnailPreview", "thumbnailWidth", "enableThumbnailForRemoteFiles"].includes(key)) {
    void invoke("cancel_media_thumbnails").catch(() => {});
    thumbnailSource = undefined;
    thumbnailSet = undefined;
    thumbnailGenerationId = 0;
    hideThumbnailPeek();
  }
  if (isPreferencesAuxiliaryWindow || !tauriInvoke) renderPreferences();
  if (isFilterAuxiliaryWindow && ["savedVideoFilters", "savedAudioFilters"].includes(key)) {
    renderFilterPanel();
  }
  if ([
    "arrowBtnAction",
    "themeMaterial",
    "oscPosition",
    "controlBarToolbarButtons",
    "controlBarStickToCenter",
    "showChapterPos",
    "showRemainingTime",
    "timeDisplayPrecision",
    "enableOSD",
    "displayTimeAndBatteryInFullScreen",
    "alwaysFloatOnTop",
    "alwaysShowOnTopIcon",
    "musicModeShowPlaylist",
    "musicModeShowAlbumArt",
    "playlistShowMetadata",
    "playlistShowMetadataInMusicMode",
  ].includes(key)) {
    if (key.startsWith("musicMode")) miniLayoutFingerprint = "";
    render(state);
  }
  if (
    refreshPluginRuntime
    && key === "iinaEnablePluginSystem"
    && !isAuxiliaryWindow
    && !isMiniPlayerWindow
  ) {
    void queuePluginRuntimeRefresh();
  }
}

async function setPreferenceValues(changes) {
  for (const [key, value] of changes) {
    preferences = await invoke("set_preference", { change: { key, value } });
  }
  renderPreferences();
}

function showOpenUrlPanel(isAlternativeAction, { enqueue = false } = {}) {
  if (!isOpenUrlAuxiliaryWindow && tauriInvoke) {
    void invoke("show_open_url_window", {
      isAlternativeAction: Boolean(isAlternativeAction),
      enqueue: Boolean(enqueue),
    }).catch((error) => {
      console.error("Unable to show the Open URL window", error);
    });
    return;
  }
  openUrlAlternativeAction = isAlternativeAction;
  openUrlEnqueueAction = enqueue;
  openUrlCredentialLookupId += 1;
  clearTimeout(openUrlCredentialLookupTimer);
  els.openUrlModal.hidden = false;
  els.urlField.value = "";
  els.urlUsername.value = "";
  els.urlPassword.value = "";
  els.urlRemember.checked = false;
  els.urlError.hidden = true;
  els.urlField.classList.remove("is-invalid");
  els.urlHttpPrefix.hidden = true;
  els.urlOpenButton.disabled = false;
  els.urlField.focus();
}

function closeOpenUrlPanel() {
  openUrlCredentialLookupId += 1;
  clearTimeout(openUrlCredentialLookupTimer);
  els.openUrlModal.hidden = true;
  if (isOpenUrlAuxiliaryWindow) {
    void invoke("hide_auxiliary_window").catch((error) => {
      console.error("Unable to hide the Open URL window", error);
    });
  }
}

async function showOnlineSubtitlePanel(providerId = null) {
  if (!state.current_url) {
    showOsd("No Media");
    return;
  }
  const requestId = ++onlineSubtitleRequestId;
  onlineSubtitleCandidates = [];
  pluginSubtitleCandidates.clear();
  selectedOnlineSubtitleId = null;
  onlineSubtitleBusy = true;
  onlineSubtitleFlowPhase = "searching";
  els.onlineSubtitlesAccessory.hidden = true;
  renderOnlineSubtitleCandidates();
  const resolvedProviderId = String(providerId || getPreferenceValue("onlineSubProvider") || ":opensubtitles");
  showOsd("Finding online subtitles…", {
    autoHide: false,
    detail: `${trKey("Localizable", "osd.find_online_sub.source", "from")} ${onlineSubtitleProviderName(resolvedProviderId)}`,
  });
  try {
    const result = await searchActiveOnlineSubtitleProvider(resolvedProviderId);
    if (requestId !== onlineSubtitleRequestId) return;
    if (result === null) {
      // IINA's CUSTOM_IMPLEMENTATION sentinel dismisses the built-in workflow without turning
      // the plugin-owned chooser into a search failure.
      resetOnlineSubtitleFlow();
      return;
    }
    onlineSubtitleCandidates = Array.from(result?.candidates || []);
    onlineSubtitleBusy = false;
    const plan = planOnlineSubtitleSearchResult(onlineSubtitleCandidates);
    onlineSubtitleFlowPhase = plan.phase;
    selectedOnlineSubtitleId = plan.selectedId;
    if (plan.effect === "empty") {
      resetOnlineSubtitleFlow();
      showOsd("No subtitles found");
    } else if (plan.effect === "download") {
      showOsd(trFormat("%d subtitles found. Downloading…", 1), { literal: true });
      await downloadSelectedOnlineSubtitles();
    } else {
      renderOnlineSubtitleCandidates();
      els.onlineSubtitlesAccessory.hidden = false;
      showOsd(trFormat("%d subtitles found. Downloading…", onlineSubtitleCandidates.length), {
        accessory: els.onlineSubtitlesAccessory,
        literal: true,
        persistent: true,
      });
    }
  } catch (error) {
    if (requestId !== onlineSubtitleRequestId) return;
    const message = onlineSubtitleErrorMessage(error);
    resetOnlineSubtitleFlow();
    showOsd(message, { literal: true });
  }
}

function onlineSubtitleProviderName(providerId) {
  if (providerId === ":opensubtitles") return "opensubtitles.com";
  if (providerId === ":assrt") return "assrt.net";
  if (providerId === ":shooter") return "shooter.cn";
  const match = /^plugin:([^:]+):(.+)$/.exec(providerId);
  if (!match) return providerId;
  const runtime = pluginRuntimes.get(match[1]);
  return runtime?.spec.subtitle_providers?.find((provider) => provider?.id === match[2])?.name
    || match[2];
}

function resetOnlineSubtitleFlow({ hideOsdSurface = true } = {}) {
  onlineSubtitleRequestId += 1;
  onlineSubtitleCandidates = [];
  pluginSubtitleCandidates.clear();
  selectedOnlineSubtitleId = null;
  onlineSubtitleBusy = false;
  onlineSubtitleFlowPhase = "idle";
  if (hideOsdSurface) hideOsd(true);
  els.onlineSubtitlesAccessory.hidden = true;
  els.onlineSubtitlesList.replaceChildren();
}

function cancelOnlineSubtitleChooser() {
  const transition = cancelOnlineSubtitleSelection(onlineSubtitleFlowPhase);
  resetOnlineSubtitleFlow();
  if (transition.effect === "canceled") showOsd("Canceled");
}

async function searchActiveOnlineSubtitleProvider(providerOverride = null) {
  const providerId = String(providerOverride || getPreferenceValue("onlineSubProvider") || ":opensubtitles");
  const match = /^plugin:([^:]+):(.+)$/.exec(providerId);
  if (!match) return invoke("search_online_subtitles", { providerId });

  const [, identifier, registeredProviderId] = match;
  const runtime = pluginRuntimes.get(identifier);
  const api = runtime?.subtitleApi;
  if (!runtime || !api) {
    throw new Error("The selected plugin subtitle provider is unavailable");
  }
  const found = await new Promise((resolve, reject) => {
    api.__invokeSearch(
      registeredProviderId,
      resolve,
      (error) => reject(new Error(String(error))),
    );
  });
  if (found === null) return null;
  if (!Array.isArray(found)) {
    throw new Error("provider.search should return an array of subtitle items.");
  }
  const candidates = found.map((item, index) => {
    if (!pluginSubtitleItemStates.has(item)) {
      throw new Error("provider.search should return an array of subtitle items.");
    }
    const description = item.desc || {};
    const id = `plugin-subtitle-${++nextPluginSubtitleCandidateId}`;
    pluginSubtitleCandidates.set(id, { runtime, item });
    return {
      id,
      name: String(description.name || `Subtitle ${index + 1}`),
      left: String(description.left || ""),
      right: String(description.right || ""),
    };
  });
  return {
    provider_id: registeredProviderId,
    provider_name: `${runtime.spec.name} - ${registeredProviderId}`,
    query: state.media_title || "",
    candidates,
  };
}

function renderOnlineSubtitleCandidates() {
  els.onlineSubtitlesList.replaceChildren();
  for (const candidate of onlineSubtitleCandidates) {
    const selected = selectedOnlineSubtitleId === candidate.id;
    const row = document.createElement("button");
    row.type = "button";
    row.className = selected ? "online-subtitle-row selected" : "online-subtitle-row";
    row.disabled = onlineSubtitleBusy;
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", String(selected));
    row.addEventListener("click", () => {
      selectedOnlineSubtitleId = selectOnlineSubtitleCandidate(onlineSubtitleCandidates, candidate.id);
      renderOnlineSubtitleCandidates();
    });
    row.addEventListener("dblclick", () => {
      selectedOnlineSubtitleId = selectOnlineSubtitleCandidate(onlineSubtitleCandidates, candidate.id);
      renderOnlineSubtitleCandidates();
      void downloadSelectedOnlineSubtitles();
    });

    const name = document.createElement("span");
    name.className = "online-subtitle-name";
    name.textContent = candidate.name || "Subtitle";
    const left = document.createElement("span");
    left.className = "online-subtitle-left";
    left.textContent = candidate.left || "";
    const right = document.createElement("span");
    right.className = "online-subtitle-right";
    right.textContent = candidate.right || "";
    row.append(left, right, name);
    els.onlineSubtitlesList.append(row);
  }
  els.onlineSubtitlesDownloadButton.disabled = onlineSubtitleBusy || selectedOnlineSubtitleId === null;
}

async function downloadSelectedOnlineSubtitles() {
  if (onlineSubtitleBusy || selectedOnlineSubtitleId === null) return;
  onlineSubtitleBusy = true;
  onlineSubtitleFlowPhase = "downloading";
  const selected = [selectedOnlineSubtitleId];
  if (!els.onlineSubtitlesAccessory.hidden) {
    els.onlineSubtitlesAccessory.hidden = true;
    hideOsd(true);
  }
  try {
    const result = selected.every((id) => pluginSubtitleCandidates.has(id))
      ? await downloadPluginSubtitleCandidates(selected)
      : await invoke("download_online_subtitles", { candidates: selected });
    setPlayerState(result?.player, { force: true });
    const downloadedPaths = Array.from(result?.downloaded_paths || []);
    resetOnlineSubtitleFlow();
    showOsd("Subtitle downloaded", {
      detail: downloadedPaths.map((path) => titleFromPath(path)).join("\n"),
    });
  } catch (error) {
    const message = onlineSubtitleErrorMessage(error);
    resetOnlineSubtitleFlow();
    showOsd(message, { literal: true });
  }
}

async function downloadPluginSubtitleCandidates(ids) {
  const records = ids.map((id) => pluginSubtitleCandidates.get(id)).filter(Boolean);
  const identifier = records[0]?.runtime.spec.identifier;
  if (!identifier || records.some((record) => record.runtime.spec.identifier !== identifier)) {
    throw new Error("Select subtitles from one plugin provider at a time");
  }
  const urls = [];
  for (const record of records) {
    const downloaded = await downloadPluginSubtitleItem(record.item);
    urls.push(...pluginSubtitleDownloadUrlsForNativeBoundary(downloaded));
  }
  return invoke("download_plugin_subtitles", { identifier, urls });
}

async function saveDownloadedSubtitle() {
  try {
    const path = await invoke("save_downloaded_subtitle_dialog");
    if (path) showOsd("Subtitle saved");
  } catch (error) {
    showOsd(onlineSubtitleErrorMessage(error));
  }
}

function onlineSubtitleErrorMessage(error) {
  const message = String(error?.message || error || "Unable to search online subtitles").trim();
  if (message.startsWith("OPEN_SUBTITLES_LOGIN:")) return tr("Cannot login");
  if (message.startsWith("OPEN_SUBTITLES_INVALID_TOKEN:")) return tr("Network error");
  if (message.startsWith("ONLINE_SUBTITLE_CANNOT_CONNECT:")) return tr("Cannot connect");
  if (message.startsWith("ONLINE_SUBTITLE_NETWORK_ERROR:")) return tr("Network error");
  if (message.startsWith("ONLINE_SUBTITLE_TIMED_OUT:")) return tr("Timed Out");
  if (message.startsWith("ONLINE_SUBTITLE_FILE_ERROR:")) return tr("Error reading file");
  if (message.startsWith("ONLINE_SUBTITLE_CANCELED:")) return tr("Canceled");
  return message || "Unable to search online subtitles";
}

async function submitOpenUrlPanel() {
  const result = normalizedOpenUrl();
  if (!result.ok) {
    els.urlError.hidden = false;
    els.urlField.classList.add("is-invalid");
    els.urlOpenButton.disabled = true;
    return;
  }
  if (els.urlRemember.checked && els.urlUsername.value) {
    try {
      await invoke("write_http_auth_credentials", {
        url: result.url,
        username: els.urlUsername.value,
        password: els.urlPassword.value,
      });
    } catch {
      // IINA ignores Keychain failures and still opens the URL.
    }
  }
  closeOpenUrlPanel();
  try {
    if (tauriInvoke) {
      const submission = await invoke("submit_open_url", {
        url: result.url,
        isAlternativeAction: Boolean(openUrlAlternativeAction),
        enqueue: Boolean(openUrlEnqueueAction),
      });
      if (submission?.player) {
        setPlayerState(submission.player, { force: true, presentOsd: true });
      }
    } else if (openUrlEnqueueAction) {
      setPlayerState(await invoke("enqueue_media_paths", { paths: [result.url] }), { force: true, presentOsd: true });
    } else {
      await openMediaForMenuAction(result.url, openUrlAlternativeAction);
    }
  } catch {
    showOsd("Open Failed");
  }
}

function updateOpenUrlValidation({ loadCredentials = false } = {}) {
  const raw = els.urlField.value.trim();
  if (!raw) {
    els.urlError.hidden = true;
    els.urlField.classList.remove("is-invalid");
    els.urlHttpPrefix.hidden = true;
    els.urlOpenButton.disabled = false;
    if (loadCredentials) {
      openUrlCredentialLookupId += 1;
      clearTimeout(openUrlCredentialLookupTimer);
      els.urlUsername.value = "";
      els.urlPassword.value = "";
    }
    return;
  }
  const result = normalizedOpenUrl();
  els.urlError.hidden = result.ok;
  els.urlField.classList.toggle("is-invalid", !result.ok);
  els.urlHttpPrefix.hidden = !result.ok || result.hasScheme;
  els.urlOpenButton.disabled = !result.ok;
  if (loadCredentials) {
    if (result.ok) scheduleOpenUrlCredentialLookup(result.url);
    else {
      openUrlCredentialLookupId += 1;
      clearTimeout(openUrlCredentialLookupTimer);
    }
  }
}

function scheduleOpenUrlCredentialLookup(url) {
  const lookupId = ++openUrlCredentialLookupId;
  const key = openUrlCredentialKey(url);
  clearTimeout(openUrlCredentialLookupTimer);
  els.urlUsername.value = "";
  els.urlPassword.value = "";
  openUrlCredentialLookupTimer = setTimeout(async () => {
    let credentials = null;
    try {
      credentials = await invoke("read_http_auth_credentials", { url });
    } catch {
      credentials = null;
    }
    const current = normalizedOpenUrl();
    if (
      lookupId !== openUrlCredentialLookupId ||
      els.openUrlModal.hidden ||
      !current.ok ||
      openUrlCredentialKey(current.url) !== key
    ) {
      return;
    }
    els.urlUsername.value = credentials?.username || "";
    els.urlPassword.value = credentials?.password || "";
  }, 100);
}

function openUrlCredentialKey(url) {
  try {
    const parsed = new URL(url);
    return `${parsed.hostname.toLowerCase()}\n${parsed.port}`;
  } catch {
    return "";
  }
}

function normalizedOpenUrl() {
  const raw = els.urlField.value.trim();
  if (!raw) return { ok: false };
  const hasScheme = /^[A-Za-z][A-Za-z0-9+.-]*:/.test(raw);
  const value = hasScheme ? raw : `http://${raw}`;
  try {
    const url = new URL(value);
    if (!url.host) return { ok: false };
    if (els.urlUsername.value) {
      url.username = els.urlUsername.value;
      if (els.urlPassword.value) {
        url.password = els.urlPassword.value;
      }
    }
    return { ok: true, url: url.toString(), hasScheme };
  } catch {
    return { ok: false };
  }
}

function handleDragEnter(event) {
  if (hasSupportedDropData(event.dataTransfer)) {
    event.preventDefault();
    els.app.classList.add("is-drop-target");
    updatePlaylistDropReveal(event.clientX, htmlDropHasPlayableFiles(event.dataTransfer));
  }
}

function handleDragOver(event) {
  if (hasSupportedDropData(event.dataTransfer)) {
    event.preventDefault();
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = "copy";
    }
    els.app.classList.add("is-drop-target");
    updatePlaylistDropReveal(event.clientX, htmlDropHasPlayableFiles(event.dataTransfer));
  }
}

function handleDragLeave(event) {
  if (!event.relatedTarget || !document.documentElement.contains(event.relatedTarget)) {
    els.app.classList.remove("is-drop-target");
    finishPlaylistDropReveal();
  }
}

async function handleDrop(event) {
  event.preventDefault();
  els.app.classList.remove("is-drop-target");
  const droppedData = dropTargetData(event.dataTransfer);
  const x = event.clientX;
  const y = event.clientY;
  try {
    await playlistDropRevealPromise;
    const destination = playlistInsertionIndexAtPoint(x, y);
    const resolvedTargets = playlistDropTargets({
      ...droppedData,
      allowAbsolutePathText: destination === null,
    });
    if (!resolvedTargets.length) return;
    if (destination !== null) {
      event.stopPropagation();
      await insertPlaylistItems(resolvedTargets, destination);
      return;
    }
    setPlayerState(await invoke("open_dropped_media_paths", { paths: resolvedTargets }), { force: true, presentOsd: true });
  } finally {
    finishPlaylistDropReveal();
  }
}

async function handleNativeFileDrop(payload) {
  const targets = Array.isArray(payload?.paths) ? payload.paths.filter(Boolean) : [];
  const x = Number(payload?.position?.x);
  const y = Number(payload?.position?.y);
  try {
    if (!targets.length) return;
    await playlistDropRevealPromise;
    const destination = Number.isFinite(x) && Number.isFinite(y)
      ? playlistInsertionIndexAtPoint(x, y)
      : null;
    if (destination !== null) {
      await insertPlaylistItems(targets, destination);
      return;
    }
    setPlayerState(await invoke("open_dropped_media_paths", { paths: targets }), { force: true, presentOsd: true });
  } finally {
    els.app.classList.remove("is-drop-target");
    finishPlaylistDropReveal();
  }
}

function handleNativeFileDrag(payload) {
  const phase = String(payload?.phase || "");
  if (phase === "leave") {
    els.app.classList.remove("is-drop-target");
    finishPlaylistDropReveal();
    return;
  }
  if (phase === "enter") {
    els.app.classList.toggle("is-drop-target", Boolean(payload?.accepted));
    playlistDropRevealHasPlayableFiles = Boolean(payload?.has_playable_files);
  }
  const x = Number(payload?.position?.x);
  if (Number.isFinite(x)) {
    updatePlaylistDropReveal(x);
  }
}

function htmlDropHasPlayableFiles(dataTransfer) {
  if (!dataTransfer) return false;
  const paths = Array.from(dataTransfer.files ?? [])
    .map((file) => file.path || file.webkitRelativePath || file.name)
    .filter(Boolean);
  if (paths.length) return paths.some(playlistDropPathMayBePlayable);
  return dropTargets(dataTransfer, { allowAbsolutePathText: true }).some(playlistDropPathMayBePlayable);
}

function updatePlaylistDropReveal(pointerX, hasPlayableFiles = playlistDropRevealHasPlayableFiles) {
  playlistDropRevealPointerX = Number(pointerX);
  playlistDropRevealHasPlayableFiles = Boolean(hasPlayableFiles);
  const shouldReveal = playlistDropShouldReveal({
    pointerX: playlistDropRevealPointerX,
    viewportWidth: window.innerWidth,
    hasPlayableFiles: playlistDropRevealHasPlayableFiles,
    miniPlayer: isMiniPlayerWindow || state.mode !== "player",
    playlistVisible: state.sidebar.visible && state.sidebar.tab === "playlist",
  });
  if (!shouldReveal) {
    clearTimeout(playlistDropRevealTimer);
    playlistDropRevealTimer = undefined;
    return;
  }
  if (playlistDropRevealTimer || playlistDropRevealAutoOpened) return;
  const generation = ++playlistDropRevealGeneration;
  playlistDropRevealTimer = setTimeout(() => {
    playlistDropRevealTimer = undefined;
    if (generation !== playlistDropRevealGeneration) return;
    if (!playlistDropShouldReveal({
      pointerX: playlistDropRevealPointerX,
      viewportWidth: window.innerWidth,
      hasPlayableFiles: playlistDropRevealHasPlayableFiles,
      miniPlayer: isMiniPlayerWindow || state.mode !== "player",
      playlistVisible: state.sidebar.visible && state.sidebar.tab === "playlist",
    })) return;
    playlistDropRevealAutoOpened = true;
    playlistDropRevealPromise = command({ type: "show-sidebar", tab: "playlist" }).catch(() => {
      playlistDropRevealAutoOpened = false;
    });
  }, PLAYLIST_DROP_REVEAL_DELAY_MS);
}

function finishPlaylistDropReveal() {
  clearTimeout(playlistDropRevealTimer);
  playlistDropRevealTimer = undefined;
  playlistDropRevealGeneration += 1;
  playlistDropRevealHasPlayableFiles = false;
  playlistDropRevealPointerX = Number.NaN;
  if (!playlistDropRevealAutoOpened) {
    playlistDropRevealPromise = Promise.resolve();
    return;
  }
  playlistDropRevealAutoOpened = false;
  const reveal = playlistDropRevealPromise;
  playlistDropRevealPromise = Promise.resolve();
  void reveal.finally(() => {
    if (state.sidebar.visible && state.sidebar.tab === "playlist") {
      void command({ type: "hide-sidebar" });
    }
  });
}

function playlistInsertionIndexAtPoint(x, y) {
  if (!state.sidebar.visible || state.sidebar.tab !== "playlist") return null;
  const bounds = els.sidebarContent.getBoundingClientRect();
  if (x < bounds.left || x > bounds.right || y < bounds.top || y > bounds.bottom) return null;
  const rowRects = [...els.sidebarContent.querySelectorAll(".sidebar-row")]
    .map((row) => row.getBoundingClientRect());
  return playlistRowInsertionIndex(rowRects, y, state.playlist.length);
}

async function insertPlaylistItems(paths, destination) {
  const nextState = await invoke("playlist_insert_items", { paths, destination });
  setPlayerState(nextState, { force: true, presentOsd: true });
}

function hasSupportedDropData(dataTransfer) {
  if (!dataTransfer) return false;
  const types = Array.from(dataTransfer.types ?? []);
  return Boolean(dataTransfer.files?.length) || types.includes("text/uri-list") || types.includes("text/plain");
}

function dropTargets(dataTransfer, options = {}) {
  return playlistDropTargets({ ...dropTargetData(dataTransfer), ...options });
}

function dropTargetData(dataTransfer) {
  if (!dataTransfer) return {};
  return {
    filePaths: Array.from(dataTransfer.files ?? [])
      .map((file) => file.path || file.webkitRelativePath)
      .filter(Boolean),
    uriList: dataTransfer.getData("text/uri-list"),
    text: dataTransfer.getData("text/plain"),
  };
}

function sidebarWidthForTab(tab) {
  if (tab === "playlist" || tab === "chapters") {
    const preferred = Number(getPreferenceValue("playlistWidth")) || 270;
    return Math.min(400, Math.max(240, preferred));
  }
  return 360;
}

function renderSidebar(nextState) {
  hidePlaylistContextMenu();
  els.sidebarContent.oncontextmenu = null;
  els.sidebar.style.width = `${sidebarWidthForTab(nextState.sidebar.tab)}px`;
  updatePluginSidebarTabVisibility(nextState);
  const pluginRuntime = activePluginSidebarId ? pluginRuntimes.get(activePluginSidebarId) : null;
  if (pluginRuntime?.sidebar) {
    document.querySelectorAll("[data-sidebar-tab]").forEach((button) => {
      button.classList.remove("active");
    });
    els.sidebarTabs
      ?.querySelector(`[data-plugin-sidebar="${CSS.escape(pluginRuntime.spec.identifier)}"]`)
      ?.classList.add("active");
    els.sidebarContent.replaceChildren(pluginRuntime.sidebar);
    return;
  }
  activePluginSidebarId = null;
  document.querySelectorAll("[data-sidebar-tab]").forEach((button) => {
    button.classList.toggle("active", button.dataset.sidebarTab === nextState.sidebar.tab);
  });
  els.sidebarContent.replaceChildren();
  const tab = nextState.sidebar.tab;
  if (tab === "playlist") {
    renderPlaylistList(nextState, "Playlist is empty");
  } else if (tab === "chapters") {
    renderChapterList(nextState.chapters, "No chapters");
  } else if (tab === "video") {
    renderQuickSettings(nextState, "video");
    renderTrackList(
      quickSettingsTrackRows(nextState.tracks.video),
      "video",
      "No video tracks",
      { heading: "Video track", enabled: Boolean(nextState.current_url) }
    );
  } else if (tab === "audio") {
    renderQuickSettings(nextState, "audio");
    renderTrackList(
      quickSettingsTrackRows(nextState.tracks.audio),
      "audio",
      "No audio tracks",
      { heading: "Audio track", enabled: Boolean(nextState.current_url) }
    );
  } else {
    renderQuickSettings(nextState, "subtitles");
    const sections = subtitleTrackSections(nextState.tracks.subtitles, nextState.second_subtitle_id);
    renderTrackList(sections.primary, "subtitles", "No subtitles", {
      heading: "Subtitle:",
      enabled: Boolean(nextState.current_url),
    });
    const swap = renderQuickSettingsAction(
      "",
      Boolean(nextState.current_url) && sections.canSwap,
      () => command({ type: "swap-subtitle-tracks" })
    );
    const primarySubtitleLabel = tr("Subtitle:").replace(/[:：]\s*$/, "");
    const secondarySubtitleLabel = tr("Secondary subtitle:").replace(/[:：]\s*$/, "");
    swap.textContent = `${primarySubtitleLabel} ⇄ ${secondarySubtitleLabel}`;
    swap.setAttribute("aria-label", swap.textContent);
    swap.classList.add("sidebar-subtitle-swap");
    els.sidebarContent.append(swap);
    renderTrackList(sections.secondary, "second-subtitles", "No subtitles", {
      heading: "Secondary subtitle:",
      enabled: Boolean(nextState.current_url),
    });
  }
}

function renderQuickSettings(nextState, tab) {
  const settings = nextState.quick_settings || {};
  const enabled = Boolean(nextState.current_url);
  const panel = document.createElement("section");
  panel.className = "sidebar-quick-settings";

  if (tab === "video") {
    panel.append(
      renderQuickSettingsCheckbox("Hardware decoding", Boolean(settings.hardware_decoding), enabled, (checked) => ({
        type: "set-hardware-decoding",
        enabled: checked,
        decoder: Number(getPreferenceValue("hardwareDecoder")),
      })),
      renderQuickSettingsCheckbox(
        "HDR",
        Boolean(settings.hdr_available && settings.hdr_enabled),
        enabled && Boolean(settings.hdr_available),
        (checked) => ({ type: "set-hdr-enabled", enabled: checked })
      ),
      renderQuickSettingsCheckbox("Deinterlace", Boolean(settings.deinterlace), enabled, (checked) => ({
        type: "set-deinterlace",
        enabled: checked,
      })),
      renderQuickSettingsSpeed(nextState.speed, enabled),
      renderQuickSettingsSegments(
        "Aspect",
        ["Default", "4:3", "16:9", "16:10", "21:9", "5:4"],
        settings.video_aspect || "Default",
        enabled,
        (aspect) => ({ type: "set-video-aspect", aspect }),
        (aspect) => aspect
      ),
      renderQuickSettingsText(
        "Custom aspect",
        settings.video_aspect === "Default" ? "" : settings.video_aspect,
        "4:3",
        enabled,
        (aspect) => ({ type: "set-video-aspect", aspect })
      ),
      renderQuickSettingsSegments(
        "Crop",
        ["None", "4:3", "16:9", "16:10", "21:9", "5:4"],
        settings.video_crop || "None",
        enabled,
        (crop) => ({ type: "set-video-crop", crop }),
        (crop) => crop
      ),
      renderQuickSettingsAction("Custom Crop", enabled && Boolean(selectedVideoDimensions(nextState)), startCustomCropEditor),
      renderQuickSettingsSegments(
        "Rotation",
        [0, 90, 180, 270],
        Number(settings.video_rotate) || 0,
        enabled,
        (degrees) => ({ type: "set-video-rotate", degrees }),
        (degrees) => `${degrees}°`
      )
    );
    for (const [option, label] of [
      ["brightness", "Brightness"],
      ["contrast", "Contrast"],
      ["saturation", "Saturation"],
      ["gamma", "Gamma"],
      ["hue", "Hue"],
    ]) {
      panel.append(
        renderQuickSettingsRange(label, settings[option], -100, 100, 1, enabled, (value) => ({
          type: "set-video-equalizer",
          option,
          value,
        }), true)
      );
    }
  } else if (tab === "audio") {
    panel.append(
      renderQuickSettingsAction("Load External Audio", enabled, () => loadExternalTrackFromNativeDialog("audio")),
      renderQuickSettingsRange("Audio delay", settings.audio_delay, -10, 10, 0.05, enabled, (seconds) => ({
        type: "set-audio-delay",
        seconds,
      }), true, "s"),
      renderAudioEqualizer(settings.audio_eq, Boolean(settings.audio_eq_active), enabled)
    );
  } else {
    panel.append(
      renderQuickSettingsAction("Load External Subtitle", enabled, () => loadExternalTrackFromNativeDialog("subtitles")),
      renderQuickSettingsRange("Subtitle delay", settings.sub_delay, -10, 10, 0.05, enabled, (seconds) => ({
        type: "set-sub-delay",
        seconds,
      }), true, "s"),
      renderSubtitleScale(settings.sub_scale, enabled),
      renderQuickSettingsRange("Subtitle position", settings.sub_pos, 0, 100, 1, enabled, (position) => ({
        type: "set-sub-position",
        position,
      }), false, "%"),
      renderSubtitleTextStyle(nextState, settings, enabled)
    );
  }

  els.sidebarContent.append(panel);
}

function renderQuickSettingsAction(label, enabled, action) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "sidebar-quick-action";
  button.textContent = tr(label);
  button.disabled = !enabled;
  button.addEventListener("click", () => void action());
  return button;
}

function renderQuickSettingsCheckbox(label, checked, enabled, payloadForValue) {
  const row = document.createElement("label");
  row.className = "sidebar-quick-checkbox";
  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = checked;
  input.disabled = !enabled;
  input.addEventListener("change", () => command(payloadForValue(input.checked)));
  const text = document.createElement("span");
  text.textContent = tr(label);
  row.append(input, text);
  return row;
}

function renderQuickSettingsSegments(label, values, selected, enabled, payloadForValue, labelForValue) {
  const row = document.createElement("div");
  row.className = "sidebar-quick-segments-row";
  const title = document.createElement("span");
  title.className = "sidebar-quick-label";
  title.textContent = tr(label);
  const segments = document.createElement("div");
  segments.className = "sidebar-quick-segments";
  segments.classList.toggle("wide", values.length > 4);
  for (const value of values) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = localizedQuickSettingsValue(labelForValue(value));
    button.disabled = !enabled;
    button.classList.toggle("active", value === selected);
    button.setAttribute("aria-pressed", String(value === selected));
    button.addEventListener("click", () => command(payloadForValue(value)));
    segments.append(button);
  }
  row.append(title, segments);
  return row;
}

function renderQuickSettingsText(label, value, placeholder, enabled, payloadForValue) {
  const row = document.createElement("label");
  row.className = "sidebar-quick-text";
  const title = document.createElement("span");
  title.className = "sidebar-quick-label";
  title.textContent = tr(label);
  const input = document.createElement("input");
  input.type = "text";
  input.value = value || "";
  input.placeholder = tr(placeholder);
  input.disabled = !enabled;
  input.addEventListener("change", () => command(payloadForValue(input.value)));
  row.append(title, input);
  return row;
}

function renderQuickSettingsSpeed(rawSpeed, enabled) {
  const row = document.createElement("label");
  row.className = "sidebar-quick-speed";
  const title = document.createElement("span");
  title.className = "sidebar-quick-label";
  title.textContent = trKey("QuickSettingViewController", "1KQ-oZ-A2x.title", "Speed:");
  const speed = Number(rawSpeed);
  const normalizedSpeed = Number.isFinite(speed) && speed > 0 ? speed : 1;
  const slider = document.createElement("input");
  slider.type = "range";
  slider.min = "0";
  slider.max = "24";
  slider.step = "1";
  slider.value = String(Math.max(0, Math.min(24, Math.log(normalizedSpeed / 0.25) / Math.log(64) * 24)));
  slider.disabled = !enabled;
  slider.addEventListener("change", () => {
    const value = 0.25 * Math.pow(64, Number(slider.value) / 24);
    command({ type: "set-speed", speed: value });
  });
  const input = document.createElement("input");
  input.type = "number";
  input.min = "0.01";
  input.step = "0.01";
  input.value = String(normalizedSpeed);
  input.disabled = !enabled;
  input.addEventListener("change", () => {
    const value = input.value.trim() === "" ? 1 : Number(input.value);
    if (Number.isFinite(value) && value > 0) {
      command({ type: "set-speed", speed: value });
    } else {
      input.value = "1";
      command({ type: "set-speed", speed: 1 });
    }
  });
  const suffix = document.createElement("span");
  suffix.textContent = "x";
  row.append(title, slider, input, suffix);
  return row;
}

function subtitleScaleSliderValue(rawScale) {
  const scale = Math.max(0.1, Math.min(10, Number(rawScale) || 1));
  const display = scale >= 1 ? scale : -1 / scale;
  return display > 0 ? display - 1 : display + 1;
}

function subtitleScaleFromSliderValue(rawValue) {
  const value = Number(rawValue);
  if (!Number.isFinite(value)) return 1;
  if (value > 0) return Math.round((value + 1) * 20) / 20;
  const mapped = Math.round((value - 1) * 20) / 20;
  return -1 / mapped;
}

function renderSubtitleScale(rawScale, enabled) {
  const row = document.createElement("div");
  row.className = "sidebar-quick-sub-scale";
  const title = document.createElement("span");
  title.className = "sidebar-quick-label";
  title.textContent = tr("Subtitle scale");
  const slider = document.createElement("input");
  slider.type = "range";
  slider.min = "-9";
  slider.max = "9";
  slider.step = "0.05";
  slider.value = String(subtitleScaleSliderValue(rawScale));
  slider.disabled = !enabled;
  const output = document.createElement("output");
  const setOutput = (scale) => { output.textContent = `${scale.toFixed(2)}x`; };
  setOutput(Math.max(0.1, Math.min(10, Number(rawScale) || 1)));
  slider.addEventListener("input", () => setOutput(subtitleScaleFromSliderValue(slider.value)));
  slider.addEventListener("change", () => command({ type: "set-sub-scale", scale: subtitleScaleFromSliderValue(slider.value) }));
  const reset = document.createElement("button");
  reset.type = "button";
  reset.textContent = tr("Reset");
  reset.disabled = !enabled || Math.abs((Number(rawScale) || 1) - 1) < 0.0001;
  reset.addEventListener("click", () => command({ type: "set-sub-scale", scale: 1 }));
  row.append(title, slider, output, reset);
  return row;
}

function renderAudioEqualizer(rawGains, active, enabled) {
  const gains = Array.from(rawGains || []).slice(0, 10).map((gain) => Number(gain) || 0);
  while (gains.length < 10) gains.push(0);
  const panel = document.createElement("section");
  panel.className = "sidebar-audio-eq";
  const header = document.createElement("div");
  header.className = "sidebar-audio-eq-header";
  const title = document.createElement("span");
  title.className = "sidebar-quick-label";
  title.textContent = "Audio Equalizer";
  const reset = document.createElement("button");
  reset.type = "button";
  reset.className = "sidebar-audio-eq-reset";
  reset.textContent = "Reset";
  reset.disabled = !enabled || !active;
  reset.addEventListener("click", () => command({ type: "reset-audio-equalizer" }));
  header.append(title, reset);

  const bands = document.createElement("div");
  bands.className = "sidebar-audio-eq-bands";
  const labels = ["31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];
  for (const [index, label] of labels.entries()) {
    const band = document.createElement("label");
    band.className = "sidebar-audio-eq-band";
    const input = document.createElement("input");
    input.type = "range";
    input.min = "-12";
    input.max = "12";
    input.step = "0.1";
    input.value = String(Math.max(-12, Math.min(12, gains[index])));
    input.disabled = !enabled;
    input.setAttribute("aria-label", `${label} Hz gain`);
    input.addEventListener("change", () => {
      gains[index] = Number(input.value);
      command({ type: "set-audio-equalizer", gains });
    });
    const frequency = document.createElement("span");
    frequency.textContent = label;
    band.append(input, frequency);
    bands.append(band);
  }
  panel.append(header, bands);
  return panel;
}

function renderSubtitleTextStyle(nextState, settings, enabled) {
  const activeTrack = nextState.tracks.subtitles?.find((track) => track.selected && Number(track.id) !== 0);
  const styleEnabled = enabled && subtitleTextStyleAvailable(activeTrack);
  const panel = document.createElement("section");
  panel.className = "sidebar-subtitle-style";
  const title = document.createElement("span");
  title.className = "sidebar-quick-label";
  title.textContent = tr("Text Style:");
  panel.append(
    title,
    renderQuickSettingsColor("Text color", settings.sub_text_color, styleEnabled, "text"),
    renderQuickSettingsSelect("Text size", [30, 35, 40, 45, 50, 55, 60, 65, 70], settings.sub_text_size, styleEnabled, (size) => ({
      type: "set-subtitle-text-size",
      size,
    })),
    renderSubtitleFontPicker(settings.sub_font, styleEnabled),
    renderQuickSettingsColor("Border color", settings.sub_border_color, styleEnabled, "border"),
    renderQuickSettingsSelect("Border width", [0, 0.25, 0.5, 1, 1.5, 2, 2.5, 3, 4, 5], settings.sub_border_size, styleEnabled, (size) => ({
      type: "set-subtitle-border-size",
      size,
    })),
    renderQuickSettingsColor("Background", settings.sub_background_color, styleEnabled, "background")
  );
  return panel;
}

function renderSubtitleFontPicker(font, enabled) {
  const row = document.createElement("div");
  row.className = "sidebar-quick-font";
  const title = document.createElement("span");
  title.className = "sidebar-quick-label";
  title.textContent = tr("Font");
  const button = document.createElement("button");
  button.type = "button";
  button.disabled = !enabled;
  button.textContent = font || tr("Choose a Font");
  button.title = tr("Choose a Font");
  button.addEventListener("click", chooseSubtitleFontFromNativeDialog);
  row.append(title, button);
  return row;
}

function selectedVideoDimensions(nextState = state) {
  const track = nextState.tracks.video?.find((candidate) => candidate.selected) || nextState.tracks.video?.[0];
  const width = Number(track?.metadata?.demux_width);
  const height = Number(track?.metadata?.demux_height);
  return Number.isFinite(width) && Number.isFinite(height) && width > 0 && height > 0 ? { width, height } : null;
}

function activeDelogoFilter(nextState = state) {
  return (nextState.video_filters || []).find((filter) =>
    filter?.label === "iina_delogo"
      || String(filter?.string_format || filter?.stringFormat || "").startsWith("@iina_delogo:")
  ) || null;
}

function startCustomCropEditor() {
  const dimensions = selectedVideoDimensions();
  if (!dimensions || customCropEditor) return;
  const saved = state.quick_settings?.custom_crop;
  const validSaved = saved
    && Number.isFinite(Number(saved.x))
    && Number.isFinite(Number(saved.y))
    && Number.isFinite(Number(saved.width))
    && Number.isFinite(Number(saved.height));
  const crop = validSaved
    ? clampCropRect({
      x: Math.trunc(Number(saved.x)),
      y: Math.trunc(Number(saved.y)),
      width: Math.trunc(Number(saved.width)),
      height: Math.trunc(Number(saved.height)),
    }, dimensions)
    : { x: 0, y: 0, width: dimensions.width, height: dimensions.height };
  customCropEditor = {
    mode: "crop",
    empty: false,
    dimensions,
    crop,
    drag: null,
    root: null,
    canvas: null,
    selection: null,
    masks: [],
    label: null,
    done: null,
  };
  renderCustomCropEditor();
}

function startCustomDelogoEditor() {
  const dimensions = selectedVideoDimensions();
  if (!dimensions || customCropEditor) {
    if (!dimensions) showOsd("Delogo unavailable for current video");
    return;
  }
  customCropEditor = {
    mode: "delogo",
    empty: true,
    dimensions,
    crop: { x: 0, y: 0, width: 1, height: 1 },
    drag: null,
    root: null,
    canvas: null,
    selection: null,
    masks: [],
    label: null,
    done: null,
  };
  renderCustomCropEditor();
}

function closeCustomCropEditor() {
  customCropEditor?.root?.remove();
  customCropEditor = null;
}

function layoutCustomCropEditor() {
  const editor = customCropEditor;
  if (!editor?.canvas) return;
  const rect = cropEditorCanvasRect(editor);
  Object.assign(editor.canvas.style, {
    left: `${rect.left}px`,
    top: `${rect.top}px`,
    width: `${rect.width}px`,
    height: `${rect.height}px`,
  });
}

function cropEditorCanvasRect(editor) {
  const stageRect = els.videoStage.getBoundingClientRect();
  const sourceAspect = editor.dimensions.width / editor.dimensions.height;
  const stageAspect = stageRect.width / Math.max(1, stageRect.height);
  let width = stageRect.width;
  let height = stageRect.height;
  if (stageAspect > sourceAspect) {
    height = stageRect.height;
    width = height * sourceAspect;
  } else {
    width = stageRect.width;
    height = width / sourceAspect;
  }
  return { left: (stageRect.width - width) / 2, top: (stageRect.height - height) / 2, width, height };
}

function cropEditorPoint(event, editor) {
  const rect = editor.canvas.getBoundingClientRect();
  const x = Math.max(0, Math.min(editor.dimensions.width, (event.clientX - rect.left) / rect.width * editor.dimensions.width));
  const y = Math.max(0, Math.min(editor.dimensions.height, (event.clientY - rect.top) / rect.height * editor.dimensions.height));
  return { x, y };
}

function clampCropRect(crop, dimensions) {
  const x = Math.max(0, Math.min(dimensions.width - 1, Math.trunc(crop.x)));
  const y = Math.max(0, Math.min(dimensions.height - 1, Math.trunc(crop.y)));
  const width = Math.max(1, Math.min(dimensions.width - x, Math.trunc(crop.width)));
  const height = Math.max(1, Math.min(dimensions.height - y, Math.trunc(crop.height)));
  return { x, y, width, height };
}

function cropEditorHitEdge(point, crop, editor) {
  const rect = editor.canvas.getBoundingClientRect();
  const edge = Math.max(2, 5 / Math.max(rect.width / editor.dimensions.width, rect.height / editor.dimensions.height));
  if (Math.abs(point.y - crop.y) <= edge && point.x >= crop.x && point.x <= crop.x + crop.width) return "top";
  if (Math.abs(point.y - (crop.y + crop.height)) <= edge && point.x >= crop.x && point.x <= crop.x + crop.width) return "bottom";
  if (Math.abs(point.x - crop.x) <= edge && point.y >= crop.y && point.y <= crop.y + crop.height) return "left";
  if (Math.abs(point.x - (crop.x + crop.width)) <= edge && point.y >= crop.y && point.y <= crop.y + crop.height) return "right";
  return null;
}

function updateCustomCropEditor() {
  const editor = customCropEditor;
  if (!editor) return;
  const { crop, dimensions } = editor;
  const left = crop.x / dimensions.width * 100;
  const top = crop.y / dimensions.height * 100;
  const width = crop.width / dimensions.width * 100;
  const height = crop.height / dimensions.height * 100;
  const hidden = editor.mode === "delogo" && editor.empty;
  editor.selection.hidden = hidden;
  Object.assign(editor.selection.style, { left: `${left}%`, top: `${top}%`, width: `${width}%`, height: `${height}%` });
  const [topMask, rightMask, bottomMask, leftMask] = editor.masks;
  for (const mask of editor.masks) mask.hidden = hidden;
  Object.assign(topMask.style, { left: "0", top: "0", width: "100%", height: `${top}%` });
  Object.assign(rightMask.style, { left: `${left + width}%`, top: `${top}%`, width: `${100 - left - width}%`, height: `${height}%` });
  Object.assign(bottomMask.style, { left: "0", top: `${top + height}%`, width: "100%", height: `${100 - top - height}%` });
  Object.assign(leftMask.style, { left: "0", top: `${top}%`, width: `${left}%`, height: `${height}%` });
  editor.label.textContent = hidden ? "" : `(${crop.x}, ${crop.y}) (${crop.width}x${crop.height})`;
  if (editor.done) editor.done.disabled = hidden;
}

function setCustomCropAspect(aspect) {
  const editor = customCropEditor;
  if (!editor || editor.mode !== "crop") return;
  const sourceAspect = editor.dimensions.width / editor.dimensions.height;
  const width = sourceAspect > aspect ? Math.trunc(editor.dimensions.height * aspect) : editor.dimensions.width;
  const height = sourceAspect > aspect ? editor.dimensions.height : Math.trunc(editor.dimensions.width / aspect);
  editor.crop = { x: Math.trunc((editor.dimensions.width - width) / 2), y: Math.trunc((editor.dimensions.height - height) / 2), width, height };
  updateCustomCropEditor();
}

async function commitCustomCropEditor() {
  const editor = customCropEditor;
  if (!editor || (editor.mode === "delogo" && editor.empty)) return;
  const crop = clampCropRect(editor.crop, editor.dimensions);
  const type = editor.mode === "delogo" ? "set-delogo-region" : "set-custom-video-crop";
  closeCustomCropEditor();
  await command({ type, ...crop });
}

function renderCustomCropEditor() {
  const editor = customCropEditor;
  if (!editor) return;
  const root = document.createElement("section");
  root.className = "crop-editor";
  root.addEventListener("click", (event) => event.stopPropagation());
  root.addEventListener("dblclick", (event) => event.stopPropagation());
  root.addEventListener("auxclick", (event) => event.stopPropagation());
  root.addEventListener("contextmenu", (event) => event.preventDefault());
  const canvas = document.createElement("div");
  canvas.className = "crop-editor-canvas";
  const canvasRect = cropEditorCanvasRect(editor);
  Object.assign(canvas.style, { left: `${canvasRect.left}px`, top: `${canvasRect.top}px`, width: `${canvasRect.width}px`, height: `${canvasRect.height}px` });
  const masks = Array.from({ length: 4 }, () => {
    const mask = document.createElement("div");
    mask.className = "crop-editor-mask";
    canvas.append(mask);
    return mask;
  });
  const selection = document.createElement("div");
  selection.className = "crop-editor-selection";
  canvas.append(selection);
  canvas.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const point = cropEditorPoint(event, editor);
    const edge = editor.empty ? null : cropEditorHitEdge(point, editor.crop, editor);
    editor.empty = false;
    editor.drag = { kind: edge || "free", start: point, crop: { ...editor.crop } };
    canvas.setPointerCapture(event.pointerId);
  });
  canvas.addEventListener("pointermove", (event) => {
    const drag = editor.drag;
    if (!drag) return;
    const point = cropEditorPoint(event, editor);
    const bounds = editor.dimensions;
    if (drag.kind === "free") {
      editor.crop = clampCropRect({ x: Math.min(drag.start.x, point.x), y: Math.min(drag.start.y, point.y), width: Math.abs(point.x - drag.start.x), height: Math.abs(point.y - drag.start.y) }, bounds);
    } else {
      const crop = { ...drag.crop };
      if (drag.kind === "top") {
        const bottom = crop.y + crop.height;
        crop.y = Math.max(0, Math.min(point.y, bottom - 1));
        crop.height = bottom - crop.y;
      } else if (drag.kind === "bottom") {
        crop.height = Math.max(1, Math.min(bounds.height, point.y) - crop.y);
      } else if (drag.kind === "left") {
        const right = crop.x + crop.width;
        crop.x = Math.max(0, Math.min(point.x, right - 1));
        crop.width = right - crop.x;
      } else if (drag.kind === "right") {
        crop.width = Math.max(1, Math.min(bounds.width, point.x) - crop.x);
      }
      editor.crop = clampCropRect(crop, bounds);
    }
    updateCustomCropEditor();
  });
  const endDrag = () => { editor.drag = null; };
  canvas.addEventListener("pointerup", endDrag);
  canvas.addEventListener("pointercancel", endDrag);
  const controls = document.createElement("div");
  controls.className = `crop-editor-controls${editor.mode === "delogo" ? " crop-editor-controls--delogo" : ""}`;
  const label = document.createElement("span");
  label.className = "crop-editor-label";
  const presets = document.createElement("div");
  presets.className = "crop-editor-presets";
  if (editor.mode === "crop") {
    for (const [labelText, aspect] of [["4:3", 4 / 3], ["16:9", 16 / 9], ["16:10", 16 / 10], ["5:4", 5 / 4], ["3:2", 3 / 2], ["21:9", 21 / 9]]) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = labelText;
      button.addEventListener("click", () => setCustomCropAspect(aspect));
      presets.append(button);
    }
  }
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.textContent = editor.mode === "delogo"
    ? trKey("FreeSelectingViewController", "I8a-rb-zex.title", "Cancel")
    : trKey("CropSettingsViewController", "BtO-vw-tiy.title", "Cancel");
  cancel.addEventListener("click", closeCustomCropEditor);
  const done = document.createElement("button");
  done.type = "button";
  done.textContent = editor.mode === "delogo"
    ? trKey("FreeSelectingViewController", "SFp-iy-h4f.title", "Done")
    : trKey("CropSettingsViewController", "eu5-yH-hHg.title", "Done");
  done.addEventListener("click", () => void commitCustomCropEditor());
  if (editor.mode === "delogo") {
    const instructions = document.createElement("div");
    instructions.className = "crop-editor-instructions";
    const title = document.createElement("strong");
    title.textContent = trKey("FreeSelectingViewController", "mCM-Di-cvS.title", "Select Region");
    const detail = document.createElement("span");
    detail.textContent = trKey(
      "FreeSelectingViewController",
      "XyF-Qa-CSn.title",
      "Drag to select the desired region.",
    );
    instructions.append(title, detail);
    controls.append(instructions, label, cancel, done);
  } else {
    controls.append(label, presets, cancel, done);
  }
  root.append(canvas, controls);
  els.videoStage.append(root);
  Object.assign(editor, { root, canvas, selection, masks, label, done });
  updateCustomCropEditor();
}

function renderQuickSettingsColor(label, rawColor, enabled, target) {
  const row = document.createElement("label");
  row.className = "sidebar-quick-color";
  const title = document.createElement("span");
  title.className = "sidebar-quick-label";
  title.textContent = tr(label);
  const value = mpvColorToInput(rawColor);
  const color = document.createElement("input");
  color.type = "color";
  color.value = value.hex;
  color.disabled = !enabled;
  const alpha = document.createElement("input");
  alpha.type = "range";
  alpha.min = "0";
  alpha.max = "1";
  alpha.step = "0.01";
  alpha.value = String(value.alpha);
  alpha.disabled = !enabled;
  const commit = () => command({
    type: "set-subtitle-style-color",
    target,
    color: htmlColorToMpv(color.value, Number(alpha.value)),
  });
  color.addEventListener("change", commit);
  alpha.addEventListener("change", commit);
  row.append(title, color, alpha);
  return row;
}

function renderQuickSettingsSelect(label, values, selected, enabled, payloadForValue) {
  const row = document.createElement("label");
  row.className = "sidebar-quick-select";
  const title = document.createElement("span");
  title.className = "sidebar-quick-label";
  title.textContent = tr(label);
  const select = document.createElement("select");
  select.disabled = !enabled;
  for (const value of values) {
    const option = document.createElement("option");
    option.value = String(value);
    option.textContent = String(value);
    option.selected = Number(selected) === value;
    select.append(option);
  }
  select.addEventListener("change", () => command(payloadForValue(Number(select.value))));
  row.append(title, select);
  return row;
}

function renderQuickSettingsRange(label, rawValue, min, max, step, enabled, payloadForValue, signed = false, suffix = "") {
  const row = document.createElement("label");
  row.className = "sidebar-quick-range";
  const title = document.createElement("span");
  title.className = "sidebar-quick-label";
  title.textContent = tr(label);
  const input = document.createElement("input");
  input.type = "range";
  input.min = String(min);
  input.max = String(max);
  input.step = String(step);
  input.disabled = !enabled;
  const initial = Math.max(min, Math.min(max, Number(rawValue) || 0));
  input.value = String(initial);
  const output = document.createElement("output");
  output.className = "sidebar-quick-output";
  const updateOutput = () => {
    const value = Number(input.value);
    output.textContent = `${signed ? formatSignedNumber(value) : formatTrackNumber(value)}${suffix}`;
  };
  updateOutput();
  input.addEventListener("input", updateOutput);
  input.addEventListener("change", () => command(payloadForValue(Number(input.value))));
  row.append(title, input, output);
  return row;
}

function formatSignedNumber(value) {
  const number = Number(value) || 0;
  const formatted = formatTrackNumber(number);
  return number > 0 ? `+${formatted}` : formatted;
}

function normalizeMpvColor(rawColor) {
  const values = String(rawColor || "")
    .split("/")
    .map((value) => Number(value.trim()));
  if (!(values.length === 3 || values.length === 4) || values.some((value) => !Number.isFinite(value) || value < 0 || value > 1)) {
    return null;
  }
  if (values.length === 3) values.push(1);
  return values.map((value) => formatTrackNumber(value)).join("/");
}

function mpvColorToInput(rawColor) {
  const normalized = normalizeMpvColor(rawColor) || "1/1/1/1";
  const [red, green, blue, alpha] = normalized.split("/").map(Number);
  const toHex = (value) => Math.round(value * 255).toString(16).padStart(2, "0");
  return { hex: `#${toHex(red)}${toHex(green)}${toHex(blue)}`, alpha };
}

function htmlColorToMpv(hex, alpha) {
  const match = /^#([0-9a-f]{6})$/i.exec(String(hex));
  if (!match) return "1/1/1/1";
  const value = Number.parseInt(match[1], 16);
  const components = [
    (value >> 16) & 0xff,
    (value >> 8) & 0xff,
    value & 0xff,
    Math.max(0, Math.min(1, Number(alpha) || 0)),
  ];
  return components
    .map((component, index) => formatTrackNumber(index < 3 ? component / 255 : component))
    .join("/");
}

function renderPlaylistList(nextState, emptyText) {
  const items = nextState.playlist || [];
  const details = playlistCacheDetails(nextState, items);
  els.sidebarContent.oncontextmenu = (event) => {
    if (event.target.closest(".playlist-toolbar")) return;
    event.preventDefault();
    event.stopPropagation();
    showPlaylistContextMenu(event, []);
  };
  const toolbar = renderPlaylistToolbar(items, details);
  els.sidebarContent.append(toolbar);

  if (!items.length) {
    const empty = document.createElement("div");
    empty.className = "sidebar-empty";
    empty.textContent = tr(emptyText);
    els.sidebarContent.append(empty);
    return;
  }

  items.forEach((item, index) => {
    const detail = details[index];
    const metadata = playlistMetadata(
      detail,
      preferences.values,
      nextState.mode === "mini-player",
    );
    const row = document.createElement("button");
    row.type = "button";
    const classes = ["sidebar-row", "sidebar-action-row", "playlist-item-row"];
    if (item.current || item.playing) classes.push("active");
    if (playlistSelection.has(index)) classes.push("selected");
    row.className = classes.join(" ");
    row.title = item.path || item.title || "";
    row.setAttribute("aria-selected", String(playlistSelection.has(index)));
    row.draggable = true;
    const stateMarker = document.createElement("span");
    stateMarker.className = "playlist-row-state";
    stateMarker.textContent = item.playing ? "▶" : "";
    stateMarker.setAttribute("aria-hidden", "true");
    const title = document.createElement("span");
    title.className = "playlist-row-title";
    title.textContent = metadata?.title || item.title || item.path || "";
    const artist = document.createElement("span");
    artist.className = "playlist-row-artist";
    artist.textContent = metadata?.artist || "";
    artist.hidden = !metadata;
    const duration = document.createElement("span");
    duration.className = "playlist-row-duration";
    const durationSeconds = Number(detail?.duration_seconds ?? item.duration_seconds);
    duration.textContent = durationSeconds > 0 ? formatTime(durationSeconds) : "";
    const progressTrack = document.createElement("span");
    progressTrack.className = "playlist-row-progress-track";
    const progress = document.createElement("span");
    progress.className = "playlist-row-progress";
    progress.style.width = `${playlistProgressFraction(detail) * 100}%`;
    progressTrack.append(progress);
    row.append(stateMarker, title, artist, duration, progressTrack);
    row.addEventListener("click", (event) => {
      selectPlaylistRow(index, event);
    });
    row.addEventListener("dblclick", async () => {
      await command({ type: "select-playlist-item", index });
      clearPlaylistSelection();
    });
    row.addEventListener("contextmenu", (event) => {
      event.preventDefault();
      event.stopPropagation();
      showPlaylistContextMenu(event, selectedPlaylistIndexesForContext(index));
    });
    row.addEventListener("dragstart", (event) => beginPlaylistDrag(event, index, row));
    row.addEventListener("dragover", (event) => {
      const rect = row.getBoundingClientRect();
      const destination = event.clientY < rect.top + rect.height / 2 ? index : index + 1;
      updatePlaylistDropTarget(event, row, destination);
    });
    row.addEventListener("drop", (event) => {
      const rect = row.getBoundingClientRect();
      const destination = event.clientY < rect.top + rect.height / 2 ? index : index + 1;
      void dropPlaylistItems(event, destination);
    });
    row.addEventListener("dragend", endPlaylistDrag);
    els.sidebarContent.append(row);
  });

  const endDropTarget = document.createElement("div");
  endDropTarget.className = "playlist-drop-end";
  endDropTarget.setAttribute("aria-hidden", "true");
  endDropTarget.addEventListener("dragover", (event) => updatePlaylistDropTarget(event, endDropTarget, items.length));
  endDropTarget.addEventListener("drop", (event) => void dropPlaylistItems(event, items.length));
  els.sidebarContent.append(endDropTarget);
}

function renderPlaylistToolbar(items, details) {
  const toolbar = document.createElement("div");
  toolbar.className = "playlist-toolbar";

  const durationSummary = playlistDurationSummary(details, [...playlistSelection]);
  const summary = document.createElement("div");
  summary.className = "playlist-summary";
  const selectedCount = playlistSelection.size;
  summary.textContent = !durationSummary
    ? ""
    : selectedCount
      ? trFormat(
          "%@ of %@ selected",
          formatTime(durationSummary.selectedSeconds),
          formatTime(durationSummary.totalSeconds),
        )
      : trFormat("%@ in total", formatTime(durationSummary.totalSeconds));

  const actions = document.createElement("div");
  actions.className = "playlist-toolbar-actions";
  actions.append(
    playlistToolbarButton("+", "Add Files to Playlist", () => addMediaToPlaylistFromNativeDialog()),
    playlistToolbarButton("\u2212", "Remove Selected Items", () => removeSelectedPlaylistItems(), playlistSelection.size === 0),
    playlistToolbarButton("Clear", "Clear Playlist", () => clearPlaylist())
  );
  toolbar.append(summary, actions);
  return toolbar;
}

function playlistCacheDetails(nextState, items) {
  const cached = Array.isArray(nextState.playlist_cache?.items)
    ? nextState.playlist_cache.items
    : [];
  return items.map((item, index) => {
    const aligned = cached[index];
    if (aligned?.path === item.path) return aligned;
    return cached.find((detail) => detail?.path === item.path) || {
      path: item.path,
      ready: false,
      duration_seconds: item.duration_seconds ?? null,
      playback_progress_seconds: null,
    };
  });
}

function playlistToolbarButton(label, title, action, disabled = false) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "playlist-toolbar-button";
  button.textContent = tr(label);
  button.title = tr(title);
  button.setAttribute("aria-label", tr(title));
  button.disabled = disabled;
  button.addEventListener("click", () => void action());
  return button;
}

function selectPlaylistRow(index, event) {
  if (event.shiftKey && playlistSelectionAnchor >= 0) {
    const lower = Math.min(playlistSelectionAnchor, index);
    const upper = Math.max(playlistSelectionAnchor, index);
    playlistSelection = new Set(Array.from({ length: upper - lower + 1 }, (_, offset) => lower + offset));
  } else if (event.metaKey || event.ctrlKey) {
    if (playlistSelection.has(index)) {
      playlistSelection.delete(index);
    } else {
      playlistSelection.add(index);
      playlistSelectionAnchor = index;
    }
  } else {
    playlistSelection = new Set([index]);
    playlistSelectionAnchor = index;
  }
  if (!playlistSelection.size) playlistSelectionAnchor = -1;
  renderSidebar(state);
}

function selectedPlaylistIndexesForContext(index) {
  return playlistContextTargetIndexes([...playlistSelection], index, state.playlist.length);
}

function normalizePlaylistSelection(playlistLength) {
  playlistSelection = new Set([...playlistSelection].filter((index) => index >= 0 && index < playlistLength));
  if (!playlistSelection.has(playlistSelectionAnchor)) {
    playlistSelectionAnchor = playlistSelection.size ? Math.min(...playlistSelection) : -1;
  }
}

function clearPlaylistSelection() {
  if (!playlistSelection.size) return;
  playlistSelection.clear();
  playlistSelectionAnchor = -1;
  renderSidebar(state);
}

function mockPlayPlaylistItemsNext(indexes) {
  const currentIndex = mockState.playlist.findIndex((item) => item.current || item.playing);
  if (currentIndex < 0) return;
  const selected = normalizePlaylistIndexes(indexes, mockState.playlist.length).filter((index) => index !== currentIndex);
  if (!selected.length) return;
  const selectedSet = new Set(selected);
  const moved = selected.map((index) => mockState.playlist[index]);
  const remaining = mockState.playlist.filter((_, index) => !selectedSet.has(index));
  const currentInRemaining = remaining.findIndex((item) => item.current || item.playing);
  remaining.splice(currentInRemaining + 1, 0, ...moved);
  mockState.playlist = remaining;
  mockState.playlist.forEach((item, index) => { item.id = index + 1; });
  syncMockMpvProperties();
}

function beginPlaylistDrag(event, index, row) {
  if (!playlistSelection.has(index)) {
    playlistSelection = new Set([index]);
    playlistSelectionAnchor = index;
  }
  playlistDragIndexes = [...playlistSelection].sort((left, right) => left - right);
  event.dataTransfer.effectAllowed = "move";
  event.dataTransfer.setData("application/x-iima-playlist", JSON.stringify(playlistDragIndexes));
  const paths = playlistDragIndexes.map((itemIndex) => state.playlist[itemIndex]?.path).filter(Boolean);
  event.dataTransfer.setData("text/uri-list", paths.map(playlistPathAsUri).join("\r\n"));
  event.dataTransfer.setData("text/plain", paths.join("\n"));
  row.classList.add("playlist-dragging");
}

function playlistPathAsUri(path) {
  if (/^[^:/?#]+:(?:\/\/)?/.test(path)) return path;
  const normalized = String(path).startsWith("/") ? String(path) : `/${path}`;
  return `file://${encodeURI(normalized)}`;
}

function updatePlaylistDropTarget(event, target, destination) {
  if (!isPlaylistDrag(event.dataTransfer) && !hasSupportedDropData(event.dataTransfer)) return;
  event.preventDefault();
  event.stopPropagation();
  event.dataTransfer.dropEffect = isPlaylistDrag(event.dataTransfer) ? "move" : "copy";
  clearPlaylistDropTarget();
  target.classList.add(destination === state.playlist.length ? "playlist-drop-end-active" : "playlist-drop-before");
}

async function dropPlaylistItems(event, destination) {
  if (!isPlaylistDrag(event.dataTransfer) && !hasSupportedDropData(event.dataTransfer)) return;
  event.preventDefault();
  event.stopPropagation();
  clearPlaylistDropTarget();

  if (isPlaylistDrag(event.dataTransfer)) {
    const indexes = normalizePlaylistIndexes(playlistDragIndexes, state.playlist.length);
    playlistDragIndexes = [];
    await movePlaylistItems(indexes, destination);
  } else {
    const targets = dropTargets(event.dataTransfer);
    if (targets.length) await insertPlaylistItems(targets, destination);
  }
}

async function movePlaylistItems(indexes, destination) {
  const selectedIndexes = normalizePlaylistIndexes(indexes, state.playlist.length);
  const reordered = reorderedPlaylistItems(state.playlist, selectedIndexes, destination);
  if (!reordered.moved) return;

  const nextState = await invoke("player_command", {
    command: { type: "move-playlist-items", indexes: selectedIndexes, destination },
  });
  playlistSelection = new Set(reordered.selectedIndexes);
  playlistSelectionAnchor = reordered.selectedIndexes[0] ?? -1;
  setPlayerState(nextState, { force: true });
}

async function playSelectedPlaylistItemsNext(indexes = [...playlistSelection]) {
  const selectedIndexes = normalizePlaylistIndexes(indexes, state.playlist.length);
  if (!selectedIndexes.length) return;
  const nextState = await invoke("playlist_play_next", { indexes: selectedIndexes });
  playlistSelection = new Set(playlistSelectionAfterAction(playlistSelection, "play-next"));
  playlistSelectionAnchor = -1;
  setPlayerState(nextState, { force: true });
}

function endPlaylistDrag() {
  playlistDragIndexes = [];
  clearPlaylistDropTarget();
  document.querySelectorAll(".playlist-dragging").forEach((row) => row.classList.remove("playlist-dragging"));
}

function clearPlaylistDropTarget() {
  document
    .querySelectorAll(".playlist-drop-before, .playlist-drop-end-active")
    .forEach((target) => target.classList.remove("playlist-drop-before", "playlist-drop-end-active"));
}

function isPlaylistDrag(dataTransfer) {
  return Boolean(dataTransfer && Array.from(dataTransfer.types ?? []).includes("application/x-iima-playlist"));
}

async function removeSelectedPlaylistItems(indexes = [...playlistSelection]) {
  const selectedIndexes = normalizePlaylistIndexes(indexes, state.playlist.length);
  if (!selectedIndexes.length) return;
  const nextState = await invoke("playlist_remove_items", { indexes: selectedIndexes });
  playlistSelection = new Set(playlistSelectionAfterAction(playlistSelection, "remove"));
  playlistSelectionAnchor = -1;
  setPlayerState(nextState, { force: true });
}

async function clearPlaylist() {
  playlistSelection.clear();
  playlistSelectionAnchor = -1;
  await command({ type: "clear-playlist" });
}

function handlePlaylistSelectionShortcut(event) {
  if (!state.sidebar.visible || state.sidebar.tab !== "playlist") return false;
  const commandKey = event.metaKey || event.ctrlKey;
  const key = String(event.key).toLowerCase();
  if (commandKey && key === "a") {
    event.preventDefault();
    playlistSelection = new Set(state.playlist.map((_, index) => index));
    playlistSelectionAnchor = state.playlist.length ? 0 : -1;
    renderSidebar(state);
    return true;
  }
  if (commandKey && key === "v") {
    event.preventDefault();
    void pastePlaylistItems();
    return true;
  }
  if (commandKey && key === "c" && playlistSelection.size) {
    event.preventDefault();
    void copyPlaylistItems();
    return true;
  }
  if (commandKey && key === "x" && playlistSelection.size) {
    event.preventDefault();
    void cutPlaylistItems();
    return true;
  }
  if (!playlistSelection.size) return false;
  if (event.key === "Escape") {
    event.preventDefault();
    clearPlaylistSelection();
    return true;
  }
  if (event.key === "Backspace" || event.key === "Delete") {
    event.preventDefault();
    void removeSelectedPlaylistItems();
    return true;
  }
  return false;
}

function playlistEditContextIsActive() {
  return state.sidebar.visible && state.sidebar.tab === "playlist" && !isTypingTarget(document.activeElement);
}

function handlePlaylistCopyEvent(event) {
  if (!playlistEditContextIsActive() || !playlistSelection.size) return;
  event.preventDefault();
  void copyPlaylistItems();
}

function handlePlaylistCutEvent(event) {
  if (!playlistEditContextIsActive() || !playlistSelection.size) return;
  event.preventDefault();
  void cutPlaylistItems();
}

function handlePlaylistPasteEvent(event) {
  if (!playlistEditContextIsActive()) return;
  event.preventDefault();
  void pastePlaylistItems();
}

async function copyPlaylistItems(indexes = [...playlistSelection]) {
  const selectedIndexes = normalizePlaylistIndexes(indexes, state.playlist.length);
  if (!selectedIndexes.length) return;
  await invoke("playlist_copy_items", { indexes: selectedIndexes });
}

async function cutPlaylistItems(indexes = [...playlistSelection]) {
  const selectedIndexes = normalizePlaylistIndexes(indexes, state.playlist.length);
  if (!selectedIndexes.length) return;
  await invoke("playlist_copy_items", { indexes: selectedIndexes });
  await removeSelectedPlaylistItems(selectedIndexes);
}

async function pastePlaylistItems() {
  if (!(await invoke("playlist_can_paste_filenames"))) return;
  const destination = playlistPasteDestination([...playlistSelection], state.playlist.length);
  const nextState = await invoke("playlist_paste_items", { destination });
  if (nextState) setPlayerState(nextState, { force: true, presentOsd: true });
}

function showPlaylistContextMenu(event, indexes) {
  hidePlaylistContextMenu();
  const model = playlistContextMenuModel(state.playlist, indexes);
  const menu = document.createElement("div");
  menu.className = "playlist-context-menu";
  menu.role = "menu";
  let pluginItemsAppended = false;
  for (const item of model.menu) {
    if (!pluginItemsAppended && item.kind === "action" && item.id === "add-file") {
      appendPluginPlaylistContextItems(menu, model.targets.selected);
      pluginItemsAppended = true;
    }
    if (item.kind === "separator") {
      const separator = document.createElement("div");
      separator.className = "playlist-context-menu-separator";
      separator.role = "separator";
      menu.append(separator);
    } else if (item.kind === "header") {
      const header = document.createElement("div");
      header.className = "playlist-context-menu-header";
      header.textContent = tr(item.label);
      menu.append(header);
    } else {
      menu.append(playlistContextMenuButton(item.label, () => executePlaylistContextAction(item.id, model.targets.selected)));
    }
  }
  if (!pluginItemsAppended) appendPluginPlaylistContextItems(menu, model.targets.selected);
  menu.addEventListener("click", (menuEvent) => menuEvent.stopPropagation());
  document.body.append(menu);

  const rect = menu.getBoundingClientRect();
  const left = Math.max(8, Math.min(event.clientX, window.innerWidth - rect.width - 8));
  const top = Math.max(8, Math.min(event.clientY, window.innerHeight - rect.height - 8));
  menu.style.left = `${left}px`;
  menu.style.top = `${top}px`;
  playlistContextMenu = menu;
}

function appendPluginPlaylistContextItems(menu, indexes) {
  const groups = [];
  for (const runtime of pluginRuntimes.values()) {
    if (typeof runtime.playlistMenuItemBuilder !== "function") continue;
    try {
      const items = runtime.playlistMenuItemBuilder(Array.from(indexes));
      if (Array.isArray(items) && items.length) groups.push({ runtime, items });
    } catch (error) {
      console.error(`[${runtime.spec.identifier}] playlist menu builder failed`, error);
    }
  }
  if (!groups.length) return;

  const header = document.createElement("div");
  header.className = "playlist-context-menu-header";
  header.textContent = tr("Plugins");
  menu.append(header);
  for (const { runtime, items } of groups) {
    appendPluginPlaylistMenuLevel(menu, runtime, items, 0);
  }
  const separator = document.createElement("div");
  separator.className = "playlist-context-menu-separator";
  separator.role = "separator";
  menu.append(separator);
}

function appendPluginPlaylistMenuLevel(menu, runtime, items, depth) {
  for (const item of items) {
    if (!item || typeof item !== "object") continue;
    if (item.separator) {
      const separator = document.createElement("div");
      separator.className = "playlist-context-menu-separator";
      separator.role = "separator";
      menu.append(separator);
      continue;
    }
    const children = Array.isArray(item.items) ? item.items : [];
    const action = typeof item.action === "function"
      ? async () => {
          try {
            await item.action(item);
          } catch (error) {
            console.error(`[${runtime.spec.identifier}] playlist menu action failed`, error);
          }
        }
      : async () => {};
    const button = playlistContextMenuButton(`${item.selected ? "✓ " : ""}${String(item.title || "")}`, action);
    button.disabled = item.enabled === false || (!item.action && !children.length);
    button.style.paddingInlineStart = `${12 + depth * 16}px`;
    if (children.length) {
      button.classList.add("playlist-context-menu-parent");
      button.disabled = true;
    }
    menu.append(button);
    if (children.length) appendPluginPlaylistMenuLevel(menu, runtime, children, depth + 1);
  }
}

async function executePlaylistContextAction(action, indexes) {
  if (action === "play-next") return playSelectedPlaylistItemsNext(indexes);
  if (action === "open-new-window") return invoke("playlist_open_items_in_new_window", { indexes });
  if (action === "remove") return removeSelectedPlaylistItems(indexes);
  if (action === "trash") {
    const result = await invoke("playlist_trash_items", { indexes });
    playlistSelection = new Set(playlistSelectionAfterAction(playlistSelection, "trash"));
    playlistSelectionAnchor = -1;
    setPlayerState(result.player, { force: true });
    if (result.failures?.length) {
      showOsd(trFormat("Error deleting: %@", result.failures[0].error || "Unable to move file to Trash"));
    }
    return;
  }
  if (action === "open-browser") return invoke("playlist_open_network_items", { indexes });
  if (action === "copy-urls") return invoke("playlist_copy_network_urls", { indexes });
  if (action === "reveal") {
    await invoke("playlist_reveal_items", { indexes });
    playlistSelection = new Set(playlistSelectionAfterAction(playlistSelection, "reveal"));
    playlistSelectionAnchor = -1;
    renderSidebar(state);
    return;
  }
  if (action === "add-file") return addMediaToPlaylistFromNativeDialog();
  if (action === "add-url") {
    const nextState = await invoke("playlist_add_url_dialog");
    if (nextState) setPlayerState(nextState, { force: true, presentOsd: true });
    return;
  }
  if (action === "clear") return clearPlaylist();
}

function playlistContextMenuButton(label, action) {
  const button = document.createElement("button");
  button.type = "button";
  button.role = "menuitem";
  button.textContent = tr(label);
  button.addEventListener("click", async (event) => {
    event.preventDefault();
    event.stopPropagation();
    hidePlaylistContextMenu();
    await action();
  });
  return button;
}

function hidePlaylistContextMenu() {
  if (!playlistContextMenu) return;
  playlistContextMenu.remove();
  playlistContextMenu = undefined;
}

async function toggleRemainingTimeDisplay(event) {
  event.preventDefault();
  event.stopPropagation();
  if (!(Number(state.duration_seconds) > 0)) return;
  await setPreferenceValue("showRemainingTime", !Boolean(getPreferenceValue("showRemainingTime")));
}

function showTimeContextMenu(event) {
  event.preventDefault();
  event.stopPropagation();
  hideContextMenus();
  const menu = document.createElement("div");
  menu.className = "time-context-menu";
  menu.role = "menu";

  const header = document.createElement("div");
  header.className = "time-context-menu-header";
  header.textContent = tr("Precision");
  menu.append(header);

  const precision = timeDisplayPrecision();
  TIME_PRECISION_KEYS.forEach((key, index) => {
    const button = document.createElement("button");
    button.type = "button";
    button.role = "menuitemradio";
    button.classList.toggle("is-checked", index === precision);
    button.setAttribute("aria-checked", String(index === precision));
    button.dataset.precision = key;
    button.textContent = tr(TIME_PRECISION_LABELS[index]);
    button.addEventListener("click", async (menuEvent) => {
      menuEvent.preventDefault();
      menuEvent.stopPropagation();
      hideTimeContextMenu();
      await setPreferenceValue("timeDisplayPrecision", index);
    });
    menu.append(button);
  });
  menu.addEventListener("click", (menuEvent) => menuEvent.stopPropagation());
  document.body.append(menu);

  const rect = menu.getBoundingClientRect();
  const left = Math.max(8, Math.min(event.clientX, window.innerWidth - rect.width - 8));
  const top = Math.max(8, Math.min(event.clientY, window.innerHeight - rect.height - 8));
  menu.style.left = `${left}px`;
  menu.style.top = `${top}px`;
  timeContextMenu = menu;
}

function hideTimeContextMenu() {
  if (!timeContextMenu) return;
  timeContextMenu.remove();
  timeContextMenu = undefined;
}

function hideContextMenus() {
  hidePlaylistContextMenu();
  hideTimeContextMenu();
}

function renderChapterList(items, emptyText) {
  if (!items.length) {
    const empty = document.createElement("div");
    empty.className = "sidebar-empty";
    empty.textContent = tr(emptyText);
    els.sidebarContent.append(empty);
    return;
  }

  items.forEach((item) => {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "sidebar-row sidebar-action-row";
    row.textContent = `${formatTime(item.time_seconds)} ${item.title}`;
    row.addEventListener("dblclick", () => {
      command({ type: "seek", seconds: Number(item.time_seconds) || 0 });
    });
    els.sidebarContent.append(row);
  });
}

function renderList(items, label, emptyText) {
  if (!items.length) {
    const empty = document.createElement("div");
    empty.className = "sidebar-empty";
    empty.textContent = tr(emptyText);
    els.sidebarContent.append(empty);
    return;
  }
  for (const item of items) {
    const row = document.createElement("div");
    row.className = item.current || item.playing || item.selected ? "sidebar-row active" : "sidebar-row";
    row.textContent = label(item);
    els.sidebarContent.append(row);
  }
}

function renderTrackList(items, kind, emptyText, options = {}) {
  if (options.heading) {
    const heading = document.createElement("div");
    heading.className = "sidebar-track-heading";
    heading.textContent = tr(options.heading);
    els.sidebarContent.append(heading);
  }
  if (!items.length) {
    const empty = document.createElement("div");
    empty.className = "sidebar-empty";
    empty.textContent = tr(emptyText);
    els.sidebarContent.append(empty);
    return;
  }

  for (const item of items) {
    const row = document.createElement("button");
    const detail = trackMetadataDetail(item, kind === "second-subtitles" ? "subtitles" : kind);
    const badges = trackStatusBadges(item);
    row.type = "button";
    row.className = item.selected ? "sidebar-row sidebar-track-row active" : "sidebar-row sidebar-track-row";
    row.disabled = options.enabled === false;
    row.setAttribute("aria-pressed", String(Boolean(item.selected)));
    row.title = [
      localizedTrackKindTitle(kind),
      trackDisplayTitle(item),
      detail,
      badges.map(localizedTrackBadge).join(", "),
    ].filter(Boolean).join("\n");
    row.addEventListener("click", () => {
      if (!item.selected) {
        command({ type: "select-track", kind, id: item.id });
      }
    });

    const title = document.createElement("span");
    title.className = "sidebar-track-title";
    title.textContent = item.virtual
      ? `${item.selected ? "✓ " : ""}${trKey("Localizable", "quicksetting.item_none", "None")}`
      : trackLabel(item);
    row.append(title);

    if (detail) {
      const detailElement = document.createElement("span");
      detailElement.className = "sidebar-track-detail";
      detailElement.textContent = detail;
      row.append(detailElement);
    }

    if (badges.length) {
      const badgeRow = document.createElement("span");
      badgeRow.className = "sidebar-track-badges";
      for (const badge of badges) {
        const badgeElement = document.createElement("span");
        badgeElement.className = "track-badge";
        badgeElement.textContent = localizedTrackBadge(badge);
        badgeRow.append(badgeElement);
      }
      row.append(badgeRow);
    }

    els.sidebarContent.append(row);
  }
}

function trackLabel(item) {
  return `${item.selected ? "✓ " : ""}${trackDisplayTitle(item)}`;
}

function trackDisplayTitle(item) {
  if (item.virtual || Number(item.id) === 0) return "None";
  const id = Number(item.id);
  const title = String(item.title || "").trim();
  const sourceTitle = String(item.metadata?.source_title || "").trim();
  if (sourceTitle) return Number.isSafeInteger(id) && id > 0 ? `#${id} ${sourceTitle}` : sourceTitle;
  if (title) {
    if (Number.isSafeInteger(id) && id > 0 && title.startsWith(`#${id}`)) return `#${id}`;
    if (Number.isSafeInteger(id) && id > 0) return `#${id} ${title}`;
    return title;
  }
  if (Number.isSafeInteger(id) && id > 0) return `#${id}`;
  return "Track";
}

function trackMetadataDetail(item, kind) {
  const metadata = item.metadata ?? {};
  const parts = [];
  if (metadata.source_id !== null && metadata.source_id !== undefined && Number.isSafeInteger(Number(metadata.source_id))) {
    parts.push(`Source #${metadata.source_id}`);
  }
  if (metadata.language) parts.push(String(metadata.language).toUpperCase());
  if (metadata.codec) parts.push(String(metadata.codec));

  if (kind === "video") {
    const dimensions = trackDimensions(metadata);
    if (dimensions) parts.push(dimensions);
    if (Number.isFinite(Number(metadata.demux_fps))) parts.push(`${formatTrackNumber(metadata.demux_fps)} fps`);
    if (Number.isFinite(Number(metadata.demux_rotation)) && Number(metadata.demux_rotation) !== 0) {
      parts.push(`${metadata.demux_rotation} deg`);
    }
    if (metadata.demux_par && metadata.demux_par !== "1:1") parts.push(`PAR ${metadata.demux_par}`);
  }

  if (kind === "audio") {
    const channelText = metadata.demux_channel_count
      ? `${metadata.demux_channel_count} ch`
      : metadata.audio_channels || metadata.demux_channels;
    if (channelText) parts.push(String(channelText));
    if (Number.isFinite(Number(metadata.demux_samplerate))) {
      parts.push(`${formatTrackNumber(Number(metadata.demux_samplerate) / 1000)} kHz`);
    }
  }

  if (metadata.decoder_description && metadata.decoder_description !== metadata.codec) {
    parts.push(String(metadata.decoder_description));
  }
  if (Number.isFinite(Number(metadata.demux_bitrate)) && Number(metadata.demux_bitrate) > 0) {
    parts.push(formatBitrate(metadata.demux_bitrate));
  }
  if (metadata.external_filename) {
    const externalTitle = titleFromPath(String(metadata.external_filename));
    if (!trackDisplayTitle(item).includes(externalTitle)) parts.push(externalTitle);
  }

  return [...new Set(parts.filter(Boolean))].join(" | ");
}

function trackDimensions(metadata) {
  const width = Number(metadata.demux_width);
  const height = Number(metadata.demux_height);
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return "";
  return `${width}x${height}`;
}

function trackStatusBadges(item) {
  return trackStatusBadgesForQuickSettings(item);
}

function formatTrackNumber(value) {
  const number = Number(value);
  if (!Number.isFinite(number)) return "";
  if (Number.isInteger(number)) return String(number);
  return number.toFixed(3).replace(/0+$/, "").replace(/\.$/, "");
}

function formatBitrate(value) {
  const number = Number(value);
  if (!Number.isFinite(number) || number <= 0) return "";
  if (number >= 1_000_000) return `${formatTrackNumber(number / 1_000_000)} Mbps`;
  return `${formatTrackNumber(number / 1000)} kbps`;
}

function titleFromPath(path) {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}

function menuRequestLabel(action) {
  const labels = {
    preferences: "Preferences",
    "playback-history": "Playback History",
    "save-current-playlist": "Save Playlist",
    screenshot: "Screenshot",
    "goto-screenshot-folder": "Screenshot Folder",
    "video-filters": "Video Filters",
    "audio-filters": "Audio Filters",
    "load-external-audio": "Load External Audio",
    "load-external-subtitle": "Load External Subtitle",
    "find-online-subtitles": "Find Online Subtitles",
    "save-downloaded-subtitle": "Save Subtitle",
    help: "IINA Help",
    "release-highlights": "Release Highlights",
    github: "GitHub",
    website: "Website",
  };
  return labels[action] ?? "Not Available";
}

function showPlayerOsd(message, nextState) {
  const presentation = playerOsdPresentation(message, nextState);
  showOsd(presentation.message, {
    detail: presentation.detail,
    progress: presentation.progress,
    literal: presentation.literal,
  });
}

function playerOsdPresentation(rawMessage, nextState) {
  const message = String(rawMessage || "").trim();
  const position = Math.max(0, Number(nextState?.position_seconds) || 0);
  const duration = Math.max(0, Number(nextState?.duration_seconds) || 0);
  const timeDetail = `${formatTime(position)} / ${formatTime(duration)}`;

  if (message === "Paused") return { message: trKey("Localizable", "osd.pause", "Pause"), detail: timeDetail };
  if (message === "Playing") return { message: trKey("Localizable", "osd.resume", "Resume"), detail: timeDetail };
  if (message === "Stopped") return { message: trKey("Localizable", "osd.stop", "Stop") };
  if (message === "Muted") return { message: trKey("Localizable", "osd.mute", "Mute") };
  if (message === "Unmuted") return { message: trKey("Localizable", "osd.unmute", "Unmute") };
  if (message === "File Loop") return { message: trKey("Localizable", "osd.file_loop", "Loop Single File") };
  if (message === "Playlist Loop") return { message: trKey("Localizable", "osd.playlist_loop", "Loop Playlist") };
  if (message === "Loop Off") return { message: trKey("Localizable", "osd.no_loop", "Disable Looping") };
  if (message === "A-B Loop: Cleared") return { message: trKey("Localizable", "osd.abloop.clear", "AB-Loop: Cleared") };
  if (message === "A-B Loop: A" || message === "A-B Loop: B") {
    const isA = message.endsWith(": A");
    return {
      message: trKey("Localizable", isA ? "osd.abloop.a" : "osd.abloop.b", isA ? "AB-Loop: A" : "AB-Loop: B"),
      detail: timeDetail,
    };
  }
  if (message.startsWith("Seek ")) {
    return { message: timeDetail, progress: duration > 0 ? position / duration : 0 };
  }
  if (message.startsWith("Opening ")) {
    return { message: message.slice("Opening ".length), literal: true };
  }

  const volumeMatch = message.match(/^Volume\s+(-?\d+(?:\.\d+)?)%$/);
  if (volumeMatch) {
    const volume = Number(volumeMatch[1]);
    const maxVolume = Math.max(1, Number(getPreferenceValue("maxVolume")) || 100);
    return {
      message: trKeyFormat("Localizable", "osd.volume", "Volume: %i", Math.round(volume)),
      progress: volume / maxVolume,
    };
  }

  const speedMatch = message.match(/^Speed\s+(-?\d+(?:\.\d+)?)x$/);
  if (speedMatch) {
    return { message: trKeyFormat("Localizable", "osd.speed", "Speed: %.2fx", Number(speedMatch[1])) };
  }

  const aspectMatch = message.match(/^Aspect Ratio:\s*(.*)$/);
  if (aspectMatch) {
    return {
      message: trKeyFormat(
        "Localizable",
        "osd.aspect",
        "Aspect Ratio: %@",
        localizedQuickSettingsValue(aspectMatch[1]),
      ),
    };
  }

  const cropMatch = message.match(/^Crop:\s*(.*)$/);
  if (cropMatch) {
    return {
      message: trKeyFormat(
        "Localizable",
        "osd.crop",
        "Crop: %@",
        localizedQuickSettingsValue(cropMatch[1]),
      ),
    };
  }

  const rotateMatch = message.match(/^Rotate\s+(-?\d+)°$/);
  if (rotateMatch) {
    return { message: trKeyFormat("Localizable", "osd.rotate", "Rotate: %i°", Number(rotateMatch[1])) };
  }

  const toggleMatch = message.match(/^(Deinterlace|Hardware Decoding)\s+(On|Off)$/);
  if (toggleMatch) {
    const deinterlace = toggleMatch[1] === "Deinterlace";
    const template = deinterlace ? "Deinterlace: %@" : "Hardware Decoding: %@";
    const value = toggleMatch[2] === "On"
      ? trKey("Localizable", "general.on", "On")
      : trKey("Localizable", "general.off", "Off");
    return {
      message: trKeyFormat("Localizable", deinterlace ? "osd.deinterlace" : "osd.hwdec", template, value),
    };
  }

  const delayMatch = message.match(/^(Audio|Subtitle) Delay:\s*([+-]?\d+(?:\.\d+)?)s$/);
  if (delayMatch) {
    const value = Number(delayMatch[2]);
    const isAudio = delayMatch[1] === "Audio";
    if (value === 0) {
      return {
        message: trKey(
          "Localizable",
          isAudio ? "osd.audio_delay.nodelay" : "osd.sub_delay.nodelay",
          isAudio ? "Audio Delay: No Delay" : "Subtitle Delay: No Delay",
        ),
        progress: 0.5,
      };
    }
    const later = value > 0;
    const direction = later ? "Later" : "Earlier";
    const template = `${isAudio ? "Audio" : "Subtitle"} Delay: %.2fs ${direction}`;
    return {
      message: trKeyFormat(
        "Localizable",
        `osd.${isAudio ? "audio" : "sub"}_delay.${later ? "later" : "earlier"}`,
        template,
        Math.abs(value),
      ),
      progress: (value + 10) / 20,
    };
  }

  const equalizerMatch = message.match(/^(Contrast|Gamma|Hue|Saturation|Brightness):\s*([+-]?\d+)$/);
  if (equalizerMatch) {
    const value = Number(equalizerMatch[2]);
    return {
      message: trKeyFormat(
        "Localizable",
        `osd.video_eq.${equalizerMatch[1].toLowerCase()}`,
        `${equalizerMatch[1]}: %i`,
        value,
      ),
      progress: (value + 100) / 200,
    };
  }

  const subtitlePositionMatch = message.match(/^Subtitle Position:\s*([+-]?\d+(?:\.\d+)?)$/);
  if (subtitlePositionMatch) {
    const value = Number(subtitlePositionMatch[1]);
    return {
      message: trKeyFormat("Localizable", "osd.subtitle_pos", "Subtitle Position: %.1f", value),
      progress: value / 100,
    };
  }

  const subtitleScaleMatch = message.match(/^Subtitle Scale:\s*([+-]?\d+(?:\.\d+)?)$/);
  if (subtitleScaleMatch) {
    return {
      message: trKeyFormat(
        "Localizable",
        "osd.subtitle_scale",
        "Subtitle Scale: %.2fx",
        Number(subtitleScaleMatch[1]),
      ),
    };
  }

  const chapterMatch = message.match(/^Chapter:\s*(.*)$/);
  if (chapterMatch) {
    const chapterCount = Math.max(
      0,
      Number(nextState?.mpv_properties?.chapters) || nextState?.chapters?.length || 0
    );
    const chapterIndex = Math.max(0, Number(nextState?.mpv_properties?.chapter) || 0) + 1;
    return {
      message: trKeyFormat("Localizable", "osd.chapter", "Chapter: %@", chapterMatch[1]),
      detail: `(${chapterIndex}/${chapterCount}) ${timeDetail}`,
    };
  }

  const playlistMatch = message.match(/^Added\s+(\d+)(?:\s+Files)?\s+to Playlist$/);
  if (playlistMatch) {
    return {
      message: trKeyFormat(
        "Localizable",
        "osd.add_to_playlist",
        "Added %i Files to Playlist",
        Number(playlistMatch[1]),
      ),
    };
  }
  if (message === "Cleared Playlist") {
    return { message: trKey("Localizable", "osd.clear_playlist", "Cleared Playlist") };
  }

  const filterMatch = message.match(/^Added Filter:\s*(.*)$/);
  if (filterMatch) {
    return {
      message: trKeyFormat("Localizable", "osd.filter_added", "Added Filter: %@", filterMatch[1]),
    };
  }

  return { message };
}

function showOsd(message, options = {}) {
  if (osdPersistent && !options.persistent && !options.replacePersistent) return;
  clearOsdTimers();
  els.osd.replaceChildren();
  els.osd.className = "osd";
  osdPersistent = Boolean(options.persistent);
  if (!message || !Boolean(getPreferenceValue("enableOSD"))) {
    hideOsd(true);
    return;
  }

  const textSize = Math.max(5, Number(getPreferenceValue("osdTextSize")) || 20);
  const accessoryTextSize = Math.max(11, Math.min(25, textSize * 0.5));
  els.osd.style.setProperty("--osd-text-size", `${textSize}px`);
  els.osd.style.setProperty("--osd-accessory-text-size", `${accessoryTextSize}px`);

  const content = document.createElement("div");
  content.className = "osd-content";
  const messageEl = document.createElement("div");
  messageEl.className = "osd-message";
  messageEl.textContent = options.literal ? String(message) : tr(String(message));
  content.append(messageEl);

  if (options.detail) {
    const detail = document.createElement("div");
    detail.className = "osd-accessory-text";
    detail.textContent = String(options.detail);
    content.append(detail);
  } else if (Number.isFinite(Number(options.progress))) {
    const progress = document.createElement("progress");
    progress.className = "osd-progress";
    progress.max = 1;
    progress.value = Math.max(0, Math.min(1, Number(options.progress)));
    content.append(progress);
  }

  if (options.previewPath) {
    els.osd.classList.add("osd--screenshot");
    const accessory = document.createElement("div");
    accessory.className = "screenshot-osd-accessory";
    const image = document.createElement("img");
    image.className = "screenshot-osd-image";
    image.alt = "";
    image.src = localFileSrc(options.previewPath);
    if (!options.savedToFile) {
      let cleaned = false;
      const cleanTemporaryScreenshot = () => {
        if (cleaned) return;
        cleaned = true;
        invoke("delete_screenshot_file", { path: options.previewPath }).catch(() => {});
      };
      image.addEventListener("load", cleanTemporaryScreenshot, { once: true });
      image.addEventListener("error", cleanTemporaryScreenshot, { once: true });
      setTimeout(cleanTemporaryScreenshot, 5000);
    }
    accessory.append(image);

    if (options.savedToFile) {
      const actions = document.createElement("div");
      actions.className = "screenshot-osd-actions";
      actions.append(
        screenshotActionButton("DELETE", "delete_screenshot_file", options.previewPath),
        screenshotActionButton("EDIT", "open_screenshot_file", options.previewPath),
        screenshotActionButton("REVEAL", "reveal_screenshot_file", options.previewPath),
      );
      accessory.append(actions);
    }
    content.append(accessory);
  }

  if (options.accessory) {
    els.osd.classList.add("osd--accessory");
    options.accessory.hidden = false;
    content.append(options.accessory);
  }

  els.osd.append(content);
  els.osd.hidden = false;
  if (options.persistent || options.autoHide === false) return;
  const configuredTimeout = Math.max(0, Number(getPreferenceValue("osdAutoHideTimeout")) || 0) * 1000;
  const timeout = Number.isFinite(Number(options.timeout)) ? Math.max(0, Number(options.timeout)) : configuredTimeout;
  osdTimer = setTimeout(() => hideOsd(), timeout);
}

function clearOsdTimers() {
  clearTimeout(osdTimer);
  clearTimeout(osdHideTimer);
  osdTimer = undefined;
  osdHideTimer = undefined;
}

function hideOsd(immediate = false) {
  clearOsdTimers();
  osdPersistent = false;
  if (els.osd.hidden) return;
  if (immediate) {
    els.osd.hidden = true;
    els.osd.classList.remove("osd--hiding");
    return;
  }
  els.osd.classList.add("osd--hiding");
  osdHideTimer = setTimeout(() => {
    els.osd.hidden = true;
    els.osd.classList.remove("osd--hiding");
    osdHideTimer = undefined;
  }, 500);
}

function screenshotActionButton(label, command, path) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "screenshot-osd-button";
  button.textContent = label;
  button.addEventListener("click", async (event) => {
    event.preventDefault();
    event.stopPropagation();
    try {
      await invoke(command, { path });
      hideOsd();
    } catch {
      showOsd("Screenshot Action Failed");
    }
  });
  return button;
}

function timeDisplayPrecision() {
  const precision = Math.round(Number(getPreferenceValue("timeDisplayPrecision")) || 0);
  return Math.max(0, Math.min(3, precision));
}

function formatTimeWithPrecision(seconds, precision = timeDisplayPrecision()) {
  const safeSeconds = Math.max(0, Number(seconds) || 0);
  const total = Math.floor(safeSeconds);
  const hours = Math.floor(total / 3600);
  const mins = Math.floor((total % 3600) / 60);
  const minuteText = mins.toString().padStart(2, "0");
  let secondText;
  if (precision >= 1 && precision <= 3) {
    secondText = (safeSeconds % 60).toFixed(precision).padStart(precision + 3, "0");
  } else {
    secondText = (total % 60).toString().padStart(2, "0");
  }
  if (hours > 0) return `${hours}:${minuteText}:${secondText}`;
  return `${minuteText}:${secondText}`;
}

function formatTime(seconds) {
  const total = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(total / 3600);
  const mins = Math.floor((total % 3600) / 60);
  const secs = total % 60;
  const minuteText = mins.toString().padStart(2, "0");
  const secondText = secs.toString().padStart(2, "0");
  if (hours > 0) {
    return `${hours}:${minuteText}:${secondText}`;
  }
  return `${minuteText}:${secondText}`;
}
