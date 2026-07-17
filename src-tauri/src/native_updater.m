#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>

#include <dispatch/dispatch.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static NSString *const IIMAReferenceStableAppcastURL = @"https://www.iina.io/appcast.xml";
static NSString *const IIMAReferenceBetaAppcastURL = @"https://www.iina.io/appcast-beta.xml";
static NSString *const IIMAReferencePublicEdKey = @"UpwCRYfYOg0OGgQHY6RUdrV29yPcdkvxGlEfq46r6a0=";

static NSString *iima_updater_trimmed_bundle_string(NSString *key) {
  id value = [NSBundle.mainBundle objectForInfoDictionaryKey:key];
  if (![value isKindOfClass:NSString.class]) {
    return nil;
  }
  NSString *candidate = [(NSString *)value stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
  return candidate.length == 0 ? nil : candidate;
}

static BOOL iima_updater_is_owned_channel(void) {
  NSString *mode = iima_updater_trimmed_bundle_string(@"IIMAUpdateChannelMode");
  if ([mode isEqualToString:@"owned"]) {
    return YES;
  }
  NSString *stable = iima_updater_trimmed_bundle_string(@"SUFeedURL");
  NSString *beta = iima_updater_trimmed_bundle_string(@"IIMABetaFeedURL");
  NSString *publicKey = iima_updater_trimmed_bundle_string(@"SUPublicEDKey");
  return (stable != nil && ![stable isEqualToString:IIMAReferenceStableAppcastURL]) ||
    (beta != nil && ![beta isEqualToString:IIMAReferenceBetaAppcastURL]) ||
    (publicKey != nil && ![publicKey isEqualToString:IIMAReferencePublicEdKey]);
}

static NSString *iima_updater_bundle_url(NSString *key, NSString *fallback) {
  NSString *candidate = iima_updater_trimmed_bundle_string(key);
  // IINA 1.3.5 does not persist every delegate-selected channel URL in Info.plist. Missing keys
  // therefore use the reference constant in reference mode, while an explicitly project-owned
  // channel still fails closed. NSURLComponents raises NSInvalidArgumentException for a nil
  // string, so this check must happen before constructing it and before crossing the Rust FFI.
  if (candidate == nil) {
    return iima_updater_is_owned_channel() ? nil : fallback;
  }
  NSURLComponents *components = [NSURLComponents componentsWithString:candidate];
  if (
    ![components.scheme.lowercaseString isEqualToString:@"https"] ||
    components.host.length == 0 ||
    components.user.length > 0 ||
    components.password.length > 0 ||
    components.fragment.length > 0
  ) {
    // Reference mode reproduces IINA's source constants. An owned package must
    // fail closed instead of silently pairing its key with an IINA appcast.
    return iima_updater_is_owned_channel() ? nil : fallback;
  }
  return candidate;
}

static NSString *iima_updater_stable_appcast_url(void) {
  return iima_updater_bundle_url(@"SUFeedURL", IIMAReferenceStableAppcastURL);
}

static NSString *iima_updater_beta_appcast_url(void) {
  return iima_updater_bundle_url(@"IIMABetaFeedURL", IIMAReferenceBetaAppcastURL);
}

@interface NSObject (IIMASparkleDynamic)
- (instancetype)initWithUpdaterDelegate:(id)updaterDelegate userDriverDelegate:(id)userDriverDelegate;
- (id)updater;
- (void)checkForUpdates:(id)sender;
- (void)clearFeedURLFromUserDefaults;
- (BOOL)canCheckForUpdates;
- (BOOL)automaticallyChecksForUpdates;
- (void)setAutomaticallyChecksForUpdates:(BOOL)value;
- (NSTimeInterval)updateCheckInterval;
- (void)setUpdateCheckInterval:(NSTimeInterval)value;
@end

@interface IIMAUpdaterDelegate : NSObject
@property(nonatomic) BOOL receiveBetaUpdates;
@end

@implementation IIMAUpdaterDelegate
- (NSString *)feedURLStringForUpdater:(id)updater {
  (void)updater;
  return self.receiveBetaUpdates ? iima_updater_beta_appcast_url() : iima_updater_stable_appcast_url();
}
@end

static id IIMAUpdaterController = nil;
static IIMAUpdaterDelegate *IIMAUpdaterDelegateInstance = nil;
static NSBundle *IIMASparkleBundle = nil;
static NSString *IIMAUpdaterLastError = nil;
static BOOL IIMAReceiveBetaUpdates = NO;

static void iima_updater_reset_runtime(void) {
  IIMAUpdaterController = nil;
  IIMAUpdaterDelegateInstance = nil;
  IIMASparkleBundle = nil;
}

static void iima_updater_record_exception(NSString *operation, NSException *exception) {
  NSString *reason = exception.reason ?: @"Unknown Objective-C exception";
  IIMAUpdaterLastError = [NSString stringWithFormat:@"Sparkle %@ exception (%@): %@",
                                                    operation,
                                                    exception.name ?: @"NSException",
                                                    reason];
  // A dynamic Sparkle call may throw after partially mutating its controller.  Discard every
  // runtime reference so a later operation must perform a clean initialization instead of
  // treating a poisoned partial controller as available.
  iima_updater_reset_runtime();
}

static char *iima_updater_copy_utf8(NSString *value) {
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

static void iima_updater_on_main_sync(dispatch_block_t block) {
  if (NSThread.isMainThread) {
    block();
  } else {
    dispatch_sync(dispatch_get_main_queue(), block);
  }
}

static BOOL iima_updater_guarded_on_main(NSString *operation, dispatch_block_t block) {
  __block BOOL completed = NO;
  @try {
    iima_updater_on_main_sync(^{
      @try {
        block();
        completed = YES;
      } @catch (NSException *exception) {
        iima_updater_record_exception(operation, exception);
      }
    });
  } @catch (NSException *exception) {
    // Also contain failures raised while entering the main-queue boundary itself.  No Objective-C
    // exception is allowed to unwind into a Rust/Tauri callback.
    iima_updater_record_exception(operation, exception);
  }
  return completed;
}

static NSURL *iima_sparkle_framework_url(void) {
  NSString *override = NSProcessInfo.processInfo.environment[@"IIMA_SPARKLE_FRAMEWORK"];
  if (override.length > 0) {
    return [NSURL fileURLWithPath:override];
  }
  return [NSBundle.mainBundle.privateFrameworksURL URLByAppendingPathComponent:@"Sparkle.framework"];
}

static BOOL iima_updater_initialize_on_main(void) {
  if (IIMAUpdaterController != nil) {
    IIMAUpdaterDelegateInstance.receiveBetaUpdates = IIMAReceiveBetaUpdates;
    return YES;
  }

  NSString *stableAppcastURL = iima_updater_stable_appcast_url();
  NSString *betaAppcastURL = iima_updater_beta_appcast_url();
  if (stableAppcastURL == nil || betaAppcastURL == nil) {
    IIMAUpdaterLastError = @"The signed update channel contains an invalid HTTPS appcast URL";
    return NO;
  }
  if (
    iima_updater_is_owned_channel() &&
    ([stableAppcastURL isEqualToString:IIMAReferenceStableAppcastURL] ||
     [betaAppcastURL isEqualToString:IIMAReferenceBetaAppcastURL] ||
     [iima_updater_trimmed_bundle_string(@"SUPublicEDKey") isEqualToString:IIMAReferencePublicEdKey])
  ) {
    IIMAUpdaterLastError = @"A project-owned update channel must not reuse IINA update artifacts";
    return NO;
  }

  NSURL *framework_url = iima_sparkle_framework_url();
  if (framework_url == nil || ![NSFileManager.defaultManager fileExistsAtPath:framework_url.path]) {
    IIMAUpdaterLastError = @"Sparkle.framework is not bundled";
    return NO;
  }
  NSBundle *framework = [NSBundle bundleWithURL:framework_url];
  NSError *load_error = nil;
  if (framework == nil || ![framework loadAndReturnError:&load_error]) {
    IIMAUpdaterLastError = load_error.localizedDescription ?: @"Unable to load Sparkle.framework";
    return NO;
  }

  Class controller_class = NSClassFromString(@"SPUStandardUpdaterController");
  if (controller_class == Nil) {
    IIMAUpdaterLastError = @"Sparkle updater controller is unavailable";
    return NO;
  }
  IIMAUpdaterDelegate *delegate = [IIMAUpdaterDelegate new];
  delegate.receiveBetaUpdates = IIMAReceiveBetaUpdates;
  id controller = [[controller_class alloc] initWithUpdaterDelegate:delegate userDriverDelegate:nil];
  if (controller == nil) {
    IIMAUpdaterLastError = @"Unable to create the Sparkle updater controller";
    return NO;
  }

  id updater = [controller updater];
  if ([updater respondsToSelector:@selector(clearFeedURLFromUserDefaults)]) {
    [updater clearFeedURLFromUserDefaults];
  }
  // Commit the runtime only after every potentially throwing validation call above succeeds.
  // This keeps a failed initialization retryable and prevents the early-return path at the top
  // from accepting a partially initialized controller.
  IIMASparkleBundle = framework;
  IIMAUpdaterDelegateInstance = delegate;
  IIMAUpdaterController = controller;
  IIMAUpdaterLastError = nil;
  return YES;
}

static NSString *iima_updater_status_json_on_main(NSError **error_out) {
  id updater = IIMAUpdaterController == nil ? nil : [IIMAUpdaterController updater];
  BOOL can_check = updater != nil && [updater respondsToSelector:@selector(canCheckForUpdates)]
    ? [updater canCheckForUpdates]
    : NO;
  BOOL automatic = updater != nil && [updater respondsToSelector:@selector(automaticallyChecksForUpdates)]
    ? [updater automaticallyChecksForUpdates]
    : NO;
  NSTimeInterval interval = updater != nil && [updater respondsToSelector:@selector(updateCheckInterval)]
    ? [updater updateCheckInterval]
    : 86400.0;
  NSString *framework_version = IIMASparkleBundle.infoDictionary[@"CFBundleShortVersionString"];
  NSString *selectedFeedURL = IIMAReceiveBetaUpdates
    ? iima_updater_beta_appcast_url()
    : iima_updater_stable_appcast_url();
  NSDictionary *status = @{
    @"available": @(IIMAUpdaterController != nil),
    @"can_check_for_updates": @(can_check),
    @"automatically_checks_for_updates": @(automatic),
    @"update_check_interval": @(interval),
    @"receive_beta_updates": @(IIMAReceiveBetaUpdates),
    @"feed_url": selectedFeedURL ?: @"",
    @"framework_version": framework_version ?: @"",
    @"error": IIMAUpdaterLastError ?: [NSNull null],
  };
  NSData *json = [NSJSONSerialization dataWithJSONObject:status options:0 error:error_out];
  return json == nil ? nil : [[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding];
}

int32_t iima_updater_initialize(int32_t receive_beta_updates, char **error_out) {
  if (error_out != NULL) {
    *error_out = NULL;
  }
  IIMAReceiveBetaUpdates = receive_beta_updates != 0;
  __block BOOL success = NO;
  iima_updater_guarded_on_main(@"initialization", ^{
    success = iima_updater_initialize_on_main();
  });
  if (!success && error_out != NULL) {
    *error_out = iima_updater_copy_utf8(IIMAUpdaterLastError ?: @"Unable to initialize Sparkle");
  }
  return success ? 0 : -1;
}

int32_t iima_updater_set_receive_beta(int32_t receive_beta_updates, char **error_out) {
  if (error_out != NULL) {
    *error_out = NULL;
  }
  BOOL requested = receive_beta_updates != 0;
  __block BOOL success = NO;
  iima_updater_guarded_on_main(@"receive-beta setting", ^{
    IIMAReceiveBetaUpdates = requested;
    IIMAUpdaterDelegateInstance.receiveBetaUpdates = IIMAReceiveBetaUpdates;
    IIMAUpdaterLastError = nil;
    success = YES;
  });
  if (!success && error_out != NULL) {
    *error_out = iima_updater_copy_utf8(IIMAUpdaterLastError ?: @"Unable to change the update channel");
  }
  return success ? 0 : -1;
}

int32_t iima_updater_set_automatic_checks(int32_t enabled, char **error_out) {
  if (error_out != NULL) {
    *error_out = NULL;
  }
  __block BOOL success = NO;
  iima_updater_guarded_on_main(@"automatic-check setting", ^{
    if (!iima_updater_initialize_on_main()) {
      return;
    }
    id updater = [IIMAUpdaterController updater];
    if (![updater respondsToSelector:@selector(setAutomaticallyChecksForUpdates:)]) {
      IIMAUpdaterLastError = @"Sparkle automatic-check settings are unavailable";
      return;
    }
    [updater setAutomaticallyChecksForUpdates:enabled != 0];
    IIMAUpdaterLastError = nil;
    success = YES;
  });
  if (!success && error_out != NULL) {
    *error_out = iima_updater_copy_utf8(IIMAUpdaterLastError ?: @"Unable to change automatic update checks");
  }
  return success ? 0 : -1;
}

int32_t iima_updater_set_check_interval(double interval, char **error_out) {
  if (error_out != NULL) {
    *error_out = NULL;
  }
  __block BOOL success = NO;
  iima_updater_guarded_on_main(@"check-interval setting", ^{
    if (!iima_updater_initialize_on_main()) {
      return;
    }
    id updater = [IIMAUpdaterController updater];
    if (![updater respondsToSelector:@selector(setUpdateCheckInterval:)]) {
      IIMAUpdaterLastError = @"Sparkle update interval settings are unavailable";
      return;
    }
    [updater setUpdateCheckInterval:interval];
    IIMAUpdaterLastError = nil;
    success = YES;
  });
  if (!success && error_out != NULL) {
    *error_out = iima_updater_copy_utf8(IIMAUpdaterLastError ?: @"Unable to change the update interval");
  }
  return success ? 0 : -1;
}

int32_t iima_updater_check_for_updates(char **error_out) {
  if (error_out != NULL) {
    *error_out = NULL;
  }
  __block BOOL success = NO;
  iima_updater_guarded_on_main(@"manual-update check", ^{
    if (!iima_updater_initialize_on_main()) {
      return;
    }
    [IIMAUpdaterController checkForUpdates:nil];
    IIMAUpdaterLastError = nil;
    success = YES;
  });
  if (!success && error_out != NULL) {
    *error_out = iima_updater_copy_utf8(IIMAUpdaterLastError ?: @"Unable to check for updates");
  }
  return success ? 0 : -1;
}

char *iima_updater_status_json(char **error_out) {
  if (error_out != NULL) {
    *error_out = NULL;
  }
  __block NSError *error = nil;
  __block NSString *json = nil;
  iima_updater_guarded_on_main(@"status read", ^{
    json = iima_updater_status_json_on_main(&error);
  });
  if (json == nil && error_out != NULL) {
    *error_out = iima_updater_copy_utf8(
      IIMAUpdaterLastError ?: error.localizedDescription ?: @"Unable to serialize updater status"
    );
  }
  return iima_updater_copy_utf8(json);
}

void iima_updater_free_string(char *value) {
  free(value);
}
