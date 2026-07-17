import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const checkOnly = process.argv.includes("--check");
const referencePath = new URL("../参考/iina/iina/StringEncodingName.swift", import.meta.url);
const outputPath = new URL("../src-tauri/src/native_text_encoding.m", import.meta.url);
const sdk = execFileSync("/usr/bin/xcrun", ["--sdk", "macosx", "--show-sdk-path"], {
  encoding: "utf8",
}).trim();
const headerPath = `${sdk}/System/Library/Frameworks/CoreFoundation.framework/Headers/CFStringEncodingExt.h`;
const reference = readFileSync(referencePath, "utf8");
const header = readFileSync(headerPath, "utf8");

const external = Array.from(
  reference.matchAll(/"([^"]+)":\s*CFStringEncodings\.([A-Za-z0-9_]+)\.rawValue/gu),
  ([, name, swiftCase]) => ({ name, swiftCase }),
);
const foundation = Array.from(
  reference.matchAll(/"([^"]+)":\s*String\.Encoding\.([A-Za-z0-9_]+)/gu),
  ([, name, swiftCase]) => ({ name, swiftCase }),
);
const foundationConstants = new Map(Object.entries({
  ascii: "NSASCIIStringEncoding",
  iso2022JP: "NSISO2022JPStringEncoding",
  isoLatin1: "NSISOLatin1StringEncoding",
  isoLatin2: "NSISOLatin2StringEncoding",
  japaneseEUC: "NSJapaneseEUCStringEncoding",
  macOSRoman: "NSMacOSRomanStringEncoding",
  nextstep: "NSNEXTSTEPStringEncoding",
  nonLossyASCII: "NSNonLossyASCIIStringEncoding",
  shiftJIS: "NSShiftJISStringEncoding",
  symbol: "NSSymbolStringEncoding",
  unicode: "NSUnicodeStringEncoding",
  utf16: "NSUTF16StringEncoding",
  utf16BigEndian: "NSUTF16BigEndianStringEncoding",
  utf16LittleEndian: "NSUTF16LittleEndianStringEncoding",
  utf32: "NSUTF32StringEncoding",
  utf32BigEndian: "NSUTF32BigEndianStringEncoding",
  utf32LittleEndian: "NSUTF32LittleEndianStringEncoding",
  utf8: "NSUTF8StringEncoding",
  windowsCP1250: "NSWindowsCP1250StringEncoding",
  windowsCP1251: "NSWindowsCP1251StringEncoding",
  windowsCP1252: "NSWindowsCP1252StringEncoding",
  windowsCP1253: "NSWindowsCP1253StringEncoding",
  windowsCP1254: "NSWindowsCP1254StringEncoding",
}));
const foundationRows = foundation.map(({ name, swiftCase }) => {
  const constant = foundationConstants.get(swiftCase);
  if (!constant) throw new Error(`Missing Foundation encoding constant for ${swiftCase}`);
  return `  IIMA_NS_ENCODING("${name}", ${constant})`;
});
const constants = new Map(
  Array.from(
    header.matchAll(/kCFStringEncoding([A-Za-z0-9_]+)[^=\n]*=\s*(?:0x[0-9A-Fa-f]+|[0-9]+)/gu),
    ([, suffix]) => [suffix.toLocaleLowerCase(), suffix],
  ),
);
const rows = external.map(({ name, swiftCase }) => {
  const suffix = constants.get(swiftCase.toLocaleLowerCase());
  if (!suffix) throw new Error(`Missing CoreFoundation encoding constant for ${swiftCase}`);
  return `  { "${name}", kCFStringEncoding${suffix} },`;
});

const generated = `// Generated from IINA 1.3.5 StringEncodingName.swift by
// scripts/generate-native-text-encoding.mjs. Do not hand edit the mapping.
#import <Foundation/Foundation.h>
#import <CoreFoundation/CFStringEncodingExt.h>
#import <stdlib.h>
#import <string.h>

typedef struct {
  const char *name;
  CFStringEncoding encoding;
} IIMAExternalEncoding;

static const IIMAExternalEncoding iima_external_encodings[] = {
${rows.join("\n")}
};

static BOOL iima_encoding_for_name(const char *name, NSStringEncoding *encoding) {
  if (name == NULL || encoding == NULL) return NO;
#define IIMA_NS_ENCODING(key, value) if (strcmp(name, key) == 0) { *encoding = value; return YES; }
${foundationRows.join("\n")}
#undef IIMA_NS_ENCODING
  size_t count = sizeof(iima_external_encodings) / sizeof(iima_external_encodings[0]);
  for (size_t index = 0; index < count; index += 1) {
    if (strcmp(name, iima_external_encodings[index].name) == 0) {
      *encoding = CFStringConvertEncodingToNSStringEncoding(iima_external_encodings[index].encoding);
      return *encoding != kCFStringEncodingInvalidId;
    }
  }
  return NO;
}

// 0 = success, 1 = unknown encoding, 2 = invalid byte sequence, 3 = allocation failure.
int iima_native_decode_text(const uint8_t *bytes,
                            size_t length,
                            const char *encoding_name,
                            uint8_t **output,
                            size_t *output_length) {
  if (output == NULL || output_length == NULL) return 3;
  *output = NULL;
  *output_length = 0;
  NSStringEncoding encoding = 0;
  if (!iima_encoding_for_name(encoding_name, &encoding)) return 1;
  NSData *data = [NSData dataWithBytes:bytes length:length];
  NSString *text = [[NSString alloc] initWithData:data encoding:encoding];
  if (text == nil) return 2;
  NSData *utf8 = [text dataUsingEncoding:NSUTF8StringEncoding allowLossyConversion:NO];
  if (utf8 == nil) return 2;
  if (utf8.length == 0) return 0;
  uint8_t *copy = malloc(utf8.length);
  if (copy == NULL) return 3;
  memcpy(copy, utf8.bytes, utf8.length);
  *output = copy;
  *output_length = utf8.length;
  return 0;
}

void iima_native_decode_text_free(uint8_t *output) {
  free(output);
}
`;

const outputFile = fileURLToPath(outputPath);
if (checkOnly) {
  const current = readFileSync(outputPath, "utf8");
  if (current !== generated) {
    throw new Error(`${outputFile} is stale; run node scripts/generate-native-text-encoding.mjs`);
  }
} else {
  writeFileSync(outputPath, generated);
  process.stdout.write(
    `Generated ${external.length} CoreFoundation and ${foundation.length} Foundation text encoding mappings in ${outputFile}\n`,
  );
}
