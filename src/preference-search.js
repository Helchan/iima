const SEARCH_TERM_EXTRANEOUS_PATTERN = /[:…()"\n]/gu;
const SEARCH_QUERY_SEPARATOR_PATTERN = /[ 　]+/u;

/**
 * Mirrors PreferenceWindowController.formSearchTerm(_:).
 *
 * IINA indexes the text visible in a preference NIB after trimming its edges
 * and removing label-only punctuation. Full-width punctuation is deliberately
 * retained because the AppKit implementation removes the same ASCII set.
 */
export function formPreferenceSearchTerm(value) {
  return String(value ?? "")
    .trim()
    .replace(SEARCH_TERM_EXTRANEOUS_PATTERN, "");
}

/**
 * Normalized indexed text. Kept as the public normalization helper because it
 * is also used when locating a rendered section title after navigation.
 */
export function normalizePreferenceSearchTerm(value) {
  return formPreferenceSearchTerm(value).toLowerCase();
}

/**
 * Mirrors searchFieldAction(_:): lower-case, trim only trailing whitespace,
 * then remove one trailing colon. Leading and in-query half/full-width spaces
 * remain token separators, just as they are while walking IINA's Trie.
 */
export function normalizePreferenceSearchQuery(value) {
  let query = String(value ?? "").toLowerCase().replace(/\s+$/u, "");
  if (query.endsWith(":")) query = query.slice(0, -1).replace(/\s+$/u, "");
  return query;
}

export function preferenceSearchTokens(rawQuery) {
  return normalizePreferenceSearchQuery(rawQuery)
    .split(SEARCH_QUERY_SEPARATOR_PATTERN)
    .filter(Boolean);
}

/**
 * IINA inserts every suffix of every indexed word into a Trie. Walking that
 * Trie is equivalent to requiring each query token to be a substring of at
 * least one word in the localized tab/section/label path.
 */
export function preferenceSearchTokensMatch(candidates, rawQuery) {
  const words = candidates
    .filter((value) => value !== undefined && value !== null && value !== "")
    .flatMap((value) => normalizePreferenceSearchTerm(value).split(" "))
    .filter(Boolean);
  const tokens = preferenceSearchTokens(rawQuery);
  return tokens.length > 0 && tokens.every(
    (token) => words.some((word) => word.includes(token)),
  );
}

/**
 * Most preference controls render their label verbatim. Composite controls
 * can override that default with `searchLabels`, keeping the index tied to
 * the text that is actually visible instead of an implementation-only label.
 */
export function preferenceSearchLabelsForControl(control) {
  const rawLabels = control?.searchLabels === undefined
    ? (control?.label ? [{ label: control.label, l10n: control.l10n }] : [])
    : control.searchLabels;
  return (rawLabels || []).flatMap((rawLabel) => {
    if (typeof rawLabel === "string") return rawLabel ? [{ label: rawLabel }] : [];
    if (Array.isArray(rawLabel)) {
      return rawLabel[0]
        ? [{ label: rawLabel[0], l10n: rawLabel[1], targetKey: rawLabel[2] }]
        : [];
    }
    return rawLabel?.label ? [{
      label: rawLabel.label,
      l10n: rawLabel.l10n,
      targetKey: rawLabel.targetKey,
    }] : [];
  });
}

/**
 * Build the same stable index shape as makeTries(_:): section first, followed
 * by the labels discovered inside that section. No relevance re-sort is
 * applied later; the reference completion table preserves this source order.
 */
export function buildPreferenceSearchEntries(panes) {
  const entries = [];
  let sourceOrder = 0;
  for (const pane of panes || []) {
    for (const section of pane.sections || []) {
      entries.push({
        pane,
        section,
        control: null,
        item: null,
        label: null,
        key: null,
        sourceOrder: sourceOrder++,
      });
      for (const control of section.controls || []) {
        for (const searchLabel of preferenceSearchLabelsForControl(control)) {
          entries.push({
            pane,
            section,
            control,
            item: null,
            label: searchLabel.label,
            l10n: searchLabel.l10n,
            targetKey: searchLabel.targetKey,
            key: control.key || control.valueKey || null,
            sourceOrder: sourceOrder++,
          });
        }
        for (const item of control.items || []) {
          if (!item?.label) continue;
          entries.push({
            pane,
            section,
            control,
            item,
            label: item.label,
            key: item.key || control.key || control.valueKey || null,
            sourceOrder: sourceOrder++,
          });
        }
      }
    }
  }
  return entries;
}

export function filterPreferenceSearchEntries(entries, rawQuery, candidatesForEntry) {
  return (entries || []).filter((entry) => (
    preferenceSearchTokensMatch(candidatesForEntry(entry), rawQuery)
  ));
}

/**
 * Native table navigation stops at its first/last row. A fresh Down chooses
 * the first row and a fresh Up chooses the last row.
 */
export function nextPreferenceSearchIndex(currentIndex, resultCount, direction) {
  if (!Number.isInteger(resultCount) || resultCount <= 0) return -1;
  if (currentIndex < 0 || currentIndex >= resultCount) {
    return direction < 0 ? resultCount - 1 : 0;
  }
  return Math.max(0, Math.min(resultCount - 1, currentIndex + (direction < 0 ? -1 : 1)));
}

export function preferenceSearchTargetKeys(entry) {
  const keys = [
    entry?.targetKey,
    entry?.item?.key,
    entry?.key,
    entry?.control?.key,
    entry?.control?.valueKey,
  ];
  const dependency = entry?.control?.visibleWhen;
  if (Array.isArray(dependency) && dependency[0]) keys.push(dependency[0]);
  const dependsOn = entry?.control?.dependsOn;
  for (const value of Array.isArray(dependsOn) ? dependsOn : [dependsOn]) {
    if (typeof value === "string") keys.push(value);
    else if (value?.key) keys.push(value.key);
  }
  return [...new Set(keys.filter(Boolean).map(String))];
}
