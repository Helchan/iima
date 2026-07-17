let translations = Object.freeze({});
let contextTranslations = Object.freeze({});
let activeLocale = "en";

function canonicalLocale(value) {
  try {
    return Intl.getCanonicalLocales(String(value).replaceAll("_", "-"))[0] ?? "";
  } catch {
    return "";
  }
}

export function resolveLocale(preferredLocales, supportedLocales) {
  const supported = new Map(
    supportedLocales.map((locale) => [locale.toLocaleLowerCase("en"), locale]),
  );
  for (const preferred of preferredLocales) {
    const locale = canonicalLocale(preferred);
    if (!locale) continue;
    const lower = locale.toLocaleLowerCase("en");
    if (supported.has(lower)) return supported.get(lower);

    const parts = lower.split("-");
    for (let length = parts.length - 1; length > 1; length -= 1) {
      const parent = parts.slice(0, length).join("-");
      if (supported.has(parent)) return supported.get(parent);
    }

    const [language] = parts;
    if (language === "zh") {
      const traditional = /-(hant|tw|hk|mo)(?:-|$)/.test(lower);
      const chinese = traditional ? "zh-hant" : "zh-hans";
      if (supported.has(chinese)) return supported.get(chinese);
    }
    if (supported.has(language)) return supported.get(language);
  }
  return supported.get("en") ?? supportedLocales[0] ?? "en";
}

export function localizationContextIdentifier(table, key) {
  const rawTable = String(table ?? "").trim();
  const rawKey = String(key ?? "").trim();
  if (!rawTable || !rawKey) return "";
  const filename = rawTable.endsWith(".strings") ? rawTable : `${rawTable}.strings`;
  return `${filename}:${rawKey}`;
}

export function translationFromCatalog(catalog, value, table = null, key = null) {
  const text = String(value ?? "");
  const context = localizationContextIdentifier(table, key);
  if (context && Object.hasOwn(catalog?.contexts ?? {}, context)) {
    return String(catalog.contexts[context]);
  }
  return String(catalog?.translations?.[text] ?? text);
}

export function tr(value) {
  return translationFromCatalog({ translations, contexts: contextTranslations }, value);
}

export function trKey(table, key, value) {
  return translationFromCatalog(
    { translations, contexts: contextTranslations },
    value,
    table,
    key,
  );
}

