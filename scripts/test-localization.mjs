import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, readdir } from "node:fs/promises";
import { join } from "node:path";
import {
  formatLocalizedTemplate,
  localizationContextIdentifier,
  resolveLocale,
  translationFromCatalog,
} from "../src/localization.js";
import { IINA_OSC_TOOLBAR_BUTTONS, PREFERENCE_PANES } from "../src/preference-panes.js";

const localeRoot = join(import.meta.dirname, "..", "src", "locales");
const referenceLocaleRoot = join(import.meta.dirname, "..", "参考", "iina", "iina");
const manifest = JSON.parse(await readFile(join(localeRoot, "manifest.json"), "utf8"));
const supported = manifest.locales.map((locale) => locale.id);

assert.equal(resolveLocale(["zh-CN"], supported), "zh-Hans");
assert.equal(resolveLocale(["zh-HK"], supported), "zh-Hant");
assert.equal(resolveLocale(["pt-BR"], supported), "pt-BR");
assert.equal(resolveLocale(["sr-Latn-RS"], supported), "sr-Latn");
assert.equal(resolveLocale(["en-AU"], supported), "en");
assert.equal(resolveLocale(["invalid_locale", "ja-JP"], supported), "ja");

assert.equal(formatLocalizedTemplate("Volume: %i", 72.9), "Volume: 72");
assert.equal(formatLocalizedTemplate("Speed: %.2fx", 1.234), "Speed: 1.23x");
assert.equal(formatLocalizedTemplate("Volume + %.0f%%", 5), "Volume + 5%");
assert.equal(formatLocalizedTemplate("%2$@ of %1$@ selected", "10:00", "03:00"), "03:00 of 10:00 selected");
assert.equal(formatLocalizedTemplate("0x%08X", 255), "0x000000FF");
assert.equal(formatLocalizedTemplate("Missing %@ %d", "value"), "Missing value %d");
assert.equal(localizationContextIdentifier("Localizable", "osd.speed"), "Localizable.strings:osd.speed");
assert.equal(localizationContextIdentifier("Localizable.strings", "menu.speed"), "Localizable.strings:menu.speed");

const simplifiedChinese = JSON.parse(
  await readFile(join(localeRoot, "zh-Hans.json"), "utf8"),
);
assert.equal(simplifiedChinese.translations["Check for Updates..."], "检查更新…");
assert.equal(simplifiedChinese.translations["Check for updates"], "自动检查更新");
assert.equal(simplifiedChinese.translations["Receive beta updates"], "接收测试版本（Beta）更新");
assert.equal(simplifiedChinese.translations.Cancel, "取消");
assert.equal(
  translationFromCatalog(
    simplifiedChinese,
    "Resume",
    "InitialWindowController",
    "KWZ-BM-GBN.title",
  ),
  "继续播放",
);
assert.equal(
  translationFromCatalog(simplifiedChinese, "Resume", "Localizable", "osd.resume"),
  "继续",
);
assert.equal(
  translationFromCatalog(simplifiedChinese, "Amount", "FilterPresets", "sharpen.amount"),
  "锐化量",
);
assert.equal(
  translationFromCatalog(simplifiedChinese, "Amount", "FilterPresets", "blur.amount"),
  "模糊量",
);
assert.equal(
  formatLocalizedTemplate(
    translationFromCatalog(simplifiedChinese, "Speed: %.2fx", "Localizable", "osd.speed"),
    1.25,
  ),
  "速度：1.25x",
);
assert.equal(
  formatLocalizedTemplate(
    translationFromCatalog(simplifiedChinese, "Speed: %.2fx", "Localizable", "menu.speed"),
    1.25,
  ),
  "速度: 1.25x",
);
assert.equal(
  translationFromCatalog(simplifiedChinese, "File", "Missing", "missing"),
  "文件",
  "a missing context must preserve the source-based compatibility fallback",
);
assert.equal(
  formatLocalizedTemplate(simplifiedChinese.translations["Finished with %d success and %d failed."], 3, 1),
  "操作已完成。3 成功，1 失败。",
);
assert.equal(
  formatLocalizedTemplate(simplifiedChinese.translations["Chapter: %@"], "Opening"),
  "章节：Opening",
);

