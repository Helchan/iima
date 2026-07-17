import { readFile, writeFile } from "node:fs/promises";

const ENTRY_PATTERN = /^"([a-z]{2,3})"\s*=\s*"([^"]*)";\s*$/gmu;

export function parseIinaIso639Strings(source) {
  const languages = [];
  for (const match of String(source || "").matchAll(ENTRY_PATTERN)) {
    languages.push({
      code: match[1],
      names: match[2].split(";").map((name) => name.trim()).filter(Boolean),
    });
  }
  languages.sort((left, right) => (
    `${left.names[0]} (${left.code})`.localeCompare(`${right.names[0]} (${right.code})`, "en")
  ));
  return languages;
}

export function serializeIinaIso639Strings(source) {
  return `${JSON.stringify(parseIinaIso639Strings(source))}\n`;
}

export async function generateIso639Catalog(sourcePath, outputPath) {
  const source = await readFile(sourcePath, "utf8");
  const serialized = serializeIinaIso639Strings(source);
  await writeFile(outputPath, serialized);
  return serialized;
}
