import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { htmlForBuildPlatform } from "./platform-html.mjs";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const src = join(root, "src");
const dist = join(root, "dist");

await rm(dist, { recursive: true, force: true });
await mkdir(dist, { recursive: true });
await cp(src, dist, { recursive: true });
const indexPath = join(dist, "index.html");
const indexHtml = await readFile(indexPath, "utf8");
await writeFile(indexPath, htmlForBuildPlatform(indexHtml), "utf8");

console.log(`Built frontend into ${dist}`);
