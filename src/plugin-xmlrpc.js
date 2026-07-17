const MAX_XML_RPC_BYTES = 2 * 1024 * 1024;
const MAX_XML_RPC_DEPTH = 32;
const MAX_XML_RPC_NODES = 10_000;

export function xmlRpcEncodeCall(method, args = []) {
  const methodName = String(method ?? "");
  if (!methodName || methodName.length > 512 || methodName.includes("\0")) {
    throw new Error("XML-RPC method is invalid");
  }
  const values = Array.from(args || []);
  if (values.length > 1_000) throw new Error("XML-RPC argument count exceeds 1000");
  const seen = new WeakSet();
  const params = values
    .map((value) => `<param>${xmlRpcEncodeValue(value, 0, seen)}</param>`)
    .join("");
  const xml = `<?xml version="1.0" encoding="UTF-8"?><methodCall><methodName>${escapeXml(methodName)}</methodName><params>${params}</params></methodCall>`;
  if (new TextEncoder().encode(xml).length > MAX_XML_RPC_BYTES) {
    throw new Error("XML-RPC request exceeds 2 MiB");
  }
  return xml;
}

function xmlRpcEncodeValue(value, depth, seen) {
  if (depth > MAX_XML_RPC_DEPTH) throw new Error("XML-RPC value nesting exceeds 32 levels");
  if (typeof value === "boolean") return `<value><boolean>${value ? 1 : 0}</boolean></value>`;
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new Error("XML-RPC numbers must be finite");
    if (Number.isInteger(value)) {
      if (!Number.isSafeInteger(value)) {
        throw new Error("XML-RPC integer exceeds JavaScript precision");
      }
      return `<value><int>${value}</int></value>`;
    }
    return `<value><double>${value}</double></value>`;
  }
  if (typeof value === "string") return `<value><string>${escapeXml(value)}</string></value>`;
  if (value instanceof Date) {
    if (!Number.isFinite(value.getTime())) throw new Error("XML-RPC Date is invalid");
    return `<value><dateTime.iso8601>${formatXmlRpcDate(value)}</dateTime.iso8601></value>`;
  }
  if (value instanceof Uint8Array || value instanceof ArrayBuffer) {
    const bytes = value instanceof Uint8Array ? value : new Uint8Array(value);
    return `<value><base64>${encodeBase64(bytes)}</base64></value>`;
  }
  if (value && typeof value === "object") {
    if (seen.has(value)) throw new Error("XML-RPC values cannot contain cycles");
    seen.add(value);
    try {
      if (Array.isArray(value)) {
        return `<value><array><data>${value
          .map((item) => xmlRpcEncodeValue(item, depth + 1, seen))
          .join("")}</data></array></value>`;
      }
      const members = Object.entries(value)
        .map(([key, item]) => `<member><name>${escapeXml(key)}</name>${xmlRpcEncodeValue(item, depth + 1, seen)}</member>`)
        .join("");
      return `<value><struct>${members}</struct></value>`;
    } finally {
      seen.delete(value);
    }
  }
  // IINA 1.3.5 emits an empty <value> for unsupported bridge values.
  return "<value></value>";
}

export function xmlRpcDecodeResponse(xml) {
  const raw = String(xml ?? "");
  if (new TextEncoder().encode(raw).length > MAX_XML_RPC_BYTES) {
    throw new Error("XML-RPC response exceeds 2 MiB");
  }
  const root = parseXml(raw);
  if (root.name !== "methodResponse") throw new Error("Bad XML-RPC response");
  if (child(root, "fault")) return { fault: true, value: undefined };
  const params = child(root, "params");
  const param = params && children(params, "param");
  if (!param || param.length !== 1) throw new Error("Bad XML-RPC response");
  const value = child(param[0], "value");
  if (!value) throw new Error("Bad XML-RPC response");
  return { fault: false, value: decodeValue(value, 0) };
}

