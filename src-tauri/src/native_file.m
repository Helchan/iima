#import <Cocoa/Cocoa.h>
#import <dispatch/dispatch.h>

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static char *iima_native_file_copy_utf8(NSString *value) {
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

static int32_t iima_native_file_operation(const char *path_utf8,
                                          BOOL move_to_trash,
                                          char **error_out) {
  @autoreleasepool {
    if (error_out != NULL) {
      *error_out = NULL;
    }
    if (path_utf8 == NULL) {
      if (error_out != NULL) {
        *error_out = iima_native_file_copy_utf8(@"File path is missing");
      }
      return -1;
    }

    NSString *path = [NSString stringWithUTF8String:path_utf8];
    if (path.length == 0) {
      if (error_out != NULL) {
        *error_out = iima_native_file_copy_utf8(@"File path is invalid");
      }
      return -1;
    }

    NSURL *url = [NSURL fileURLWithPath:path];
    NSError *error = nil;
    BOOL success = move_to_trash
        ? [NSFileManager.defaultManager trashItemAtURL:url resultingItemURL:nil error:&error]
        : [NSFileManager.defaultManager removeItemAtURL:url error:&error];
    if (success) {
      return 0;
    }
    if (error_out != NULL) {
      *error_out = iima_native_file_copy_utf8(error.localizedDescription ?: @"File operation failed");
    }
    return -1;
  }
}

int32_t iima_native_file_trash(const char *path_utf8, char **error_out) {
  return iima_native_file_operation(path_utf8, YES, error_out);
}

int32_t iima_native_file_remove(const char *path_utf8, char **error_out) {
  return iima_native_file_operation(path_utf8, NO, error_out);
}

static int32_t iima_native_file_fail(NSString *message, char **error_out) {
  if (error_out != NULL) {
    *error_out = iima_native_file_copy_utf8(message);
  }
  return -1;
}

int32_t iima_native_file_reveal_paths(const char *paths_json_utf8, char **error_out) {
  if (error_out != NULL) {
    *error_out = NULL;
  }
  if (paths_json_utf8 == NULL) {
    return iima_native_file_fail(@"File paths are missing", error_out);
  }

  NSString *payload = [NSString stringWithUTF8String:paths_json_utf8];
  NSData *data = [payload dataUsingEncoding:NSUTF8StringEncoding];
  NSError *jsonError = nil;
  id object = data == nil ? nil : [NSJSONSerialization JSONObjectWithData:data options:0 error:&jsonError];
  if (![object isKindOfClass:NSArray.class] || jsonError != nil) {
    return iima_native_file_fail(jsonError.localizedDescription ?: @"File paths are invalid", error_out);
  }

  NSMutableArray<NSURL *> *urls = [NSMutableArray array];
  for (id value in (NSArray *)object) {
    if ([value isKindOfClass:NSString.class] && [value length] > 0) {
      [urls addObject:[NSURL fileURLWithPath:value]];
    }
  }
  if (urls.count == 0) {
    return 0;
  }

  void (^reveal)(void) = ^{
    [NSWorkspace.sharedWorkspace activateFileViewerSelectingURLs:urls];
  };
  if (NSThread.isMainThread) {
    reveal();
  } else {
    dispatch_sync(dispatch_get_main_queue(), reveal);
  }
  return 0;
}

int32_t iima_native_file_copy_text(const char *text_utf8, char **error_out) {
  if (error_out != NULL) {
    *error_out = NULL;
  }
  if (text_utf8 == NULL) {
    return iima_native_file_fail(@"Clipboard text is missing", error_out);
  }
  NSString *text = [NSString stringWithUTF8String:text_utf8];
  if (text == nil) {
    return iima_native_file_fail(@"Clipboard text is invalid", error_out);
  }

  __block BOOL success = NO;
  void (^copy)(void) = ^{
    NSPasteboard *pasteboard = NSPasteboard.generalPasteboard;
    [pasteboard clearContents];
    success = [pasteboard writeObjects:@[text]];
  };
  if (NSThread.isMainThread) {
    copy();
  } else {
    dispatch_sync(dispatch_get_main_queue(), copy);
  }
  return success ? 0 : iima_native_file_fail(@"Unable to copy playlist URLs", error_out);
}

void iima_native_file_free_string(char *value) {
  free(value);
}
