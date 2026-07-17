import { initializeLocalization, tr, trKey } from "./localization.js";

await initializeLocalization();

const tauriInvoke = window.__TAURI__?.core?.invoke;
const tabButtons = [...document.querySelectorAll("[role=tab]")];
const panes = [...document.querySelectorAll("[data-pane]")];
const trackPicker = document.querySelector("#track-picker");
const watchRows = document.querySelector("#watch-rows");
const watchAddButton = document.querySelector("#watch-add");
const watchRemoveButton = document.querySelector("#watch-remove");
const watchDialog = document.querySelector("#watch-dialog");
const watchPropertyField = document.querySelector("#watch-property");
const watchDialogAddButton = document.querySelector("#watch-dialog-add");
const runtimeState = document.querySelector("#runtime-state");
const errorBanner = document.querySelector("#inspector-error");

const mockSnapshot = {
  sessionLabel: "main",
  mediaTitle: "Big Buck Bunny",
  hasMedia: true,
  general: {
    video: { format: "h264", size: "1920×1080", bitRate: "4 Mbps", codec: "h264", hardwareDecoder: "videotoolbox", driver: "libmpv", primaries: "bt.709 / bt.1886 (SDR)", fps: "23.976", colorspace: "bt.709 (SDR)", pixelFormat: "nv12 (HW)" },
    audio: { format: "floatp", channels: "stereo", bitRate: "192 Kbps", codec: "aac", driver: "coreaudio", sampleRate: "48000" },
  },
  tracks: [{ key: "video:1", kind: "Video", readableTitle: "Video #1 · h264", id: 1, defaultTrack: true, forced: false, selected: true, external: false, sourceId: "1", title: null, language: null, filePath: null, codec: "h264", decoder: "h264 (VideoToolbox)", fps: "23.976", channels: null, sampleRate: null }],
  file: { path: "/Movies/Big Buck Bunny.mp4", size: "30.1 MB", format: "MPEG-4", duration: "09:56", chapters: "0", editions: null },
  status: { avSyncDifference: "0.000", totalAvSync: "0.000", droppedFrames: "0", mistimedFrames: "0", displayFps: "60", estimatedOutputFps: "23.976", estimatedDisplayFps: "60" },
  watchProperties: [{ name: "pause", value: "false", valid: true, error: null }],
  runtime: { executorLifecycle: "client-ready", clientRunning: true, executorError: null, rendererInstalled: true, rendererAttached: true, rendererBackend: "opengl" },
  refreshIntervalMs: 1000,
};

let snapshot = mockSnapshot;
let selectedTrackKey = "";
let selectedWatchIndex = -1;
let refreshInFlight = false;
let errorTimer = 0;

function invoke(command, args = {}) {
  if (tauriInvoke) return tauriInvoke(command, args);
  if (command === "get_inspector_snapshot") return Promise.resolve(structuredClone(mockSnapshot));
  if (command === "set_inspector_watch_properties") {
    mockSnapshot.watchProperties = args.properties.map((name) => ({ name, value: "<Error>", valid: true, error: null }));
    return Promise.resolve(args.properties);
  }
  return Promise.resolve(null);
}

function showError(error) {
  window.clearTimeout(errorTimer);
  errorBanner.textContent = String(error?.message ?? error ?? tr("Unknown Error"));
  errorBanner.hidden = false;
  errorTimer = window.setTimeout(() => { errorBanner.hidden = true; }, 3500);
}

function activateTab(name, focus = false) {
  for (const button of tabButtons) {
    const active = button.dataset.tab === name;
    button.setAttribute("aria-selected", String(active));
    button.tabIndex = active ? 0 : -1;
    if (active && focus) button.focus();
  }
  for (const pane of panes) pane.hidden = pane.dataset.pane !== name;
}

function displayValue(value) {
  return value === null || value === undefined || value === "" ? "N/A" : String(value);
}

