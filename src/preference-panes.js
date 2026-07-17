import { MOUSE_CLICK_ACTION_OPTIONS } from "./mouse-actions.js";

const referenceContext = (table, key) => Object.freeze({ table, key });

export const ADVANCED_HELP_URL = "https://github.com/iina/iina/wiki/MPV-Options-and-Properties";

export const IINA_DEFAULT_OSC_TOOLBAR_BUTTONS = Object.freeze([2, 1, 0]);
export const IINA_OSC_TOOLBAR_BUTTONS = Object.freeze([
  Object.freeze({ value: 0, label: "Quick Settings", l10n: referenceContext("Localizable", "osc_toolbar.settings") }),
  Object.freeze({ value: 1, label: "Playlist and chapters", l10n: referenceContext("Localizable", "osc_toolbar.playlist") }),
  Object.freeze({ value: 2, label: "Toggle Picture-in-Picture", l10n: referenceContext("Localizable", "osc_toolbar.pip") }),
  Object.freeze({ value: 3, label: "Toggle full screen", l10n: referenceContext("Localizable", "osc_toolbar.full_screen") }),
  Object.freeze({ value: 4, label: "Enter music mode", l10n: referenceContext("Localizable", "osc_toolbar.music_mode") }),
  Object.freeze({ value: 5, label: "Choose subtitle track", l10n: referenceContext("Localizable", "osc_toolbar.sub_track") }),
  Object.freeze({ value: 6, label: "Take a screenshot", l10n: referenceContext("Localizable", "osc_toolbar.screenshot") }),
]);

// IINA 1.3.5's CharEncoding.list. Keeping the represented mpv code beside the
// visible title prevents arbitrary values from reaching `sub-codepage`.
export const SUBTITLE_ENCODING_OPTIONS = [
  ["auto", "Auto detect"],
  ["UTF-8", "Universal (UTF-8)"],
  ["UTF-16", "Universal (UTF-16)"],
  ["UTF-16BE", "Universal (UTF-16BE)"],
  ["UTF-16LE", "Universal (UTF-16LE)"],
  ["ISO-8859-6", "Arabic (ISO-8859-6)"],
  ["WINDOWS-1256", "Arabic (WINDOWS-1256)"],
  ["LATIN7", "Baltic (LATIN7)"],
  ["WINDOWS-1257", "Baltic (WINDOWS-1257)"],
  ["LATIN8", "Celtic (LATIN8)"],
  ["WINDOWS-1250", "Central European (WINDOWS-1250)"],
  ["ISO-8859-5", "Cyrillic (ISO-8859-5)"],
  ["WINDOWS-1251", "Cyrillic (WINDOWS-1251)"],
  ["ISO-8859-2", "Eastern European (ISO-8859-2)"],
  ["WINDOWS-1252", "Western Languages (WINDOWS-1252)"],
  ["ISO-8859-7", "Greek (ISO-8859-7)"],
  ["WINDOWS-1253", "Greek (WINDOWS-1253)"],
  ["ISO-8859-8", "Hebrew (ISO-8859-8)"],
  ["WINDOWS-1255", "Hebrew (WINDOWS-1255)"],
  ["SHIFT-JIS", "Japanese (SHIFT-JIS)"],
  ["ISO-2022-JP-2", "Japanese (ISO-2022-JP-2)"],
  ["EUC-KR", "Korean (EUC-KR)"],
  ["CP949", "Korean (CP949)"],
  ["ISO-2022-KR", "Korean (ISO-2022-KR)"],
  ["LATIN6", "Nordic (LATIN6)"],
  ["LATIN4", "North European (LATIN4)"],
  ["KOI8-R", "Russian (KOI8-R)"],
  ["GBK", "Simplified Chinese (GBK)"],
  ["GB18030", "Simplified Chinese (GB18030)"],
  ["ISO-2022-CN-EXT", "Simplified Chinese (ISO-2022-CN-EXT)"],
  ["LATIN3", "South European (LATIN3)"],
  ["LATIN10", "South-Eastern European (LATIN10)"],
  ["TIS-620", "Thai (TIS-620)"],
  ["WINDOWS-874", "Thai (WINDOWS-874)"],
  ["EUC-TW", "Traditional Chinese (EUC-TW)"],
  ["BIG5", "Traditional Chinese (BIG5)"],
  ["BIG5-HKSCS", "Traditional Chinese (BIG5-HKSCS)"],
  ["LATIN5", "Turkish (LATIN5)"],
  ["WINDOWS-1254", "Turkish (WINDOWS-1254)"],
  ["KOI8-U", "Ukrainian (KOI8-U)"],
  ["WINDOWS-1258", "Vietnamese (WINDOWS-1258)"],
  ["VISCII", "Vietnamese (VISCII)"],
  ["LATIN1", "Western European (LATIN1)"],
  ["LATIN-9", "Western European (LATIN-9)"],
];

