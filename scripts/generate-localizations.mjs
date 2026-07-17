import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const referenceRoot = join(root, "参考", "iina", "iina");
const outputRoot = join(root, "src", "locales");
const aboutDocumentsOutput = join(root, "src", "assets", "iina", "about-documents.json");
const nativeMenuOutput = join(root, "src-tauri", "src", "native-menu-locales.json");
const englishDirectory = join(referenceRoot, "en.lproj");
const stringsdictPath = join(referenceRoot, "Translators.stringsdict");
const nativeUiSource = (
  await Promise.all(
    ["menu.rs", "commands.rs", "plugins.rs", "native_font_picker.m"].map((filename) =>
      readFile(join(root, "src-tauri", "src", filename), "utf8"),
    ),
  )
).join("\n");
const checkOnly = process.argv.includes("--check");
const rtlLocales = new Set(["ar", "fa", "he", "ug", "ur"]);

function decodeAppleString(value, path) {
  const normalized = value
    .replace(/\\U([0-9a-fA-F]{4})/g, "\\u$1")
    .replace(/[\u0000-\u001f]/g, (character) => {
      return `\\u${character.codePointAt(0).toString(16).padStart(4, "0")}`;
    });
  try {
    return JSON.parse(`"${normalized}"`);
  } catch (error) {
    throw new Error(`Unable to decode a string in ${path}: ${error.message}`);
  }
}

