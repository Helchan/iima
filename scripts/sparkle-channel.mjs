export const REFERENCE_STABLE_APPCAST_URL = "https://www.iina.io/appcast.xml";
export const REFERENCE_BETA_APPCAST_URL = "https://www.iina.io/appcast-beta.xml";
export const REFERENCE_PUBLIC_ED_KEY =
  "UpwCRYfYOg0OGgQHY6RUdrV29yPcdkvxGlEfq46r6a0=";

function requiredHttpsUrl(value, label) {
  let parsed;
  try {
    parsed = new URL(value);
  } catch {
    throw new Error(`${label} must be a valid HTTPS URL`);
  }
  if (
    parsed.protocol !== "https:" ||
    !parsed.hostname ||
    parsed.username ||
    parsed.password ||
    parsed.hash
  ) {
    throw new Error(`${label} must be a valid HTTPS URL`);
  }
  return parsed.href;
}

function requiredEd25519PublicKey(value) {
  const normalized = String(value || "").trim();
  // An Ed25519 public key is exactly 32 bytes. Requiring canonical padded
  // Base64 keeps Node's permissive decoder from silently accepting junk,
  // URL-safe variants, whitespace, or surplus padding in the signed plist.
  if (!/^[A-Za-z0-9+/]{43}=$/.test(normalized)) {
    throw new Error("IIMA_SPARKLE_PUBLIC_ED_KEY must be base64 encoded");
  }
  const decoded = Buffer.from(normalized, "base64");
  if (
    decoded.length !== 32 ||
    decoded.toString("base64") !== normalized
  ) {
    throw new Error("IIMA_SPARKLE_PUBLIC_ED_KEY must encode one 32-byte Ed25519 public key");
  }
  return normalized;
}

export function resolveSparkleChannel(environment = process.env) {
  const stableOverride = environment.IIMA_STABLE_APPCAST_URL?.trim();
  const betaOverride = environment.IIMA_BETA_APPCAST_URL?.trim();
  const keyOverride = environment.IIMA_SPARKLE_PUBLIC_ED_KEY?.trim();
  const dsaOverride = environment.IIMA_SPARKLE_DSA_PUBLIC_KEY?.trim();
  const owned = Boolean(stableOverride || betaOverride || keyOverride || dsaOverride);

  if (!owned) {
    return {
      mode: "reference",
      stableAppcastUrl: REFERENCE_STABLE_APPCAST_URL,
      betaAppcastUrl: REFERENCE_BETA_APPCAST_URL,
      publicEdKey: REFERENCE_PUBLIC_ED_KEY,
      dsaPublicKeyPath: null,
    };
  }
  if (!stableOverride || !keyOverride) {
    throw new Error(
      "A Tauri-owned update channel requires IIMA_STABLE_APPCAST_URL and IIMA_SPARKLE_PUBLIC_ED_KEY",
    );
  }

  const stableAppcastUrl = requiredHttpsUrl(
    stableOverride,
    "IIMA_STABLE_APPCAST_URL",
  );
  const betaAppcastUrl = betaOverride
    ? requiredHttpsUrl(betaOverride, "IIMA_BETA_APPCAST_URL")
    : stableAppcastUrl;
  const publicEdKey = requiredEd25519PublicKey(keyOverride);
  if (
    stableAppcastUrl === REFERENCE_STABLE_APPCAST_URL ||
    betaAppcastUrl === REFERENCE_BETA_APPCAST_URL ||
    publicEdKey === REFERENCE_PUBLIC_ED_KEY
  ) {
    throw new Error(
      "A Tauri-owned update channel must not reuse IINA's appcasts or Ed25519 public key",
    );
  }
  return {
    mode: "owned",
    stableAppcastUrl,
    betaAppcastUrl,
    publicEdKey,
    dsaPublicKeyPath: dsaOverride || null,
  };
}

export function validateDownloadUrlPrefix(value) {
  const normalized = requiredHttpsUrl(value, "download URL prefix");
  const url = new URL(normalized);
  if (url.search) {
    throw new Error("download URL prefix must not contain a query string");
  }
  if (!url.pathname.endsWith("/")) url.pathname += "/";
  return url.href;
}