function setValue(element, value) {
  const available = value !== null && value !== undefined && value !== "";
  element.textContent = available ? String(value) : "N/A";
  element.classList.toggle("is-unavailable", !available);
  element.title = available ? String(value) : "";
}

function renderGeneral() {
  for (const element of document.querySelectorAll("[data-general]")) {
    const [group, key] = element.dataset.general.split(".");
    setValue(element, snapshot.general?.[group]?.[key]);
  }
}

function renderTrack() {
  const tracks = Array.isArray(snapshot.tracks) ? snapshot.tracks : [];
  if (!tracks.some((track) => track.key === selectedTrackKey)) {
    selectedTrackKey = tracks[0]?.key ?? "";
  }
  const currentOptions = [...trackPicker.options].map((option) => option.value).join("\n");
  const nextOptions = tracks.map((track) => track.key).join("\n");
  if (currentOptions !== nextOptions) {
    trackPicker.replaceChildren(...tracks.map((track) => new Option(track.readableTitle, track.key)));
  } else {
    tracks.forEach((track, index) => { trackPicker.options[index].textContent = track.readableTitle; });
  }
  trackPicker.disabled = tracks.length === 0;
  trackPicker.value = selectedTrackKey;

  const track = tracks.find((candidate) => candidate.key === selectedTrackKey);
  const flags = track ? [
    track.defaultTrack && trKey("InspectorWindowController", "S3X-Df-UMB.title", "Default"),
    track.forced && trKey("InspectorWindowController", "0vG-Pg-DIl.title", "Forced"),
    track.selected && trKey("InspectorWindowController", "P65-zr-dOH.title", "Selected"),
    track.external && trKey("InspectorWindowController", "PLL-fC-blc.title", "External"),
  ].filter(Boolean).join(", ") : null;
  const values = {
    id: track?.id,
    properties: flags,
    sourceId: track?.sourceId,
    title: track?.title,
    language: track?.language,
    filePath: track?.filePath,
    codec: track?.codec,
    decoder: track?.decoder,
    fps: track?.fps,
    channels: track?.channels,
    sampleRate: track?.sampleRate,
  };
  for (const element of document.querySelectorAll("[data-track]")) setValue(element, values[element.dataset.track]);
}

function renderFileAndStatus() {
  for (const element of document.querySelectorAll("[data-file]")) setValue(element, snapshot.file?.[element.dataset.file]);
  for (const element of document.querySelectorAll("[data-status]")) setValue(element, snapshot.status?.[element.dataset.status]);
}

function watchNames() {
  return (snapshot.watchProperties ?? []).map((property) => property.name);
}

function renderWatchProperties() {
  if (watchRows.querySelector(".watch-property-input:focus")) return;
  const properties = Array.isArray(snapshot.watchProperties) ? snapshot.watchProperties : [];
  if (selectedWatchIndex >= properties.length) selectedWatchIndex = -1;
  const fragment = document.createDocumentFragment();
  properties.forEach((property, index) => {
    const row = document.createElement("div");
    row.className = `watch-row${property.valid ? "" : " watch-invalid"}`;
    row.dataset.index = String(index);
    row.setAttribute("role", "row");
    row.setAttribute("aria-selected", String(index === selectedWatchIndex));

    const input = document.createElement("input");
    input.className = "watch-property-input";
    input.value = property.name;
    input.maxLength = 96;
    input.spellcheck = false;
    input.setAttribute("aria-label", tr("Property"));
    const value = document.createElement("span");
    value.className = "watch-value";
    value.setAttribute("role", "gridcell");
    value.textContent = property.valid ? displayValue(property.value ?? "<Error>") : tr(property.error ?? "Invalid property name");
    value.title = value.textContent;
    row.append(input, value);
    fragment.append(row);
  });
  watchRows.replaceChildren(fragment);
  watchRemoveButton.disabled = selectedWatchIndex < 0;
}

function renderRuntimeState() {
  const runtime = snapshot.runtime ?? {};
  runtimeState.textContent = runtime.clientRunning && runtime.rendererAttached ? "" : displayValue(runtime.executorError ?? runtime.executorLifecycle);
  runtimeState.title = runtimeState.textContent;
}