function decodeValue(valueNode, depth) {
  if (depth > MAX_XML_RPC_DEPTH) throw new Error("XML-RPC value nesting exceeds 32 levels");
  const type = valueNode.children[0];
  if (!type) return valueNode.text;
  const text = textContent(type);
  switch (type.name) {
    case "boolean": {
      const normalized = text.trim().toLowerCase();
      if (normalized === "1" || normalized === "true") return true;
      if (normalized === "0" || normalized === "false") return false;
      throw new Error("Invalid XML-RPC boolean");
    }
    case "int":
    case "i4": {
      const normalized = text.trim();
      if (!/^[+-]?\d+$/.test(normalized)) throw new Error("Invalid XML-RPC integer");
      const result = Number(normalized);
      if (!Number.isSafeInteger(result)) throw new Error("XML-RPC integer exceeds JavaScript precision");
      return result;
    }
    case "double": {
      const result = Number(text.trim());
      if (!Number.isFinite(result)) throw new Error("Invalid XML-RPC double");
      return result;
    }
    case "string":
      return text;
    case "dateTime.iso8601":
      return parseXmlRpcDate(text.trim());
    case "base64":
      return decodeBase64(text);
    case "array": {
      const data = child(type, "data");
      return data ? children(data, "value").map((item) => decodeValue(item, depth + 1)) : [];
    }
    case "struct": {
      const result = {};
      for (const member of children(type, "member")) {
        const name = child(member, "name");
        const value = child(member, "value");
        if (!name || !value) throw new Error("Invalid XML-RPC struct member");
        Object.defineProperty(result, textContent(name), {
          value: decodeValue(value, depth + 1),
          enumerable: true,
          configurable: true,
          writable: true,
        });
      }
      return result;
    }
    default:
      throw new Error(`Unsupported XML-RPC value type: ${type.name}`);
  }
}

function parseXml(raw) {
  if (/<!DOCTYPE|<!ENTITY/i.test(raw)) throw new Error("XML-RPC DTDs and entities are not allowed");
  let index = 0;
  let nodes = 0;
  const roots = [];
  const stack = [];
  const appendText = (value, decode = true) => {
    if (!stack.length) {
      if (value.trim()) throw new Error("Unexpected XML-RPC text outside the root element");
      return;
    }
    stack[stack.length - 1].text += decode ? decodeXmlEntities(value) : value;
  };

  while (index < raw.length) {
    if (raw[index] !== "<") {
      const next = raw.indexOf("<", index);
      const end = next < 0 ? raw.length : next;
      appendText(raw.slice(index, end));
      index = end;
      continue;
    }
    if (raw.startsWith("<?", index)) {
      const end = raw.indexOf("?>", index + 2);
      if (end < 0) throw new Error("Unterminated XML-RPC processing instruction");
      index = end + 2;
      continue;
    }
    if (raw.startsWith("<!--", index)) {
      const end = raw.indexOf("-->", index + 4);
      if (end < 0) throw new Error("Unterminated XML-RPC comment");
      index = end + 3;
      continue;
    }
    if (raw.startsWith("<![CDATA[", index)) {
      const end = raw.indexOf("]]>", index + 9);
      if (end < 0) throw new Error("Unterminated XML-RPC CDATA");
      appendText(raw.slice(index + 9, end), false);
      index = end + 3;
      continue;
    }
    const close = raw.slice(index).match(/^<\/([A-Za-z_][A-Za-z0-9_.:-]*)\s*>/);
    if (close) {
      const node = stack.pop();
      if (!node || node.name !== close[1]) throw new Error("Mismatched XML-RPC closing element");
      index += close[0].length;
      continue;
    }
    const selfClosing = raw.slice(index).match(/^<([A-Za-z_][A-Za-z0-9_.:-]*)\s*\/>/);
    const opening = selfClosing || raw.slice(index).match(/^<([A-Za-z_][A-Za-z0-9_.:-]*)\s*>/);
    if (!opening) throw new Error("Invalid XML-RPC element");
    if (++nodes > MAX_XML_RPC_NODES) throw new Error("XML-RPC response has too many elements");
    if (stack.length >= MAX_XML_RPC_DEPTH + 8) throw new Error("XML-RPC XML nesting exceeds the limit");
    const node = { name: opening[1], text: "", children: [] };
    if (stack.length) stack[stack.length - 1].children.push(node);
    else roots.push(node);
    index += opening[0].length;
    if (!selfClosing) stack.push(node);
  }
  if (stack.length || roots.length !== 1) throw new Error("Incomplete XML-RPC response");
  return roots[0];
}

