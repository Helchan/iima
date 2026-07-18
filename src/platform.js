export function isMacOSHost({ userAgentDataPlatform = "", platform = "", userAgent = "" } = {}) {
  // WKWebView may expose a truthy but non-descriptive userAgentData.platform (for example
  // "Unknown"). Test every available signal instead of letting that value mask MacIntel or the
  // Macintosh user-agent fallback.
  return [userAgentDataPlatform, platform, userAgent]
    .some((value) => /mac/i.test(String(value ?? "")));
}
