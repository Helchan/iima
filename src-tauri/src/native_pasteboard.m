#import <Cocoa/Cocoa.h>
#import <dispatch/dispatch.h>

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static NSPasteboardType const IIMAPlaylistItemType = @"IINAPlaylistItem";
static NSPasteboardType const IIMALegacyFilenamesType = @"NSFilenamesPboardType";
static NSPasteboardType const IIMALegacyURLType = @"NSURL";

static char *iima_playlist_pasteboard_copy_utf8(NSString *value) {
  if (value == nil || value.UTF8String == NULL) {
    return NULL;
  }
  size_t length = strlen(value.UTF8String);
  char *copy = malloc(length + 1);
  if (copy != NULL) {
    memcpy(copy, value.UTF8String, length + 1);
  }
  return copy;
}

static void iima_playlist_pasteboard_set_error(char **errorOut, NSString *message) {
  if (errorOut != NULL) {
    *errorOut = iima_playlist_pasteboard_copy_utf8(message);
  }
}

static NSArray *iima_playlist_pasteboard_json_array(const char *jsonUTF8, NSError **errorOut) {
  if (jsonUTF8 == NULL) {
    return nil;
  }
  NSString *json = [NSString stringWithUTF8String:jsonUTF8];
  NSData *data = [json dataUsingEncoding:NSUTF8StringEncoding];
  id value = data == nil ? nil : [NSJSONSerialization JSONObjectWithData:data options:0 error:errorOut];
  return [value isKindOfClass:NSArray.class] ? value : nil;
}

int32_t iima_native_playlist_pasteboard_write(const char *indexesJSONUTF8,
                                               const char *pathsJSONUTF8,
                                               char **errorOut) {
  if (errorOut != NULL) {
    *errorOut = NULL;
  }
  NSError *error = nil;
  NSArray *indexes = iima_playlist_pasteboard_json_array(indexesJSONUTF8, &error);
  NSArray *paths = iima_playlist_pasteboard_json_array(pathsJSONUTF8, &error);
  if (indexes == nil || paths == nil || error != nil) {
    iima_playlist_pasteboard_set_error(errorOut, error.localizedDescription ?: @"Playlist data is invalid");
    return -1;
  }

  NSMutableIndexSet *indexSet = [NSMutableIndexSet indexSet];
  for (id value in indexes) {
    if ([value isKindOfClass:NSNumber.class] && [value integerValue] >= 0) {
      [indexSet addIndex:[value unsignedIntegerValue]];
    }
  }
  NSMutableArray<NSString *> *filenames = [NSMutableArray array];
  for (id value in paths) {
    if ([value isKindOfClass:NSString.class]) {
      [filenames addObject:value];
    }
  }

  __block BOOL success = NO;
  void (^writePasteboard)(void) = ^{
    NSPasteboard *pasteboard = NSPasteboard.generalPasteboard;
    [pasteboard declareTypes:@[IIMAPlaylistItemType, IIMALegacyFilenamesType] owner:nil];
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    NSData *indexData = [NSKeyedArchiver archivedDataWithRootObject:indexSet];
#pragma clang diagnostic pop
    success = [pasteboard setData:indexData forType:IIMAPlaylistItemType]
      && [pasteboard setPropertyList:filenames forType:IIMALegacyFilenamesType];
  };
  if (NSThread.isMainThread) {
    writePasteboard();
  } else {
    dispatch_sync(dispatch_get_main_queue(), writePasteboard);
  }
  if (!success) {
    iima_playlist_pasteboard_set_error(errorOut, @"Unable to write playlist pasteboard data");
    return -1;
  }
  return 0;
}

static NSArray<NSString *> *iima_playlist_pasteboard_resolve_aliases(NSArray *values) {
  NSMutableArray<NSString *> *result = [NSMutableArray array];
  for (id value in values) {
    if (![value isKindOfClass:NSString.class]) {
      continue;
    }
    NSURL *url = [NSURL fileURLWithPath:value];
    NSError *error = nil;
    NSURL *resolved = [NSURL URLByResolvingAliasFileAtURL:url
                                                  options:NSURLBookmarkResolutionWithoutUI | NSURLBookmarkResolutionWithoutMounting
                                                    error:&error];
    [result addObject:(resolved != nil && error == nil) ? resolved.path : value];
  }
  return result;
}

static char *iima_playlist_pasteboard_payload(NSString *kind, NSArray<NSString *> *values,
                                               char **errorOut) {
  NSError *error = nil;
  NSData *data = [NSJSONSerialization dataWithJSONObject:@{@"kind": kind, @"values": values}
                                                 options:0
                                                   error:&error];
  NSString *json = data == nil ? nil : [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
  if (json == nil) {
    iima_playlist_pasteboard_set_error(errorOut, error.localizedDescription ?: @"Unable to encode playlist pasteboard data");
    return NULL;
  }
  return iima_playlist_pasteboard_copy_utf8(json);
}

char *iima_native_playlist_pasteboard_read(char **errorOut) {
  if (errorOut != NULL) {
    *errorOut = NULL;
  }
  __block char *result = NULL;
  void (^readPasteboard)(void) = ^{
    NSPasteboard *pasteboard = NSPasteboard.generalPasteboard;
    id filenames = [pasteboard propertyListForType:IIMALegacyFilenamesType];
    if ([filenames isKindOfClass:NSArray.class]) {
      result = iima_playlist_pasteboard_payload(@"filenames", iima_playlist_pasteboard_resolve_aliases(filenames), errorOut);
      return;
    }
    id urls = [pasteboard propertyListForType:IIMALegacyURLType];
    if ([urls isKindOfClass:NSArray.class]) {
      NSMutableArray<NSString *> *strings = [NSMutableArray array];
      for (id value in (NSArray *)urls) {
        if ([value isKindOfClass:NSString.class]) {
          [strings addObject:value];
        }
      }
      result = iima_playlist_pasteboard_payload(@"urls", strings, errorOut);
      return;
    }
    NSString *string = [pasteboard stringForType:NSPasteboardTypeString];
    if (string != nil) {
      result = iima_playlist_pasteboard_payload(@"string", @[string], errorOut);
    }
  };
  if (NSThread.isMainThread) {
    readPasteboard();
  } else {
    dispatch_sync(dispatch_get_main_queue(), readPasteboard);
  }
  return result;
}

int32_t iima_native_playlist_pasteboard_has_filenames(void) {
  __block BOOL result = NO;
  void (^inspect)(void) = ^{
    result = [NSPasteboard.generalPasteboard.types containsObject:IIMALegacyFilenamesType];
  };
  if (NSThread.isMainThread) {
    inspect();
  } else {
    dispatch_sync(dispatch_get_main_queue(), inspect);
  }
  return result ? 1 : 0;
}

void iima_native_playlist_pasteboard_free(char *value) {
  free(value);
}