export const PREFERENCE_PANES = [
  {
    id: "general",
    title: "General",
    l10n: referenceContext("Localizable", "preference.general"),
    sections: [
      {
        title: "Behavior:",
        controls: [
          {
            key: "actionAfterLaunch",
            label: "At launch:",
            type: "select",
            defaultValue: 0,
            options: [
              [0, "Show welcome window"],
              [1, "Show open file panel"],
              [2, "Do nothing", referenceContext("PrefGeneralViewController", "JfZ-D8-aF2.title")],
            ],
          },
          { key: "alwaysOpenInNewWindow", label: "Always open media in new window", type: "checkbox", defaultValue: true },
          { key: "quitWhenNoOpenedWindow", label: "Quit after all windows are closed", type: "checkbox", defaultValue: false },
          { key: "keepOpenOnFileEnd", label: "Keep window open after playback finishes", type: "checkbox", defaultValue: true },
          { key: "resumeLastPosition", label: "Resume last playback position", type: "checkbox", defaultValue: true },
          { key: "useLegacyFullScreen", label: "Use legacy full screen", type: "checkbox", defaultValue: false },
          { key: "blackOutMonitor", label: "Black out other monitors while in full screen", type: "checkbox", defaultValue: false },
          {
            key: "autoSwitchToMusicMode",
            label: "Switch to \"music mode\" automatically when playing an audio file",
            l10n: referenceContext("PrefGeneralViewController", "KfL-r7-Uav.title"),
            type: "checkbox",
            defaultValue: true,
          },
          {
            key: "updaterAutomaticallyChecks",
            intervalKey: "updaterCheckInterval",
            label: "Check for updates",
            type: "updater-check",
            options: [
              [3600, "Hourly"],
              [86400, "Daily"],
              [604800, "Weekly"],
              [2629800, "Monthly"],
            ],
          },
          { key: "receiveBetaUpdate", label: "Receive beta updates", type: "checkbox", defaultValue: false },
          {
            key: "generalMediaOpenedHint",
            label: "When media is opened:",
            l10n: referenceContext("PrefGeneralViewController", "GNV-VA-NBW.title"),
            type: "disclosure",
            disclosureId: "general-media-opened",
            defaultOpen: false,
          },
          {
            key: "pauseWhenOpen",
            label: "Pause",
            l10n: referenceContext("PrefGeneralViewController", "FbF-jJ-W5b.title"),
            type: "checkbox",
            defaultValue: false,
            disclosureGroup: "general-media-opened",
          },
          {
            key: "fullScreenWhenOpen",
            label: "Enter fullscreen",
            type: "checkbox",
            defaultValue: false,
            disclosureGroup: "general-media-opened",
          },
          {
            key: "generalPauseResumeHint",
            label: "Pause/resume when:",
            l10n: referenceContext("PrefGeneralViewController", "xr2-rB-O0x.title"),
            type: "disclosure",
            disclosureId: "general-pause-resume",
            defaultOpen: false,
          },
          { key: "pauseWhenMinimized", label: "Minimized/un-minimized", type: "checkbox", defaultValue: false, disclosureGroup: "general-pause-resume" },
          { key: "pauseWhenInactive", label: "Window becomes inactive/active", type: "checkbox", defaultValue: false, disclosureGroup: "general-pause-resume" },
          { key: "playWhenEnteringFullScreen", label: "Play upon entering full screen", type: "checkbox", defaultValue: false, disclosureGroup: "general-pause-resume" },
          { key: "pauseWhenLeavingFullScreen", label: "Pause upon leaving full screen", type: "checkbox", defaultValue: false, disclosureGroup: "general-pause-resume" },
          { key: "pauseWhenGoesToSleep", label: "Pause when machine goes to sleep", type: "checkbox", defaultValue: true, disclosureGroup: "general-pause-resume" },
        ],
      },
      {
        title: "History:",
        controls: [
          { key: "recordPlaybackHistory", label: "Enable playback history", type: "checkbox", defaultValue: true },
          { key: "recordRecentFiles", label: "Enable \"Open Recent\" menu", type: "checkbox", defaultValue: true },
          {
            key: "trackAllFilesInRecentOpenMenu",
            label: "Track all played files in \"Open Recent\" menu",
            type: "checkbox",
            defaultValue: true,
            dependsOn: "recordRecentFiles",
            hint: "Otherwise only files that opened manually will show up in this menu.",
          },
        ],
      },
      {
        title: "Playlist:",
        controls: [
          {
            key: "playlistAutoAdd",
            label: "Add files in the same folder automatically",
            type: "checkbox",
            defaultValue: true,
            hint: "Requires restarting IINA to take effect.",
            restartRequired: true,
          },
          { key: "playlistAutoPlayNext", label: "Play next item automatically", type: "checkbox", defaultValue: true },
          {
            key: "playlistShowMetadata",
            label: "Show artist and track name for audio files when available",
            type: "checkbox",
            defaultValue: true,
          },
          {
            key: "playlistShowMetadataInMusicMode",
            label: "Only in \"music mode\"",
            type: "checkbox",
            defaultValue: true,
            dependsOn: "playlistShowMetadata",
          },
        ],
      },
      {
        title: "Screenshots:",
        controls: [
          { key: "screenshotSaveToFile", label: "Save to", type: "checkbox", defaultValue: true },
          {
            key: "screenShotFolder",
            label: "Destination:",
            l10n: referenceContext("PrefGeneralViewController", "PhL-vP-i7L.title"),
            type: "folder",
            defaultValue: "~/Pictures/Screenshots",
            pickerCommand: "choose_screenshot_folder",
            dependsOn: "screenshotSaveToFile",
          },
          {
            key: "screenShotFormat",
            label: "Format:",
            l10n: referenceContext("PrefGeneralViewController", "oJY-Iz-3sO.title"),
            type: "select",
            defaultValue: 0,
            dependsOn: "screenshotSaveToFile",
            options: [
              [0, "PNG"],
              [1, "JPEG (.jpg)"],
              [2, "JPEG (.jpeg)"],
            ],
          },
          { key: "screenshotCopyToClipboard", label: "Copy to clipboard", type: "checkbox", defaultValue: false },
          { key: "screenShotIncludeSubtitle", label: "Include subtitles", type: "checkbox", defaultValue: true },
          { key: "screenshotShowPreview", label: "Show previews after taking screenshots", type: "checkbox", defaultValue: true },
        ],
      },
    ],
  },
  {
    id: "ui",
    title: "UI",
    l10n: referenceContext("Localizable", "preference.ui"),
    sections: [
      {
        title: "Appearance:",
        controls: [
          {
            key: "themeMaterial",
            label: "Theme:",
            type: "select",
            defaultValue: 0,
            options: [
              [0, "Dark"],
              [2, "Light"],
              [4, "Match System Appearance"],
            ],
          },
        ],
      },
      {
        title: "Window:",
        l10n: referenceContext("PrefUIViewController", "GKy-3g-4MB.title"),
        controls: [
          {
            key: "usePhysicalResolution",
            label: "Use physical resolution on Retina displays",
            type: "checkbox",
            defaultValue: true,
          },
          {
            key: "resizeWindowTiming",
            label: "Resize the window to fit video size:",
            l10n: referenceContext("PrefUIViewController", "Zjr-q7-WsD.title"),
            type: "radio-group",
            defaultValue: 1,
            searchLabels: [
              { label: "Resize the window to fit video size:", l10n: referenceContext("PrefUIViewController", "Zjr-q7-WsD.title") },
              { label: "Always", l10n: referenceContext("PrefUIViewController", "yIN-jg-MxS.title") },
              { label: "When media is opened manually", l10n: referenceContext("PrefUIViewController", "CJE-ap-IH8.title") },
              { label: "Disabled", l10n: referenceContext("PrefUIViewController", "duf-3s-3bL.title") },
            ],
            options: [
              [0, "Always"],
              [1, "When media is opened manually"],
              [2, "Disabled", referenceContext("PrefUIViewController", "duf-3s-3bL.title")],
            ],
          },
          {
            key: "resizeWindowOption",
            label: "Resize window to:",
            type: "select",
            defaultValue: 2,
            dependsOn: { key: "resizeWindowTiming", notEquals: 2 },
            options: [
              [0, "fit the screen"],
              [1, "0.5x video size"],
              [2, "1x video size"],
              [3, "1.5x video size"],
              [4, "2x video size"],
            ],
          },
          {
            key: "alwaysFloatOnTop",
            label: "Always float on top while playing",
            type: "checkbox",
            defaultValue: false,
          },
          {
            key: "initialWindowSizePosition",
            label: "Initial window size and position:",
            type: "window-geometry",
            defaultValue: "",
            searchLabels: [
              {
                label: "Initial window size:",
                l10n: referenceContext("PrefUIViewController", "zOq-Em-wUe.title"),
                targetKey: "initialWindowSizePosition:size",
              },
              {
                label: "Initial window position:",
                l10n: referenceContext("PrefUIViewController", "Ofm-qE-RgQ.title"),
                targetKey: "initialWindowSizePosition:position",
              },
            ],
          },
          {
            key: "alwaysShowOnTopIcon",
            label: "Always show float on top status in the title bar",
            type: "checkbox",
            defaultValue: false,
          },
        ],
      },
      {
        title: "On Screen Controller:",
        controls: [
          {
            key: "oscPosition",
            label: "Layout:",
            type: "osc-layout",
            defaultValue: 0,
            options: [
              [0, "Floating"],
              [1, "Top", referenceContext("PrefUIViewController", "se2-DE-pJk.title")],
              [2, "Bottom", referenceContext("PrefUIViewController", "0Gp-vj-UzY.title")],
            ],
          },
          {
            key: "controlBarAutoHideTimeout",
            label: "Auto hide after:",
            l10n: referenceContext("PrefUIViewController", "82A-XQ-BvS.title"),
            suffix: "s",
            type: "number",
            min: 0,
            step: 0.1,
            defaultValue: 2.5,
          },
          { key: "controlBarStickToCenter", label: "Snap to center when dragging", type: "checkbox", defaultValue: true },
          { key: "showChapterPos", label: "Show chapter position in progress bar", type: "checkbox", defaultValue: false },
          { key: "showRemainingTime", label: "Show remaining time instead of total duration", type: "checkbox", defaultValue: false },
          {
            key: "arrowBtnAction",
            label: "Use left/right button for:",
            type: "select",
            defaultValue: 0,
            options: [
              [0, "Speed", referenceContext("PrefUIViewController", "TWq-Xb-8Ee.title")],
              [1, "Previous / Next Media"],
              [2, "Fast Forward / Rewind"],
            ],
          },
          {
            key: "controlBarToolbarButtons",
            label: "Toolbar:",
            type: "osc-toolbar",
            defaultValue: [2, 1, 0],
            searchLabels: ["Toolbar:", "Customize…"],
          },
        ],
      },
      {
        title: "On Screen Display:",
        controls: [
          { key: "enableOSD", label: "Enable OSD", type: "checkbox", defaultValue: true },
          {
            key: "osdAutoHideTimeout",
            label: "Auto hide after:",
            l10n: referenceContext("PrefUIViewController", "NUr-hY-gfo.title"),
            suffix: "s",
            type: "number",
            min: 0,
            step: 0.1,
            defaultValue: 1,
            dependsOn: "enableOSD",
          },
          {
            key: "osdTextSize",
            label: "Text size:",
            suffix: "pt",
            type: "number",
            min: 5,
            step: 1,
            defaultValue: 20,
            dependsOn: "enableOSD",
          },
          {
            key: "displayTimeAndBatteryInFullScreen",
            label: "Display time and battery info when in full screen",
            type: "checkbox",
            defaultValue: false,
            dependsOn: "enableOSD",
          },
        ],
      },
      {
        title: "Thumbnail Preview:",
        controls: [
          { key: "enableThumbnailPreview", label: "Enable thumbnail preview", type: "checkbox", defaultValue: true },
          { key: "maxThumbnailPreviewCacheSize", label: "Maximum cache size:", suffix: "MB", type: "number", min: 0, step: 1, defaultValue: 500, dependsOn: "enableThumbnailPreview" },
        ],
      },
      {
        title: "Picture-in-Picture:",
        controls: [
          {
            key: "windowBehaviorWhenPip",
            label: "When entering Picture-in-Picture:",
            l10n: referenceContext("PrefUIViewController", "EUd-RD-g0T.title"),
            secondaryLabel: "Window:",
            secondaryL10n: referenceContext("PrefUIViewController", "AmS-Sl-b2h.title"),
            type: "radio-group",
            defaultValue: 0,
            searchLabels: [
              { label: "When entering Picture-in-Picture:", l10n: referenceContext("PrefUIViewController", "EUd-RD-g0T.title") },
              { label: "Window:", l10n: referenceContext("PrefUIViewController", "AmS-Sl-b2h.title") },
              { label: "Do nothing", l10n: referenceContext("PrefUIViewController", "Zc7-sP-Rdp.title") },
              { label: "Hide", l10n: referenceContext("PrefUIViewController", "xtL-XT-xzb.title") },
              { label: "Minimize", l10n: referenceContext("PrefUIViewController", "H1F-mm-Saq.title") },
            ],
            options: [
              [0, "Do nothing", referenceContext("PrefUIViewController", "Zc7-sP-Rdp.title")],
              [1, "Hide"],
              [2, "Minimize", referenceContext("PrefUIViewController", "H1F-mm-Saq.title")],
            ],
          },
          { key: "pauseWhenPip", label: "Automatically pause playback", type: "checkbox", defaultValue: false },
          {
            key: "togglePipByMinimizingWindow",
            label: "Toggle Picture-in-Picture by minimizing/un-minimizing the window",
            type: "checkbox",
            defaultValue: false,
          },
        ],
      },
    ],
  },
  {
    id: "video_audio",
    title: "Video/Audio",
    l10n: referenceContext("Localizable", "preference.video_audio"),
    sections: [
      {
        title: "Video:",
        controls: [
          {
            key: "videoThreads",
            label: "Number of threads:",
            l10n: referenceContext("PrefCodecViewController", "50C-1R-wlJ.title"),
            type: "number",
            min: 0,
            step: 1,
            defaultValue: 0,
            hint: "Default: 0 (Auto)",
          },
          {
            key: "hardwareDecoder",
            label: "Hardware decoder:",
            type: "select",
            defaultValue: 1,
            options: [
              [0, "Disabled", referenceContext("PrefCodecViewController", "rFu-rL-EJC.title")],
              [1, "Auto", referenceContext("PrefCodecViewController", "tAQ-oU-1AJ.title")],
              [2, "Auto (Copy)"],
            ],
            descriptions: {
              0: "Disable hardware decoding.",
              1: "Enable hardware decoding. However, most of the video filters won't work properly.",
              2: "Enable hardware decoding. It will consume double the memory but work with video filters.",
            },
          },
          {
            key: "forceDedicatedGPU",
            label: "Force dedicated GPU",
            type: "checkbox",
            defaultValue: false,
            hint: "Always use the dedicated GPU for rendering (if it exists). This can improve performance but may reduce battery life.",
          },
          {
            key: "loadIccProfile",
            label: "Load ICC profile",
            type: "checkbox",
            defaultValue: true,
            hint: "Load the ICC profile for current display and use it to transform video RGB to screen output. (Only for SDR mode)",
          },
          {
            key: "enableHdrSupport",
            label: "Enable HDR mode",
            type: "checkbox",
            defaultValue: true,
            hint: "Enable HDR mode by default when playing HDR videos.",
          },
          {
            key: "enableToneMapping",
            label: "Enable tone mapping",
            type: "checkbox",
            defaultValue: false,
            helpUrl: "https://en.wikipedia.org/wiki/Tone_mapping",
          },
          {
            key: "toneMappingTargetPeak",
            label: "Target peak:",
            type: "number",
            min: 0,
            step: 1,
            defaultValue: 0,
            suffix: "nits",
            dependsOn: "enableToneMapping",
            hint: "Measured peak brightness of the display. If set to 0, IINA detects this value. If detection fails, IINA sets the target peak to 400.",
            helpUrl: "https://mpv.io/manual/stable/#options-target-peak",
          },
          {
            key: "toneMappingAlgorithm",
            label: "Algorithm:",
            type: "select",
            defaultValue: 0,
            dependsOn: "enableToneMapping",
            options: [
              [0, "auto", { algorithm: "auto" }],
              [1, "clip", { algorithm: "clip" }],
              [2, "mobius", { algorithm: "mobius" }],
              [3, "reinhard", { algorithm: "reinhard" }],
              [4, "hable", { algorithm: "hable" }],
              [5, "bt.2390", { algorithm: "bt.2390" }],
              [6, "gamma", { algorithm: "gamma" }],
              [7, "linear", { algorithm: "linear" }],
            ],
            helpUrl: "https://mpv.io/manual/stable/#options-tone-mapping",
          },
        ],
      },
      {
        title: "Audio:",
        controls: [
          {
            key: "audioThreads",
            label: "Number of threads:",
            l10n: referenceContext("PrefCodecViewController", "tO4-pw-2W9.title"),
            type: "number",
            min: 0,
            step: 1,
            defaultValue: 0,
            hint: "Default: 0 (Auto)",
          },
          {
            key: "audioLanguage",
            label: "Preferred language:",
            l10n: referenceContext("PrefCodecViewController", "wjt-O6-KbC.title"),
            type: "tokens",
            defaultValue: "",
          },
          {
            key: "spdifOutput",
            label: "S/PDIF output:",
            type: "checkbox-group",
            items: [
              { key: "spdifAC3", label: "AC3", defaultValue: false },
              { key: "spdifDTS", label: "DTS", defaultValue: false },
              { key: "spdifDTSHD", label: "DTS-HD", defaultValue: false },
            ],
          },
          {
            key: "enableInitialVolume",
            label: "Initial volume:",
            type: "checkbox-number",
            valueKey: "initialVolume",
            step: 1,
            defaultValue: false,
            valueDefault: 100,
          },
          {
            key: "maxVolume",
            label: "Maximum volume:",
            type: "number",
            min: 100,
            max: 1000,
            step: 1,
            defaultValue: 100,
            hint: "(100-1000)",
          },
          {
            key: "audioDevice",
            descriptionKey: "audioDeviceDesc",
            label: "Preferred audio device:",
            type: "audio-device",
            defaultValue: "auto",
            descriptionDefault: "Autoselect device",
          },
        ],
      },
    ],
  },
  {
    id: "subtitle",
    title: "Subtitle",
    l10n: referenceContext("Localizable", "preference.subtitle"),
    sections: [
      {
        title: "Auto Load:",
        controls: [
          {
            key: "subAutoLoadIINA",
            label: "",
            ariaLabel: "Auto Load:",
            type: "select",
            defaultValue: 2,
            searchLabels: [],
            options: [
              [0, "Disabled", referenceContext("PrefSubViewController", "u31-eV-Arr.title")],
              [1, "Subtitles containing media filename"],
              [2, "Detect intelligently by IINA"],
            ],
          },
          {
            key: "subAutoLoadAdvancedDisclosure",
            label: "Advanced",
            l10n: referenceContext("PrefSubViewController", "DUA-7H-Yje.title"),
            type: "disclosure",
            disclosureId: "subtitle-auto-load-advanced",
            defaultOpen: false,
          },
          {
            key: "subAutoLoadPriorityString",
            label: "Subtitles have priority when filename containing:",
            l10n: referenceContext("PrefSubViewController", "7pf-Kb-TNr.title"),
            type: "text",
            defaultValue: "",
            hint: "Please enter a comma-separated list of strings.",
            hintL10n: referenceContext("PrefSubViewController", "Mk1-sv-QB8.title"),
            disclosureGroup: "subtitle-auto-load-advanced",
          },
          {
            key: "subAutoLoadSearchPath",
            label: "Also search subtitles in following directories:",
            l10n: referenceContext("PrefSubViewController", "QsS-wD-2hY.title"),
            type: "text",
            defaultValue: "./*",
            hint: "Directories are separated by colons (:). Relative paths and wildcards (*) at the end are allowed.",
            hintL10n: referenceContext("PrefSubViewController", "Luv-2B-9v8.title"),
            disclosureGroup: "subtitle-auto-load-advanced",
          },
        ],
      },
      {
        title: "ASS Subtitles:",
        controls: [
          {
            key: "ignoreAssStyles",
            label: "Ignore ASS styles",
            type: "checkbox",
            defaultValue: false,
            hint: "If enabled, all ASS subtitles will be drawn using the styles below.",
          },
          {
            key: "subOverrideLevel",
            label: "Override level:",
            type: "slider",
            defaultValue: 2,
            min: 0,
            max: 2,
            step: 1,
            options: [
              [0, "yes"],
              [1, "force"],
              [2, "strip"],
            ],
          },
        ],
      },
      {
        title: "Text Subtitles:",
        controls: [
          {
            key: "subTextFont",
            label: "Font:",
            type: "font",
            pickerCommand: "choose_subtitle_font_dialog",
            defaultValue: "sans-serif",
          },
          {
            key: "subTextSize",
            label: "Size:",
            l10n: referenceContext("PrefSubViewController", "Vbl-Im-3UI.title"),
            type: "number",
            min: 0,
            step: 0.1,
            defaultValue: 55,
          },
          { key: "subTextColor", label: "Color:", type: "color", defaultValue: "1/1/1/1" },
          { key: "subBgColor", label: "Background:", type: "color", defaultValue: "0/0/0/0" },
          { key: "subBold", label: "Bold", type: "checkbox", defaultValue: false },
          { key: "subItalic", label: "Italic", type: "checkbox", defaultValue: false },
          {
            key: "subtitleBorderGroup",
            label: "Border",
            l10n: referenceContext("PrefSubViewController", "t5n-mc-dss.title"),
            type: "group-heading",
          },
          { key: "subBorderSize", label: "Size:", l10n: referenceContext("PrefSubViewController", "gkc-qt-sD9.title"), type: "number", min: 0, step: 0.1, defaultValue: 3 },
          { key: "subBorderColor", label: "Color:", l10n: referenceContext("PrefSubViewController", "vDc-dw-liI.title"), type: "color", defaultValue: "0/0/0/1" },
          {
            key: "subTextAdvancedDisclosure",
            label: "Advanced",
            l10n: referenceContext("PrefSubViewController", "X7X-18-FJg.title"),
            type: "disclosure",
            disclosureId: "subtitle-text-advanced",
            defaultOpen: false,
          },
          {
            key: "subtitleShadowGroup",
            label: "Shadow",
            l10n: referenceContext("PrefSubViewController", "BgE-Kk-MXC.title"),
            type: "group-heading",
            disclosureGroup: "subtitle-text-advanced",
          },
          { key: "subShadowSize", label: "Offset:", l10n: referenceContext("PrefSubViewController", "dtP-9o-yhy.title"), type: "number", min: 0, step: 0.1, defaultValue: 0, disclosureGroup: "subtitle-text-advanced" },
          { key: "subShadowColor", label: "Color:", l10n: referenceContext("PrefSubViewController", "SIh-Sv-6XI.title"), type: "color", defaultValue: "0/0/0/0", disclosureGroup: "subtitle-text-advanced" },
          {
            key: "subtitleOtherStylesGroup",
            label: "Other Styles",
            l10n: referenceContext("PrefSubViewController", "egf-g3-b7a.title"),
            type: "group-heading",
            disclosureGroup: "subtitle-text-advanced",
          },
          { key: "subBlur", label: "Blur:", l10n: referenceContext("PrefSubViewController", "aO9-D6-KG8.title"), type: "number", min: 0, max: 20, step: 0.1, defaultValue: 0, disclosureGroup: "subtitle-text-advanced" },
          { key: "subSpacing", label: "Font spacing:", l10n: referenceContext("PrefSubViewController", "9EP-XB-lyO.title"), type: "number", step: 0.1, defaultValue: 0, disclosureGroup: "subtitle-text-advanced" },
        ],
      },
      {
        title: "Position:",
        l10n: referenceContext("PrefSubViewController", "4Tb-Yh-PdP.title"),
        controls: [
          {
            key: "subAlignX",
            label: "Align X:",
            type: "select",
            defaultValue: 1,
            options: [
              [0, "Left"],
              [1, "Center", referenceContext("PrefSubViewController", "slU-Ox-aeF.title")],
              [2, "Right"],
            ],
          },
          {
            key: "subAlignY",
            label: "Align Y:",
            type: "select",
            defaultValue: 2,
            options: [
              [0, "Top", referenceContext("PrefSubViewController", "1Tq-5k-o6R.title")],
              [1, "Center", referenceContext("PrefSubViewController", "e0e-dB-Ir6.title")],
              [2, "Bottom", referenceContext("PrefSubViewController", "6i0-Sx-V7F.title")],
            ],
          },
          { key: "subMarginX", label: "Margin X:", type: "number", step: 1, defaultValue: 25 },
          { key: "subMarginY", label: "Margin Y:", type: "number", step: 1, defaultValue: 22 },
          { key: "subPos", label: "Vertical position:", type: "number", min: 0, max: 100, step: 0.1, suffix: "%", defaultValue: 100 },
          { key: "displayInLetterBox", label: "Display subtitles in letterboxes while in full screen", type: "checkbox", defaultValue: true },
          { key: "subScaleWithWindow", label: "Scale subtitles with window size", type: "checkbox", defaultValue: true },
        ],
      },
      {
        title: "Online Subtitles:",
        controls: [
          {
            key: "onlineSubProvider",
            label: "Download subtitles from:",
            l10n: referenceContext("PrefSubViewController", "TPK-CJ-HX7.title"),
            type: "select",
            defaultValue: ":opensubtitles",
            options: [
              [":opensubtitles", "opensubtitles.com"],
              [":assrt", "assrt.net"],
              [":shooter", "shooter.cn"],
            ],
          },
          {
            key: "openSubUsername",
            label: "OpenSubtitles account:",
            type: "opensubtitles-account",
            defaultValue: "",
            visibleWhen: ["onlineSubProvider", ":opensubtitles"],
            helpUrl: "https://github.com/iina/iina/wiki/Download-Online-Subtitles#opensubtitles",
          },
          {
            key: "assrtToken",
            label: "Assrt API token:",
            l10n: referenceContext("PrefSubViewController", "0aU-O1-R6G.title"),
            type: "text",
            defaultValue: "",
            visibleWhen: ["onlineSubProvider", ":assrt"],
            helpUrl: "https://github.com/iina/iina/wiki/Download-Online-Subtitles#assrt",
          },
          {
            key: "autoSearchOnlineSub",
            label: "Search online subtitles automatically",
            l10n: referenceContext("PrefSubViewController", "C3p-uP-8u8.title"),
            type: "checkbox",
            defaultValue: false,
          },
          {
            key: "autoSearchOnlineSubHint",
            label: "If enabled, IINA will automatically search online subtitles only for videos without loaded subtitles and longer than 20 mintues.",
            type: "hint",
          },
        ],
      },
      {
        title: "Other:",
        controls: [
          {
            key: "subLang",
            label: "Preferred language:",
            l10n: referenceContext("PrefSubViewController", "zaE-bH-XfT.title"),
            type: "tokens",
            defaultValue: "",
            hint: "This option will be stored as ISO 639-2 language code and will works for both mpv and opensubtitles.",
            hintL10n: referenceContext("PrefSubViewController", "Z2M-dh-eq1.title"),
          },
          {
            key: "defaultEncoding",
            label: "Default encoding:",
            type: "select",
            defaultValue: "auto",
            options: SUBTITLE_ENCODING_OPTIONS,
          },
        ],
      },
    ],
  },
  {
    id: "network",
    title: "Network",
    l10n: referenceContext("Localizable", "preference.network"),
    sections: [
      {
        title: "Cache:",
        controls: [
          { key: "enableCache", label: "Enable cache", type: "checkbox", defaultValue: true },
          {
            key: "defaultCacheSize",
            label: "Default cache size (KB):",
            type: "number",
            min: 0,
            step: 1,
            defaultValue: 153600,
            dependsOn: "enableCache",
            hint: "Default: 153600",
          },
          {
            key: "secPrefech",
            label: "Seconds to prefetch:",
            type: "number",
            min: 0,
            step: 1,
            defaultValue: 36000,
            dependsOn: "enableCache",
            hint: "Default: 36000",
          },
        ],
      },
      {
        title: "Network:",
        controls: [
          { key: "userAgent", label: "User agent:", type: "text", defaultValue: "" },
          {
            key: "httpProxy",
            label: "HTTP proxy:",
            type: "text",
            defaultValue: "",
            prefix: "http://",
            hint: "Requires restarting IINA to take effect.",
          },
          {
            key: "transportRTSPThrough",
            label: "Transport RTSP stream through:",
            type: "select",
            defaultValue: 1,
            options: [
              [0, "Auto", referenceContext("PrefNetworkViewController", "aNr-lv-K0r.title")],
              [1, "TCP"],
              [2, "UDP"],
              [3, "HTTP"],
            ],
          },
        ],
      },
      {
        title: "youtube-dl:",
        controls: [
          {
            key: "ytdlEnabled",
            label: "Enable youtube-dl",
            type: "checkbox",
            defaultValue: true,
            helpUrl: "https://github.com/rg3/youtube-dl/blob/master/README.md#readme",
          },
          {
            key: "ytdlSearchPath",
            label: "Custom youtube-dl path:",
            type: "text",
            defaultValue: "",
            placeholder: "/usr/local/bin",
            dependsOn: "ytdlEnabled",
            hint: "IINA will search youtube-dl in this folder. Restart needed.",
          },
          {
            key: "ytdlRawOptions",
            label: "Raw options:",
            type: "text",
            defaultValue: "",
            dependsOn: "ytdlEnabled",
            hint: "Format: <key>=<value>[,<key>=<value>[,...]]",
          },
        ],
      },
    ],
  },
  {
    id: "control",
    title: "Control",
    l10n: referenceContext("Localizable", "preference.control"),
    sections: [
      {
        title: "Trackpad:",
        l10n: referenceContext("PrefControlViewController", "I6k-ab-IwJ.title"),
        controls: [
          {
            key: "pinchAction",
            label: "Pinch to:",
            l10n: referenceContext("PrefControlViewController", "E8H-pO-MLc.title"),
            type: "select",
            defaultValue: 0,
            options: [
              [0, "Adjust window size", referenceContext("PrefControlViewController", "Ols-Lr-fiH.title")],
              [1, "Fullscreen", referenceContext("PrefControlViewController", "Hm3-dh-jgb.title")],
              [2, "None", referenceContext("PrefControlViewController", "chS-d2-TG1.title")],
            ],
          },
          {
            key: "forceTouchAction",
            label: "Force Touch to:",
            l10n: referenceContext("PrefControlViewController", "dpH-PO-WKt.title"),
            type: "select",
            defaultValue: 0,
            options: MOUSE_CLICK_ACTION_OPTIONS.forceTouchAction,
          },
        ],
      },
      {
        title: "Mouse:",
        l10n: referenceContext("PrefControlViewController", "eA4-AD-oUu.title"),
        controls: [
          {
            key: "verticalScrollAction",
            label: "Scroll vertically to:",
            l10n: referenceContext("PrefControlViewController", "hNU-dJ-5Cj.title"),
            type: "select",
            defaultValue: 0,
            options: [
              [0, "Adjust volume", referenceContext("PrefControlViewController", "SYg-hW-b6r.title")],
              [1, "Seek", referenceContext("PrefControlViewController", "RCe-3m-q9N.title")],
              [2, "None", referenceContext("PrefControlViewController", "NjV-fH-YgN.title")],
            ],
          },
          {
            key: "horizontalScrollAction",
            label: "Scroll horizontally to:",
            l10n: referenceContext("PrefControlViewController", "w1Y-jy-vNc.title"),
            type: "select",
            defaultValue: 1,
            options: [
              [1, "Seek", referenceContext("PrefControlViewController", "wcS-tP-wav.title")],
              [2, "None", referenceContext("PrefControlViewController", "guu-dK-YfL.title")],
            ],
          },
          {
            key: "useExactSeek",
            label: "Seek type:",
            l10n: referenceContext("PrefControlViewController", "1yb-m4-1sK.title"),
            type: "select",
            defaultValue: 0,
            options: [
              [0, "Keyframe seek", referenceContext("PrefControlViewController", "wBB-fx-Mao.title")],
              [1, "Exact seek", referenceContext("PrefControlViewController", "70P-6t-4jC.title")],
              [2, "Auto", referenceContext("PrefControlViewController", "K9H-w9-wmT.title")],
            ],
          },
          {
            key: "exactSeekHint",
            label: "Exact seek is more precise but can cause lag and higher CPU usage.",
            l10n: referenceContext("PrefControlViewController", "6FZ-Qm-0k0.title"),
            type: "hint",
          },
          {
            key: "relativeSeekAmount",
            label: "Sensitivity for normal seek:",
            l10n: referenceContext("PrefControlViewController", "of2-ec-OkC.title"),
            type: "slider",
            defaultValue: 3,
            min: 1,
            max: 4,
            step: 1,
          },
          {
            key: "volumeScrollAmount",
            label: "Sensitivity for volume:",
            l10n: referenceContext("PrefControlViewController", "YQH-1a-WcM.title"),
            type: "slider",
            defaultValue: 3,
            min: 1,
            max: 4,
            step: 1,
          },
          {
            key: "videoViewAcceptsFirstMouse",
            label: "Accepts first mouse click when not focused",
            l10n: referenceContext("PrefControlViewController", "gZK-ry-lQs.title"),
            type: "checkbox",
            defaultValue: false,
          },
          {
            key: "singleClickAction",
            label: "Single click to:",
            l10n: referenceContext("PrefControlViewController", "QhO-As-ZLL.title"),
            type: "select",
            defaultValue: 3,
            options: MOUSE_CLICK_ACTION_OPTIONS.singleClickAction,
          },
          {
            key: "doubleClickAction",
            label: "Double click to:",
            l10n: referenceContext("PrefControlViewController", "GPH-fM-XHN.title"),
            type: "select",
            defaultValue: 1,
            options: MOUSE_CLICK_ACTION_OPTIONS.doubleClickAction,
          },
          {
            key: "rightClickAction",
            label: "Right click to:",
            l10n: referenceContext("PrefControlViewController", "Rqy-g2-AP2.title"),
            type: "select",
            defaultValue: 2,
            options: MOUSE_CLICK_ACTION_OPTIONS.rightClickAction,
          },
          {
            key: "middleClickAction",
            label: "Middle click to:",
            l10n: referenceContext("PrefControlViewController", "Gyd-Qt-FEo.title"),
            type: "select",
            defaultValue: 0,
            options: MOUSE_CLICK_ACTION_OPTIONS.middleClickAction,
          },
        ],
      },
    ],
  },
  {
    id: "keybindings",
    title: "Key Bindings",
    l10n: referenceContext("Localizable", "preference.keybindings"),
    sections: [
      {
        title: "Configuration:",
        controls: [
          {
            key: "currentInputConfigName",
            label: "Current configuration",
            type: "keybinding-profile",
            searchLabels: ["Current configuration", "New", "Duplicate", "Import", "Delete", "Reveal"],
          },
          {
            key: "useMediaKeys",
            label: "Use system media control",
            l10n: referenceContext("PrefKeyBindingViewController", "2WY-CR-baD.title"),
            type: "checkbox",
          },
          { key: "displayKeyBindingRawValues", label: "Display raw values", type: "checkbox" },
        ],
      },
      {
        title: "Bindings:",
        controls: [{
          key: "modeledKeyBindings",
          label: "Key mappings",
          type: "keybindings",
          searchLabels: ["Add", "Export", "Keep Last", "Reload", "All", "Conflicts", "Raw", "Key", "Modifiers", "Action"],
        }],
      },
    ],
  },
  {
    id: "advanced",
    title: "Advanced",
    l10n: referenceContext("Localizable", "preference.advanced"),
    sections: [
      {
        title: "Advanced Settings:",
        controls: [
          {
            key: "enableAdvancedSettings",
            label: "Enable advanced settings",
            l10n: referenceContext("PrefAdvancedViewController", "7ml-VU-xWv.title"),
            type: "checkbox",
            defaultValue: false,
            helpUrl: ADVANCED_HELP_URL,
            helpCommand: "open_advanced_help",
          },
          {
            key: "advancedRestartHint",
            label: "The following settings will require restarting IINA to take effect.",
            type: "hint",
          },
        ],
      },
      {
        title: "Logging:",
        controls: [
          {
            key: "logLevel",
            label: "Log level:",
            type: "select",
            defaultValue: 1,
            dependsOn: "enableAdvancedSettings",
            options: [
              [0, "Verbose"],
              [1, "Debug"],
              [2, "Warning", referenceContext("PrefAdvancedViewController", "exP-JC-h7t.title")],
              [3, "Error", referenceContext("PrefAdvancedViewController", "sl8-Df-vbX.title")],
            ],
          },
          {
            key: "enableLogging",
            label: "Enable logging to file",
            type: "checkbox",
            defaultValue: false,
            dependsOn: "enableAdvancedSettings",
          },
          {
            key: "advancedLogActions",
            type: "advanced-actions",
            dependsOn: "enableAdvancedSettings",
            searchLabels: ["Show log viewer", "Open log directory"],
            actions: [
              { command: "show_log_viewer", label: "Show log viewer" },
              { command: "open_log_directory", label: "Open log directory" },
            ],
          },
        ],
      },
      {
        title: "Settings:",
        controls: [
          {
            key: "useMpvOsd",
            label: "Use mpv's OSD",
            type: "checkbox",
            defaultValue: false,
            dependsOn: "enableAdvancedSettings",
          },
          {
            key: "userOptions",
            label: "Additional mpv options",
            type: "advanced-options",
            defaultValue: [],
            dependsOn: "enableAdvancedSettings",
          },
          {
            key: "useUserDefinedConfDir",
            label: "Use config directory:",
            type: "checkbox",
            defaultValue: false,
            dependsOn: "enableAdvancedSettings",
          },
          {
            key: "userDefinedConfDir",
            label: "Config directory",
            type: "advanced-config-directory",
            defaultValue: "~/.config/mpv/",
            placeholder: "~/.config/mpv/",
            pickerCommand: "choose_advanced_config_directory",
            dependsOn: ["enableAdvancedSettings", "useUserDefinedConfDir"],
            searchLabels: ["Config directory", "Choose directory…"],
          },
        ],
      },
    ],
  },
  {
    id: "plugins",
    title: "Plugins",
    l10n: referenceContext("Localizable", "preference.plugins"),
    sections: [
      {
        title: "Plugins:",
        controls: [
          { key: "iinaEnablePluginSystem", label: "Enable plugin system", type: "checkbox" },
          {
            key: "pluginManager",
            label: "Installed plugins",
            type: "plugins",
            searchLabels: ["Installed plugins", "Install...", "GitHub..."],
          },
        ],
      },
    ],
  },
  {
    id: "utilities",
    title: "Utilities",
    l10n: referenceContext("Localizable", "preference.utilities"),
    sections: [
      {
        title: "Default Application",
        controls: [
          {
            key: "defaultApplication",
            label: "Set IINA as the Default Application…",
            type: "utility-default-application",
          },
        ],
      },
      {
        title: "Restore Alerts",
        controls: [
          {
            key: "restoreSuppressedAlerts",
            label: "Restore Suppressed Alerts…",
            type: "utility-restore-alerts",
          },
        ],
      },
      {
        title: "Clear Cache",
        controls: [
          {
            key: "utilityCache",
            label: "Clear playback data and thumbnail cache",
            type: "utility-clear-cache",
            searchLabels: [
              { label: "Clear Saved Playback Progress…", l10n: referenceContext("PrefUtilsViewController", "4nM-C4-9oM.title") },
              { label: "Clear Playback History…", l10n: referenceContext("PrefUtilsViewController", "MP6-Em-Lp4.title") },
              { label: "Clear Thumbnail Cache…", l10n: referenceContext("PrefUtilsViewController", "hsG-JB-TT9.title") },
              { label: "Current thumbnail cache:", l10n: referenceContext("PrefUtilsViewController", "uV3-aF-UB7.title") },
            ],
          },
        ],
      },
      {
        title: "Get Browser Extensions for IINA",
        controls: [
          {
            key: "browserExtensions",
            label: "Chrome Firefox",
            type: "utility-browser-extensions",
            searchLabels: [
              { label: "Chrome", l10n: referenceContext("PrefUtilsViewController", "yuw-2i-ICd.title") },
              { label: "Firefox", l10n: referenceContext("PrefUtilsViewController", "f9i-sS-MJN.title") },
            ],
          },
        ],
      },
    ],
  },
];

