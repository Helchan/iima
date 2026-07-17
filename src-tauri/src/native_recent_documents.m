#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>

#include <dispatch/dispatch.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static char *iima_recent_copy_utf8(NSString *value) {
  if (value == nil) {
    return NULL;
  }
  const char *utf8 = value.UTF8String;
  if (utf8 == NULL) {
    return NULL;
  }
  size_t length = strlen(utf8);
  char *copy = malloc(length + 1);
  if (copy != NULL) {
    memcpy(copy, utf8, length + 1);
  }
  return copy;
}

static void iima_recent_on_main_sync(dispatch_block_t block) {
  if (NSThread.isMainThread) {
    block();
  } else {
    dispatch_sync(dispatch_get_main_queue(), block);
  }
}

static NSURL *iima_recent_url_from_string(NSString *value) {
  if (![value isKindOfClass:NSString.class] || value.length == 0) {
    return nil;
  }
  if ([value hasPrefix:@"file:"] || [value rangeOfString:@"://"].location != NSNotFound) {
    return [NSURL URLWithString:value];
  }
  return [NSURL fileURLWithPath:value];
}

static NSString *iima_recent_snapshot_json(NSError **error_out) {
  NSMutableArray<NSDictionary *> *documents = [NSMutableArray array];
  for (NSURL *url in NSDocumentController.sharedDocumentController.recentDocumentURLs) {
    NSString *absolute = url.absoluteString;
    if (absolute.length == 0) {
      continue;
    }
    NSString *path = url.isFileURL ? url.path : absolute;
    NSString *title = url.lastPathComponent;
    if (title.length == 0) {
      title = url.host.length > 0 ? url.host : absolute;
    }
    NSMutableDictionary *document = [@{
      @"url": absolute,
      @"path": path ?: absolute,
      @"title": title,
    } mutableCopy];
    NSData *bookmark = [url bookmarkDataWithOptions:0
                     includingResourceValuesForKeys:nil
                                      relativeToURL:nil
                                              error:nil];
    if (bookmark != nil) {
      document[@"bookmark"] = [bookmark base64EncodedStringWithOptions:0];
    }
    [documents addObject:document];
  }

  NSData *json = [NSJSONSerialization dataWithJSONObject:documents options:0 error:error_out];
  if (json == nil) {
    return nil;
  }
  return [[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding];
}

static NSURL *iima_recent_restore_url(id document, BOOL *stale_out) {
  if ([document isKindOfClass:NSString.class]) {
    return iima_recent_url_from_string(document);
  }
  if (![document isKindOfClass:NSDictionary.class]) {
    return nil;
  }

  NSDictionary *entry = document;
  NSString *bookmark_base64 = entry[@"bookmark"];
  if ([bookmark_base64 isKindOfClass:NSString.class] && bookmark_base64.length > 0) {
    NSData *bookmark = [[NSData alloc] initWithBase64EncodedString:bookmark_base64 options:0];
    if (bookmark != nil) {
      BOOL stale = NO;
      NSURL *url = [NSURL URLByResolvingBookmarkData:bookmark
                                            options:0
                                      relativeToURL:nil
                                bookmarkDataIsStale:&stale
                                              error:nil];
      if (url != nil) {
        if (stale_out != NULL) {
          *stale_out = *stale_out || stale;
        }
        return url;
      }
    }
  }

  NSString *fallback = entry[@"url"];
  if (![fallback isKindOfClass:NSString.class] || fallback.length == 0) {
    // Migrate the pre-AppKit Tauri model stored as { id, path, title }.
    fallback = entry[@"path"];
  }
  return iima_recent_url_from_string(fallback);
}

int32_t iima_recent_documents_is_sonoma_or_newer(void) {
  if (@available(macOS 14.0, *)) {
    return 1;
  }
  return 0;
}

char *iima_recent_documents_snapshot_json(char **error_out) {
  if (error_out != NULL) {
    *error_out = NULL;
  }
  __block NSString *json = nil;
  __block NSError *error = nil;
  iima_recent_on_main_sync(^{
    json = iima_recent_snapshot_json(&error);
  });
  if (json == nil) {
    if (error_out != NULL) {
      *error_out = iima_recent_copy_utf8(error.localizedDescription ?: @"Unable to serialize recent documents");
    }
    return NULL;
  }
  return iima_recent_copy_utf8(json);
}

int32_t iima_recent_documents_note(const char *value_utf8, char **error_out) {
  if (error_out != NULL) {
    *error_out = NULL;
  }
  if (value_utf8 == NULL) {
    return -1;
  }
  NSString *value = [NSString stringWithUTF8String:value_utf8];
  NSURL *url = iima_recent_url_from_string(value);
  if (url == nil) {
    if (error_out != NULL) {
      *error_out = iima_recent_copy_utf8(@"Recent document path or URL is invalid");
    }
    return -1;
  }
  iima_recent_on_main_sync(^{
    [NSDocumentController.sharedDocumentController noteNewRecentDocumentURL:url];
  });
  return 0;
}

int32_t iima_recent_documents_clear(char **error_out) {
  if (error_out != NULL) {
    *error_out = NULL;
  }
  iima_recent_on_main_sync(^{
    [NSDocumentController.sharedDocumentController clearRecentDocuments:nil];
  });
  return 0;
}

int32_t iima_recent_documents_restore_if_empty(const char *json_utf8,
                                               int32_t *restored_out,
                                               int32_t *stale_out,
                                               char **error_out) {
  if (restored_out != NULL) {
    *restored_out = 0;
  }
  if (stale_out != NULL) {
    *stale_out = 0;
  }
  if (error_out != NULL) {
    *error_out = NULL;
  }
  if (json_utf8 == NULL) {
    return -1;
  }
  if (@available(macOS 14.0, *)) {
  } else {
    return 0;
  }

  NSData *json_data = [[NSData alloc] initWithBytes:json_utf8 length:strlen(json_utf8)];
  NSError *parse_error = nil;
  id decoded = [NSJSONSerialization JSONObjectWithData:json_data options:0 error:&parse_error];
  if (![decoded isKindOfClass:NSArray.class]) {
    if (error_out != NULL) {
      *error_out = iima_recent_copy_utf8(parse_error.localizedDescription ?: @"Recent document backup is not an array");
    }
    return -1;
  }

  __block BOOL restored = NO;
  __block BOOL stale = NO;
  iima_recent_on_main_sync(^{
    NSDocumentController *controller = NSDocumentController.sharedDocumentController;
    if (controller.recentDocumentURLs.count != 0) {
      return;
    }
    for (id document in (NSArray *)decoded) {
      NSURL *url = iima_recent_restore_url(document, &stale);
      if (url == nil) {
        continue;
      }
      [controller noteNewRecentDocumentURL:url];
      restored = YES;
    }
  });
  if (restored_out != NULL) {
    *restored_out = restored ? 1 : 0;
  }
  if (stale_out != NULL) {
    *stale_out = stale ? 1 : 0;
  }
  return 0;
}

void iima_recent_documents_free_string(char *value) {
  free(value);
}
