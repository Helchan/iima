import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { validateDownloadUrlPrefix } from "./sparkle-channel.mjs";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const defaultTool = resolve(
  root,
  "参考/iina/build/DerivedData/SourcePackages/artifacts/sparkle/Sparkle/bin/generate_appcast",
);

export function parseAppcastArguments(argv, environment = process.env) {
  const options = {
    archiveDirectory: null,
    downloadUrlPrefix: environment.IIMA_SPARKLE_DOWNLOAD_URL_PREFIX?.trim() || null,
    output: null,
    channel: null,
    account: environment.IIMA_SPARKLE_ACCOUNT?.trim() || null,
    edKeyFile: environment.IIMA_SPARKLE_ED_KEY_FILE?.trim() || null,
    privateKey: environment.IIMA_SPARKLE_PRIVATE_KEY?.trim() || null,
    dryRun: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    const take = (name) => {
      const value = argv[index + 1];
      if (!value || value.startsWith("--")) throw new Error(`${name} requires a value`);
      index += 1;
      return value;
    };
    if (argument === "--download-url-prefix") options.downloadUrlPrefix = take(argument);
    else if (argument === "--output") options.output = take(argument);
    else if (argument === "--channel") options.channel = take(argument);
    else if (argument === "--account") options.account = take(argument);
    else if (argument === "--ed-key-file") options.edKeyFile = take(argument);
    else if (argument === "--dry-run") options.dryRun = true;
    else if (argument.startsWith("--")) throw new Error(`Unknown option: ${argument}`);
    else if (options.archiveDirectory) throw new Error("Only one archive directory is allowed");
    else options.archiveDirectory = argument;
  }

  if (!options.archiveDirectory) throw new Error("An archive directory is required");
  if (!options.downloadUrlPrefix) throw new Error("A download URL prefix is required");
  const keySources = [options.account, options.edKeyFile, options.privateKey].filter(Boolean);
  if (keySources.length !== 1) {
    throw new Error(
      "Choose exactly one signing source: --account, --ed-key-file, or IIMA_SPARKLE_PRIVATE_KEY",
    );
  }

  options.archiveDirectory = resolve(options.archiveDirectory);
  options.downloadUrlPrefix = validateDownloadUrlPrefix(options.downloadUrlPrefix);
  if (options.edKeyFile) {
    if (options.edKeyFile === "-") {
      throw new Error(
        "Use IIMA_SPARKLE_PRIVATE_KEY for stdin signing; --ed-key-file must name a file",
      );
    }
    options.edKeyFile = resolve(options.edKeyFile);
  }
  if (options.output) options.output = resolve(options.output);
  return options;
}

export function buildGenerateAppcastInvocation(options, environment = process.env) {
  const tool = resolve(environment.IIMA_GENERATE_APPCAST || defaultTool);
  const args = ["--download-url-prefix", options.downloadUrlPrefix];
  let stdin = null;
  if (options.account) args.push("--account", options.account);
  if (options.edKeyFile) args.push("--ed-key-file", options.edKeyFile);
  if (options.privateKey) {
    args.push("--ed-key-file", "-");
    stdin = `${options.privateKey.trim()}\n`;
  }
  if (options.output) args.push("-o", options.output);
  if (options.channel) args.push("--channel", options.channel);
  args.push(options.archiveDirectory);
  return { tool, args, stdin };
}

export function sanitizedAppcastEnvironment(environment = process.env) {
  const childEnvironment = { ...environment };
  // Once copied to stdin the private key must not remain available through the
  // child process environment (or diagnostics that dump that environment).
  delete childEnvironment.IIMA_SPARKLE_PRIVATE_KEY;
  return childEnvironment;
}

function main() {
  const options = parseAppcastArguments(process.argv.slice(2));
  const invocation = buildGenerateAppcastInvocation(options);
  if (!existsSync(invocation.tool)) throw new Error(`Sparkle generate_appcast not found: ${invocation.tool}`);
  if (!existsSync(options.archiveDirectory)) throw new Error(`Archive directory not found: ${options.archiveDirectory}`);
  if (options.edKeyFile && !existsSync(options.edKeyFile)) throw new Error(`Ed25519 key file not found: ${options.edKeyFile}`);

  console.log(`$ ${[invocation.tool, ...invocation.args].join(" ")}`);
  if (options.dryRun) return;
  const result = spawnSync(invocation.tool, invocation.args, {
    cwd: root,
    env: sanitizedAppcastEnvironment(process.env),
    input: invocation.stdin || undefined,
    stdio: invocation.stdin ? ["pipe", "inherit", "inherit"] : "inherit",
  });
  if (result.status !== 0) throw new Error(`generate_appcast exited with ${result.status}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