export const PREFERENCE_PANE_ORDER = Object.freeze(PREFERENCE_PANES.map((pane) => pane.id));

export function preferenceDisclosureChildren(controls, disclosureId) {
  return (controls || []).filter((control) => control.disclosureGroup === disclosureId);
}

export function preferenceTopLevelControls(controls) {
  return (controls || []).filter((control) => !control.disclosureGroup);
}

export function preferenceControlByKey(key) {
  for (const pane of PREFERENCE_PANES) {
    for (const section of pane.sections) {
      for (const control of section.controls) {
        if (control.key === key || control.valueKey === key) return control;
        if (control.items?.some((item) => item.key === key)) return control;
      }
    }
  }
  return undefined;
}

export function preferenceControlEnabledForValues(control, values = {}) {
  if (!control?.dependsOn) return true;
  const dependencies = Array.isArray(control.dependsOn) ? control.dependsOn : [control.dependsOn];
  return dependencies.every((dependency) => {
    if (typeof dependency === "string") return Boolean(values[dependency]);
    if (!dependency || typeof dependency !== "object") return true;
    if (Object.hasOwn(dependency, "equals")) {
      return String(values[dependency.key]) === String(dependency.equals);
    }
    if (Object.hasOwn(dependency, "notEquals")) {
      return String(values[dependency.key]) !== String(dependency.notEquals);
    }
    return Boolean(values[dependency.key]);
  });
}

