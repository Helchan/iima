#import <Foundation/Foundation.h>
#import <stdint.h>
#import <stdlib.h>
#import <string.h>

char *iima_native_preferred_languages_json(void) {
  @autoreleasepool {
    NSArray<NSString *> *languages = NSLocale.preferredLanguages;
    if (languages == nil) {
      languages = @[];
    }
    NSError *error = nil;
    NSData *data = [NSJSONSerialization dataWithJSONObject:languages
                                                   options:0
                                                     error:&error];
    if (data == nil || error != nil) {
      return NULL;
    }
    char *result = malloc(data.length + 1);
    if (result == NULL) {
      return NULL;
    }
    memcpy(result, data.bytes, data.length);
    result[data.length] = '\0';
    return result;
  }
}

void iima_native_free_localization_string(char *value) { free(value); }