function child(node, name) {
  return node.children.find((entry) => entry.name === name) || null;
}

function children(node, name) {
  return node.children.filter((entry) => entry.name === name);
}

function textContent(node) {
  return node.text + node.children.map(textContent).join("");
}

function escapeXml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
}

function decodeXmlEntities(value) {
  return value.replace(/&(#x[0-9a-f]+|#\d+|amp|lt|gt|quot|apos);/gi, (match, entity) => {
    const named = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" };
    const lower = entity.toLowerCase();
    if (named[lower]) return named[lower];
    const codePoint = lower.startsWith("#x")
      ? Number.parseInt(lower.slice(2), 16)
      : Number.parseInt(lower.slice(1), 10);
    if (!Number.isInteger(codePoint) || codePoint < 0 || codePoint > 0x10ffff || (codePoint >= 0xd800 && codePoint <= 0xdfff)) {
      throw new Error("Invalid XML-RPC character entity");
    }
    return String.fromCodePoint(codePoint);
  }).replace(/&[^;\s<]+;/g, () => {
    throw new Error("Unsupported XML-RPC entity");
  });
}

function formatXmlRpcDate(date) {
  const pad = (value) => String(value).padStart(2, "0");
  return `${date.getFullYear()}${pad(date.getMonth() + 1)}${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

function parseXmlRpcDate(value) {
  const match = value.match(/^(\d{4})(\d{2})(\d{2})T(\d{2}):(\d{2}):(\d{2})$/);
  if (!match) throw new Error("Invalid XML-RPC dateTime.iso8601");
  const date = new Date(
    Number(match[1]),
    Number(match[2]) - 1,
    Number(match[3]),
    Number(match[4]),
    Number(match[5]),
    Number(match[6]),
  );
  if (
    !Number.isFinite(date.getTime())
    || date.getFullYear() !== Number(match[1])
    || date.getMonth() !== Number(match[2]) - 1
    || date.getDate() !== Number(match[3])
    || date.getHours() !== Number(match[4])
    || date.getMinutes() !== Number(match[5])
    || date.getSeconds() !== Number(match[6])
  ) {
    throw new Error("Invalid XML-RPC dateTime.iso8601");
  }
  return date;
}

function encodeBase64(bytes) {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let result = "";
  for (let index = 0; index < bytes.length; index += 3) {
    const a = bytes[index];
    const b = bytes[index + 1];
    const c = bytes[index + 2];
    result += alphabet[a >> 2];
    result += alphabet[((a & 3) << 4) | ((b ?? 0) >> 4)];
    result += b == null ? "=" : alphabet[((b & 15) << 2) | ((c ?? 0) >> 6)];
    result += c == null ? "=" : alphabet[c & 63];
  }
  return result;
}

function decodeBase64(value) {
  const normalized = value.replace(/\s/g, "");
  if (!/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(normalized)) {
    throw new Error("Invalid XML-RPC base64");
  }
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  const bytes = [];
  for (let index = 0; index < normalized.length; index += 4) {
    const a = alphabet.indexOf(normalized[index]);
    const b = alphabet.indexOf(normalized[index + 1]);
    const c = normalized[index + 2] === "=" ? 0 : alphabet.indexOf(normalized[index + 2]);
    const d = normalized[index + 3] === "=" ? 0 : alphabet.indexOf(normalized[index + 3]);
    bytes.push((a << 2) | (b >> 4));
    if (normalized[index + 2] !== "=") bytes.push(((b & 15) << 4) | (c >> 2));
    if (normalized[index + 3] !== "=") bytes.push(((c & 3) << 6) | d);
  }
  return Uint8Array.from(bytes);
}