const IINA_GEOMETRY_PATTERN = /^(?:(\d+%?)?(?:x(\d+%?))?)?(?:([+-])(\d+%?)([+-])(\d+%?))?$/u;

function geometryMagnitude(value, fallback) {
  const normalized = String(value ?? "").trim();
  return /^\d+$/u.test(normalized) ? normalized : fallback;
}

/**
 * Parses the mpv geometry subset edited by IINA 1.3.5's UI pane.
 *
 * IINA's editor exposes one size dimension at a time. If an imported geometry
 * contains both dimensions, its Swift controller gives height precedence.
 */
export function parseIinaWindowGeometry(rawValue) {
  const defaults = {
    sizeEnabled: false,
    sizeDimension: "width",
    sizeValue: "1280",
    sizeUnit: "point",
    positionEnabled: false,
    xOffset: "20",
    xUnit: "point",
    xAnchor: "left",
    yOffset: "20",
    yUnit: "point",
    yAnchor: "top",
  };
  const geometry = String(rawValue ?? "");
  if (!geometry) return defaults;
  const match = IINA_GEOMETRY_PATTERN.exec(geometry);
  if (!match) return defaults;

  const [, width, height, xSign, x, ySign, y] = match;
  const size = height || width;
  const hasPosition = Boolean(xSign && x && ySign && y);
  return {
    ...defaults,
    sizeEnabled: Boolean(size),
    sizeDimension: height ? "height" : "width",
    sizeValue: size ? size.replace(/%$/u, "") : defaults.sizeValue,
    sizeUnit: size?.endsWith("%") ? "percent" : "point",
    positionEnabled: hasPosition,
    xOffset: hasPosition ? x.replace(/%$/u, "") : defaults.xOffset,
    xUnit: hasPosition && x.endsWith("%") ? "percent" : "point",
    xAnchor: xSign === "-" ? "right" : "left",
    yOffset: hasPosition ? y.replace(/%$/u, "") : defaults.yOffset,
    yUnit: hasPosition && y.endsWith("%") ? "percent" : "point",
    yAnchor: ySign === "+" ? "bottom" : "top",
  };
}