const APPLE_FORMAT_PATTERN = /%(?:(\d+)\$)?([-+ 0#]*)(\d+)?(?:\.(\d+))?(?:hh|h|ll|l|L|z|t|j)?([@diuoxXfFeEgGaAcsp%])/g;

function numericValue(value) {
  const number = Number(value);
  return Number.isFinite(number) ? number : 0;
}

function formattedArgument(value, specifier, precision) {
  if (specifier === "@" || specifier === "s" || specifier === "p") {
    return String(value ?? "");
  }
  if (specifier === "c") {
    return typeof value === "number" ? String.fromCodePoint(value) : String(value ?? "").charAt(0);
  }

  const number = numericValue(value);
  if (specifier === "d" || specifier === "i" || specifier === "u") {
    return String(Math.trunc(specifier === "u" ? Math.abs(number) : number));
  }
  if (specifier === "o") return Math.trunc(Math.abs(number)).toString(8);
  if (specifier === "x" || specifier === "X") {
    const result = Math.trunc(Math.abs(number)).toString(16);
    return specifier === "X" ? result.toUpperCase() : result;
  }
  if (specifier === "e" || specifier === "E") {
    const result = number.toExponential(precision ?? 6);
    return specifier === "E" ? result.toUpperCase() : result;
  }
  if (specifier === "g" || specifier === "G") {
    const result = number.toPrecision(precision ?? 6).replace(/(\.\d*?[1-9])0+(e|$)|\.0+(e|$)/i, "$1$2");
    return specifier === "G" ? result.toUpperCase() : result;
  }
  return number.toFixed(precision ?? 6);
}

function applyFormatWidth(value, flags, width, numeric) {
  let result = value;
  if (numeric && !result.startsWith("-") && flags.includes("+")) result = `+${result}`;
  const targetWidth = Number(width) || 0;
  if (result.length >= targetWidth) return result;
  const padding = (flags.includes("0") && !flags.includes("-")) ? "0" : " ";
  const count = targetWidth - result.length;
  if (flags.includes("-")) return result.padEnd(targetWidth, padding);
  if (padding === "0" && /^[+-]/.test(result)) {
    return `${result[0]}${padding.repeat(count)}${result.slice(1)}`;
  }
  return result.padStart(targetWidth, padding);
}

export function formatLocalizedTemplate(template, ...values) {
  let nextArgument = 0;
  return String(template ?? "").replace(
    APPLE_FORMAT_PATTERN,
    (token, position, flags, width, precision, specifier) => {
      if (specifier === "%") return "%";
      const argumentIndex = position ? Number(position) - 1 : nextArgument++;
      if (argumentIndex < 0 || argumentIndex >= values.length) return token;
      const numeric = !["@", "s", "c", "p"].includes(specifier);
      const formatted = formattedArgument(
        values[argumentIndex],
        specifier,
        precision === undefined ? undefined : Number(precision),
      );
      return applyFormatWidth(formatted, flags, width, numeric);
    },
  );
}

export function trFormat(source, ...values) {
  return formatLocalizedTemplate(tr(source), ...values);
}

export function trKeyFormat(table, key, source, ...values) {
  return formatLocalizedTemplate(trKey(table, key, source), ...values);
}

function translateTextNode(node) {
  const source = node.nodeValue ?? "";
  const leading = source.match(/^\s*/s)?.[0] ?? "";
  const trailing = source.match(/\s*$/s)?.[0] ?? "";
  const core = source.slice(leading.length, source.length - trailing.length);
  if (!core) return;
  const table = node.parentElement?.dataset.i18nTable;
  const key = node.parentElement?.dataset.i18nKey;
  const translated = table && key ? trKey(table, key, core) : tr(core);
  if (translated !== core) node.nodeValue = `${leading}${translated}${trailing}`;
}

export function localizeStaticDocument(root = document.documentElement) {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const tagName = node.parentElement?.tagName;
    if (tagName === "SCRIPT" || tagName === "STYLE" || tagName === "NOSCRIPT") continue;
    translateTextNode(node);
  }
  for (const element of root.querySelectorAll("[title], [placeholder], [aria-label]")) {
    for (const attribute of ["title", "placeholder", "aria-label"]) {
      if (!element.hasAttribute(attribute)) continue;
      const source = element.getAttribute(attribute);
      const contextAttribute = `data-i18n-${attribute}-key`;
      const table = element.getAttribute("data-i18n-table");
      const key = element.getAttribute(contextAttribute);
      const translated = table && key ? trKey(table, key, source) : tr(source);
      if (translated !== source) element.setAttribute(attribute, translated);
    }
  }
}

export async function initializeLocalization() {
  let manifest;
  try {
    const response = await fetch(new URL("./locales/manifest.json", import.meta.url));
    if (!response.ok) return activeLocale;
    manifest = await response.json();
  } catch {
    return activeLocale;
  }

  const localeEntries = Array.isArray(manifest.locales) ? manifest.locales : [];
  const preferred = Array.isArray(navigator.languages) && navigator.languages.length
    ? navigator.languages
    : [navigator.language || manifest.defaultLocale || "en"];
  activeLocale = resolveLocale(preferred, localeEntries.map((entry) => entry.id));
  const selected = localeEntries.find((entry) => entry.id === activeLocale);
  if (selected?.file) {
    try {
      const response = await fetch(new URL(`./locales/${selected.file}`, import.meta.url));
      if (response.ok) {
        const catalog = await response.json();
        translations = Object.freeze(catalog.translations ?? {});
        contextTranslations = Object.freeze(catalog.contexts ?? {});
      }
    } catch {
      translations = Object.freeze({});
      contextTranslations = Object.freeze({});
    }
  }

  document.documentElement.lang = activeLocale;
  document.documentElement.dir = selected?.rtl ? "rtl" : "ltr";
  localizeStaticDocument();
  window.__IIMA_LOCALE__ = activeLocale;
  return activeLocale;
}
