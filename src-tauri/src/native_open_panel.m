#import <Cocoa/Cocoa.h>
#import <dispatch/dispatch.h>

#include <stdlib.h>
#include <string.h>

char *iima_native_open_media_panel(const char *title) {
  __block char *result = NULL;
  void (^showPanel)(void) = ^{
    @autoreleasepool {
      NSOpenPanel *panel = [NSOpenPanel openPanel];
      if (title != NULL) {
        panel.title = [NSString stringWithUTF8String:title];
      }
      panel.canCreateDirectories = NO;
      panel.canChooseFiles = YES;
      panel.canChooseDirectories = YES;
      panel.allowsMultipleSelection = YES;
      panel.resolvesAliases = YES;

      if ([panel runModal] != NSModalResponseOK) {
        return;
      }

      NSMutableArray<NSString *> *paths = [NSMutableArray arrayWithCapacity:panel.URLs.count];
      for (NSURL *url in panel.URLs) {
        if (url.path != nil) {
          [paths addObject:url.path];
        }
      }
      NSError *error = nil;
      NSData *json = [NSJSONSerialization dataWithJSONObject:paths options:0 error:&error];
      if (json == nil || error != nil) {
        return;
      }
      NSString *payload = [[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding];
      if (payload != nil) {
        result = strdup(payload.UTF8String);
      }
    }
  };

  if ([NSThread isMainThread]) {
    showPanel();
  } else {
    dispatch_sync(dispatch_get_main_queue(), showPanel);
  }
  return result;
}

void iima_native_open_media_panel_free(char *paths) {
  free(paths);
}
