/**
 * Keep preference disclosure state in the rendered controls, not in the
 * persisted preference value. This mirrors IINA's CollapseView: opening a
 * search result may reveal a folded region without firing the trigger's
 * original preference action.
 */
export function setPreferenceCollapseOpen(trigger, content, open) {
  const nextOpen = Boolean(open);
  if (trigger) {
    trigger.setAttribute?.("aria-expanded", String(nextOpen));
    if (trigger.type === "checkbox") trigger.checked = nextOpen;
  }
  if (content) {
    content.hidden = !nextOpen;
    if (content.dataset?.prefCollapseDisableContents === "true") {
      for (const field of content.querySelectorAll?.("input, select, textarea, button") || []) {
        field.disabled = !nextOpen;
      }
    }
  }
  const collapse = trigger?.closest?.("[data-pref-collapse]")
    || content?.closest?.("[data-pref-collapse]");
  collapse?.classList?.toggle?.("is-open", nextOpen);
  return nextOpen;
}

export function togglePreferenceCollapse(trigger, content) {
  const open = trigger?.getAttribute?.("aria-expanded") === "true";
  return setPreferenceCollapseOpen(trigger, content, !open);
}

/**
 * Search navigation expands the owning disclosure without dispatching a
 * change/click event, so hidden preference values are never rewritten.
 */
export function expandPreferenceCollapseForSearch(target) {
  const collapse = target?.closest?.("[data-pref-collapse]");
  if (!collapse) return false;
  const trigger = collapse.querySelector?.("[data-pref-collapse-trigger]");
  const content = collapse.querySelector?.("[data-pref-collapse-content]");
  if (!trigger || !content) return false;
  setPreferenceCollapseOpen(trigger, content, true);
  return true;
}