export function buildIinaWindowGeometry(model = {}) {
  let geometry = "";
  if (model.sizeEnabled) {
    if (model.sizeDimension === "height") geometry += "x";
    geometry += geometryMagnitude(model.sizeValue, "1280");
    if (model.sizeUnit === "percent") geometry += "%";
  }
  if (model.positionEnabled) {
    geometry += model.xAnchor === "right" ? "-" : "+";
    geometry += geometryMagnitude(model.xOffset, "20");
    if (model.xUnit === "percent") geometry += "%";
    geometry += model.yAnchor === "bottom" ? "+" : "-";
    geometry += geometryMagnitude(model.yOffset, "20");
    if (model.yUnit === "percent") geometry += "%";
  }
  return geometry;
}

export function normalizeIinaOscToolbarButtons(rawValue) {
  if (!Array.isArray(rawValue)) return [...IINA_DEFAULT_OSC_TOOLBAR_BUTTONS];
  const seen = new Set();
  const result = [];
  for (const rawButton of rawValue) {
    const button = Number(rawButton);
    if (!Number.isInteger(button) || button < 0 || button > 6 || seen.has(button)) continue;
    seen.add(button);
    result.push(button);
    if (result.length === 5) break;
  }
  return result;
}

export function normalizePreferenceNumber(control, rawValue) {
  const fallback = Number(control?.defaultValue ?? 0);
  let value = Number(rawValue);
  if (!Number.isFinite(value)) value = Number.isFinite(fallback) ? fallback : 0;
  if (Number(control?.step) === 1) value = Math.round(value);
  if (control?.min !== undefined) value = Math.max(Number(control.min), value);
  if (control?.max !== undefined) value = Math.min(Number(control.max), value);
  return value;
}

