let languageCatalogPromise;

export function loadIinaLanguageCatalog() {
  if (!languageCatalogPromise) {
    languageCatalogPromise = fetch(new URL("./assets/iina/iso639.json", import.meta.url))
      .then((response) => {
        if (!response.ok) throw new Error(`Unable to load ISO 639 catalog (${response.status})`);
        return response.json();
      })
      .then((languages) => (Array.isArray(languages) ? languages : []));
  }
  return languageCatalogPromise;
}

function languageByCode(languages, code) {
  return (languages || []).find((language) => language.code === code);
}

export function languageTokenFromEditingString(rawValue, languages = []) {
  const editingString = String(rawValue ?? "").trim();
  if (!editingString) return null;
  const descriptionMatch = /^.+?\(([a-z]{2,3})\)$/u.exec(editingString);
  const requestedCode = descriptionMatch?.[1] || editingString;
  const language = languageByCode(languages, requestedCode);
  if (language) {
    return {
      code: language.code,
      identifier: language.code,
      editingString: `${language.names[0]} (${language.code})`,
    };
  }
  return {
    code: null,
    identifier: editingString.toLowerCase().replaceAll(",", ";").trim(),
    editingString,
  };
}

export function languageTokensFromCsv(rawValue, languages = []) {
  if (!String(rawValue ?? "")) return [];
  return String(rawValue)
    .split(",")
    .map((value) => languageTokenFromEditingString(value, languages))
    .filter(Boolean);
}

export function serializeLanguageTokens(tokens) {
  return (tokens || []).map((token) => token.identifier).join(",");
}

export function appendUniqueLanguageTokens(currentTokens, addedTokens) {
  const result = [...(currentTokens || [])];
  const identifiers = new Set(result.map((token) => token.identifier));
  for (const token of addedTokens || []) {
    if (!token?.identifier || identifiers.has(token.identifier)) continue;
    identifiers.add(token.identifier);
    result.push(token);
  }
  return result;
}

export function iinaLanguageTokenCompletions(languages, rawQuery, selectedTokens = []) {
  const query = String(rawQuery ?? "").toLowerCase();
  if (!query) return [];
  const selectedCodes = new Set(selectedTokens.map((token) => token.code).filter(Boolean));
  return (languages || []).filter((language) => (
    !selectedCodes.has(language.code)
      && language.names.some((name) => name.toLowerCase().startsWith(query))
  )).map((language) => ({
    code: language.code,
    identifier: language.code,
    editingString: `${language.names[0]} (${language.code})`,
  }));
}

export function nextLanguageCompletionIndex(currentIndex, resultCount, direction) {
  if (!Number.isInteger(resultCount) || resultCount <= 0) return -1;
  if (currentIndex < 0 || currentIndex >= resultCount) {
    return direction < 0 ? resultCount - 1 : 0;
  }
  return Math.max(0, Math.min(resultCount - 1, currentIndex + (direction < 0 ? -1 : 1)));
}