const arabic = JSON.parse(await readFile(join(localeRoot, "ar.json"), "utf8"));
assert.equal(arabic.rtl, true);
assert.ok(Object.keys(arabic.translations).length > 500);
assert.equal(manifest.locales.length, 56);
assert.equal(manifest.locales.reduce((total, locale) => total + locale.entries, 0), 33_847);
assert.equal(manifest.locales.reduce((total, locale) => total + locale.contextEntries, 0), 65_181);
assert.equal(manifest.locales.reduce((total, locale) => total + locale.ambiguous, 0), 730);
assert.deepEqual(manifest.stringsdict.files, ["Translators.stringsdict"]);
assert.equal(manifest.stringsdict.pluralKeys, 1);
assert.equal(manifest.stringsdict.nonEmptyPluralVariants, 0);
assert.equal(manifest.stringsdict.placeholderOnly, true);
const stringsdictSource = await readFile(join(referenceLocaleRoot, "Translators.stringsdict"), "utf8");
assert.equal(
  createHash("sha256").update(stringsdictSource).digest("hex"),
  manifest.stringsdict.sha256,
);
for (const pluralVariant of ["zero", "one", "two", "few", "many", "other"]) {
  assert.match(stringsdictSource, new RegExp(`<key>${pluralVariant}</key>\\s*<string></string>`));
}

const nativeMenu = JSON.parse(
  await readFile(join(import.meta.dirname, "..", "src-tauri", "src", "native-menu-locales.json"), "utf8"),
);
assert.equal(nativeMenu.defaultLocale, "en");
assert.equal(Object.keys(nativeMenu.locales).length, 55);
assert.equal(nativeMenu.locales["zh-Hans"].File, "文件");
assert.equal(nativeMenu.locales["zh-Hans"]["Check for Updates..."], "检查更新…");
assert.equal(nativeMenu.locales["zh-Hans"]["Speed: %.2fx"], "速度: %.2fx");
assert.equal(nativeMenu.locales["zh-Hans"]["Choose a Font"], "选择字体");
assert.equal(nativeMenu.locales["zh-Hans"]["Choose Media Files"], "选择媒体文件");
assert.equal(nativeMenu.locales["zh-Hans"]["Type to filter..."], "输入以过滤…");
assert.equal(nativeMenu.locales["zh-Hans"]["Save Downloaded Subtitle"], "保存下载的字幕");
assert.equal(
  nativeMenu.contexts["zh-Hans"]["Localizable.strings:osd.speed"],
  "速度：%.2fx",
);
assert.equal(
  nativeMenu.contexts["zh-Hans"]["Localizable.strings:menu.speed"],
  "速度: %.2fx",
);
assert.equal(
  nativeMenu.contexts["zh-Hans"]["MainMenu.strings:H8h-7b-M4v.title"],
  "视频",
);