export function normalizePreferenceTokens(rawValue) {
  const seen = new Set();
  const tokens = [];
  for (const rawToken of String(rawValue ?? "").split(/[\n,]+/u)) {
    const token = rawToken.trim().toLowerCase();
    if (!token || seen.has(token)) continue;
    seen.add(token);
    tokens.push(token);
  }
  return tokens.join(",");
}

export function normalizeAdvancedUserOptions(rawValue) {
  if (!Array.isArray(rawValue)) return [];
  return rawValue.flatMap((option) => (
    Array.isArray(option)
      && option.length === 2
      && typeof option[0] === "string"
      && typeof option[1] === "string"
      && !option[0].includes("\0")
      && !option[1].includes("\0")
      ? [[option[0], option[1]]]
      : []
  ));
}

export function advancedUserOptionsWithAdded(rawValue) {
  return [...normalizeAdvancedUserOptions(rawValue), ["name", "value"]];
}

export function advancedUserOptionsWithRemoved(rawValue, selectedIndex) {
  const options = normalizeAdvancedUserOptions(rawValue);
  if (!Number.isInteger(selectedIndex) || selectedIndex < 0 || selectedIndex >= options.length) return options;
  options.splice(selectedIndex, 1);
  return options;
}

export function advancedUserOptionsWithEdit(rawValue, rowIndex, columnIndex, rawText) {
  const options = normalizeAdvancedUserOptions(rawValue);
  const value = typeof rawText === "string" ? rawText : "";
  if (!options[rowIndex] || ![0, 1].includes(columnIndex) || !value || value.includes("\0")) return null;
  options[rowIndex][columnIndex] = value;
  return options;
}

