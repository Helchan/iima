import { initializeLocalization, tr } from "./localization.js";

await initializeLocalization();

const tauriInvoke = window.__TAURI__?.core?.invoke;
const tauriListen = window.__TAURI__?.event?.listen;

const mockHistory = [
  {
    id: "mock-1",
    path: "/Users/demo/Movies/Big Buck Bunny.mp4",
    name: "Big Buck Bunny.mp4",
    played: true,
    added_date: new Date().toISOString(),
    duration_seconds: 596,
    progress_seconds: 128,
    file_exists: true,
  },
  {
    id: "mock-2",
    path: "/Users/demo/Music/Overture.flac",
    name: "Overture.flac",
    played: true,
    added_date: new Date(Date.now() - 86_400_000).toISOString(),
    duration_seconds: 264,
    progress_seconds: null,
    file_exists: true,
  },
];

async function invoke(command, args = {}) {
  if (tauriInvoke) return tauriInvoke(command, args);
  if (command === "get_playback_history") return structuredClone(mockHistory);
  if (command === "remove_playback_history_entries") {
    const ids = new Set(args.ids || []);
    for (let index = mockHistory.length - 1; index >= 0; index -= 1) {
      if (ids.has(mockHistory[index].id)) mockHistory.splice(index, 1);
    }
    return structuredClone(mockHistory);
  }
  return null;
}

const els = {
  window: document.querySelector(".history-window"),
  groupBy: document.querySelector("#history-group-by"),
  search: document.querySelector("#history-search"),
  searchOptionsButton: document.querySelector("#history-search-options-button"),
  groups: document.querySelector("#history-groups"),
  contextMenu: document.querySelector("#history-context-menu"),
  searchMenu: document.querySelector("#history-search-menu"),
  confirmLayer: document.querySelector("#history-confirm-layer"),
  confirmCancel: document.querySelector("#history-confirm-cancel"),
  confirmOk: document.querySelector("#history-confirm-ok"),
};

let historyItems = await invoke("get_playback_history");
let groupBy = "date";
let searchOption = "path";
let selectedIds = new Set();
let selectionAnchor = -1;
let collapsedGroups = new Set();
let deleteInFlight = false;
let lastFocusRefresh = 0;

renderHistory();

els.groupBy.addEventListener("change", () => {
  groupBy = els.groupBy.value === "folder" ? "folder" : "date";
  els.window.classList.toggle("group-folder", groupBy === "folder");
  collapsedGroups.clear();
  selectionAnchor = -1;
  renderHistory();
});

els.search.addEventListener("input", () => {
  selectionAnchor = -1;
  renderHistory();
});

els.searchOptionsButton.addEventListener("click", (event) => {
  event.stopPropagation();
  if (els.searchMenu.hidden) {
    hideMenus();
    const rect = els.searchOptionsButton.getBoundingClientRect();
    showMenu(els.searchMenu, rect.left, rect.bottom + 2);
    els.searchOptionsButton.setAttribute("aria-expanded", "true");
  } else {
    hideMenus();
  }
});

els.searchMenu.addEventListener("click", (event) => {
  const button = event.target.closest("[data-search-option]");
  if (!button) return;
  searchOption = button.dataset.searchOption === "filename" ? "filename" : "path";
  updateSearchMenuChecks();
  hideMenus();
  renderHistory();
  els.search.focus();
});

els.groups.addEventListener("click", (event) => {
  const disclosure = event.target.closest("[data-group-disclosure]");
  if (disclosure) {
    const key = disclosure.dataset.groupDisclosure;
    if (collapsedGroups.has(key)) collapsedGroups.delete(key);
    else collapsedGroups.add(key);
    renderHistory();
    return;
  }
  const row = event.target.closest(".history-entry-row");
  if (!row) return;
  selectHistoryRow(row.dataset.historyId, event);
});

els.groups.addEventListener("dblclick", (event) => {
  const row = event.target.closest(".history-entry-row");
  if (!row) return;
  selectOnly(row.dataset.historyId);
  void playSelected(false);
});

els.groups.addEventListener("contextmenu", (event) => {
  const row = event.target.closest(".history-entry-row");
  if (!row) return;
  event.preventDefault();
  if (!selectedIds.has(row.dataset.historyId)) selectOnly(row.dataset.historyId);
  configureContextMenu();
  hideMenus();
  showMenu(els.contextMenu, event.clientX, event.clientY);
});

els.contextMenu.addEventListener("click", (event) => {
  const action = event.target.closest("[data-action]")?.dataset.action;
  if (!action) return;
  hideMenus();
  if (action === "play") void playSelected(false);
  if (action === "play-new") void playSelected(true);
  if (action === "reveal") void revealSelected();
  if (action === "delete") showDeleteConfirmation();
});

els.confirmCancel.addEventListener("click", hideDeleteConfirmation);
els.confirmOk.addEventListener("click", () => void removeSelectedHistory());

document.addEventListener("pointerdown", (event) => {
  if (!event.target.closest(".history-menu") && !event.target.closest("#history-search-options-button")) {
    hideMenus();
  }
});