function parseStringsTable(source, path) {
  const entries = new Map();
  const pattern = /(?:^|\n)\s*(?:"((?:\\.|[^"\\])*)"|([A-Za-z0-9_.-]+))\s*=\s*"((?:\\.|[^"\\])*)"\s*;/g;
  for (const match of source.matchAll(pattern)) {
    const key = decodeAppleString(match[1] ?? match[2], path);
    const value = decodeAppleString(match[3], path);
    entries.set(key, value);
  }
  if (source.includes("=") && entries.size === 0) {
    throw new Error(`No localization entries were parsed from ${path}`);
  }
  return entries;
}

async function readStringsTable(path) {
  return parseStringsTable(await readFile(path, "utf8"), path);
}

function addCandidate(candidates, source, translation) {
  if (!source || !translation || source === translation) return;
  const translations = candidates.get(source) ?? new Map();
  translations.set(translation, (translations.get(translation) ?? 0) + 1);
  candidates.set(source, translations);
}

function contextIdentifier(filename, key) {
  return `${filename}:${key}`;
}

function addContext(contexts, filename, key, translation) {
  if (translation === undefined) return;
  contexts.set(contextIdentifier(filename, key), translation);
}

function sortedObject(entries) {
  return Object.fromEntries(
    [...entries].sort(([left], [right]) => left.localeCompare(right, "en")),
  );
}

function resolveCandidates(candidates) {
  const translations = {};
  const ambiguousSources = [];
  for (const source of [...candidates.keys()].sort((left, right) => left.localeCompare(right, "en"))) {
    const choices = [...candidates.get(source).entries()].sort(
      (left, right) => right[1] - left[1] || left[0].localeCompare(right[0]),
    );
    if (choices.length > 1) ambiguousSources.push(source);
    translations[source] = choices[0][0];
  }
  return { translations, ambiguous: ambiguousSources.length, ambiguousSources };
}

async function writeArtifact(path, contents) {
  if (checkOnly) {
    let current;
    try {
      current = await readFile(path, "utf8");
    } catch {
      throw new Error(`Missing generated localization artifact: ${path}`);
    }
    if (current !== contents) {
      throw new Error(`Generated localization artifact is stale: ${path}`);
    }
    return;
  }
  await writeFile(path, contents);
}

const englishFiles = (await readdir(englishDirectory, { withFileTypes: true }))
  .filter((entry) => entry.isFile() && entry.name.endsWith(".strings"))
  .map((entry) => entry.name)
  .sort();
const englishTables = new Map();
for (const filename of englishFiles) {
  englishTables.set(filename, await readStringsTable(join(englishDirectory, filename)));
}

const stringsdictSource = await readFile(stringsdictPath, "utf8");
const pluralVariantNames = ["zero", "one", "two", "few", "many", "other"];
const nonEmptyPluralVariants = pluralVariantNames.filter((variant) => {
  const pattern = new RegExp(`<key>${variant}</key>\\s*<string>([^<]+)</string>`);
  return pattern.test(stringsdictSource);
});
const stringsdict = {
  files: ["Translators.stringsdict"],
  sha256: createHash("sha256").update(stringsdictSource).digest("hex"),
  pluralKeys: (stringsdictSource.match(/<key>NSStringLocalizedFormatKey<\/key>/g) ?? []).length,
  nonEmptyPluralVariants: nonEmptyPluralVariants.length,
  placeholderOnly: nonEmptyPluralVariants.length === 0,
};
if (stringsdict.pluralKeys !== 1 || !stringsdict.placeholderOnly) {
  throw new Error("IINA 1.3.5 Translators.stringsdict is no longer the expected empty plural placeholder");
}

function sourceAppearsInNativeMenu(source) {
  const sources = [source];
  if (source.includes("…")) sources.push(source.replaceAll("…", "..."));
  return sources.some((candidate) => nativeUiSource.includes(JSON.stringify(candidate)));
}

const localeDirectories = (await readdir(referenceRoot, { withFileTypes: true }))
  .filter((entry) => entry.isDirectory() && entry.name.endsWith(".lproj"))
  .map((entry) => entry.name)
  .filter((name) => name !== "Base.lproj" && name !== "en.lproj")
  .sort();

await mkdir(outputRoot, { recursive: true });
const manifestLocales = [{
  id: "en",
  file: null,
  rtl: false,
  entries: 0,
  contextEntries: 0,
  ambiguous: 0,
  ambiguousSources: [],
}];
const nativeMenuLocales = {};
const nativeMenuContextLocales = {};
for (const directory of localeDirectories) {
  const locale = directory.slice(0, -".lproj".length);
  const candidates = new Map();
  const contexts = new Map();
  const nativeMenuCandidates = new Map();
  const nativeMenuContexts = new Map();
  for (const filename of englishFiles) {
    const targetPath = join(referenceRoot, directory, filename);
    let targetTable;
    try {
      targetTable = await readStringsTable(targetPath);
    } catch (error) {
      if (error.code === "ENOENT") continue;
      throw error;
    }
    const englishTable = englishTables.get(filename);
    for (const [key, source] of englishTable) {
      const translation = targetTable.get(key);
      if (translation === undefined) continue;
      addCandidate(candidates, source, translation);
      addContext(contexts, filename, key, translation);
      if (source.includes("…")) {
        addCandidate(candidates, source.replaceAll("…", "..."), translation);
      }
      const nativeMenuSource =
        filename === "MainMenu.strings" ||
        (filename === "Localizable.strings" && key.startsWith("menu.")) ||
        sourceAppearsInNativeMenu(source);
      if (nativeMenuSource) {
        addCandidate(nativeMenuCandidates, source, translation);
        addContext(nativeMenuContexts, filename, key, translation);
        if (source.includes("…")) {
          addCandidate(nativeMenuCandidates, source.replaceAll("…", "..."), translation);
        }
      }
    }
  }

  const { translations, ambiguous, ambiguousSources } = resolveCandidates(candidates);
  nativeMenuLocales[locale] = resolveCandidates(nativeMenuCandidates).translations;
  nativeMenuContextLocales[locale] = sortedObject(nativeMenuContexts);
  const filename = `${locale}.json`;
  const catalog = {
    locale,
    rtl: rtlLocales.has(locale.split("-")[0]),
    translations,
    contexts: sortedObject(contexts),
  };
  await writeArtifact(join(outputRoot, filename), `${JSON.stringify(catalog)}\n`);
  manifestLocales.push({
    id: locale,
    file: filename,
    rtl: catalog.rtl,
    entries: Object.keys(translations).length,
    contextEntries: contexts.size,
    ambiguous,
    ambiguousSources,
  });
}

const manifest = {
  source: "IINA release/1.3.5 localization tables",
  defaultLocale: "en",
  stringsdict,
  locales: manifestLocales,
};
await writeArtifact(join(outputRoot, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);

function rtfPlainText(path) {
  return execFileSync("/usr/bin/textutil", ["-convert", "txt", "-stdout", path], {
    encoding: "utf8",
    maxBuffer: 4 * 1024 * 1024,
  }).replaceAll("\r\n", "\n");
}

function rtfHtmlFragment(path) {
  const html = execFileSync("/usr/bin/textutil", ["-convert", "html", "-stdout", path], {
    encoding: "utf8",
    maxBuffer: 4 * 1024 * 1024,
  }).replaceAll("\r\n", "\n");
  const style = html.match(/<style type="text\/css">\s*([\s\S]*?)\s*<\/style>/)?.[1] ?? "";
  const body = html.match(/<body>\s*([\s\S]*?)\s*<\/body>/)?.[1] ?? "";
  if (!style || !body) {
    throw new Error(`Unable to extract Cocoa HTML from ${path}`);
  }
  if (/@import|url\s*\(|expression\s*\(/i.test(style)) {
    throw new Error(`Unsafe external CSS reference in Cocoa HTML from ${path}`);
  }
  return { style, body };
}

const contributionDirectories = (await readdir(referenceRoot, { withFileTypes: true }))
  .filter((entry) => entry.isDirectory() && entry.name.endsWith(".lproj"))
  .map((entry) => entry.name)
  .sort((left, right) => left.localeCompare(right, "en"));
const licenses = {};
const licenseHtml = {};
const aboutSources = {};
for (const directory of contributionDirectories) {
  const contributionPath = join(referenceRoot, directory, "Contribution.rtf");
  let source;
  try {
    source = await readFile(contributionPath);
  } catch (error) {
    if (error.code === "ENOENT") continue;
    throw error;
  }
  const locale = directory.slice(0, -".lproj".length);
  licenses[locale] = rtfPlainText(contributionPath);
  licenseHtml[locale] = rtfHtmlFragment(contributionPath);
  aboutSources[`licenses/${locale}`] = createHash("sha256").update(source).digest("hex");
}
const creditsPath = join(referenceRoot, "Credits.rtf");
const creditsSource = await readFile(creditsPath);
const aboutDocuments = {
  source: "IINA release/1.3.5 Contribution.rtf and Credits.rtf",
  sources: {
    ...sortedObject(new Map(Object.entries(aboutSources))),
    credits: createHash("sha256").update(creditsSource).digest("hex"),
  },
  licenses: Object.fromEntries(
    Object.entries(licenses).sort(([left], [right]) => left.localeCompare(right, "en")),
  ),
  licenseHtml: Object.fromEntries(
    Object.entries(licenseHtml).sort(([left], [right]) => left.localeCompare(right, "en")),
  ),
  credits: rtfPlainText(creditsPath),
  creditsHtml: rtfHtmlFragment(creditsPath),
};
await writeArtifact(aboutDocumentsOutput, `${JSON.stringify(aboutDocuments)}\n`);
const nativeMenuCatalog = {
  source: "IINA release/1.3.5 native menu localization tables",
  defaultLocale: "en",
  locales: nativeMenuLocales,
  contexts: nativeMenuContextLocales,
};
await writeArtifact(nativeMenuOutput, `${JSON.stringify(nativeMenuCatalog)}\n`);

const totalEntries = manifestLocales.reduce((total, locale) => total + locale.entries, 0);
console.log(
  `${checkOnly ? "Verified" : "Generated"} ${manifestLocales.length} locale catalogs with ${totalEntries} translated strings`,
);
