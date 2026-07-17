const PLUGIN_SYNC_MAX_PROTOCOL_BYTES = 64 * 1024 * 1024;
const PLUGIN_SYNC_TOKEN_PATTERN = /^[0-9a-f]{64}$/u;
const PLUGIN_SYNC_ENDPOINT_PATTERN = /^iima-plugin-sync:\/\/localhost\/invoke$/u;

function encodedByteLength(value) {
  if (typeof TextEncoder === "function") return new TextEncoder().encode(value).byteLength;
  // WebKit versions supported by this application expose TextEncoder. The
  // fallback keeps the invariant executable in isolated Node tests as well.
  return unescape(encodeURIComponent(value)).length;
}

function transportError(message, cause = undefined) {
  const error = new Error(String(message || "Plugin synchronization failed"));
  error.name = "PluginSyncError";
  if (cause !== undefined) error.cause = cause;
  return error;
}

function validateGrantDescriptor(descriptor, expectedRole) {
  if (!descriptor || typeof descriptor !== "object") {
    throw transportError("Plugin synchronization authorization is unavailable");
  }
  const token = String(descriptor.token ?? "");
  const endpoint = String(descriptor.endpoint ?? "");
  const role = String(descriptor.role ?? "");
  if (
    !PLUGIN_SYNC_TOKEN_PATTERN.test(token)
    || !PLUGIN_SYNC_ENDPOINT_PATTERN.test(endpoint)
    || role !== expectedRole
  ) {
    throw transportError("Plugin synchronization authorization is invalid");
  }
  return { token, endpoint, role };
}

function decodeResponse(xhr) {
  const responseText = String(xhr.responseText ?? "");
  if (encodedByteLength(responseText) > PLUGIN_SYNC_MAX_PROTOCOL_BYTES) {
    throw transportError("Plugin synchronization response exceeds 64 MiB");
  }
  let envelope;
  try {
    envelope = JSON.parse(responseText);
  } catch (cause) {
    throw transportError("Plugin synchronization returned invalid JSON", cause);
  }
  if (!envelope || typeof envelope !== "object" || Array.isArray(envelope)) {
    throw transportError("Plugin synchronization returned an invalid response");
  }
  if (xhr.status < 200 || xhr.status >= 300 || envelope.ok !== true) {
    throw transportError(
      typeof envelope.error === "string" && envelope.error
        ? envelope.error
        : `Plugin synchronization failed with HTTP ${xhr.status || 0}`,
    );
  }
  if (!("value" in envelope)) {
    throw transportError("Plugin synchronization response has no value");
  }
  return envelope.value;
}

/**
 * Create the parent-realm half of IINA's synchronous plugin API bridge.
 *
 * The opaque grant and XMLHttpRequest constructor stay in this closure. Only
 * the invoke function is captured by individual capability methods; neither
 * the descriptor nor the transport object is passed into a plugin realm.
 */
export async function createPluginSyncTransport({
  identifier,
  role,
  invoke,
  XMLHttpRequestClass = globalThis.XMLHttpRequest,
}) {
  if (typeof identifier !== "string" || !identifier) {
    throw transportError("Plugin synchronization requires an identifier");
  }
  if (typeof invoke !== "function" || typeof XMLHttpRequestClass !== "function") {
    throw transportError("Plugin synchronization transport is unavailable");
  }
  if (!["entry", "global"].includes(role)) {
    throw transportError("Plugin synchronization requires an entry/global role");
  }
  const descriptor = validateGrantDescriptor(
    await invoke("plugin_sync_prepare_grant", { identifier, role }),
    role,
  );
  let active = true;

  const invokeSync = (method, args = {}) => {
    if (!active) throw transportError("Plugin synchronization authorization has been revoked");
    if (typeof method !== "string" || !/^[a-z.]{1,96}$/u.test(method)) {
      throw transportError("Plugin synchronization method is invalid");
    }
    let requestText;
    try {
      requestText = JSON.stringify({
        grant: descriptor.token,
        method,
        args: args ?? {},
      });
    } catch (cause) {
      throw transportError("Plugin synchronization arguments are not serializable", cause);
    }
    if (typeof requestText !== "string" || encodedByteLength(requestText) > PLUGIN_SYNC_MAX_PROTOCOL_BYTES) {
      throw transportError("Plugin synchronization request exceeds 64 MiB");
    }

    const xhr = new XMLHttpRequestClass();
    try {
      xhr.open("POST", descriptor.endpoint, false);
      // Keep the native envelope explicit. WebKit may send this custom-scheme
      // request directly even though application/json normally triggers CORS
      // preflight; Rust still validates a narrow, grant-free OPTIONS request
      // on runtimes that do emit one, and validates Origin on every POST.
      xhr.setRequestHeader("Content-Type", "application/json");
      xhr.send(requestText);
    } catch (cause) {
      throw transportError("Plugin synchronization request could not be completed", cause);
    }
    return decodeResponse(xhr);
  };

  const revoke = async () => {
    if (!active) return;
    active = false;
    await invoke("plugin_sync_revoke_grant", { identifier, grant: descriptor.token });
  };

  return Object.freeze({ invokeSync, revoke });
}

export function withPluginFileSystemPermission(hasPermission, unavailableValue, operation) {
  if (typeof hasPermission !== "function" || !hasPermission("file-system")) {
    return unavailableValue;
  }
  return operation();
}

export const pluginFileSystemPermissionError =
  'To call this API, the plugin must declare permission "file-system" in its Info.json.';

/**
 * Reproduce JavascriptAPI.parsePath's synchronous permission/local-path gate.
 *
 * The global-entry controller has no PlayerCore in IINA. Consequently track
 * and @current magic paths are ordinary relative strings there, even though
 * this port's controller and child realms share one native player window.
 */
export function pluginPathForApi(path, {
  hasFileSystemPermission = false,
  playerAvailable = true,
  forceLocalPath = true,
} = {}) {
  const raw = String(path);
  const privatePath = raw.startsWith("@tmp/") || raw.startsWith("@data/");
  const playerTrackPath = playerAvailable && (
    raw.startsWith("@video/")
    || raw.startsWith("@audio/")
    || raw.startsWith("@sub")
  );

  if (!privatePath && !playerTrackPath && !hasFileSystemPermission) {
    throw new Error(pluginFileSystemPermissionError);
  }

  if (forceLocalPath && !privatePath && !playerTrackPath) {
    const currentPath = playerAvailable && raw.startsWith("@current/");
    const homePath = raw.startsWith("~/");
    if (!raw.startsWith("/") && !currentPath && !homePath) {
      throw new Error(`The path should be an absolute path: ${raw}`);
    }
  }

  return raw;
}

export function pluginFileHandleReadValue(bytes, createUint8Array = (value) => Uint8Array.from(value)) {
  return bytes == null ? null : createUint8Array(bytes);
}

export function pluginWebSocketPort(value) {
  const port = Number(value);
  if (!Number.isInteger(port) || port < 1 || port > 65_535) {
    throw new Error("ws.createServer: port not specified");
  }
  return port;
}

export function pluginWebSocketHandlerValue(current, handler) {
  if (handler == null || current !== null) return null;
  return ["object", "function"].includes(typeof handler) ? handler : null;
}

export const pluginSyncProtocolLimit = PLUGIN_SYNC_MAX_PROTOCOL_BYTES;
