#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef NS_ENUM(NSInteger, IIMAFakeSparkleThrowPoint) {
  IIMAFakeSparkleThrowNone = 0,
  IIMAFakeSparkleThrowControllerInit,
  IIMAFakeSparkleThrowClearFeed,
  IIMAFakeSparkleThrowReceiveBeta,
  IIMAFakeSparkleThrowSetAutomaticChecks,
  IIMAFakeSparkleThrowSetCheckInterval,
  IIMAFakeSparkleThrowCheckForUpdates,
  IIMAFakeSparkleThrowStatusCanCheck,
};

static IIMAFakeSparkleThrowPoint IIMAFakeSparkleThrow = IIMAFakeSparkleThrowNone;

static void IIMAFakeSparkleMaybeThrow(IIMAFakeSparkleThrowPoint point, NSString *reason) {
  if (IIMAFakeSparkleThrow == point) {
    @throw [NSException exceptionWithName:@"IIMAFakeSparkleException" reason:reason userInfo:nil];
  }
}

// native_updater.m dynamically loads Sparkle through NSBundle. The executable harness replaces
// that loader at compile time, while retaining every production FFI function and guard verbatim.
// No test hook is compiled into the application.
@interface IIMAFakeUpdaterBundle : NSObject
+ (instancetype)mainBundle;
+ (instancetype)bundleWithURL:(NSURL *)url;
- (id)objectForInfoDictionaryKey:(NSString *)key;
- (NSURL *)privateFrameworksURL;
- (BOOL)loadAndReturnError:(NSError **)error;
- (NSDictionary *)infoDictionary;
@end

@implementation IIMAFakeUpdaterBundle
+ (instancetype)mainBundle {
  static IIMAFakeUpdaterBundle *bundle = nil;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    bundle = [IIMAFakeUpdaterBundle new];
  });
  return bundle;
}

+ (instancetype)bundleWithURL:(NSURL *)url {
  (void)url;
  return [IIMAFakeUpdaterBundle new];
}

- (id)objectForInfoDictionaryKey:(NSString *)key {
  (void)key;
  return nil;
}

- (NSURL *)privateFrameworksURL {
  return [NSURL fileURLWithPath:@"/tmp" isDirectory:YES];
}

- (BOOL)loadAndReturnError:(NSError **)error {
  if (error != NULL) {
    *error = nil;
  }
  return YES;
}

- (NSDictionary *)infoDictionary {
  return @{
    @"CFBundleShortVersionString": @"fake-sparkle-1.0",
  };
}
@end

#define NSBundle IIMAFakeUpdaterBundle
#include "../src-tauri/src/native_updater.m"
#undef NSBundle

@interface IIMAFakeSparkleUpdater : NSObject
@property(nonatomic) BOOL automaticallyChecks;
@property(nonatomic) NSTimeInterval interval;
@end

@implementation IIMAFakeSparkleUpdater
- (void)clearFeedURLFromUserDefaults {
  IIMAFakeSparkleMaybeThrow(
    IIMAFakeSparkleThrowClearFeed,
    @"clear-feed injected failure"
  );
}

- (BOOL)canCheckForUpdates {
  IIMAFakeSparkleMaybeThrow(
    IIMAFakeSparkleThrowStatusCanCheck,
    @"status can-check injected failure"
  );
  return YES;
}

- (BOOL)automaticallyChecksForUpdates {
  return self.automaticallyChecks;
}

- (void)setAutomaticallyChecksForUpdates:(BOOL)value {
  IIMAFakeSparkleMaybeThrow(
    IIMAFakeSparkleThrowSetAutomaticChecks,
    @"automatic-check setting injected failure"
  );
  self.automaticallyChecks = value;
}

- (NSTimeInterval)updateCheckInterval {
  return self.interval;
}

- (void)setUpdateCheckInterval:(NSTimeInterval)value {
  IIMAFakeSparkleMaybeThrow(
    IIMAFakeSparkleThrowSetCheckInterval,
    @"check-interval setting injected failure"
  );
  self.interval = value;
}
@end

@interface SPUStandardUpdaterController : NSObject
@property(nonatomic, strong) IIMAFakeSparkleUpdater *fakeUpdater;
@end

@implementation SPUStandardUpdaterController
- (instancetype)initWithUpdaterDelegate:(id)updaterDelegate userDriverDelegate:(id)userDriverDelegate {
  (void)updaterDelegate;
  (void)userDriverDelegate;
  IIMAFakeSparkleMaybeThrow(
    IIMAFakeSparkleThrowControllerInit,
    @"controller init injected failure"
  );
  self = [super init];
  if (self != nil) {
    self.fakeUpdater = [IIMAFakeSparkleUpdater new];
    self.fakeUpdater.interval = 86400.0;
  }
  return self;
}

- (id)updater {
  return self.fakeUpdater;
}

- (void)checkForUpdates:(id)sender {
  (void)sender;
  IIMAFakeSparkleMaybeThrow(
    IIMAFakeSparkleThrowCheckForUpdates,
    @"manual-update check injected failure"
  );
}
@end

