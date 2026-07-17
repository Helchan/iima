const invoke = window.__TAURI__?.core?.invoke;
const listen = window.__TAURI__?.event?.listen;
const emitTo = window.__TAURI__?.event?.emitTo;

const history = document.querySelector("#developer-history");
const input = document.querySelector("#developer-input");
const runButton = document.querySelector("#developer-run");
const globalButton = document.querySelector("#developer-global");
const clearButton = document.querySelector("#developer-clear");
const splitter = document.querySelector("#developer-splitter");

let context = null;
let nextRequestId = 0;
let nextPromptIndex = 0;
let recallIndex = -1;
const prompts = [];
const pendingIndexes = new Map();

function appendRow(label, content, className = "") {
  const row = document.createElement("li");
  row.className = `developer-row ${className}`.trim();
  const rowLabel = document.createElement("span");
  rowLabel.className = "developer-row-label";
  rowLabel.textContent = label;
  const value = document.createElement("span");
  value.append(content);
  row.append(rowLabel, value);
  history.append(row);
  row.scrollIntoView({ block: "end" });
}

function appendPrompt(source, index) {
  appendRow(`[${index}]:`, document.createTextNode(source), "developer-prompt");
}

function valueNode(result) {
  const kind = String(result?.kind || "undefined");
  if (kind === "array" || kind === "object") {
    const details = document.createElement("details");
    details.className = "developer-object";
    const summary = document.createElement("summary");
    summary.textContent = String(result.title || (kind === "array" ? "Array" : "Object"));
    details.append(summary);
    for (const [key, value] of Array.from(result.entries || [])) {
      const row = document.createElement("div");
      row.className = "developer-object-row";
      const keyNode = document.createElement("span");
      keyNode.className = "developer-object-key";
      keyNode.textContent = `${key}:`;
      row.append(keyNode, document.createTextNode(String(value)));
      details.append(row);
    }
    if (Number(result.remaining) > 0) {
      const remaining = document.createElement("div");
      remaining.className = "developer-object-row";
      remaining.textContent = `… (${result.remaining} more)`;
      details.append(remaining);
    }
    return details;
  }
  const value = document.createElement("span");
  value.className = `developer-value developer-value--${kind}`;
  if (kind === "null" || kind === "undefined") value.textContent = kind;
  else value.textContent = String(result?.value ?? "");
  return value;
}

function appendMessage(message, level, stack = "") {
  const row = document.createElement("li");
  row.className = `developer-message developer-message--${level}`;
  row.textContent = message;
  if (stack) {
    const stackNode = document.createElement("div");
    stackNode.className = "developer-stack";
    stackNode.textContent = stack;
    row.append(stackNode);
  }
  history.append(row);
  row.scrollIntoView({ block: "end" });
}

async function execute(source, { preserveInput = false } = {}) {
  const normalized = String(source || "").trim();
  if (!normalized || !context || !emitTo) return;
  const requestId = ++nextRequestId;
  const promptIndex = ++nextPromptIndex;
  pendingIndexes.set(requestId, promptIndex);
  appendPrompt(normalized, promptIndex);
  if (normalized !== "$global") {
    prompts.push(normalized);
    recallIndex = -1;
  }
  await emitTo(context.ownerLabel, "iima-plugin-developer-tool-evaluate", {
    identifier: context.identifier,
    role: context.role,
    contextId: context.contextId,
    requestId,
    source: normalized,
  });
  if (!preserveInput) input.value = "";
}

runButton.addEventListener("click", () => void execute(input.value));
globalButton.addEventListener("click", () => void execute("$global", { preserveInput: true }));
clearButton.addEventListener("click", () => {
  history.replaceChildren();
  pendingIndexes.clear();
});
splitter.addEventListener("pointerdown", (event) => {
  event.preventDefault();
  splitter.setPointerCapture(event.pointerId);
  const startY = event.clientY;
  const initialHeight = document.querySelector(".developer-console").getBoundingClientRect().height;
  const move = (moveEvent) => {
    const height = Math.max(108, Math.min(134, initialHeight + startY - moveEvent.clientY));
    document.documentElement.style.setProperty("--developer-console-height", `${height}px`);
  };
  const stop = () => {
    splitter.removeEventListener("pointermove", move);
    splitter.removeEventListener("pointerup", stop);
    splitter.removeEventListener("pointercancel", stop);
  };
  splitter.addEventListener("pointermove", move);
  splitter.addEventListener("pointerup", stop);
  splitter.addEventListener("pointercancel", stop);
});
input.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && event.metaKey) {
    event.preventDefault();
    void execute(input.value);
    return;
  }
  if (event.key === "ArrowUp" && !input.value && prompts.length) {
    event.preventDefault();
    recallIndex = Math.min(recallIndex + 1, prompts.length - 1);
    input.value = prompts[prompts.length - 1 - recallIndex];
  } else if (event.key !== "ArrowUp") {
    recallIndex = -1;
  }
});

if (listen) {
  await listen("iima-plugin-developer-tool-context", (event) => {
    context = event.payload;
  });
  await listen("iima-plugin-developer-tool-result", (event) => {
    if (!context || event.payload?.contextId !== context.contextId) return;
    const requestId = Number(event.payload?.requestId);
    if (!pendingIndexes.delete(requestId)) return;
    if (event.payload?.exception) {
      appendMessage(
        `Exception: ${event.payload.exception.message || "Unknown exception"}`,
        "error",
        String(event.payload.exception.stack || "???"),
      );
      return;
    }
    appendRow("→", valueNode(event.payload?.result));
  });
  await listen("iima-plugin-developer-tool-log", (event) => {
    if (!context || event.payload?.contextId !== context.contextId) return;
    appendMessage(String(event.payload?.message || ""), String(event.payload?.level || "debug"));
  });
}

if (invoke) {
  try {
    context = await invoke("get_plugin_developer_tool_context");
  } catch (error) {
    appendMessage(String(error?.message || error || "Unable to bind plugin Developer Tool"), "error");
  }
}
input.focus();
