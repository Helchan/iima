function numberPayload(value) {
  if (Number.isNaN(value)) return "NaN";
  if (value === Number.POSITIVE_INFINITY) return "Infinity";
  if (value === Number.NEGATIVE_INFINITY) return "-Infinity";
  if (Object.is(value, -0)) return "-0";
  return String(value);
}

export function encodePluginMpvValue(value, seen = new Set(), depth = 0) {
  if (depth > 32) throw new TypeError("mpv.set object exceeds the nesting limit");
  if (value === null) return { type: "null" };
  if (typeof value === "boolean") return { type: "flag", value };
  if (typeof value === "number") return { type: "double", value: numberPayload(value) };
  if (typeof value === "string") return { type: "string", value };
  if (typeof value !== "object") {
    throw new TypeError("mpv.set only supports numbers, strings, booleans and objects.");
  }
  if (seen.has(value)) throw new TypeError("mpv.set cannot encode a cyclic object");
  seen.add(value);
  try {
    if (Array.isArray(value)) {
      return {
        type: "array",
        value: value.map((item) => encodePluginMpvValue(item, seen, depth + 1)),
      };
    }
    const encoded = {};
    for (const [key, item] of Object.entries(value)) {
      encoded[key] = encodePluginMpvValue(item, seen, depth + 1);
    }
    return { type: "map", value: encoded };
  } finally {
    seen.delete(value);
  }
}

export function decodePluginMpvValue(encoded) {
  if (!encoded || typeof encoded !== "object") return null;
  switch (encoded.type) {
    case "null": return null;
    case "flag": return Boolean(encoded.value);
    case "int64":
    case "double": return Number(encoded.value);
    case "string": return String(encoded.value ?? "");
    case "array": return Array.from(encoded.value || [], decodePluginMpvValue);
    case "map": return Object.fromEntries(
      Object.entries(encoded.value || {}).map(([key, value]) => [key, decodePluginMpvValue(value)])
    );
    case "byte-array": return Array.from(encoded.value || [], (byte) => Number(byte) & 0xff);
    default: return null;
  }
}