export function preferenceColorInputState(rawValue, fallback = "1/1/1/1") {
  const parsed = parsePreferenceColor(rawValue) || parsePreferenceColor(fallback) || [1, 1, 1, 1];
  const toHex = (component) => Math.round(component * 255).toString(16).padStart(2, "0");
  return {
    hex: `#${toHex(parsed[0])}${toHex(parsed[1])}${toHex(parsed[2])}`,
    alpha: parsed[3],
    preservedRaw: Boolean(rawValue && typeof rawValue === "object"),
  };
}

export function preferenceColorValue(hex, alpha) {
  const match = /^#([0-9a-f]{6})$/iu.exec(String(hex));
  if (!match) return null;
  const value = Number.parseInt(match[1], 16);
  const components = [
    (value >> 16) & 0xff,
    (value >> 8) & 0xff,
    value & 0xff,
  ].map((component) => component / 255);
  components.push(Math.max(0, Math.min(1, Number(alpha) || 0)));
  return components.map(formatPreferenceColorComponent).join("/");
}

function parsePreferenceColor(rawValue) {
  if (typeof rawValue !== "string") return null;
  const hex = /^#([0-9a-f]{6})([0-9a-f]{2})?$/iu.exec(rawValue);
  if (hex) {
    const rgb = Number.parseInt(hex[1], 16);
    return [
      ((rgb >> 16) & 0xff) / 255,
      ((rgb >> 8) & 0xff) / 255,
      (rgb & 0xff) / 255,
      hex[2] ? Number.parseInt(hex[2], 16) / 255 : 1,
    ];
  }
  const components = rawValue.split("/").map((component) => Number(component.trim()));
  if (!(components.length === 3 || components.length === 4)
      || components.some((component) => !Number.isFinite(component) || component < 0 || component > 1)) {
    return null;
  }
  if (components.length === 3) components.push(1);
  return components;
}

function formatPreferenceColorComponent(component) {
  if (component === 0 || component === 1) return String(component);
  return component.toFixed(6).replace(/0+$/u, "").replace(/\.$/u, "");
}

export function preferenceAudioDeviceOptions(devices, selectedName, selectedDescription) {
  const options = [];
  const seen = new Set();
  for (const candidate of Array.isArray(devices) ? devices : []) {
    const name = String(candidate?.name ?? "");
    if (!name || seen.has(name)) continue;
    seen.add(name);
    const description = String(candidate?.description || name);
    options.push({
      value: name,
      description,
      label: `[${description}] ${name}`,
      missing: false,
    });
  }

  const value = String(selectedName || "auto");
  if (!seen.has(value)) {
    const description = String(selectedDescription || value);
    options.push({
      value,
      description,
      label: `[${description} (missing)] ${value}`,
      missing: true,
    });
  }
  return options;
}