window.addEventListener("keydown", (event) => {
  if (!els.confirmLayer.hidden) {
    if (event.key === "Escape") {
      event.preventDefault();
      hideDeleteConfirmation();
    } else if (event.key === "Enter") {
      event.preventDefault();
      void removeSelectedHistory();
    }
    return;
  }
  if (event.metaKey && !event.altKey && !event.ctrlKey) {
    if (event.key.toLowerCase() === "f") {
      event.preventDefault();
      els.search.focus();
      els.search.select();
      return;
    }
    if (event.key.toLowerCase() === "a" && document.activeElement !== els.search) {
      event.preventDefault();
      selectedIds = new Set(filteredHistory().map((item) => item.id));
      renderSelection();
      return;
    }
  }
  if (
    (event.key === "Delete" || event.key === "Backspace")
    && document.activeElement !== els.search
    && selectedIds.size > 0
  ) {
    event.preventDefault();
    showDeleteConfirmation();
  }
});

window.addEventListener("focus", () => {
  if (Date.now() - lastFocusRefresh > 250) void refreshHistory();
});

if (tauriListen) {
  await tauriListen("iima-history-updated", () => void refreshHistory());
}

async function refreshHistory() {
  lastFocusRefresh = Date.now();
  historyItems = await invoke("get_playback_history");
  const available = new Set(historyItems.map((item) => item.id));
  selectedIds = new Set([...selectedIds].filter((id) => available.has(id)));
  renderHistory();
}

function renderHistory() {
  const groups = groupedHistory(filteredHistory());
  els.groups.replaceChildren();
  for (const [key, items] of groups) {
    const group = document.createElement("section");
    group.className = `history-group${collapsedGroups.has(key) ? " collapsed" : ""}`;

    const groupRow = document.createElement("div");
    groupRow.className = "history-group-row";
    groupRow.setAttribute("role", "row");
    const disclosure = document.createElement("button");
    disclosure.type = "button";
    disclosure.className = "history-group-disclosure";
    disclosure.dataset.groupDisclosure = key;
    disclosure.setAttribute("aria-label", tr(collapsedGroups.has(key) ? "Expand group" : "Collapse group"));
    disclosure.textContent = "▾";
    const label = document.createElement("span");
    label.textContent = key;
    groupRow.append(disclosure, label);

    const entries = document.createElement("div");
    entries.className = "history-group-entries";
    for (const item of items) entries.append(historyEntryRow(item));
    group.append(groupRow, entries);
    els.groups.append(group);
  }
  renderSelection();
}

function historyEntryRow(item) {
  const row = document.createElement("div");
  row.className = `history-entry-row${item.file_exists ? "" : " missing"}`;
  row.dataset.historyId = item.id;
  row.setAttribute("role", "row");
  row.setAttribute("aria-selected", String(selectedIds.has(item.id)));

  const file = document.createElement("div");
  file.className = "history-file-cell";
  file.title = item.path;
  const icon = document.createElement("span");
  icon.className = "history-file-icon";
  icon.setAttribute("aria-hidden", "true");
  const filename = document.createElement("span");
  filename.className = "history-file-name";
  filename.textContent = isRemotePath(item.path) ? item.path : item.name;
  file.append(icon, filename);

  const progress = document.createElement("div");
  progress.className = "history-progress-cell";
  if (Number.isFinite(Number(item.progress_seconds))) {
    const track = document.createElement("span");
    track.className = "history-progress-track";
    const fill = document.createElement("span");
    fill.className = "history-progress-fill";
    const duration = Math.max(0, Number(item.duration_seconds) || 0);
    const position = Math.max(0, Number(item.progress_seconds) || 0);
    fill.style.width = `${duration > 0 ? Math.min(100, position / duration * 100) : 0}%`;
    track.append(fill);
    const time = document.createElement("span");
    time.className = "history-progress-time";
    time.textContent = formatTime(position);
    progress.append(track, time);
  }

  const playedAt = document.createElement("div");
  playedAt.className = "history-time-cell";
  playedAt.textContent = formatPlayedAt(item.added_date);
  row.append(file, progress, playedAt);
  return row;
}

function filteredHistory() {
  const query = normalizedSearch(els.search.value);
  if (!query) return historyItems;
  return historyItems.filter((item) => {
    const source = searchOption === "filename" ? item.name : item.path;
    return normalizedSearch(source).includes(query);
  });
}

function groupedHistory(items) {
  const groups = new Map();
  for (const item of items) {
    const key = groupBy === "folder" ? parentPath(item.path) : formatGroupDate(item.added_date);
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(item);
  }
  return groups;
}

function selectHistoryRow(id, event) {
  const visible = filteredHistory();
  const index = visible.findIndex((item) => item.id === id);
  if (event.shiftKey && selectionAnchor >= 0) {
    const first = Math.min(selectionAnchor, index);
    const last = Math.max(selectionAnchor, index);
    if (!event.metaKey) selectedIds.clear();
    for (let current = first; current <= last; current += 1) selectedIds.add(visible[current].id);
  } else if (event.metaKey || event.ctrlKey) {
    if (selectedIds.has(id)) selectedIds.delete(id);
    else selectedIds.add(id);
    selectionAnchor = index;
  } else {
    selectedIds = new Set([id]);
    selectionAnchor = index;
  }
  renderSelection();
}

