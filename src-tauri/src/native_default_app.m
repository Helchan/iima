#import <CoreServices/CoreServices.h>
#import <Foundation/Foundation.h>

int iima_native_set_default_application(int video, int audio, int playlist,
                                        int *success_count, int *failed_count) {
  @autoreleasepool {
    NSArray<NSDictionary *> *declarations =
        NSBundle.mainBundle.infoDictionary[@"UTImportedTypeDeclarations"];
    NSString *bundleIdentifier = NSBundle.mainBundle.bundleIdentifier;
    if (![declarations isKindOfClass:NSArray.class] || bundleIdentifier.length == 0 ||
        success_count == NULL || failed_count == NULL) {
      return -1;
    }

    *success_count = 0;
    *failed_count = 0;
    NSDictionary<NSString *, NSNumber *> *selected = @{
      @"public.movie" : @(video != 0),
      @"public.audio" : @(audio != 0),
      @"public.text" : @(playlist != 0),
    };

    for (NSDictionary *declaration in declarations) {
      NSArray<NSString *> *conformsTo = declaration[@"UTTypeConformsTo"];
      NSDictionary *tagSpecification = declaration[@"UTTypeTagSpecification"];
      NSArray<NSString *> *extensions = tagSpecification[@"public.filename-extension"];
      if (![conformsTo isKindOfClass:NSArray.class] ||
          ![extensions isKindOfClass:NSArray.class]) {
        return -2;
      }

      BOOL shouldRegister = NO;
      for (NSString *type in conformsTo) {
        if (selected[type].boolValue) {
          shouldRegister = YES;
          break;
        }
      }
      if (!shouldRegister) {
        continue;
      }

      for (NSString *extension in extensions) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
        CFStringRef contentType = UTTypeCreatePreferredIdentifierForTag(
            kUTTagClassFilenameExtension, (__bridge CFStringRef)extension, NULL);
        OSStatus status = contentType == NULL
                              ? paramErr
                              : LSSetDefaultRoleHandlerForContentType(
                                    contentType, kLSRolesAll,
                                    (__bridge CFStringRef)bundleIdentifier);
#pragma clang diagnostic pop
        if (contentType != NULL) {
          CFRelease(contentType);
        }
        if (status == noErr) {
          *success_count += 1;
        } else {
          *failed_count += 1;
        }
      }
    }
    return 0;
  }
}
