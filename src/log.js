import { initializeLocalization, tr } from "./localization.js";

await initializeLocalization();

const tauriInvoke = window.__TAURI__?.core?.invoke;
const levelSelect = document.querySelector("#log-level");
const subsystemSelect = document.querySelector("#log-subsystem");
const saveButton = document.querySelector("#log-save");
const table = document.querySelector("#log-table");
const rows = document.querySelector("#log-rows");

let records = [];
let revision = -1;
let selection = new Set();
let selectionAnchor = -1;

const mockRecords = [
  { subsystem: "iina", level: 1, date: "09:41:18.023", message: "IINA 0.9.3 Build 93", log_string: "09:41:18.023 [iina][d] IINA 0.9.3 Build 93\n" },
  { subsystem: "mpv0", level: 0, date: "09:41:18.126", message: "Using libmpv render API", log_string: "09:41:18.126 [mpv0][v] Using libmpv render API\n" },
  { subsystem: "iina", level: 2, date: "09:41:18.214", message: "Release highlights loaded", log_string: "09:41:18.214 [iina][w] Release highlights loaded\n" },
];

function invoke(command, args = {}) {
  if (tauriInvoke) return tauriInvoke(command, args);
  if (command === "get_log_snapshot") return Promise.resolve({ revision: 1, records: mockRecords });
  return Promise.resolve(null);
}

function applyLocalization() {
  document.title = tr("Log Viewer");
  document.querySelector(".log-window").setAttribute("aria-label", tr("Log Viewer"));
  document.querySelector('label[for="log-level"]').textContent = tr("Level:");
  document.querySelector('label[for="log-subsystem"]').textContent = tr("Subsystem:");
  saveButton.textContent = tr("Save as…");
  const levelLabels = ["Verbose", "Debug", "Warning", "Error"];
  Array.from(levelSelect.options).forEach((option, index) => {
    option.textContent = tr(levelLabels[index]);
  });
  subsystemSelect.options[0].textContent = tr("All");
  const headers = document.querySelectorAll(".log-table-header [role=columnheader]");
  headers[1].textContent = tr("Time");
  headers[2].textContent = tr("Subsystem");
  headers[3].textContent = tr("Message");
}

function filteredRecordIndexes() {
  const minimumLevel = Number(levelSelect.value) || 0;
  const subsystem = subsystemSelect.value;
  return records
    .map((record, index) => ({ record, index }))
    .filter(({ record }) => Number(record.level) >= minimumLevel && (!subsystem || record.subsystem === subsystem));
}

function updateSubsystems() {
  const selected = subsystemSelect.value;
  const subsystems = [...new Set(records.map((record) => record.subsystem).filter(Boolean))].sort((a, b) => {
    if (a === "iina") return -1;
    if (b === "iina") return 1;
    return a.localeCompare(b);
  });
  subsystemSelect.replaceChildren(new Option(tr("All"), ""));
  for (const subsystem of subsystems) subsystemSelect.add(new Option(subsystem, subsystem));
  subsystemSelect.value = subsystems.includes(selected) ? selected : "";
}

function isScrolledToBottom() {
  return table.scrollTop + table.clientHeight >= table.scrollHeight - 2;
}

function render() {
  const shouldScroll = isScrolledToBottom();
  const fragment = document.createDocumentFragment();
  for (const { record, index } of filteredRecordIndexes()) {
    const row = document.createElement("div");
    row.className = "log-row";
    row.dataset.index = String(index);
    row.dataset.level = String(record.level);
    row.setAttribute("role", "row");
    row.setAttribute("aria-selected", String(selection.has(index)));
    row.title = record.log_string || record.message;

    const indicator = document.createElement("div");
    indicator.className = "log-level-dot";
    indicator.setAttribute("role", "gridcell");
    indicator.setAttribute("aria-label", ["Verbose", "Debug", "Warning", "Error"][Number(record.level)] || "");
    for (const [value, className] of [
      [record.date, "log-cell"],
      [record.subsystem, "log-cell"],
      [record.message, "log-cell log-cell--message"],
    ]) {
      const cell = document.createElement("div");
      cell.className = className;
      cell.setAttribute("role", "gridcell");
      cell.textContent = value || "";
      row.append(cell);
    }
    row.prepend(indicator);
    fragment.append(row);
  }
  rows.replaceChildren(fragment);
  if (shouldScroll) table.scrollTop = table.scrollHeight;
}

function selectRow(index, event) {
  const visible = filteredRecordIndexes().map(({ index: itemIndex }) => itemIndex);
  if (event.shiftKey && selectionAnchor >= 0) {
    const start = visible.indexOf(selectionAnchor);
    const end = visible.indexOf(index);
    if (start >= 0 && end >= 0) {
      if (!event.metaKey) selection.clear();
      const [lower, upper] = start < end ? [start, end] : [end, start];
      visible.slice(lower, upper + 1).forEach((itemIndex) => selection.add(itemIndex));
    }
  } else if (event.metaKey) {
    if (selection.has(index)) selection.delete(index);
    else selection.add(index);
    selectionAnchor = index;
  } else {
    selection = new Set([index]);
    selectionAnchor = index;
  }
  render();
}

function selectedLogString() {
  const selected = [...selection].sort((a, b) => a - b).map((index) => records[index]).filter(Boolean);
  const source = selected.length ? selected : records;
  return source.map((record) => record.log_string || `${record.date} [${record.subsystem}] ${record.message}\n`).join("");
}

async function refresh() {
  try {
    const snapshot = await invoke("get_log_snapshot");
    if (!snapshot || Number(snapshot.revision) === revision) return;
    revision = Number(snapshot.revision);
    records = Array.isArray(snapshot.records) ? snapshot.records : [];
    selection = new Set([...selection].filter((index) => index < records.length));
    updateSubsystems();
    render();
  } catch (error) {
    console.error("Unable to refresh logs", error);
  }
}

rows.addEventListener("click", (event) => {
  const row = event.target.closest(".log-row");
  if (row) selectRow(Number(row.dataset.index), event);
});
levelSelect.addEventListener("change", render);
subsystemSelect.addEventListener("change", render);
saveButton.addEventListener("click", () => invoke("save_log_records", { contents: selectedLogString() }));
table.addEventListener("keydown", async (event) => {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "a") {
    event.preventDefault();
    selection = new Set(filteredRecordIndexes().map(({ index }) => index));
    render();
  } else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "c") {
    event.preventDefault();
    await navigator.clipboard.writeText(selectedLogString());
  }
});

applyLocalization();
await refresh();
window.setInterval(refresh, 100);
