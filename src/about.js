import { initializeLocalization, trKey } from "./localization.js";

const locale = await initializeLocalization();
const tauriInvoke = window.__TAURI__?.core?.invoke;
const tabButtons = [...document.querySelectorAll("[role=tab]")];
const panes = [...document.querySelectorAll("[data-pane]")];
const versionLabel = document.querySelector("#about-version");
const mpvLabel = document.querySelector("#about-mpv");
const ffmpegLabel = document.querySelector("#about-ffmpeg");
const licenseDocument = document.querySelector("#about-license");
const creditsDocument = document.querySelector("#about-credits");
const contributorGrid = document.querySelector("#about-contributor-grid");
const GITHUB_CONTRIBUTORS_ENDPOINT = "https://api.github.com/repos/iina/iina/contributors";
const ABOUT_DOCUMENT_LINKS = new Map([
  ["https://github.com/iina/iina", "github"],
  ["https://iina.io", "website"],
  ["mailto:developers@iina.io", "email"],
  ["https://github.com/lhc70000", "collider"],
  ["https://github.com/lhc70000/iina/graphs/contributors", "legacy-contributors"],
  ["http://www.gnu.org/licenses/", "gpl"],
]);

const mockRuntime = {
  version: "0.9.3",
  build: "93",
  mpvVersion: null,
  ffmpegVersion: null,
};

function invoke(command, args = {}) {
  if (tauriInvoke) return tauriInvoke(command, args);
  if (command === "get_about_runtime") return Promise.resolve(mockRuntime);
  return Promise.resolve(null);
}

function activateTab(name, focus = false) {
  for (const button of tabButtons) {
    const selected = button.dataset.tab === name;
    button.classList.toggle("is-selected", selected);
    button.setAttribute("aria-selected", String(selected));
    button.tabIndex = selected ? 0 : -1;
    if (selected && focus) button.focus();
  }
  for (const pane of panes) pane.hidden = pane.dataset.pane !== name;
}

