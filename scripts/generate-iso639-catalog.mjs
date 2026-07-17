import { mkdir, readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  generateIso639Catalog,
  serializeIinaIso639Strings,
} from "./iso639-catalog.mjs";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const sourcePath = join(root, "参考", "iina", "iina", "ISO639.strings");
const outputPath = join(root, "src", "assets", "iina", "iso639.json");
const check = process.argv.includes("--check");

let previous = null;
try {
  previous = await readFile(outputPath, "utf8");
} catch {
  // A missing generated catalog is reported as drift in --check mode.
}

if (check) {
  const expected = serializeIinaIso639Strings(await readFile(sourcePath, "utf8"));
  if (previous !== expected) {
    throw new Error("src/assets/iina/iso639.json is stale; run npm run iso639:generate");
  }
  console.log("ISO 639 catalog matches the IINA 1.3.5 reference");
} else {
  await mkdir(dirname(outputPath), { recursive: true });
  await generateIso639Catalog(sourcePath, outputPath);
  console.log(`Generated ${outputPath}`);
}
