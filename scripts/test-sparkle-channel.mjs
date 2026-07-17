import assert from "node:assert/strict";
import {
  REFERENCE_BETA_APPCAST_URL,
  REFERENCE_PUBLIC_ED_KEY,
  REFERENCE_STABLE_APPCAST_URL,
  resolveSparkleChannel,
} from "./sparkle-channel.mjs";
import {
  buildGenerateAppcastInvocation,
  parseAppcastArguments,
  sanitizedAppcastEnvironment,
} from "./generate-sparkle-appcast.mjs";

assert.deepEqual(resolveSparkleChannel({}), {
  mode: "reference",
  stableAppcastUrl: REFERENCE_STABLE_APPCAST_URL,
  betaAppcastUrl: REFERENCE_BETA_APPCAST_URL,
  publicEdKey: REFERENCE_PUBLIC_ED_KEY,
  dsaPublicKeyPath: null,
});

const owned = resolveSparkleChannel({
  IIMA_STABLE_APPCAST_URL: "https://updates.example.test/stable.xml",
  IIMA_BETA_APPCAST_URL: "https://updates.example.test/beta.xml",
  IIMA_SPARKLE_PUBLIC_ED_KEY: Buffer.alloc(32, 7).toString("base64"),
});
assert.equal(owned.mode, "owned");
assert.equal(owned.stableAppcastUrl, "https://updates.example.test/stable.xml");
assert.equal(owned.betaAppcastUrl, "https://updates.example.test/beta.xml");
assert.deepEqual(
  resolveSparkleChannel({
    IIMA_STABLE_APPCAST_URL: "https://updates.example.test/stable.xml",
    IIMA_SPARKLE_PUBLIC_ED_KEY: Buffer.alloc(32, 8).toString("base64"),
  }).betaAppcastUrl,
  "https://updates.example.test/stable.xml",
);
assert.throws(
  () => resolveSparkleChannel({ IIMA_STABLE_APPCAST_URL: "http://updates.example.test" }),
  /requires IIMA_STABLE_APPCAST_URL and IIMA_SPARKLE_PUBLIC_ED_KEY/,
);
assert.throws(
  () => resolveSparkleChannel({
    IIMA_STABLE_APPCAST_URL: "http://updates.example.test/appcast.xml",
    IIMA_SPARKLE_PUBLIC_ED_KEY: Buffer.alloc(32).toString("base64"),
  }),
  /valid HTTPS URL/,
);
for (const invalidUrl of [
  "https://user:password@updates.example.test/appcast.xml",
  "https://updates.example.test/appcast.xml#fragment",
]) {
  assert.throws(
    () => resolveSparkleChannel({
      IIMA_STABLE_APPCAST_URL: invalidUrl,
      IIMA_SPARKLE_PUBLIC_ED_KEY: Buffer.alloc(32).toString("base64"),
    }),
    /valid HTTPS URL/,
  );
}
assert.throws(
  () => resolveSparkleChannel({
    IIMA_STABLE_APPCAST_URL: REFERENCE_STABLE_APPCAST_URL,
    IIMA_SPARKLE_PUBLIC_ED_KEY: Buffer.alloc(32).toString("base64"),
  }),
  /must not reuse IINA's appcasts/,
);
assert.throws(
  () => resolveSparkleChannel({
    IIMA_STABLE_APPCAST_URL: "https://updates.example.test/appcast.xml",
    IIMA_SPARKLE_PUBLIC_ED_KEY: `${Buffer.alloc(32).toString("base64")}=`,
  }),
  /base64 encoded/,
);

const options = parseAppcastArguments(
  ["--download-url-prefix", "https://downloads.example.test/releases", "/tmp/releases"],
  { IIMA_SPARKLE_PRIVATE_KEY: "private-key-material" },
);
const invocation = buildGenerateAppcastInvocation(options, {
  IIMA_GENERATE_APPCAST: "/tmp/generate_appcast",
});
assert.deepEqual(invocation.args, [
  "--download-url-prefix",
  "https://downloads.example.test/releases/",
  "--ed-key-file",
  "-",
  "/tmp/releases",
]);
assert.equal(invocation.stdin, "private-key-material\n");
assert.ok(!invocation.args.join(" ").includes("private-key-material"));
const accountInvocation = buildGenerateAppcastInvocation(parseAppcastArguments([
  "--download-url-prefix",
  "https://downloads.example.test/releases/",
  "--account",
  "release-signing",
  "--output",
  "/tmp/appcast.xml",
  "--channel",
  "beta",
  "/tmp/releases",
]), { IIMA_GENERATE_APPCAST: "/tmp/generate_appcast" });
assert.deepEqual(accountInvocation.args, [
  "--download-url-prefix",
  "https://downloads.example.test/releases/",
  "--account",
  "release-signing",
  "-o",
  "/tmp/appcast.xml",
  "--channel",
  "beta",
  "/tmp/releases",
]);
assert.equal(accountInvocation.stdin, null);
const fileInvocation = buildGenerateAppcastInvocation(parseAppcastArguments([
  "--download-url-prefix",
  "https://downloads.example.test/releases/",
  "--ed-key-file",
  "/tmp/sparkle-private-key",
  "/tmp/releases",
]), { IIMA_GENERATE_APPCAST: "/tmp/generate_appcast" });
assert.deepEqual(fileInvocation.args, [
  "--download-url-prefix",
  "https://downloads.example.test/releases/",
  "--ed-key-file",
  "/tmp/sparkle-private-key",
  "/tmp/releases",
]);
assert.equal(fileInvocation.stdin, null);
const childEnvironment = sanitizedAppcastEnvironment({
  PATH: "/usr/bin",
  IIMA_SPARKLE_PRIVATE_KEY: "private-key-material",
});
assert.deepEqual(childEnvironment, { PATH: "/usr/bin" });
assert.throws(
  () => parseAppcastArguments(["/tmp/releases"], {}),
  /download URL prefix/,
);
assert.throws(
  () => parseAppcastArguments([
    "--download-url-prefix",
    "https://downloads.example.test/releases?token=secret",
    "/tmp/releases",
  ], { IIMA_SPARKLE_PRIVATE_KEY: "private-key-material" }),
  /must not contain a query string/,
);
assert.throws(
  () => parseAppcastArguments([
    "--download-url-prefix",
    "https://downloads.example.test/releases",
    "--ed-key-file",
    "-",
    "/tmp/releases",
  ]),
  /stdin signing/,
);
assert.throws(
  () => parseAppcastArguments([
    "--download-url-prefix",
    "https://downloads.example.test/releases",
    "--account",
    "release-signing",
    "/tmp/releases",
  ], { IIMA_SPARKLE_PRIVATE_KEY: "private-key-material" }),
  /Choose exactly one signing source/,
);

console.log("Sparkle channel and private-key-safe appcast generation checks passed");

await import("./test-native-updater-exceptions.mjs");