tabButtons.forEach((button, index) => {
  button.addEventListener("click", () => activateTab(button.dataset.tab));
  button.addEventListener("keydown", (event) => {
    if (!["ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    let next = index;
    if (event.key === "Home") next = 0;
    else if (event.key === "End") next = tabButtons.length - 1;
    else if (event.key === "ArrowUp") next = (index - 1 + tabButtons.length) % tabButtons.length;
    else next = (index + 1) % tabButtons.length;
    activateTab(tabButtons[next].dataset.tab, true);
  });
});

for (const button of document.querySelectorAll("[data-link]")) {
  button.addEventListener("click", () => {
    Promise.resolve(invoke("open_about_link", { link: button.dataset.link })).catch(() => {});
  });
}

function localeEntry(collection) {
  const normalized = String(locale || "en").toLowerCase();
  const exact = Object.keys(collection ?? {}).find((candidate) => candidate.toLowerCase() === normalized);
  if (exact) return collection[exact];
  const family = normalized.split("-")[0];
  const parent = Object.keys(collection ?? {}).find((candidate) => candidate.toLowerCase() === family);
  return collection?.[parent] ?? collection?.en ?? collection?.Base ?? null;
}

function renderRichDocument(host, richDocument, plainText) {
  if (!richDocument?.style || !richDocument?.body || !host.attachShadow) {
    host.classList.add("is-plain");
    host.textContent = plainText ?? "";
    return;
  }
  host.classList.remove("is-plain");
  const template = document.createElement("template");
  template.innerHTML = richDocument.body;
  const allowedElements = new Set(["A", "B", "BR", "I", "P", "SPAN"]);
  for (const element of [...template.content.querySelectorAll("*")]) {
    if (!allowedElements.has(element.tagName)) {
      element.replaceWith(document.createTextNode(element.textContent ?? ""));
      continue;
    }
    for (const attribute of [...element.attributes]) {
      if (attribute.name !== "class" && !(element.tagName === "A" && attribute.name === "href")) {
        element.removeAttribute(attribute.name);
      }
    }
    if (element.tagName === "A" && !ABOUT_DOCUMENT_LINKS.has(element.getAttribute("href") ?? "")) {
      element.removeAttribute("href");
    }
  }

  const shadow = host.attachShadow({ mode: "open" });
  const style = document.createElement("style");
  style.textContent = `
    :host { color: inherit; font: inherit; user-select: text; }
    a { color: LinkText; }
    ${richDocument.style}
  `;
  shadow.append(style, template.content);
  shadow.addEventListener("click", (event) => {
    const anchor = event.target instanceof Element ? event.target.closest("a") : null;
    if (!anchor) return;
    event.preventDefault();
    const link = ABOUT_DOCUMENT_LINKS.get(anchor.getAttribute("href") ?? "");
    if (link) Promise.resolve(invoke("open_about_link", { link })).catch(() => {});
  });
}

async function loadDocuments() {
  try {
    const response = await fetch(new URL("./assets/iina/about-documents.json", import.meta.url));
    if (!response.ok) return;
    const documents = await response.json();
    renderRichDocument(
      licenseDocument,
      localeEntry(documents.licenseHtml),
      localeEntry(documents.licenses),
    );
    renderRichDocument(creditsDocument, documents.creditsHtml, documents.credits);
  } catch {
    // The reference leaves an unavailable RTF view empty rather than showing an invented error.
  }
}

async function loadRuntime() {
  try {
    const runtime = await invoke("get_about_runtime") ?? mockRuntime;
    versionLabel.textContent = `${runtime.version} Build ${runtime.build}`;
    mpvLabel.textContent = runtime.mpvVersion ?? "mpv";
    ffmpegLabel.textContent = runtime.ffmpegVersion ?? "FFmpeg";
  } catch {
    versionLabel.textContent = `${mockRuntime.version} Build ${mockRuntime.build}`;
  }
}

function nextContributorPage(response) {
  const link = response.headers.get("link") ?? "";
  const match = link.match(/<([^>]+)>;\s*rel="next"/);
  if (!match) return null;
  try {
    const candidate = new URL(match[1]);
    const endpoint = new URL(GITHUB_CONTRIBUTORS_ENDPOINT);
    const allowedQueryKeys = new Set(["anon", "page", "per_page"]);
    if (
      candidate.protocol !== "https:" ||
      candidate.username ||
      candidate.password ||
      candidate.hash ||
      candidate.origin !== endpoint.origin ||
      candidate.pathname !== endpoint.pathname ||
      [...candidate.searchParams.keys()].some((key) => !allowedQueryKeys.has(key))
    ) return null;
    return candidate.href;
  } catch {
    return null;
  }
}

function safeAvatarUrl(value) {
  try {
    const candidate = new URL(String(value ?? ""));
    if (
      candidate.protocol !== "https:" ||
      candidate.username ||
      candidate.password ||
      candidate.hash ||
      !/^avatars\d*\.githubusercontent\.com$/i.test(candidate.hostname)
    ) return null;
    return candidate.href;
  } catch {
    return null;
  }
}

function appendContributors(contributors) {
  const fragment = document.createDocumentFragment();
  for (const contributor of contributors) {
    const avatarUrl = safeAvatarUrl(contributor?.avatar_url);
    if (!contributor?.login || !avatarUrl) continue;
    const image = document.createElement("img");
    image.className = "about-avatar";
    image.src = avatarUrl;
    image.alt = contributor.login;
    image.title = contributor.login;
    image.loading = "lazy";
    image.referrerPolicy = "no-referrer";
    image.addEventListener("error", () => {
      image.removeAttribute("src");
      image.classList.add("is-unavailable");
    }, { once: true });
    fragment.append(image);
  }
  contributorGrid.append(fragment);
}

async function loadContributors() {
  let url = GITHUB_CONTRIBUTORS_ENDPOINT;
  const visited = new Set();
  while (url && !visited.has(url) && visited.size < 32) {
    visited.add(url);
    const response = await fetch(url, {
      headers: { Accept: "application/vnd.github+json" },
      cache: "force-cache",
    });
    if (!response.ok) return;
    const contributors = await response.json();
    if (!Array.isArray(contributors)) return;
    appendContributors(contributors);
    url = nextContributorPage(response);
  }
}

document.title = trKey("AboutWindowController", "F0z-JX-Cv5.title", "About");
activateTab("license");
await Promise.allSettled([loadDocuments(), loadRuntime(), loadContributors()]);