function selectOnly(id) {
  selectedIds = new Set([id]);
  selectionAnchor = filteredHistory().findIndex((item) => item.id === id);
  renderSelection();
}

function renderSelection() {
  els.groups.querySelectorAll(".history-entry-row").forEach((row) => {
    const selected = selectedIds.has(row.dataset.historyId);
    row.classList.toggle("selected", selected);
    row.setAttribute("aria-selected", String(selected));
  });
}

function selectedHistory() {
  return historyItems.filter((item) => selectedIds.has(item.id));
}

async function playSelected(newWindow) {
  const item = selectedHistory()[0];
  if (!item) return;
  await invoke("open_playback_history_item", { path: item.path, newWindow });
}

async function revealSelected() {
  const paths = selectedHistory().filter((item) => item.file_exists).map((item) => item.path);
  if (paths.length) await invoke("reveal_playback_history_items", { paths });
}

function showDeleteConfirmation() {
  if (!selectedIds.size || deleteInFlight) return;
  hideMenus();
  els.confirmLayer.hidden = false;
  requestAnimationFrame(() => els.confirmOk.focus());
}

function hideDeleteConfirmation() {
  if (deleteInFlight) return;
  els.confirmLayer.hidden = true;
  els.groups.focus();
}

async function removeSelectedHistory() {
  if (deleteInFlight || !selectedIds.size) return;
  deleteInFlight = true;
  els.confirmCancel.disabled = true;
  els.confirmOk.disabled = true;
  try {
    historyItems = await invoke("remove_playback_history_entries", { ids: [...selectedIds] });
    selectedIds.clear();
    selectionAnchor = -1;
    els.confirmLayer.hidden = true;
    renderHistory();
  } finally {
    deleteInFlight = false;
    els.confirmCancel.disabled = false;
    els.confirmOk.disabled = false;
  }
}

function configureContextMenu() {
  const selected = selectedHistory();
  const hasSelection = selected.length > 0;
  els.contextMenu.querySelector('[data-action="play"]').disabled = !hasSelection;
  els.contextMenu.querySelector('[data-action="play-new"]').disabled = !hasSelection;
  els.contextMenu.querySelector('[data-action="delete"]').disabled = !hasSelection;
  els.contextMenu.querySelector('[data-action="reveal"]').disabled = !selected.some((item) => item.file_exists);
}

function updateSearchMenuChecks() {
  els.searchMenu.querySelectorAll("[data-search-option]").forEach((button) => {
    const checked = button.dataset.searchOption === searchOption;
    button.setAttribute("aria-checked", String(checked));
    button.querySelector(".history-menu-check").textContent = checked ? "✓" : "";
  });
}

function showMenu(menu, x, y) {
  menu.hidden = false;
  menu.style.left = "0px";
  menu.style.top = "0px";
  const rect = menu.getBoundingClientRect();
  menu.style.left = `${Math.max(4, Math.min(x, window.innerWidth - rect.width - 4))}px`;
  menu.style.top = `${Math.max(4, Math.min(y, window.innerHeight - rect.height - 4))}px`;
}

function hideMenus() {
  els.contextMenu.hidden = true;
  els.searchMenu.hidden = true;
  els.searchOptionsButton.setAttribute("aria-expanded", "false");
}

function formatTime(seconds) {
  const value = Math.max(0, Math.floor(Number(seconds) || 0));
  const hours = Math.floor(value / 3600);
  const minutes = Math.floor(value % 3600 / 60);
  const remainder = value % 60;
  if (hours > 0) return `${hours}:${String(minutes).padStart(2, "0")}:${String(remainder).padStart(2, "0")}`;
  return `${minutes}:${String(remainder).padStart(2, "0")}`;
}

function formatGroupDate(value) {
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return tr("Unknown Date");
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(date);
}

function formatPlayedAt(value) {
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return "";
  if (groupBy === "folder") {
    return new Intl.DateTimeFormat(undefined, { dateStyle: "short", timeStyle: "short" }).format(date);
  }
  return new Intl.DateTimeFormat(undefined, { timeStyle: "short" }).format(date);
}

function parentPath(path) {
  if (isRemotePath(path)) {
    try {
      const url = new URL(path);
      const parts = url.pathname.split("/").filter(Boolean);
      parts.pop();
      return `/${parts.join("/")}` || "/";
    } catch {
      return path;
    }
  }
  const normalized = String(path).replace(/\\/g, "/");
  const index = normalized.lastIndexOf("/");
  if (index < 0) return ".";
  return normalized.slice(0, index) || "/";
}

function isRemotePath(path) {
  return /^[a-z][a-z0-9+.-]*:\/\//i.test(String(path));
}

function normalizedSearch(value) {
  return String(value || "")
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLocaleLowerCase();
}