function render() {
  document.title = trKey("InspectorWindowController", "F0z-JX-Cv5.title", "Inspector");
  renderGeneral();
  renderTrack();
  renderFileAndStatus();
  renderWatchProperties();
  renderRuntimeState();
}

async function persistWatchProperties(properties) {
  try {
    await invoke("set_inspector_watch_properties", { properties });
    selectedWatchIndex = Math.min(selectedWatchIndex, properties.length - 1);
    await refresh(true);
  } catch (error) {
    showError(error);
    await refresh(true);
  }
}

async function refresh(force = false) {
  if (refreshInFlight || (!force && document.visibilityState === "hidden")) return;
  refreshInFlight = true;
  try {
    snapshot = await invoke("get_inspector_snapshot") ?? snapshot;
    render();
  } catch (error) {
    showError(error);
  } finally {
    refreshInFlight = false;
  }
}

tabButtons.forEach((button, index) => {
  button.addEventListener("click", () => activateTab(button.dataset.tab));
  button.addEventListener("keydown", (event) => {
    if (!(["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key))) return;
    event.preventDefault();
    const direction = document.documentElement.dir === "rtl" ? -1 : 1;
    let next = index;
    if (event.key === "Home") next = 0;
    else if (event.key === "End") next = tabButtons.length - 1;
    else if (event.key === "ArrowLeft") next = (index - direction + tabButtons.length) % tabButtons.length;
    else next = (index + direction + tabButtons.length) % tabButtons.length;
    activateTab(tabButtons[next].dataset.tab, true);
  });
});

trackPicker.addEventListener("change", () => {
  selectedTrackKey = trackPicker.value;
  renderTrack();
});

watchRows.addEventListener("pointerdown", (event) => {
  const row = event.target.closest(".watch-row");
  if (!row) return;
  selectedWatchIndex = Number(row.dataset.index);
  for (const candidate of watchRows.querySelectorAll(".watch-row")) {
    candidate.setAttribute("aria-selected", String(candidate === row));
  }
  watchRemoveButton.disabled = false;
});

watchRows.addEventListener("change", (event) => {
  if (!event.target.matches(".watch-property-input")) return;
  const row = event.target.closest(".watch-row");
  const index = Number(row?.dataset.index);
  const properties = watchNames();
  const name = event.target.value.trim();
  if (!name || !Number.isInteger(index) || index < 0 || index >= properties.length) {
    event.target.value = properties[index] ?? "";
    return;
  }
  properties[index] = name;
  void persistWatchProperties(properties);
});

watchAddButton.addEventListener("click", () => {
  if (watchNames().length >= 32) {
    showError(tr("At most 32 Inspector watch properties are allowed"));
    return;
  }
  watchPropertyField.value = "";
  watchDialog.showModal();
  window.setTimeout(() => watchPropertyField.focus(), 0);
});

watchDialog.addEventListener("close", () => {
  if (watchDialog.returnValue !== "default") return;
  const property = watchPropertyField.value.trim();
  if (!property) return;
  const properties = watchNames();
  properties.push(property);
  selectedWatchIndex = properties.length - 1;
  void persistWatchProperties(properties);
});

watchDialogAddButton.addEventListener("click", (event) => {
  if (!watchPropertyField.value.trim()) event.preventDefault();
});

watchRemoveButton.addEventListener("click", () => {
  if (selectedWatchIndex < 0) return;
  const properties = watchNames();
  properties.splice(selectedWatchIndex, 1);
  selectedWatchIndex = Math.min(selectedWatchIndex, properties.length - 1);
  void persistWatchProperties(properties);
});

document.addEventListener("visibilitychange", () => { if (document.visibilityState === "visible") void refresh(true); });
window.addEventListener("focus", () => void refresh(true));

activateTab("general");
render();
await refresh(true);
window.setInterval(refresh, 1000);
