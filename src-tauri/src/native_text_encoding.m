// Generated from IINA 1.3.5 StringEncodingName.swift by
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
  { "ANSEL", kCFStringEncodingANSEL },
  { "big5", kCFStringEncodingBig5 },
  { "big5_E", kCFStringEncodingBig5_E },
  { "big5_HKSCS_1999", kCFStringEncodingBig5_HKSCS_1999 },
  { "CNS_11643_92_P1", kCFStringEncodingCNS_11643_92_P1 },
  { "CNS_11643_92_P2", kCFStringEncodingCNS_11643_92_P2 },
  { "CNS_11643_92_P3", kCFStringEncodingCNS_11643_92_P3 },
  { "dosArabic", kCFStringEncodingDOSArabic },
  { "dosBalticRim", kCFStringEncodingDOSBalticRim },
  { "dosCanadianFrench", kCFStringEncodingDOSCanadianFrench },
  { "dosChineseSimplif", kCFStringEncodingDOSChineseSimplif },
  { "dosChineseTrad", kCFStringEncodingDOSChineseTrad },
  { "dosCyrillic", kCFStringEncodingDOSCyrillic },
  { "dosGreek", kCFStringEncodingDOSGreek },
  { "dosGreek1", kCFStringEncodingDOSGreek1 },
  { "dosGreek2", kCFStringEncodingDOSGreek2 },
  { "dosHebrew", kCFStringEncodingDOSHebrew },
  { "dosIcelandic", kCFStringEncodingDOSIcelandic },
  { "dosJapanese", kCFStringEncodingDOSJapanese },
  { "dosKorean", kCFStringEncodingDOSKorean },
  { "dosLatin1", kCFStringEncodingDOSLatin1 },
  { "dosLatin2", kCFStringEncodingDOSLatin2 },
  { "dosLatinUS", kCFStringEncodingDOSLatinUS },
  { "dosNordic", kCFStringEncodingDOSNordic },
  { "dosPortuguese", kCFStringEncodingDOSPortuguese },
  { "dosRussian", kCFStringEncodingDOSRussian },
  { "dosThai", kCFStringEncodingDOSThai },
  { "dosTurkish", kCFStringEncodingDOSTurkish },
  { "EBCDIC_CP037", kCFStringEncodingEBCDIC_CP037 },
  { "EBCDIC_US", kCFStringEncodingEBCDIC_US },
  { "EUC_CN", kCFStringEncodingEUC_CN },
  { "EUC_JP", kCFStringEncodingEUC_JP },
  { "EUC_KR", kCFStringEncodingEUC_KR },
  { "EUC_TW", kCFStringEncodingEUC_TW },
  { "GBK_95", kCFStringEncodingGBK_95 },
  { "GB_18030_2000", kCFStringEncodingGB_18030_2000 },
  { "GB_2312_80", kCFStringEncodingGB_2312_80 },
  { "HZ_GB_2312", kCFStringEncodingHZ_GB_2312 },
  { "isoLatin10", kCFStringEncodingISOLatin10 },
  { "isoLatin2", kCFStringEncodingISOLatin2 },
  { "isoLatin3", kCFStringEncodingISOLatin3 },
  { "isoLatin4", kCFStringEncodingISOLatin4 },
  { "isoLatin5", kCFStringEncodingISOLatin5 },
  { "isoLatin6", kCFStringEncodingISOLatin6 },
  { "isoLatin7", kCFStringEncodingISOLatin7 },
  { "isoLatin8", kCFStringEncodingISOLatin8 },
  { "isoLatin9", kCFStringEncodingISOLatin9 },
  { "isoLatinArabic", kCFStringEncodingISOLatinArabic },
  { "isoLatinCyrillic", kCFStringEncodingISOLatinCyrillic },
  { "isoLatinGreek", kCFStringEncodingISOLatinGreek },
  { "isoLatinHebrew", kCFStringEncodingISOLatinHebrew },
  { "isoLatinThai", kCFStringEncodingISOLatinThai },
  { "ISO_2022_CN", kCFStringEncodingISO_2022_CN },
  { "ISO_2022_CN_EXT", kCFStringEncodingISO_2022_CN_EXT },
  { "ISO_2022_JP", kCFStringEncodingISO_2022_JP },
  { "ISO_2022_JP_1", kCFStringEncodingISO_2022_JP_1 },
  { "ISO_2022_JP_2", kCFStringEncodingISO_2022_JP_2 },
  { "ISO_2022_JP_3", kCFStringEncodingISO_2022_JP_3 },
  { "ISO_2022_KR", kCFStringEncodingISO_2022_KR },
  { "JIS_C6226_78", kCFStringEncodingJIS_C6226_78 },
  { "JIS_X0201_76", kCFStringEncodingJIS_X0201_76 },
  { "JIS_X0208_83", kCFStringEncodingJIS_X0208_83 },
  { "JIS_X0208_90", kCFStringEncodingJIS_X0208_90 },
  { "JIS_X0212_90", kCFStringEncodingJIS_X0212_90 },
  { "KOI8_R", kCFStringEncodingKOI8_R },
  { "KOI8_U", kCFStringEncodingKOI8_U },
  { "KSC_5601_87", kCFStringEncodingKSC_5601_87 },
  { "ksc_5601_92_Johab", kCFStringEncodingKSC_5601_92_Johab },
  { "macArabic", kCFStringEncodingMacArabic },
  { "macArmenian", kCFStringEncodingMacArmenian },
  { "macBengali", kCFStringEncodingMacBengali },
  { "macBurmese", kCFStringEncodingMacBurmese },
  { "macCeltic", kCFStringEncodingMacCeltic },
  { "macCentralEurRoman", kCFStringEncodingMacCentralEurRoman },
  { "macChineseSimp", kCFStringEncodingMacChineseSimp },
  { "macChineseTrad", kCFStringEncodingMacChineseTrad },
  { "macCroatian", kCFStringEncodingMacCroatian },
  { "macCyrillic", kCFStringEncodingMacCyrillic },
  { "macDevanagari", kCFStringEncodingMacDevanagari },
  { "macDingbats", kCFStringEncodingMacDingbats },
  { "macEthiopic", kCFStringEncodingMacEthiopic },
  { "macExtArabic", kCFStringEncodingMacExtArabic },
  { "macFarsi", kCFStringEncodingMacFarsi },
  { "macGaelic", kCFStringEncodingMacGaelic },
  { "macGeorgian", kCFStringEncodingMacGeorgian },
  { "macGreek", kCFStringEncodingMacGreek },
  { "macGujarati", kCFStringEncodingMacGujarati },
  { "macGurmukhi", kCFStringEncodingMacGurmukhi },
  { "macHFS", kCFStringEncodingMacHFS },
  { "macHebrew", kCFStringEncodingMacHebrew },
  { "macIcelandic", kCFStringEncodingMacIcelandic },
  { "macInuit", kCFStringEncodingMacInuit },
  { "macJapanese", kCFStringEncodingMacJapanese },
  { "macKannada", kCFStringEncodingMacKannada },
  { "macKhmer", kCFStringEncodingMacKhmer },
  { "macKorean", kCFStringEncodingMacKorean },
  { "macLaotian", kCFStringEncodingMacLaotian },
  { "macMalayalam", kCFStringEncodingMacMalayalam },
  { "macMongolian", kCFStringEncodingMacMongolian },
  { "macOriya", kCFStringEncodingMacOriya },
  { "macRomanLatin1", kCFStringEncodingMacRomanLatin1 },
  { "macRomanian", kCFStringEncodingMacRomanian },
  { "macSinhalese", kCFStringEncodingMacSinhalese },
  { "macSymbol", kCFStringEncodingMacSymbol },
  { "macTamil", kCFStringEncodingMacTamil },
  { "macTelugu", kCFStringEncodingMacTelugu },
  { "macThai", kCFStringEncodingMacThai },
  { "macTibetan", kCFStringEncodingMacTibetan },
  { "macTurkish", kCFStringEncodingMacTurkish },
  { "macUkrainian", kCFStringEncodingMacUkrainian },
  { "macVT100", kCFStringEncodingMacVT100 },
  { "macVietnamese", kCFStringEncodingMacVietnamese },
  { "nextStepJapanese", kCFStringEncodingNextStepJapanese },
  { "shiftJIS", kCFStringEncodingShiftJIS },
  { "shiftJIS_X0213", kCFStringEncodingShiftJIS_X0213 },
  { "shiftJIS_X0213_MenKuTen", kCFStringEncodingShiftJIS_X0213_MenKuTen },
  { "UTF7", kCFStringEncodingUTF7 },
  { "UTF7_IMAP", kCFStringEncodingUTF7_IMAP },
  { "VISCII", kCFStringEncodingVISCII },
  { "windowsArabic", kCFStringEncodingWindowsArabic },
  { "windowsBalticRim", kCFStringEncodingWindowsBalticRim },
  { "windowsCyrillic", kCFStringEncodingWindowsCyrillic },
  { "windowsGreek", kCFStringEncodingWindowsGreek },
  { "windowsHebrew", kCFStringEncodingWindowsHebrew },
  { "windowsKoreanJohab", kCFStringEncodingWindowsKoreanJohab },
  { "windowsLatin2", kCFStringEncodingWindowsLatin2 },
  { "windowsLatin5", kCFStringEncodingWindowsLatin5 },
  { "windowsVietnamese", kCFStringEncodingWindowsVietnamese },
};