@interface IIMAThrowingUpdaterDelegate : IIMAUpdaterDelegate
@end

@implementation IIMAThrowingUpdaterDelegate
- (void)setReceiveBetaUpdates:(BOOL)value {
  IIMAFakeSparkleMaybeThrow(
    IIMAFakeSparkleThrowReceiveBeta,
    @"receive-beta setting injected failure"
  );
  [super setReceiveBetaUpdates:value];
}
@end

static void IIMAFail(NSString *message) {
  fprintf(stderr, "native updater exception harness: %s\n", message.UTF8String);
  exit(1);
}

static void IIMARequire(BOOL condition, NSString *message) {
  if (!condition) {
    IIMAFail(message);
  }
}

static NSString *IIMATakeString(char *value) {
  if (value == NULL) {
    return nil;
  }
  NSString *result = [NSString stringWithUTF8String:value];
  iima_updater_free_string(value);
  return result;
}

typedef int32_t (^IIMAIntBoundaryCall)(char **errorOut);

static int32_t IIMACallIntBoundary(
  NSString *name,
  IIMAIntBoundaryCall call,
  NSString **errorOut
) {
  char *error = NULL;
  int32_t result = INT32_MIN;
  @try {
    result = call(&error);
  } @catch (NSException *exception) {
    IIMAFail([NSString stringWithFormat:@"%@ leaked %@: %@", name, exception.name, exception.reason]);
  }
  if (errorOut != NULL) {
    *errorOut = IIMATakeString(error);
  } else if (error != NULL) {
    iima_updater_free_string(error);
  }
  return result;
}

static NSDictionary *IIMACallStatusBoundary(NSString **errorOut) {
  char *error = NULL;
  char *json = NULL;
  @try {
    json = iima_updater_status_json(&error);
  } @catch (NSException *exception) {
    IIMAFail([NSString stringWithFormat:@"status leaked %@: %@", exception.name, exception.reason]);
  }
  NSString *errorString = IIMATakeString(error);
  if (errorOut != NULL) {
    *errorOut = errorString;
  }
  NSString *jsonString = IIMATakeString(json);
  if (jsonString == nil) {
    return nil;
  }
  NSError *decodeError = nil;
  NSDictionary *status = [NSJSONSerialization JSONObjectWithData:
    [jsonString dataUsingEncoding:NSUTF8StringEncoding]
                                                       options:0
                                                         error:&decodeError];
  IIMARequire(
    status != nil && decodeError == nil,
    [NSString stringWithFormat:@"invalid status JSON: %@", decodeError.localizedDescription]
  );
  return status;
}

static void IIMAResetHarness(void) {
  IIMAFakeSparkleThrow = IIMAFakeSparkleThrowNone;
  iima_updater_reset_runtime();
  IIMAUpdaterLastError = nil;
  IIMAReceiveBetaUpdates = NO;
}

static void IIMARequireRuntimeReset(NSString *operation) {
  IIMARequire(
    IIMAUpdaterController == nil &&
      IIMAUpdaterDelegateInstance == nil &&
      IIMASparkleBundle == nil,
    [NSString stringWithFormat:@"%@ did not reset updater runtime", operation]
  );
}

static void IIMARequireInjectedFailure(
  NSString *operation,
  NSString *expectedReason,
  int32_t result,
  NSString *error
) {
  IIMARequire(result == -1, [NSString stringWithFormat:@"%@ unexpectedly succeeded", operation]);
  IIMARequire(
    [error containsString:@"IIMAFakeSparkleException"] && [error containsString:expectedReason],
    [NSString stringWithFormat:@"%@ returned the wrong error: %@", operation, error]
  );
  IIMARequireRuntimeReset(operation);

  IIMAFakeSparkleThrow = IIMAFakeSparkleThrowNone;
  NSDictionary *status = IIMACallStatusBoundary(NULL);
  IIMARequire([status[@"available"] isEqual:@NO], @"failed runtime remained available");
  IIMARequire(
    [status[@"error"] containsString:expectedReason],
    [NSString stringWithFormat:@"%@ error was not retained in status", operation]
  );
}

static void IIMAInitializeSuccessfully(BOOL receiveBeta) {
  NSString *error = nil;
  int32_t result = IIMACallIntBoundary(@"successful initialization", ^int32_t(char **errorOut) {
    return iima_updater_initialize(receiveBeta ? 1 : 0, errorOut);
  }, &error);
  IIMARequire(result == 0, [NSString stringWithFormat:@"initialization failed: %@", error]);
  IIMARequire(
    IIMAUpdaterController != nil &&
      IIMAUpdaterDelegateInstance != nil &&
      IIMASparkleBundle != nil,
    @"successful initialization did not commit updater runtime"
  );
}

static void IIMARequireRetryAfterFailure(BOOL receiveBeta) {
  IIMAInitializeSuccessfully(receiveBeta);
  NSDictionary *status = IIMACallStatusBoundary(NULL);
  IIMARequire([status[@"available"] isEqual:@YES], @"clean initialization retry is unavailable");
  IIMARequire(status[@"error"] == NSNull.null, @"clean initialization retry retained stale error");
}