const infoPlistLocalizations = [];
const emptyInfoPlistLocalizations = [];
for (const entry of await readdir(referenceLocaleRoot, { withFileTypes: true })) {
  if (!entry.isDirectory() || !entry.name.endsWith(".lproj")) continue;
  try {
    const contents = await readFile(join(referenceLocaleRoot, entry.name, "InfoPlist.strings"), "utf8");
    if (contents.trim()) assert.match(contents, /NSHumanReadableCopyright\s*=/);
    else emptyInfoPlistLocalizations.push(entry.name);
    infoPlistLocalizations.push(entry.name);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
}
assert.equal(infoPlistLocalizations.length, 57);
assert.deepEqual(
  emptyInfoPlistLocalizations.sort(),
  ["Base.lproj", "sr-CS.lproj", "sr-Cyrl.lproj"],
);
assert.equal(
  (await readFile(join(referenceLocaleRoot, "zh-Hans.lproj", "InfoPlist.strings"), "utf8")).trim(),
  'NSHumanReadableCopyright = "基于 GPLv3 授权发布";',
);

const mainSource = await readFile(join(import.meta.dirname, "..", "src", "main.js"), "utf8");
assert.equal(
  (mainSource.match(/showOsd\("No subtitles found"\);/g) || []).length,
  1,
  "online-subtitle empty results must use IINA's exact osd.sub_not_found source key",
);
assert.equal(
  (
    mainSource.match(
      /message\.startsWith\("ONLINE_SUBTITLE_TIMED_OUT:"\)\) return tr\("Timed Out"\);/g,
    ) || []
  ).length,
  1,
  "online-subtitle timeout errors must use IINA's exact osd.timed_out source key",
);
assert.doesNotMatch(mainSource, /tr\("No subtitles found\."\)|tr\("Timed out"\)/);
for (const contract of [
  'trKeyFormat("Localizable", "osd.volume", "Volume: %i"',
  'trKeyFormat("Localizable", "osd.speed", "Speed: %.2fx"',
  'isAudio ? "osd.audio_delay.nodelay" : "osd.sub_delay.nodelay"',
  'trKeyFormat("Localizable", "osd.subtitle_pos", "Subtitle Position: %.1f"',
  '"osd.add_to_playlist"',
  'trFormat("Update found for %@"',
  '"The latest version is %@. You have %@. Would you like to update?"',
  'showUtilityInformation("Information", "No update found.")',
  'button.textContent = tr("Downloading update…")',
  'trFormat("Logged in as %@", username)',
  'tr("OpenSubtitles Login")',
  '"Cannot save your password to Keychain: %@"',
  '"Cannot login. Please check your username, password and network status.\\n\\n%@"',
  'message.startsWith("OPEN_SUBTITLES_LOGIN:")',
  'message.startsWith("OPEN_SUBTITLES_INVALID_TOKEN:")',
  'message.startsWith("ONLINE_SUBTITLE_CANNOT_CONNECT:")',
  'message.startsWith("ONLINE_SUBTITLE_NETWORK_ERROR:")',
  'message.startsWith("ONLINE_SUBTITLE_TIMED_OUT:")',
  'message.startsWith("ONLINE_SUBTITLE_FILE_ERROR:")',
  'message.startsWith("ONLINE_SUBTITLE_CANCELED:")',
  'showOsd(String(message || ""), {',
  'detail: `From plugin ${runtime.spec.name}`',
]) {
  assert.ok(mainSource.includes(contract), `Missing dynamic-localization contract: ${contract}`);
}
assert.match(
  mainSource,
  /trFormat\(\s*"%@ of %@ selected"/,
  "Missing dynamic-localization contract: selected playlist duration",
);
assert.match(mainSource, /trKey\("FilterPresets", preset\.id, preset\.label\)/);
assert.match(mainSource, /trReference\(optionLabel, optionContext\)/);
assert.match(mainSource, /trKey\("Localizable", "quicksetting\.item_default", "Default"\)/);
assert.match(mainSource, /trKey\("Localizable", "quicksetting\.item_none", "None"\)/);

const ambiguousSources = new Set(manifest.locales.flatMap((locale) => locale.ambiguousSources));
for (const sourceFile of ["main.js", "history.js", "guide.js", "log.js"]) {
  const source = await readFile(join(import.meta.dirname, "..", "src", sourceFile), "utf8");
  for (const match of source.matchAll(/\btr(?:Format)?\(\s*("(?:\\.|[^"\\])*")/g)) {
    const value = JSON.parse(match[1]);
    assert.equal(
      ambiguousSources.has(value),
      false,
      `${sourceFile} must use trKey/trKeyFormat for ambiguous source ${JSON.stringify(value)}`,
    );
  }
}

const assertContextForAmbiguous = (source, context, owner) => {
  if (!source || !ambiguousSources.has(source)) return;
  assert.ok(
    context?.table && context?.key,
    `${owner} must bind ambiguous source ${JSON.stringify(source)} to its reference table:key`,
  );
};
for (const pane of PREFERENCE_PANES) {
  assertContextForAmbiguous(pane.title, pane.l10n, `preference pane ${pane.id}`);
  for (const section of pane.sections) {
    assertContextForAmbiguous(section.title, section.l10n, `preference section ${pane.id}`);
    for (const control of section.controls) {
      assertContextForAmbiguous(control.label, control.l10n, `preference control ${control.key}`);
      assertContextForAmbiguous(
        control.secondaryLabel,
        control.secondaryL10n,
        `preference secondary label ${control.key}`,
      );
      for (const [value, label, context] of control.options || []) {
        assertContextForAmbiguous(label, context, `preference option ${control.key}:${value}`);
      }
      for (const item of control.items || []) {
        assertContextForAmbiguous(item.label, item.l10n, `preference item ${control.key}:${item.key}`);
      }
    }
  }
}
for (const spec of IINA_OSC_TOOLBAR_BUTTONS) {
  assertContextForAmbiguous(spec.label, spec.l10n, `OSC toolbar item ${spec.value}`);
}

const playerHtml = await readFile(join(import.meta.dirname, "..", "src", "index.html"), "utf8");
for (const contract of [
  'data-i18n-table="InitialWindowController" data-i18n-key="KWZ-BM-GBN.title"',
  'data-i18n-table="PlaylistViewController" data-i18n-key="rjs-Qf-mT6.label"',
  'data-i18n-table="QuickSettingViewController" data-i18n-key="CYP-el-A6A.label"',
  'data-i18n-table="QuickSettingViewController" data-i18n-key="bzk-c2-LH5.label"',
]) {
  assert.ok(playerHtml.includes(contract), `Missing static context-localization contract: ${contract}`);
}

console.log("Localization locale, exact context, formatting, and empty plural contracts verified");
