import assert from "node:assert/strict";

import { xmlRpcDecodeResponse, xmlRpcEncodeCall } from "../src/plugin-xmlrpc.js";

const request = xmlRpcEncodeCall("fixture.echo<&", [
  true,
  7,
  1.25,
  "A&B<ok>",
  ["nested", false],
  { name: "IINA", bytes: Uint8Array.from([0, 1, 254, 255]) },
]);
assert.ok(request.includes("<methodName>fixture.echo&lt;&amp;</methodName>"));
assert.ok(request.includes("<boolean>1</boolean>"));
assert.ok(request.includes("<int>7</int>"));
assert.ok(request.includes("<double>1.25</double>"));
assert.ok(request.includes("<string>A&amp;B&lt;ok&gt;</string>"));
assert.ok(request.includes("<base64>AAH+/w==</base64>"));

const decoded = xmlRpcDecodeResponse(`<?xml version="1.0"?>
  <methodResponse><params><param><value><struct>
    <member><name>ok</name><value><boolean>1</boolean></value></member>
    <member><name>items</name><value><array><data>
      <value><string>A&amp;B</string></value>
      <value><i4>-3</i4></value>
      <value><base64>AAH+/w==</base64></value>
    </data></array></value></member>
  </struct></value></param></params></methodResponse>`);
assert.equal(decoded.fault, false);
assert.equal(decoded.value.ok, true);
assert.equal(decoded.value.items[0], "A&B");
assert.equal(decoded.value.items[1], -3);
assert.deepEqual(Array.from(decoded.value.items[2]), [0, 1, 254, 255]);

assert.deepEqual(
  xmlRpcDecodeResponse("<methodResponse><fault><value><string>denied</string></value></fault></methodResponse>"),
  { fault: true, value: undefined },
);

assert.throws(
  () => xmlRpcDecodeResponse('<!DOCTYPE methodResponse [<!ENTITY x SYSTEM "file:///etc/passwd">]><methodResponse/>'),
  /DTDs and entities/,
);
assert.throws(
  () => xmlRpcDecodeResponse("<methodResponse><params></methodResponse>"),
  /Mismatched|Incomplete/,
);
assert.throws(() => xmlRpcEncodeCall("fixture", [Number.POSITIVE_INFINITY]), /finite/);
assert.throws(() => xmlRpcEncodeCall("fixture", [Number.MAX_SAFE_INTEGER + 1]), /precision/);
assert.throws(
  () => xmlRpcDecodeResponse(
    "<methodResponse><params><param><value><dateTime.iso8601>20261301T00:00:00</dateTime.iso8601></value></param></params></methodResponse>",
  ),
  /dateTime/,
);
const cyclic = {};
cyclic.self = cyclic;
assert.throws(() => xmlRpcEncodeCall("fixture", [cyclic]), /cycles/);

console.log("Plugin XML-RPC compatibility checks passed");