static void IIMATestInitializationException(IIMAFakeSparkleThrowPoint point, NSString *reason) {
  IIMAResetHarness();
  IIMAFakeSparkleThrow = point;
  NSString *error = nil;
  int32_t result = IIMACallIntBoundary(@"initialization", ^int32_t(char **errorOut) {
    return iima_updater_initialize(0, errorOut);
  }, &error);
  IIMARequireInjectedFailure(@"initialization", reason, result, error);
  IIMARequireRetryAfterFailure(NO);
}

static void IIMATestReceiveBetaException(void) {
  IIMAResetHarness();
  IIMAInitializeSuccessfully(NO);
  IIMAUpdaterDelegateInstance = (IIMAUpdaterDelegate *)[IIMAThrowingUpdaterDelegate new];
  IIMAFakeSparkleThrow = IIMAFakeSparkleThrowReceiveBeta;
  NSString *error = nil;
  int32_t result = IIMACallIntBoundary(@"receive-beta setting", ^int32_t(char **errorOut) {
    return iima_updater_set_receive_beta(1, errorOut);
  }, &error);
  IIMARequireInjectedFailure(
    @"receive-beta setting",
    @"receive-beta setting injected failure",
    result,
    error
  );
  IIMARequireRetryAfterFailure(YES);
}

static void IIMATestAutomaticChecksException(void) {
  IIMAResetHarness();
  IIMAInitializeSuccessfully(NO);
  IIMAFakeSparkleThrow = IIMAFakeSparkleThrowSetAutomaticChecks;
  NSString *error = nil;
  int32_t result = IIMACallIntBoundary(@"automatic-check setting", ^int32_t(char **errorOut) {
    return iima_updater_set_automatic_checks(1, errorOut);
  }, &error);
  IIMARequireInjectedFailure(
    @"automatic-check setting",
    @"automatic-check setting injected failure",
    result,
    error
  );
  IIMARequireRetryAfterFailure(NO);
}

static void IIMATestCheckIntervalException(void) {
  IIMAResetHarness();
  IIMAInitializeSuccessfully(NO);
  IIMAFakeSparkleThrow = IIMAFakeSparkleThrowSetCheckInterval;
  NSString *error = nil;
  int32_t result = IIMACallIntBoundary(@"check-interval setting", ^int32_t(char **errorOut) {
    return iima_updater_set_check_interval(3600.0, errorOut);
  }, &error);
  IIMARequireInjectedFailure(
    @"check-interval setting",
    @"check-interval setting injected failure",
    result,
    error
  );
  IIMARequireRetryAfterFailure(NO);
}

static void IIMATestManualCheckException(void) {
  IIMAResetHarness();
  IIMAInitializeSuccessfully(NO);
  IIMAFakeSparkleThrow = IIMAFakeSparkleThrowCheckForUpdates;
  NSString *error = nil;
  int32_t result = IIMACallIntBoundary(@"manual-update check", ^int32_t(char **errorOut) {
    return iima_updater_check_for_updates(errorOut);
  }, &error);
  IIMARequireInjectedFailure(
    @"manual-update check",
    @"manual-update check injected failure",
    result,
    error
  );
  IIMARequireRetryAfterFailure(NO);
}

static void IIMATestStatusException(void) {
  IIMAResetHarness();
  IIMAInitializeSuccessfully(NO);
  IIMAFakeSparkleThrow = IIMAFakeSparkleThrowStatusCanCheck;
  NSString *error = nil;
  NSDictionary *status = IIMACallStatusBoundary(&error);
  IIMARequire(status == nil, @"throwing status read unexpectedly returned JSON");
  IIMARequire(
    [error containsString:@"status can-check injected failure"],
    [NSString stringWithFormat:@"status read returned the wrong error: %@", error]
  );
  IIMARequireRuntimeReset(@"status read");

  IIMAFakeSparkleThrow = IIMAFakeSparkleThrowNone;
  NSDictionary *resetStatus = IIMACallStatusBoundary(NULL);
  IIMARequire([resetStatus[@"available"] isEqual:@NO], @"throwing status read remained available");
  IIMARequire(
    [resetStatus[@"error"] containsString:@"status can-check injected failure"],
    @"throwing status read did not retain its error"
  );
  IIMARequireRetryAfterFailure(NO);
}

int main(void) {
  @autoreleasepool {
    setenv("IIMA_SPARKLE_FRAMEWORK", "/tmp", 1);

    IIMATestInitializationException(
      IIMAFakeSparkleThrowControllerInit,
      @"controller init injected failure"
    );
    IIMATestInitializationException(
      IIMAFakeSparkleThrowClearFeed,
      @"clear-feed injected failure"
    );
    IIMATestReceiveBetaException();
    IIMATestAutomaticChecksException();
    IIMATestCheckIntervalException();
    IIMATestManualCheckException();
    IIMATestStatusException();

    IIMAResetHarness();
    puts("Native updater Objective-C exception-boundary checks passed");
  }
  return 0;
}
