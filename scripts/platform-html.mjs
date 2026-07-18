const HTML_ROOT = '<html lang="en">';
const MACOS_HTML_ROOT = '<html lang="en" class="platform-macos">';

export function htmlForBuildPlatform(html, platform = process.platform) {
  if (platform !== "darwin" || !html.includes(HTML_ROOT)) return html;
  return html.replace(HTML_ROOT, MACOS_HTML_ROOT);
}
