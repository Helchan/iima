import { initializeLocalization, trKey } from "./localization.js";

await initializeLocalization();

const invoke = window.__TAURI__?.core?.invoke;
const highlights = document.querySelector("#guide-highlights");
const loading = document.querySelector("#guide-loading");
const failed = document.querySelector("#guide-failed");
const continueButton = document.querySelector("#guide-continue");
const websiteButton = document.querySelector("#guide-website");

document.title = trKey("Localizable", "guide.highlights", "Highlights");
continueButton.textContent = trKey("GuideWindowController", "pRW-Nk-MIQ.title", "Continue");
websiteButton.textContent = trKey("GuideWindowController", "FJS-b9-ATj.title", "Website");
failed.querySelector("p").textContent = trKey(
  "GuideWindowController",
  "oaH-Na-skD.title",
  "Failed to load highlights. Please visit our website https://iina.io for more information.",
);

function revealHighlights() {
  loading.hidden = true;
  failed.hidden = true;
  highlights.hidden = false;
}

function revealFailure() {
  loading.hidden = true;
  highlights.hidden = true;
  failed.hidden = false;
}

highlights.addEventListener("load", revealHighlights, { once: true });
highlights.addEventListener("error", revealFailure, { once: true });

continueButton.addEventListener("click", async () => {
  if (invoke) await invoke("close_release_highlights");
  else window.close();
});

websiteButton.addEventListener("click", async () => {
  if (invoke) await invoke("open_iina_website");
  else window.open("https://iina.io", "_blank", "noopener,noreferrer");
});