static BOOL iima_encoding_for_name(const char *name, NSStringEncoding *encoding) {
  if (name == NULL || encoding == NULL) return NO;
#define IIMA_NS_ENCODING(key, value) if (strcmp(name, key) == 0) { *encoding = value; return YES; }
  IIMA_NS_ENCODING("ascii", NSASCIIStringEncoding)
  IIMA_NS_ENCODING("iso2022JP", NSISO2022JPStringEncoding)
  IIMA_NS_ENCODING("isoLatin1", NSISOLatin1StringEncoding)
  IIMA_NS_ENCODING("isoLatin2", NSISOLatin2StringEncoding)
  IIMA_NS_ENCODING("japaneseEUC", NSJapaneseEUCStringEncoding)
  IIMA_NS_ENCODING("macOSRoman", NSMacOSRomanStringEncoding)
  IIMA_NS_ENCODING("nextstep", NSNEXTSTEPStringEncoding)
  IIMA_NS_ENCODING("nonLossyASCII", NSNonLossyASCIIStringEncoding)
  IIMA_NS_ENCODING("shiftJIS", NSShiftJISStringEncoding)
  IIMA_NS_ENCODING("symbol", NSSymbolStringEncoding)
  IIMA_NS_ENCODING("unicode", NSUnicodeStringEncoding)
  IIMA_NS_ENCODING("utf16", NSUTF16StringEncoding)
  IIMA_NS_ENCODING("utf16BigEndian", NSUTF16BigEndianStringEncoding)
  IIMA_NS_ENCODING("utf16LittleEndian", NSUTF16LittleEndianStringEncoding)
  IIMA_NS_ENCODING("utf32", NSUTF32StringEncoding)
  IIMA_NS_ENCODING("utf32BigEndian", NSUTF32BigEndianStringEncoding)
  IIMA_NS_ENCODING("utf32LittleEndian", NSUTF32LittleEndianStringEncoding)
  IIMA_NS_ENCODING("utf8", NSUTF8StringEncoding)
  IIMA_NS_ENCODING("windowsCP1250", NSWindowsCP1250StringEncoding)
  IIMA_NS_ENCODING("windowsCP1251", NSWindowsCP1251StringEncoding)
  IIMA_NS_ENCODING("windowsCP1252", NSWindowsCP1252StringEncoding)
  IIMA_NS_ENCODING("windowsCP1253", NSWindowsCP1253StringEncoding)
  IIMA_NS_ENCODING("windowsCP1254", NSWindowsCP1254StringEncoding)
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
