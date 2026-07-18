#import <Cocoa/Cocoa.h>
#import <ColorSync/ColorSync.h>
#import <CoreVideo/CoreVideo.h>
#import <OpenGL/OpenGL.h>
#import <OpenGL/gl3.h>
#import <QuartzCore/CAOpenGLLayer.h>
#import <dlfcn.h>
#import <limits.h>
#import <math.h>
#import <stdatomic.h>
#import <string.h>

#import <mpv/client.h>
#import <mpv/render.h>
#import <mpv/render_gl.h>

typedef int (*iima_mpv_render_context_create_fn)(mpv_render_context **, mpv_handle *, mpv_render_param *);
typedef void (*iima_mpv_render_context_set_update_callback_fn)(mpv_render_context *, mpv_render_update_fn, void *);
typedef uint64_t (*iima_mpv_render_context_update_fn)(mpv_render_context *);
typedef void (*iima_mpv_render_context_render_fn)(mpv_render_context *, mpv_render_param *);
typedef void (*iima_mpv_render_context_report_swap_fn)(mpv_render_context *);
typedef void (*iima_mpv_render_context_free_fn)(mpv_render_context *);
typedef int (*iima_mpv_set_property_string_fn)(mpv_handle *, const char *, const char *);
typedef char *(*iima_mpv_get_property_string_fn)(mpv_handle *, const char *);
typedef void (*iima_mpv_free_fn)(void *);
typedef CFDictionaryRef (*iima_core_display_create_info_dictionary_fn)(CGDirectDisplayID);
typedef void (*iima_native_video_pip_will_close_callback_fn)(const char *);

typedef NS_ENUM(int, IIMAHdrColorSpaceKind) {
  IIMAHdrColorSpaceUnsupported = 0,
  IIMAHdrColorSpaceDisplayP3PQ = 1,
  IIMAHdrColorSpaceDisplayP3PQEOTF = 2,
  IIMAHdrColorSpaceITUR2100PQ = 3,
  IIMAHdrColorSpaceITUR2020PQ = 4,
  IIMAHdrColorSpaceITUR2020PQEOTF = 5,
};

typedef NS_ENUM(int, IIMANativeVideoRenderScheduler) {
  IIMANativeVideoRenderSchedulerUnavailable = 0,
  IIMANativeVideoRenderSchedulerDisplayLink = 1,
  IIMANativeVideoRenderSchedulerAppKitInvalidation = 2,
};

enum {
  IIMANativeVideoInstallInvalidHost = -1,
  IIMANativeVideoInstallViewCreationFailed = -2,
  IIMANativeVideoInstallParentNotReady = -3,
};

static int iima_native_video_install_parent_result(NSWindow *parent) {
  return parent == nil ? IIMANativeVideoInstallParentNotReady : 0;
}

static BOOL iima_native_video_display_link_stage_failed(CVReturn result, BOOL resourceAvailable) {
  return result != kCVReturnSuccess || !resourceAvailable;
}

int iima_native_video_test_install_parent_result(int parentAvailable) {
  return parentAvailable ? 0 : IIMANativeVideoInstallParentNotReady;
}

int iima_native_video_test_render_scheduler(
  int createResult,
  int displayLinkCreated,
  int callbackResult,
  int startResult
) {
  if (iima_native_video_display_link_stage_failed(
        (CVReturn)createResult,
        displayLinkCreated != 0
      ) || callbackResult != kCVReturnSuccess || startResult != kCVReturnSuccess) {
    return IIMANativeVideoRenderSchedulerAppKitInvalidation;
  }
  return IIMANativeVideoRenderSchedulerDisplayLink;
}

typedef struct {
  void *library;
  iima_mpv_render_context_create_fn create;
  iima_mpv_render_context_set_update_callback_fn set_update_callback;
  iima_mpv_render_context_update_fn update;
  iima_mpv_render_context_render_fn render;
  iima_mpv_render_context_report_swap_fn report_swap;
  iima_mpv_render_context_free_fn free_context;
  iima_mpv_set_property_string_fn set_property_string;
  iima_mpv_get_property_string_fn get_property_string;
  iima_mpv_free_fn free_mpv;
} IIMAMpvRenderApi;

static volatile int iima_native_video_pip_active = 0;
static volatile int iima_native_video_pip_closing = 0;
static iima_native_video_pip_will_close_callback_fn iima_native_video_pip_will_close_callback = NULL;

static BOOL iima_os_version_at_least(
  int major,
  int minor,
  int patch,
  int requiredMajor,
  int requiredMinor,
  int requiredPatch
) {
  if (major != requiredMajor) {
    return major > requiredMajor;
  }
  if (minor != requiredMinor) {
    return minor > requiredMinor;
  }
  return patch >= requiredPatch;
}

int iima_native_video_hdr_color_space_kind(
  const char *primaries,
  int macMajor,
  int macMinor,
  int macPatch
) {
  if (primaries == NULL) {
    return IIMAHdrColorSpaceUnsupported;
  }
  if (strcmp(primaries, "display-p3") == 0) {
    return iima_os_version_at_least(macMajor, macMinor, macPatch, 10, 15, 4)
      ? IIMAHdrColorSpaceDisplayP3PQ
      : IIMAHdrColorSpaceDisplayP3PQEOTF;
  }
  if (strcmp(primaries, "bt.2020") == 0) {
    if (iima_os_version_at_least(macMajor, macMinor, macPatch, 11, 0, 0)) {
      return IIMAHdrColorSpaceITUR2100PQ;
    }
    return iima_os_version_at_least(macMajor, macMinor, macPatch, 10, 15, 4)
      ? IIMAHdrColorSpaceITUR2020PQ
      : IIMAHdrColorSpaceITUR2020PQEOTF;
  }
  return IIMAHdrColorSpaceUnsupported;
}

int iima_native_video_resolve_target_peak(
  int configuredPeak,
  int referencePeakHdrLuminance,
  int displayBacklight
) {
  if (configuredPeak > 0) {
    return configuredPeak;
  }
  if (referencePeakHdrLuminance > 0) {
    return referencePeakHdrLuminance;
  }
  if (displayBacklight > 0) {
    return displayBacklight;
  }
  return 400;
}

static CGColorSpaceRef iima_srgb_color_space(void) {
  static CGColorSpaceRef colorSpace = NULL;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    colorSpace = CGColorSpaceCreateDeviceRGB();
  });
  return colorSpace;
}

#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
static CGColorSpaceRef iima_create_hdr_color_space(IIMAHdrColorSpaceKind kind) {
  switch (kind) {
    case IIMAHdrColorSpaceDisplayP3PQ:
      if (@available(macOS 10.15.4, *)) {
        return CGColorSpaceCreateWithName(kCGColorSpaceDisplayP3_PQ);
      }
      break;
    case IIMAHdrColorSpaceDisplayP3PQEOTF:
      if (@available(macOS 10.15, *)) {
        return CGColorSpaceCreateWithName(kCGColorSpaceDisplayP3_PQ_EOTF);
      }
      break;
    case IIMAHdrColorSpaceITUR2100PQ:
      if (@available(macOS 11.0, *)) {
        return CGColorSpaceCreateWithName(kCGColorSpaceITUR_2100_PQ);
      }
      break;
    case IIMAHdrColorSpaceITUR2020PQ:
      if (@available(macOS 10.15.4, *)) {
        return CGColorSpaceCreateWithName(kCGColorSpaceITUR_2020_PQ);
      }
      break;
    case IIMAHdrColorSpaceITUR2020PQEOTF:
      if (@available(macOS 10.15, *)) {
        return CGColorSpaceCreateWithName(kCGColorSpaceITUR_2020_PQ_EOTF);
      }
      break;
    case IIMAHdrColorSpaceUnsupported:
      break;
  }
  return NULL;
}
#pragma clang diagnostic pop

static iima_core_display_create_info_dictionary_fn iima_core_display_info_function(void) {
  static iima_core_display_create_info_dictionary_fn function = NULL;
  static void *coreDisplay = NULL;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    coreDisplay = dlopen(
      "/System/Library/Frameworks/CoreDisplay.framework/CoreDisplay",
      RTLD_LAZY | RTLD_LOCAL
    );
    if (coreDisplay != NULL) {
      function = (iima_core_display_create_info_dictionary_fn)dlsym(
        coreDisplay,
        "CoreDisplay_DisplayCreateInfoDictionary"
      );
    }
  });
  return function;
}

static int iima_positive_display_luminance(NSDictionary *displayInfo, NSString *key) {
  id value = displayInfo[key];
  if (![value isKindOfClass:[NSNumber class]]) {
    return 0;
  }
  long long luminance = [value longLongValue];
  return luminance > 0 && luminance <= INT_MAX ? (int)luminance : 0;
}

static int iima_discovered_target_peak(CGDirectDisplayID display) {
  iima_core_display_create_info_dictionary_fn createInfo = iima_core_display_info_function();
  if (createInfo == NULL) {
    NSLog(@"IIMA could not load CoreDisplay display-info lookup; assuming HDR400");
    return 400;
  }
  CFDictionaryRef rawDisplayInfo = createInfo(display);
  if (rawDisplayInfo == NULL) {
    NSLog(@"IIMA could not obtain display information; assuming HDR400");
    return 400;
  }
  NSDictionary *displayInfo = CFBridgingRelease(rawDisplayInfo);
  int referencePeak = iima_positive_display_luminance(
    displayInfo,
    @"ReferencePeakHDRLuminance"
  );
  int displayBacklight = iima_positive_display_luminance(displayInfo, @"DisplayBacklight");
  int peak = iima_native_video_resolve_target_peak(0, referencePeak, displayBacklight);
  if (referencePeak > 0) {
    NSLog(@"IIMA found ReferencePeakHDRLuminance: %d", referencePeak);
  } else if (displayBacklight > 0) {
    NSLog(@"IIMA found DisplayBacklight: %d", displayBacklight);
  } else {
    NSLog(@"IIMA display info has no peak luminance; assuming HDR400: %@", displayInfo);
  }
  return peak;
}

typedef struct {
  CFUUIDRef displayUUID;
  CFURLRef profileURL;
} IIMAColorSyncProfileLookup;

static bool iima_find_current_display_profile(CFDictionaryRef profileInfo, void *context) {
  IIMAColorSyncProfileLookup *lookup = context;
  if (lookup == NULL || profileInfo == NULL) {
    return true;
  }
  const void *current = CFDictionaryGetValue(profileInfo, CFSTR("DeviceProfileIsCurrent"));
  const void *deviceID = CFDictionaryGetValue(profileInfo, CFSTR("DeviceID"));
  const void *profileURL = CFDictionaryGetValue(profileInfo, CFSTR("DeviceProfileURL"));
  if (current != kCFBooleanTrue || deviceID == NULL || profileURL == NULL || !CFEqual(deviceID, lookup->displayUUID)) {
    return true;
  }
  if (CFGetTypeID(profileURL) != CFURLGetTypeID()) {
    return true;
  }
  lookup->profileURL = CFRetain(profileURL);
  return false;
}

static void *iima_gl_get_proc_address(void *context, const char *name) {
  (void)context;
  return dlsym(RTLD_DEFAULT, name);
}

@interface IIMANativeVideoView : NSOpenGLView {
@private
  mpv_handle *_mpvHandle;
  mpv_render_context *_renderContext;
  IIMAMpvRenderApi _api;
  CVDisplayLinkRef _displayLink;
  CGDirectDisplayID _currentDisplay;
  BOOL _hasCurrentDisplay;
  NSTimer *_displayIdleTimer;
  id _screenChangeObserver;
  id _screenParametersObserver;
  id _screenColorSpaceObserver;
  BOOL _loadIccProfile;
  BOOL _enableHdrSupport;
  BOOL _enableToneMapping;
  int _toneMappingTargetPeak;
  NSString *_toneMappingAlgorithm;
  IIMAHdrColorSpaceKind _activeColorSpaceKind;
  BOOL _extendedDynamicRangeContentEnabled;
  atomic_bool _needsRender;
  atomic_bool _forceRender;
  atomic_bool _colorRefreshRequested;
  atomic_bool _hdrAvailable;
  IIMANativeVideoRenderScheduler _renderScheduler;
}
- (instancetype)initWithFrame:(NSRect)frame forceDedicatedGPU:(BOOL)forceDedicatedGPU;
- (int)attachMpvHandle:(mpv_handle *)handle libraryPath:(const char *)libraryPath;
- (void)detachMpvClient;
- (void)requestRender;
- (BOOL)hasRenderRequest;
- (BOOL)activateDisplayLink;
- (void)releaseDisplayLink;
- (void)useAppKitInvalidationFallbackForStage:(NSString *)stage code:(CVReturn)code;
- (void)stopDisplayLink;
- (void)scheduleDisplayLinkIdle;
- (void)updateDisplayLink;
- (void)removeScreenObservers;
- (void)applyOutputColorSpace:(CGColorSpaceRef)colorSpace
                         kind:(IIMAHdrColorSpaceKind)kind
       extendedDynamicRange:(BOOL)extendedDynamicRange;
- (void)configureColorLoadICC:(BOOL)loadICC
                    enableHDR:(BOOL)enableHDR
             enableToneMapping:(BOOL)enableToneMapping
         toneMappingTargetPeak:(int)toneMappingTargetPeak
         toneMappingAlgorithm:(const char *)toneMappingAlgorithm;
- (void)setHdrEnabled:(BOOL)enabled;
- (void)requestColorRefresh;
- (void)setPlaybackPaused:(BOOL)paused;
- (BOOL)hasAttachedMpvClient;
- (IIMANativeVideoRenderScheduler)renderScheduler;
- (BOOL)hdrAvailable;
- (BOOL)hdrEnabled;
@end

static NSMutableDictionary<NSString *, IIMANativeVideoView *> *iima_native_video_views = nil;
static NSMutableDictionary<NSString *, NSView *> *iima_native_video_hosts = nil;
static NSMutableDictionary<NSString *, NSWindow *> *iima_native_video_windows = nil;
static NSMutableDictionary<NSString *, NSArray<id> *> *iima_native_video_window_observers = nil;
static NSMutableDictionary<NSString *, NSNumber *> *iima_native_video_frame_retry_attempts = nil;
static NSMutableDictionary<NSString *, NSNumber *> *iima_native_video_frame_update_generations = nil;
static NSMutableDictionary<NSString *, NSNumber *> *iima_native_video_live_frame_update_generations = nil;
static NSMutableSet<NSString *> *iima_native_video_live_frame_updates = nil;
static NSMutableSet<NSString *> *iima_native_video_live_resize_sessions = nil;
static NSMutableSet<NSString *> *iima_native_video_suspended_live_frame_updates = nil;
static NSMutableSet<NSString *> *iima_native_video_force_surface_updates = nil;
static atomic_bool iima_native_video_sessions_initialized = ATOMIC_VAR_INIT(false);
static uint64_t iima_native_video_frame_update_generation = 0;
static NSViewController *iima_native_video_pip_content_controller = nil;
static id iima_native_video_pip_controller = nil;
static id iima_native_video_pip_delegate = nil;
static NSString *iima_native_video_pip_session = nil;
static NSSize iima_native_video_pip_display_size = {0, 0};
static int iima_native_video_pip_origin_fullscreen = 0;

static void iima_native_video_restore_from_pip(void);
static void iima_native_video_prepare_for_pip_closure(id pip);

static NSString *iima_native_video_session_key(const char *sessionLabel) {
  if (sessionLabel == NULL || sessionLabel[0] == '\0') {
    return @"main";
  }
  NSString *key = [NSString stringWithUTF8String:sessionLabel];
  return key.length > 0 ? key : @"main";
}

static void iima_native_video_run_on_main_sync(dispatch_block_t block) {
  if ([NSThread isMainThread]) {
    block();
  } else {
    dispatch_sync(dispatch_get_main_queue(), block);
  }
}

static void iima_native_video_assert_main_thread(void) {
  NSCAssert(
    [NSThread isMainThread],
    @"native video AppKit/session state must be accessed on the main thread"
  );
}

static BOOL iima_native_video_sessions_are_initialized(void) {
  return atomic_load(&iima_native_video_sessions_initialized);
}

static void iima_native_video_ensure_sessions(void) {
  iima_native_video_assert_main_thread();
  if (iima_native_video_views == nil) {
    iima_native_video_views = [[NSMutableDictionary alloc] init];
    iima_native_video_hosts = [[NSMutableDictionary alloc] init];
    iima_native_video_windows = [[NSMutableDictionary alloc] init];
    iima_native_video_window_observers = [[NSMutableDictionary alloc] init];
    iima_native_video_frame_retry_attempts = [[NSMutableDictionary alloc] init];
    iima_native_video_frame_update_generations = [[NSMutableDictionary alloc] init];
    iima_native_video_live_frame_update_generations = [[NSMutableDictionary alloc] init];
    iima_native_video_live_frame_updates = [[NSMutableSet alloc] init];
    iima_native_video_live_resize_sessions = [[NSMutableSet alloc] init];
    iima_native_video_suspended_live_frame_updates = [[NSMutableSet alloc] init];
    iima_native_video_force_surface_updates = [[NSMutableSet alloc] init];
    atomic_store(&iima_native_video_sessions_initialized, true);
  }
}

void iima_native_video_test_initialize_empty_sessions(void) {
  iima_native_video_run_on_main_sync(^{
    iima_native_video_ensure_sessions();
  });
}

static void iima_native_video_remove_window_observers(NSString *session) {
  iima_native_video_assert_main_thread();
  NSArray<id> *observers = iima_native_video_window_observers[session];
  for (id observer in observers) {
    [[NSNotificationCenter defaultCenter] removeObserver:observer];
  }
  [iima_native_video_window_observers removeObjectForKey:session];
}

static NSTimeInterval iima_native_video_frame_retry_delay(NSUInteger attempt) {
  // Animated NSWindow resizing normally settles within the three coalesced retries below. Keep
  // the first retry close enough to avoid a visible stale frame, then back off so a transient
  // WindowServer color-space transition has time to finish.
  switch (attempt) {
    case 1:
      return 0.05;
    case 2:
      return 0.10;
    default:
      return 0.20;
  }
}

static NSTimeInterval iima_native_video_frame_update_quiet_period(void) {
  // At 60 Hz, 75 ms is four to five notification-free frames: long enough for AppKit's animated
  // resize and screen/color-space transition burst to settle, while keeping the final video
  // alignment below the 100 ms interaction-response threshold.
  static const NSTimeInterval quietPeriod = 0.075;
  return quietPeriod;
}

static CGFloat iima_native_video_corner_radius_for_style_mask(NSWindowStyleMask styleMask) {
  BOOL fullscreen = (styleMask & NSWindowStyleMaskFullScreen) != 0;
  BOOL titled = (styleMask & NSWindowStyleMaskTitled) != 0;
  return fullscreen || !titled ? 0.0 : 10.0;
}

static void iima_native_video_sync_window_shape(NSWindow *videoWindow, NSWindow *parent) {
  if (videoWindow == nil || parent == nil) {
    return;
  }
  NSView *contentView = videoWindow.contentView;
  NSView *frameView = contentView.superview ?: contentView;
  if (frameView == nil) {
    return;
  }

  // The video surface lives in a separate borderless NSWindow so WKWebView chrome can composite
  // above it. That child does not inherit the titled parent's WindowServer corner mask. Keep the
  // mask on the outer AppKit frame layer instead of IIMANativeVideoView's CAOpenGLLayer: the latter
  // remains the sole owner of ICC/HDR/EDR output state.
  frameView.wantsLayer = YES;
  CALayer *frameLayer = frameView.layer;
  if (frameLayer == nil) {
    return;
  }
  frameLayer.backgroundColor = NSColor.blackColor.CGColor;
  frameLayer.cornerRadius = iima_native_video_corner_radius_for_style_mask(parent.styleMask);
  frameLayer.masksToBounds = YES;
  if (@available(macOS 10.15, *)) {
    frameLayer.cornerCurve = kCACornerCurveContinuous;
  }
  [videoWindow invalidateShadow];
}

static uint64_t iima_native_video_next_frame_update_generation(void) {
  // Use a process-wide monotonic token so a delayed block from a removed session can never match
  // a newly created session that happens to reuse the same label.
  iima_native_video_frame_update_generation += 1;
  if (iima_native_video_frame_update_generation == 0) {
    iima_native_video_frame_update_generation = 1;
  }
  return iima_native_video_frame_update_generation;
}

static BOOL iima_native_video_apply_window_frame(NSString *session, BOOL forceSurfaceUpdate) {
  iima_native_video_assert_main_thread();
  NSView *host = iima_native_video_hosts[session];
  NSWindow *parent = host.window;
  NSWindow *videoWindow = iima_native_video_windows[session];
  if (parent == nil || videoWindow == nil) {
    return YES;
  }

  @try {
    iima_native_video_sync_window_shape(videoWindow, parent);
    NSRect targetFrame = parent.frame;
    IIMANativeVideoView *view = iima_native_video_views[session];
    if (!NSEqualRects(videoWindow.frame, targetFrame)) {
      // Geometry changes already invalidate the OpenGL view hierarchy. Avoid asking AppKit to
      // synchronously display that hierarchy from setFrame: as well; mpv's renderer owns the
      // following paint and requestRender schedules it on the normal render path.
      [videoWindow setFrame:targetFrame display:NO];
      [view requestRender];
    } else if (forceSurfaceUpdate) {
      // NSWindow may commit its frame before NSOpenGLView finishes updating the underlying CGS
      // surface. If that update throws, a later frame comparison alone cannot distinguish the
      // partial commit from success. Explicitly refresh the context on the delayed retry.
      NSOpenGLContext *context = [view openGLContext];
      [context update];
      [view requestRender];
    }
    if (videoWindow.parentWindow != parent) {
      if (videoWindow.parentWindow != nil) {
        [videoWindow.parentWindow removeChildWindow:videoWindow];
      }
      [parent addChildWindow:videoWindow ordered:NSWindowBelow];
    }
    if (parent.isVisible) {
      [videoWindow orderWindow:NSWindowBelow relativeTo:parent.windowNumber];
    }
    return YES;
  } @catch (NSException *exception) {
    // AppKit can raise NSInternalInconsistencyException here while an animated resize transitions
    // an NSOpenGLView surface between color spaces (CGSSetSurfaceColorSpace). Never allow an
    // Objective-C exception to escape into Rust; retry after the animation advances instead.
    NSLog(
      @"IIMA native video frame update deferred after %@: %@",
      exception.name,
      exception.reason ?: @"unknown AppKit exception"
    );
    return NO;
  }
}

static BOOL iima_native_video_apply_live_window_frame(NSString *session) {
  iima_native_video_assert_main_thread();
  NSView *host = iima_native_video_hosts[session];
  NSWindow *parent = host.window;
  NSWindow *videoWindow = iima_native_video_windows[session];
  if (parent == nil || videoWindow == nil) {
    return YES;
  }

  @try {
    NSRect targetFrame = parent.frame;
    IIMANativeVideoView *view = iima_native_video_views[session];
    if (!NSEqualRects(videoWindow.frame, targetFrame)) {
      // Live resize must follow every AppKit event. Restrict the hot path to geometry and render
      // invalidation; shape, shadow, hierarchy, and OpenGL recovery work are finalized once the
      // resize session ends.
      [videoWindow setFrame:targetFrame display:NO];
      [view requestRender];
    }
    return YES;
  } @catch (NSException *exception) {
    NSLog(
      @"IIMA native video live frame update deferred after %@: %@",
      exception.name,
      exception.reason ?: @"unknown AppKit exception"
    );
    return NO;
  }
}

static void iima_native_video_schedule_window_frame_retry(NSString *session, NSUInteger attempt) {
  iima_native_video_assert_main_thread();
  static const NSUInteger maxAttempts = 3;
  if (attempt > maxAttempts || iima_native_video_frame_retry_attempts[session] != nil) {
    return;
  }
  NSString *sessionKey = [session copy];
  iima_native_video_frame_retry_attempts[sessionKey] = @(attempt);
  dispatch_after(
    dispatch_time(
      DISPATCH_TIME_NOW,
      (int64_t)(iima_native_video_frame_retry_delay(attempt) * (double)NSEC_PER_SEC)
    ),
    dispatch_get_main_queue(),
    ^{
      NSNumber *pendingAttempt = iima_native_video_frame_retry_attempts[sessionKey];
      if (pendingAttempt == nil || pendingAttempt.unsignedIntegerValue != attempt) {
        return;
      }
      [iima_native_video_frame_retry_attempts removeObjectForKey:sessionKey];
      if (!iima_native_video_apply_window_frame(sessionKey, YES)) {
        if (attempt < maxAttempts) {
          iima_native_video_schedule_window_frame_retry(sessionKey, attempt + 1);
        } else {
          if ([iima_native_video_live_resize_sessions containsObject:sessionKey]) {
            // A persistent WindowServer failure must not restart a fresh retry ladder on every
            // subsequent mouse event. Suspend the fast path for the rest of this live-resize
            // session; DidEndLiveResize performs one final bounded reconciliation.
            [iima_native_video_suspended_live_frame_updates addObject:sessionKey];
          }
          NSLog(
            @"IIMA native video frame update abandoned after %lu retries for session %@",
            (unsigned long)maxAttempts,
            sessionKey
          );
        }
      }
    }
  );
}

static void iima_native_video_update_window_frame(NSString *session) {
  iima_native_video_assert_main_thread();
  // A quiet-period debounce or delayed recovery owns the next surface mutation. In particular,
  // lifecycle calls must not bypass a pending post-animation update with a synchronous one.
  if (iima_native_video_frame_update_generations[session] != nil
      || iima_native_video_frame_retry_attempts[session] != nil) {
    return;
  }
  if (!iima_native_video_apply_window_frame(session, NO)) {
    iima_native_video_schedule_window_frame_retry(session, 1);
  }
}

static void iima_native_video_cancel_delayed_frame_updates(
  NSString *session,
  BOOL preserveSurfaceRefresh
) {
  iima_native_video_assert_main_thread();
  [iima_native_video_frame_update_generations removeObjectForKey:session];
  if (iima_native_video_frame_retry_attempts[session] != nil) {
    [iima_native_video_frame_retry_attempts removeObjectForKey:session];
    if (preserveSurfaceRefresh) {
      [iima_native_video_force_surface_updates addObject:session];
    }
  }
}

static void iima_native_video_schedule_live_window_frame_update(NSString *session) {
  iima_native_video_assert_main_thread();
  NSString *sessionKey = [session copy];
  // A live-resize event supersedes only the trailing quiet-period generation. If an exception
  // recovery is already pending, let its 50/100/200 ms backoff run against the newest parent
  // frame instead of resetting attempt one at mouse-event frequency.
  [iima_native_video_frame_update_generations removeObjectForKey:sessionKey];
  if (iima_native_video_frame_retry_attempts[sessionKey] != nil
      || [iima_native_video_suspended_live_frame_updates containsObject:sessionKey]
      || [iima_native_video_live_frame_updates containsObject:sessionKey]) {
    return;
  }

  // Coalesce duplicate notifications from the same AppKit turn, but never wait for the mouse to
  // stop. The next main-queue turn observes the newest parent frame and keeps the child surface
  // visually attached throughout the drag.
  uint64_t generation = iima_native_video_next_frame_update_generation();
  iima_native_video_live_frame_update_generations[sessionKey] = @(generation);
  [iima_native_video_live_frame_updates addObject:sessionKey];
  dispatch_async(dispatch_get_main_queue(), ^{
    NSNumber *pendingGeneration =
      iima_native_video_live_frame_update_generations[sessionKey];
    if (pendingGeneration == nil || pendingGeneration.unsignedLongLongValue != generation) {
      return;
    }
    [iima_native_video_live_frame_update_generations removeObjectForKey:sessionKey];
    [iima_native_video_live_frame_updates removeObject:sessionKey];
    if (![iima_native_video_live_resize_sessions containsObject:sessionKey]) {
      return;
    }
    if (!iima_native_video_apply_live_window_frame(sessionKey)) {
      [iima_native_video_force_surface_updates addObject:sessionKey];
      iima_native_video_schedule_window_frame_retry(sessionKey, 1);
    }
  });
}

static void iima_native_video_schedule_final_window_frame_update(NSString *session) {
  iima_native_video_assert_main_thread();
  NSString *sessionKey = [session copy];
  iima_native_video_cancel_delayed_frame_updates(sessionKey, YES);
  uint64_t generation = iima_native_video_next_frame_update_generation();
  iima_native_video_frame_update_generations[sessionKey] = @(generation);
  dispatch_async(dispatch_get_main_queue(), ^{
    NSNumber *pendingGeneration = iima_native_video_frame_update_generations[sessionKey];
    if (pendingGeneration == nil || pendingGeneration.unsignedLongLongValue != generation) {
      return;
    }
    [iima_native_video_frame_update_generations removeObjectForKey:sessionKey];
    [iima_native_video_force_surface_updates removeObject:sessionKey];
    if (!iima_native_video_apply_window_frame(sessionKey, YES)) {
      iima_native_video_schedule_window_frame_retry(sessionKey, 1);
    }
  });
}

static void iima_native_video_begin_live_resize(NSString *session) {
  iima_native_video_assert_main_thread();
  NSString *sessionKey = [session copy];
  [iima_native_video_live_resize_sessions addObject:sessionKey];
  [iima_native_video_suspended_live_frame_updates removeObject:sessionKey];
  iima_native_video_schedule_live_window_frame_update(sessionKey);
}

static void iima_native_video_end_live_resize(NSString *session) {
  iima_native_video_assert_main_thread();
  NSString *sessionKey = [session copy];
  [iima_native_video_live_resize_sessions removeObject:sessionKey];
  [iima_native_video_suspended_live_frame_updates removeObject:sessionKey];
  // Invalidate a queued hot-path block before scheduling the one full post-resize reconciliation.
  [iima_native_video_live_frame_update_generations removeObjectForKey:sessionKey];
  [iima_native_video_live_frame_updates removeObject:sessionKey];
  [iima_native_video_force_surface_updates addObject:sessionKey];
  iima_native_video_schedule_final_window_frame_update(sessionKey);
}

static void iima_native_video_schedule_window_frame_update(NSString *session) {
  iima_native_video_assert_main_thread();
  // NSWindow notifications are delivered from inside AppKit's move/resize machinery. Every event
  // advances the generation and restarts the quiet period, so neither the notification stack nor
  // an intermediate main-queue turn can mutate the child OpenGL surface.
  NSString *sessionKey = [session copy];
  uint64_t generation = iima_native_video_next_frame_update_generation();
  iima_native_video_frame_update_generations[sessionKey] = @(generation);

  // A parent-window event supersedes a recovery scheduled against an earlier animation frame.
  // Preserve the stronger surface refresh for a possible partial AppKit commit, but invalidate
  // the old retry token; the quiet-period update becomes the sole owner of the next mutation.
  if (iima_native_video_frame_retry_attempts[sessionKey] != nil) {
    [iima_native_video_frame_retry_attempts removeObjectForKey:sessionKey];
    [iima_native_video_force_surface_updates addObject:sessionKey];
  }

  dispatch_after(
    dispatch_time(
      DISPATCH_TIME_NOW,
      (int64_t)(iima_native_video_frame_update_quiet_period() * (double)NSEC_PER_SEC)
    ),
    dispatch_get_main_queue(),
    ^{
      NSNumber *pendingGeneration = iima_native_video_frame_update_generations[sessionKey];
      if (pendingGeneration == nil || pendingGeneration.unsignedLongLongValue != generation) {
        return;
      }
      [iima_native_video_frame_update_generations removeObjectForKey:sessionKey];
      BOOL forceSurfaceUpdate =
        [iima_native_video_force_surface_updates containsObject:sessionKey];
      [iima_native_video_force_surface_updates removeObject:sessionKey];
      if (!iima_native_video_apply_window_frame(sessionKey, forceSurfaceUpdate)) {
        iima_native_video_schedule_window_frame_retry(sessionKey, 1);
      }
    }
  );
}

static void iima_native_video_observe_parent_window(NSString *session) {
  iima_native_video_assert_main_thread();
  iima_native_video_remove_window_observers(session);
  NSWindow *parent = iima_native_video_hosts[session].window;
  if (parent == nil) {
    return;
  }
  NSString *sessionKey = [session copy];
  NSMutableArray<id> *observers = [[NSMutableArray alloc] initWithCapacity:7];
  id liveResizeStartObserver = [[NSNotificationCenter defaultCenter]
    addObserverForName:NSWindowWillStartLiveResizeNotification
                object:parent
                 queue:[NSOperationQueue mainQueue]
            usingBlock:^(__unused NSNotification *notification) {
              iima_native_video_begin_live_resize(sessionKey);
            }];
  [observers addObject:liveResizeStartObserver];

  id liveResizeEndObserver = [[NSNotificationCenter defaultCenter]
    addObserverForName:NSWindowDidEndLiveResizeNotification
                object:parent
                 queue:[NSOperationQueue mainQueue]
            usingBlock:^(__unused NSNotification *notification) {
              iima_native_video_end_live_resize(sessionKey);
            }];
  [observers addObject:liveResizeEndObserver];

  NSArray<NSNotificationName> *geometryNames = @[
    NSWindowDidMoveNotification,
    NSWindowDidResizeNotification,
  ];
  for (NSNotificationName name in geometryNames) {
    id observer = [[NSNotificationCenter defaultCenter]
      addObserverForName:name
                  object:parent
                   queue:[NSOperationQueue mainQueue]
              usingBlock:^(__unused NSNotification *notification) {
                if ([iima_native_video_live_resize_sessions containsObject:sessionKey]) {
                  iima_native_video_schedule_live_window_frame_update(sessionKey);
                } else {
                  iima_native_video_schedule_window_frame_update(sessionKey);
                }
              }];
    [observers addObject:observer];
  }

  NSArray<NSNotificationName> *transitionNames = @[
    NSWindowDidEnterFullScreenNotification,
    NSWindowDidExitFullScreenNotification,
    NSWindowDidChangeScreenNotification,
  ];
  for (NSNotificationName name in transitionNames) {
    id observer = [[NSNotificationCenter defaultCenter]
      addObserverForName:name
                  object:parent
                   queue:[NSOperationQueue mainQueue]
              usingBlock:^(__unused NSNotification *notification) {
                iima_native_video_schedule_window_frame_update(sessionKey);
              }];
    [observers addObject:observer];
  }
  iima_native_video_window_observers[session] = observers;
}

static IIMANativeVideoView *iima_native_video_view_for_session(NSString *session) {
  iima_native_video_assert_main_thread();
  return iima_native_video_views[session];
}

@interface IIMANativeVideoPIPDelegate : NSObject
@end

static void iima_mpv_update_callback(void *context) {
  IIMANativeVideoView *view = (__bridge IIMANativeVideoView *)context;
  [view requestRender];
}

#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"

static CVReturn iima_display_link_callback(
  CVDisplayLinkRef displayLink,
  const CVTimeStamp *now,
  const CVTimeStamp *outputTime,
  CVOptionFlags flagsIn,
  CVOptionFlags *flagsOut,
  void *context
) {
  (void)displayLink;
  (void)now;
  (void)outputTime;
  (void)flagsIn;
  (void)flagsOut;
  IIMANativeVideoView *view = (__bridge IIMANativeVideoView *)context;
  if ([view hasRenderRequest]) {
    dispatch_async(dispatch_get_main_queue(), ^{
      [view setNeedsDisplay:YES];
    });
  }
  return kCVReturnSuccess;
}

@implementation IIMANativeVideoView

- (instancetype)initWithFrame:(NSRect)frame forceDedicatedGPU:(BOOL)forceDedicatedGPU {
  NSOpenGLPixelFormatAttribute requestedAttributes[10] = {
    NSOpenGLPFADoubleBuffer,
    NSOpenGLPFAAllowOfflineRenderers,
    NSOpenGLPFAColorFloat,
    NSOpenGLPFAColorSize, 64,
    NSOpenGLPFAOpenGLProfile, NSOpenGLProfileVersion3_2Core,
    NSOpenGLPFAAccelerated,
  };
  NSInteger attributeCount = 8;
  if (!forceDedicatedGPU) {
    requestedAttributes[attributeCount++] =
      (NSOpenGLPixelFormatAttribute)kCGLPFASupportsAutomaticGraphicsSwitching;
  }

  NSOpenGLPixelFormat *format = nil;
  for (NSInteger length = attributeCount; length > 0; length--) {
    // Two zero slots keep even a fallback ending in a value-taking attribute safely terminated.
    NSOpenGLPixelFormatAttribute attributes[16] = {0};
    memcpy(attributes, requestedAttributes, sizeof(NSOpenGLPixelFormatAttribute) * length);
    format = [[NSOpenGLPixelFormat alloc] initWithAttributes:attributes];
    if (format != nil) {
      break;
    }
  }
  if (format == nil) {
    return nil;
  }
  self = [super initWithFrame:frame pixelFormat:format];
  if (self) {
    self.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    self.wantsLayer = YES;
    self.wantsBestResolutionOpenGLSurface = YES;
    // Match IINA's float OpenGL surface: the backing layer decides whether current content is EDR.
    self.wantsExtendedDynamicRangeOpenGLSurface = YES;
    self.hidden = YES;
    _loadIccProfile = YES;
    _enableHdrSupport = YES;
    _enableToneMapping = NO;
    _toneMappingTargetPeak = 0;
    _toneMappingAlgorithm = @"auto";
    _activeColorSpaceKind = (IIMAHdrColorSpaceKind)-1;
    _extendedDynamicRangeContentEnabled = NO;
    atomic_init(&_needsRender, false);
    atomic_init(&_forceRender, false);
    atomic_init(&_colorRefreshRequested, true);
    atomic_init(&_hdrAvailable, false);
    _renderScheduler = IIMANativeVideoRenderSchedulerUnavailable;
    [self applyOutputColorSpace:iima_srgb_color_space()
                           kind:IIMAHdrColorSpaceUnsupported
         extendedDynamicRange:NO];
  }
  return self;
}

- (BOOL)isOpaque {
  return YES;
}

- (void)prepareOpenGL {
  [super prepareOpenGL];
  GLint swapInterval = 1;
  [[self openGLContext] setValues:&swapInterval forParameter:NSOpenGLContextParameterSwapInterval];
}

- (void)viewDidMoveToWindow {
  [super viewDidMoveToWindow];
  [self removeScreenObservers];
  if (self.window == nil) {
    return;
  }

  _activeColorSpaceKind = (IIMAHdrColorSpaceKind)-1;
  __weak IIMANativeVideoView *weakSelf = self;
  NSNotificationCenter *notificationCenter = [NSNotificationCenter defaultCenter];
  _screenChangeObserver = [notificationCenter
    addObserverForName:NSWindowDidChangeScreenNotification
                object:self.window
                 queue:[NSOperationQueue mainQueue]
            usingBlock:^(__unused NSNotification *notification) {
              [weakSelf updateDisplayLink];
            }];
  _screenParametersObserver = [notificationCenter
    addObserverForName:NSApplicationDidChangeScreenParametersNotification
                object:nil
                 queue:[NSOperationQueue mainQueue]
            usingBlock:^(__unused NSNotification *notification) {
              [weakSelf updateDisplayLink];
              [weakSelf requestColorRefresh];
            }];
  _screenColorSpaceObserver = [notificationCenter
    addObserverForName:NSScreenColorSpaceDidChangeNotification
                object:nil
                 queue:[NSOperationQueue mainQueue]
            usingBlock:^(NSNotification *notification) {
              IIMANativeVideoView *view = weakSelf;
              if (view != nil && (notification.object == nil || notification.object == view.window.screen)) {
                [view requestColorRefresh];
              }
            }];
  [self updateDisplayLink];
  // Moving the shared view between the player, Mini Player, and PIP can change NSWindow while the
  // display id stays the same. Reapply the window/layer color contract even in that case.
  [self requestColorRefresh];
}

- (void)removeScreenObservers {
  NSNotificationCenter *notificationCenter = [NSNotificationCenter defaultCenter];
  for (id observer in @[_screenChangeObserver ?: NSNull.null,
                         _screenParametersObserver ?: NSNull.null,
                         _screenColorSpaceObserver ?: NSNull.null]) {
    if (observer != NSNull.null) {
      [notificationCenter removeObserver:observer];
    }
  }
  _screenChangeObserver = nil;
  _screenParametersObserver = nil;
  _screenColorSpaceObserver = nil;
}

- (void)applyOutputColorSpace:(CGColorSpaceRef)colorSpace
                         kind:(IIMAHdrColorSpaceKind)kind
       extendedDynamicRange:(BOOL)extendedDynamicRange {
  if (colorSpace == NULL) {
    return;
  }

  CALayer *layer = self.layer;
  if (self.window.screen != nil) {
    layer.contentsScale = self.window.screen.backingScaleFactor;
  }
  if (_activeColorSpaceKind == kind
      && _extendedDynamicRangeContentEnabled == extendedDynamicRange) {
    return;
  }

  // `NSOpenGLView` uses an OpenGL-capable backing layer on supported AppKit versions. Keep the
  // selector check because AppKit owns the concrete layer class, while preserving IINA's exact
  // CAOpenGLLayer color-space/EDR contract whenever that surface is available.
  if ([layer respondsToSelector:@selector(setColorspace:)]) {
    CAOpenGLLayer *openGLLayer = (CAOpenGLLayer *)layer;
    openGLLayer.colorspace = colorSpace;
  }
  if ([layer respondsToSelector:@selector(setWantsExtendedDynamicRangeContent:)]) {
    // CAOpenGLLayer exposed this contract before the equivalent property became public on CALayer.
    // Dynamic dispatch also covers AppKit's private NSOpenGLView backing-layer implementation.
    [(CAOpenGLLayer *)layer setWantsExtendedDynamicRangeContent:extendedDynamicRange];
  }

  // Match IINA 1.3.5's VideoView contract: the OpenGL layer owns the output color space. Writing
  // NSWindow.colorSpace here asks WindowServer to mutate every child surface during animated
  // geometry changes and can raise CGSSetSurfaceColorSpace while the NSOpenGLView is resizing.
  _activeColorSpaceKind = kind;
  _extendedDynamicRangeContentEnabled = extendedDynamicRange;
}

- (void)updateDisplayLink {
  if (_displayLink == NULL || self.window == nil || self.window.screen == nil) {
    return;
  }
  NSNumber *screenNumber = self.window.screen.deviceDescription[@"NSScreenNumber"];
  if (screenNumber == nil) {
    return;
  }
  CGDirectDisplayID display = screenNumber.unsignedIntValue;
  if (_hasCurrentDisplay && _currentDisplay == display) {
    return;
  }
  CVReturn result = CVDisplayLinkSetCurrentCGDisplay(_displayLink, display);
  if (result != kCVReturnSuccess) {
    NSLog(@"IIMA could not bind display link to display %u: %d", display, result);
    return;
  }
  _currentDisplay = display;
  _hasCurrentDisplay = YES;
  _activeColorSpaceKind = (IIMAHdrColorSpaceKind)-1;

  CVTime nominalPeriod = CVDisplayLinkGetNominalOutputVideoRefreshPeriod(_displayLink);
  double refreshRate = 60.0;
  if ((nominalPeriod.flags & kCVTimeIsIndefinite) == 0 && nominalPeriod.timeValue > 0 && nominalPeriod.timeScale > 0) {
    double nominalRate = (double)nominalPeriod.timeScale / (double)nominalPeriod.timeValue;
    double actualPeriod = CVDisplayLinkGetActualOutputVideoRefreshPeriod(_displayLink);
    double actualRate = actualPeriod > 0.0 ? 1.0 / actualPeriod : 0.0;
    double delta = actualRate - nominalRate;
    if (delta < 0.0) {
      delta = -delta;
    }
    refreshRate = actualRate > 0.0 && delta <= 1.0 ? actualRate : nominalRate;
  }
  if (_mpvHandle != NULL && _api.set_property_string != NULL) {
    NSString *value = [NSString stringWithFormat:@"%.6f", refreshRate];
    int setResult = _api.set_property_string(_mpvHandle, "override-display-fps", value.UTF8String);
    if (setResult < 0) {
      NSLog(@"IIMA could not set override-display-fps: %d", setResult);
    }
  }
  atomic_store(&_colorRefreshRequested, true);
  [self requestRender];
}

- (BOOL)activateDisplayLink {
  [_displayIdleTimer invalidate];
  _displayIdleTimer = nil;
  if (_displayLink == NULL) {
    return NO;
  }
  [self updateDisplayLink];
  if (CVDisplayLinkIsRunning(_displayLink)) {
    _renderScheduler = IIMANativeVideoRenderSchedulerDisplayLink;
    return YES;
  }
  CVReturn result = CVDisplayLinkStart(_displayLink);
  if (result != kCVReturnSuccess) {
    [self useAppKitInvalidationFallbackForStage:@"start" code:result];
    return NO;
  }
  _renderScheduler = IIMANativeVideoRenderSchedulerDisplayLink;
  return YES;
}

- (void)releaseDisplayLink {
  if (_displayLink != NULL) {
    if (CVDisplayLinkIsRunning(_displayLink)) {
      CVDisplayLinkStop(_displayLink);
    }
    CVDisplayLinkRelease(_displayLink);
    _displayLink = NULL;
  }
  _currentDisplay = 0;
  _hasCurrentDisplay = NO;
}

- (void)useAppKitInvalidationFallbackForStage:(NSString *)stage code:(CVReturn)code {
  [self releaseDisplayLink];
  _renderScheduler = IIMANativeVideoRenderSchedulerAppKitInvalidation;
  NSLog(
    @"IIMA native video display-link %@ failed (%d); using AppKit invalidation fallback",
    stage,
    code
  );
}

- (void)stopDisplayLink {
  [_displayIdleTimer invalidate];
  _displayIdleTimer = nil;
  if (_displayLink != NULL && CVDisplayLinkIsRunning(_displayLink)) {
    CVReturn result = CVDisplayLinkStop(_displayLink);
    if (result != kCVReturnSuccess) {
      NSLog(@"IIMA could not stop display link: %d", result);
    }
  }
}

- (void)scheduleDisplayLinkIdle {
  [_displayIdleTimer invalidate];
  _displayIdleTimer = [NSTimer scheduledTimerWithTimeInterval:6.0
                                                         target:self
                                                       selector:@selector(stopDisplayLink)
                                                       userInfo:nil
                                                        repeats:NO];
}

- (void)setMpvString:(const char *)name value:(NSString *)value {
  if (_mpvHandle == NULL || _api.set_property_string == NULL || name == NULL || value == nil) {
    return;
  }
  int result = _api.set_property_string(_mpvHandle, name, value.UTF8String);
  if (result < 0) {
    NSLog(@"IIMA could not set mpv property %s: %d", name, result);
  }
}

- (NSString *)mpvStringProperty:(const char *)name {
  if (_mpvHandle == NULL || _api.get_property_string == NULL || _api.free_mpv == NULL || name == NULL) {
    return nil;
  }
  char *rawValue = _api.get_property_string(_mpvHandle, name);
  if (rawValue == NULL) {
    return nil;
  }
  NSString *value = [[NSString alloc] initWithUTF8String:rawValue];
  _api.free_mpv(rawValue);
  return value;
}

- (void)applyIccProfileForCurrentDisplay {
  [self applyOutputColorSpace:iima_srgb_color_space()
                           kind:IIMAHdrColorSpaceUnsupported
         extendedDynamicRange:NO];
  if (!_loadIccProfile) {
    [self setMpvString:"icc-profile" value:@""];
    return;
  }
  if (!_hasCurrentDisplay) {
    return;
  }
  CFUUIDRef displayUUID = CGDisplayCreateUUIDFromDisplayID(_currentDisplay);
  if (displayUUID == NULL) {
    return;
  }
  IIMAColorSyncProfileLookup lookup = { .displayUUID = displayUUID, .profileURL = NULL };
  ColorSyncIterateDeviceProfiles(iima_find_current_display_profile, &lookup);
  CFRelease(displayUUID);
  if (lookup.profileURL == NULL) {
    return;
  }
  NSURL *profileURL = CFBridgingRelease(lookup.profileURL);
  if (profileURL.path.length > 0 && [[NSFileManager defaultManager] fileExistsAtPath:profileURL.path]) {
    [self setMpvString:"icc-profile" value:profileURL.path];
  }
}

- (BOOL)refreshColorPipeline {
  NSString *primaries = [self mpvStringProperty:"video-params/primaries"];
  NSString *gamma = [self mpvStringProperty:"video-params/gamma"];
  if (primaries.length == 0 || gamma.length == 0) {
    atomic_store(&_hdrAvailable, false);
    return NO;
  }

  NSOperatingSystemVersion version = NSProcessInfo.processInfo.operatingSystemVersion;
  IIMAHdrColorSpaceKind colorSpaceKind = (IIMAHdrColorSpaceKind)iima_native_video_hdr_color_space_kind(
    primaries.UTF8String,
    (int)version.majorVersion,
    (int)version.minorVersion,
    (int)version.patchVersion
  );
  BOOL hdrSource = colorSpaceKind != IIMAHdrColorSpaceUnsupported;
  BOOL edrAvailable = NO;
  if (@available(macOS 10.15, *)) {
    edrAvailable = self.window.screen.maximumPotentialExtendedDynamicRangeColorComponentValue > 1.0;
  }
  atomic_store(&_hdrAvailable, hdrSource && edrAvailable);
  if (hdrSource && _enableHdrSupport && edrAvailable) {
    CGColorSpaceRef colorSpace = iima_create_hdr_color_space(colorSpaceKind);
    if (colorSpace == NULL) {
      NSLog(@"IIMA could not create HDR output color space for primaries %@", primaries);
      [self applyIccProfileForCurrentDisplay];
      return NO;
    }
    [self applyOutputColorSpace:colorSpace
                           kind:colorSpaceKind
         extendedDynamicRange:YES];
    CGColorSpaceRelease(colorSpace);
    [self setMpvString:"icc-profile" value:@""];
    [self setMpvString:"target-prim" value:primaries];
    [self setMpvString:"target-trc" value:@"pq"];
    [self setMpvString:"screenshot-tag-colorspace" value:@"yes"];
    if (_enableToneMapping) {
      int discoveredPeak = 0;
      if (_toneMappingTargetPeak <= 0) {
        discoveredPeak = _hasCurrentDisplay ? iima_discovered_target_peak(_currentDisplay) : 400;
      }
      int targetPeak = iima_native_video_resolve_target_peak(
        _toneMappingTargetPeak,
        discoveredPeak,
        0
      );
      NSString *peak = [NSString stringWithFormat:@"%d", targetPeak];
      [self setMpvString:"target-peak" value:peak];
      [self setMpvString:"tone-mapping" value:_toneMappingAlgorithm ?: @"auto"];
    } else {
      [self setMpvString:"target-peak" value:@"auto"];
      [self setMpvString:"tone-mapping" value:@""];
    }
    NSLog(@"IIMA enabled HDR output for primaries %@ gamma %@", primaries, gamma);
    return YES;
  }

  [self applyIccProfileForCurrentDisplay];
  [self setMpvString:"target-trc" value:@"auto"];
  [self setMpvString:"target-prim" value:@"auto"];
  [self setMpvString:"target-peak" value:@"auto"];
  [self setMpvString:"tone-mapping" value:@"auto"];
  [self setMpvString:"tone-mapping-param" value:@"default"];
  [self setMpvString:"screenshot-tag-colorspace" value:@"no"];
  return YES;
}

- (void)configureColorLoadICC:(BOOL)loadICC
                    enableHDR:(BOOL)enableHDR
             enableToneMapping:(BOOL)enableToneMapping
         toneMappingTargetPeak:(int)toneMappingTargetPeak
         toneMappingAlgorithm:(const char *)toneMappingAlgorithm {
  _loadIccProfile = loadICC;
  _enableHdrSupport = enableHDR;
  _enableToneMapping = enableToneMapping;
  _toneMappingTargetPeak = MAX(0, toneMappingTargetPeak);
  _toneMappingAlgorithm = toneMappingAlgorithm == NULL
    ? @"auto"
    : [[NSString alloc] initWithUTF8String:toneMappingAlgorithm];
  if (_toneMappingAlgorithm.length == 0) {
    _toneMappingAlgorithm = @"auto";
  }
  [self requestColorRefresh];
}

- (void)setHdrEnabled:(BOOL)enabled {
  _enableHdrSupport = enabled;
  [self requestColorRefresh];
}

- (void)requestColorRefresh {
  atomic_store(&_colorRefreshRequested, true);
  atomic_store(&_forceRender, true);
  [self requestRender];
}

- (void)setPlaybackPaused:(BOOL)paused {
  if (_mpvHandle == NULL || _api.set_property_string == NULL) {
    return;
  }
  _api.set_property_string(_mpvHandle, "pause", paused ? "yes" : "no");
}

- (int)attachMpvHandle:(mpv_handle *)handle libraryPath:(const char *)libraryPath {
  [self detachMpvClient];
  if (handle == NULL || libraryPath == NULL || [self openGLContext] == nil) {
    return -1;
  }
  _api.library = dlopen(libraryPath, RTLD_NOW | RTLD_LOCAL);
  if (_api.library == NULL) {
    return -2;
  }
  _api.create = (iima_mpv_render_context_create_fn)dlsym(_api.library, "mpv_render_context_create");
  _api.set_update_callback = (iima_mpv_render_context_set_update_callback_fn)dlsym(_api.library, "mpv_render_context_set_update_callback");
  _api.update = (iima_mpv_render_context_update_fn)dlsym(_api.library, "mpv_render_context_update");
  _api.render = (iima_mpv_render_context_render_fn)dlsym(_api.library, "mpv_render_context_render");
  _api.report_swap = (iima_mpv_render_context_report_swap_fn)dlsym(_api.library, "mpv_render_context_report_swap");
  _api.free_context = (iima_mpv_render_context_free_fn)dlsym(_api.library, "mpv_render_context_free");
  _api.set_property_string = (iima_mpv_set_property_string_fn)dlsym(_api.library, "mpv_set_property_string");
  _api.get_property_string = (iima_mpv_get_property_string_fn)dlsym(_api.library, "mpv_get_property_string");
  _api.free_mpv = (iima_mpv_free_fn)dlsym(_api.library, "mpv_free");
  if (_api.create == NULL || _api.set_update_callback == NULL || _api.update == NULL || _api.render == NULL || _api.report_swap == NULL || _api.free_context == NULL || _api.set_property_string == NULL || _api.get_property_string == NULL || _api.free_mpv == NULL) {
    [self detachMpvClient];
    return -3;
  }

  [[self openGLContext] makeCurrentContext];
  mpv_opengl_init_params initParams = {
    .get_proc_address = iima_gl_get_proc_address,
    .get_proc_address_ctx = NULL,
  };
  mpv_render_param parameters[] = {
    { MPV_RENDER_PARAM_API_TYPE, (void *)MPV_RENDER_API_TYPE_OPENGL },
    { MPV_RENDER_PARAM_OPENGL_INIT_PARAMS, &initParams },
    { MPV_RENDER_PARAM_INVALID, NULL },
  };
  int result = _api.create(&_renderContext, handle, parameters);
  if (result < 0 || _renderContext == NULL) {
    [self detachMpvClient];
    NSLog(@"IIMA native libmpv render context creation failed: %d", result);
    return result < 0 ? result : -4;
  }
  _api.set_update_callback(_renderContext, iima_mpv_update_callback, (__bridge void *)self);
  _mpvHandle = handle;
  if (_displayLink == NULL) {
    CVReturn displayLinkResult = CVDisplayLinkCreateWithActiveCGDisplays(&_displayLink);
    if (iima_native_video_display_link_stage_failed(
          displayLinkResult,
          _displayLink != NULL
        )) {
      [self useAppKitInvalidationFallbackForStage:@"create" code:displayLinkResult];
    } else {
      CVReturn callbackResult = CVDisplayLinkSetOutputCallback(
        _displayLink,
        iima_display_link_callback,
        (__bridge void *)self
      );
      if (callbackResult != kCVReturnSuccess) {
        [self useAppKitInvalidationFallbackForStage:@"callback" code:callbackResult];
      }
    }
  }
  if (_displayLink != NULL) {
    // A start failure switches to the same AppKit invalidation path used when creation or callback
    // registration is unavailable. `requestRender` already schedules `setNeedsDisplay:` on the
    // main queue whenever no display link is running, so keeping the valid mpv render context is
    // both deterministic and sufficient for frame delivery.
    [self activateDisplayLink];
  }
  NSLog(
    @"IIMA native libmpv render context attached (%@)",
    _renderScheduler == IIMANativeVideoRenderSchedulerDisplayLink
      ? @"display-link"
      : @"appkit-invalidation"
  );
  self.hidden = NO;
  atomic_store(&_forceRender, true);
  [self requestColorRefresh];
  return 0;
}

- (void)detachMpvClient {
  [_displayIdleTimer invalidate];
  _displayIdleTimer = nil;
  [self releaseDisplayLink];
  _renderScheduler = IIMANativeVideoRenderSchedulerUnavailable;
  _activeColorSpaceKind = (IIMAHdrColorSpaceKind)-1;
  if (_renderContext != NULL) {
    [[self openGLContext] makeCurrentContext];
    _api.set_update_callback(_renderContext, NULL, NULL);
    _api.free_context(_renderContext);
    _renderContext = NULL;
  }
  _mpvHandle = NULL;
  if (_api.library != NULL) {
    dlclose(_api.library);
  }
  _api = (IIMAMpvRenderApi){0};
  atomic_store(&_hdrAvailable, false);
  self.hidden = YES;
  NSLog(@"IIMA native libmpv render context detached");
}

- (void)requestRender {
  atomic_store(&_needsRender, true);
  if (_displayLink == NULL || !CVDisplayLinkIsRunning(_displayLink)) {
    dispatch_async(dispatch_get_main_queue(), ^{
      [self activateDisplayLink];
      [self setNeedsDisplay:YES];
    });
  }
}

#pragma clang diagnostic pop

- (BOOL)hasRenderRequest {
  return atomic_load(&_needsRender) || atomic_load(&_forceRender);
}

- (BOOL)hasAttachedMpvClient {
  return _mpvHandle != NULL && _renderContext != NULL;
}

- (IIMANativeVideoRenderScheduler)renderScheduler {
  return [self hasAttachedMpvClient]
    ? _renderScheduler
    : IIMANativeVideoRenderSchedulerUnavailable;
}

- (BOOL)hdrAvailable {
  return atomic_load(&_hdrAvailable);
}

- (BOOL)hdrEnabled {
  return _enableHdrSupport;
}

- (void)reshape {
  [super reshape];
  atomic_store(&_forceRender, true);
  [self requestRender];
}

- (void)drawRect:(NSRect)dirtyRect {
  (void)dirtyRect;
  [[self openGLContext] makeCurrentContext];
  if (_renderContext == NULL) {
    glClearColor(0.0f, 0.0f, 0.0f, 1.0f);
    glClear(GL_COLOR_BUFFER_BIT);
    [[self openGLContext] flushBuffer];
    return;
  }

  bool requested = atomic_exchange(&_needsRender, false);
  bool forceRender = atomic_exchange(&_forceRender, false);
  if (!requested && !forceRender) {
    return;
  }

  uint64_t updates = _api.update(_renderContext);
  if ((updates & MPV_RENDER_UPDATE_FRAME) || forceRender) {
    if (atomic_exchange(&_colorRefreshRequested, false) && ![self refreshColorPipeline]) {
      atomic_store(&_colorRefreshRequested, true);
    }
    glClearColor(0.0f, 0.0f, 0.0f, 1.0f);
    glClear(GL_COLOR_BUFFER_BIT);
    GLint viewport[4] = {0, 0, 0, 0};
    glGetIntegerv(GL_VIEWPORT, viewport);
    if (viewport[2] > 0 && viewport[3] > 0) {
      mpv_opengl_fbo fbo = { .fbo = 0, .w = viewport[2], .h = viewport[3], .internal_format = 0 };
      int flip = 1;
      mpv_render_param parameters[] = {
        { MPV_RENDER_PARAM_OPENGL_FBO, &fbo },
        { MPV_RENDER_PARAM_FLIP_Y, &flip },
        { MPV_RENDER_PARAM_INVALID, NULL },
      };
      _api.render(_renderContext, parameters);
      _api.report_swap(_renderContext);
    }
    [self scheduleDisplayLinkIdle];
  }
  [[self openGLContext] flushBuffer];
}

- (void)dealloc {
  [self removeScreenObservers];
  [self detachMpvClient];
}

@end

@implementation IIMANativeVideoPIPDelegate

- (BOOL)pipShouldClose:(id)pip {
  iima_native_video_prepare_for_pip_closure(pip);
  return YES;
}

- (void)pipWillClose:(id)pip {
  iima_native_video_prepare_for_pip_closure(pip);
}

- (void)pipDidClose:(id)pip {
  (void)pip;
  iima_native_video_restore_from_pip();
}

- (void)pipActionPlay:(id)pip {
  (void)pip;
  [iima_native_video_view_for_session(iima_native_video_pip_session) setPlaybackPaused:NO];
}

- (void)pipActionPause:(id)pip {
  (void)pip;
  [iima_native_video_view_for_session(iima_native_video_pip_session) setPlaybackPaused:YES];
}

- (void)pipActionStop:(id)pip {
  (void)pip;
  [iima_native_video_view_for_session(iima_native_video_pip_session) setPlaybackPaused:YES];
}

@end

static BOOL iima_native_video_load_pip_framework(void) {
  NSBundle *framework = [NSBundle bundleWithPath:@"/System/Library/PrivateFrameworks/PIP.framework"];
  if (framework == nil) {
    return NO;
  }
  if (![framework isLoaded] && ![framework load]) {
    return NO;
  }
  Class pipClass = NSClassFromString(@"PIPViewController");
  SEL presentSelector = NSSelectorFromString(@"presentViewControllerAsPictureInPicture:");
  return pipClass != Nil && [pipClass instancesRespondToSelector:presentSelector];
}

int iima_native_video_plan_pip_replacement_rect(double containerWidth,
                                                 double containerHeight,
                                                 double videoWidth,
                                                 double videoHeight,
                                                 double *values) {
  if (values == NULL || !isfinite(containerWidth) || !isfinite(containerHeight) ||
      !isfinite(videoWidth) || !isfinite(videoHeight) ||
      containerWidth <= 0 || containerHeight <= 0 || videoWidth <= 0 || videoHeight <= 0) {
    return -1;
  }
  double aspect = videoWidth / videoHeight;
  double width = containerWidth;
  double height = width / aspect;
  if (height > containerHeight) {
    height = containerHeight;
    width = height * aspect;
  }
  values[0] = (containerWidth - width) / 2.0;
  values[1] = (containerHeight - height) / 2.0;
  values[2] = width;
  values[3] = height;
  return 0;
}

static void iima_native_video_prepare_for_pip_closure(id pip) {
  iima_native_video_assert_main_thread();
  if (!iima_native_video_pip_active || iima_native_video_pip_closing || pip == nil) {
    return;
  }
  NSView *host = iima_native_video_hosts[iima_native_video_pip_session];
  NSWindow *parent = host.window;
  if (parent == nil) {
    return;
  }
  iima_native_video_pip_closing = 1;
  if (iima_native_video_pip_will_close_callback != NULL) {
    iima_native_video_pip_will_close_callback(iima_native_video_pip_session.UTF8String);
  }
  [pip setValue:parent forKey:@"replacementWindow"];
  NSRect replacementRect = parent.contentView.frame;
  if (iima_native_video_pip_origin_fullscreen) {
    double values[4] = {0};
    if (iima_native_video_plan_pip_replacement_rect(parent.frame.size.width,
                                                     parent.frame.size.height,
                                                     iima_native_video_pip_display_size.width,
                                                     iima_native_video_pip_display_size.height,
                                                     values) == 0) {
      replacementRect = NSMakeRect(values[0], values[1], values[2], values[3]);
    }
  }
  [pip setValue:[NSValue valueWithRect:replacementRect] forKey:@"replacementRect"];
  [NSApp activateIgnoringOtherApps:YES];
  [parent deminiaturize:pip];
  [parent makeKeyAndOrderFront:pip];
}

static void iima_native_video_restore_from_pip(void) {
  iima_native_video_assert_main_thread();
  if (!iima_native_video_pip_active) {
    return;
  }
  IIMANativeVideoView *view = iima_native_video_view_for_session(iima_native_video_pip_session);
  NSWindow *videoWindow = iima_native_video_windows[iima_native_video_pip_session];
  if (view != nil && videoWindow != nil) {
    [view removeFromSuperview];
    [videoWindow setContentView:view];
    iima_native_video_sync_window_shape(
      videoWindow, iima_native_video_hosts[iima_native_video_pip_session].window);
    iima_native_video_update_window_frame(iima_native_video_pip_session);
    [view updateDisplayLink];
    [view requestColorRefresh];
  }
  iima_native_video_pip_session = nil;
  iima_native_video_pip_display_size = NSZeroSize;
  iima_native_video_pip_origin_fullscreen = 0;
  iima_native_video_pip_active = 0;
  iima_native_video_pip_closing = 0;
  dispatch_async(dispatch_get_main_queue(), ^{
    if (!iima_native_video_pip_active) {
      iima_native_video_pip_content_controller = nil;
      iima_native_video_pip_controller = nil;
      iima_native_video_pip_delegate = nil;
    }
  });
}

int iima_native_video_install(void *hostView, const char *sessionLabel, int forceDedicatedGPU) {
  if (hostView == NULL) {
    return IIMANativeVideoInstallInvalidHost;
  }
  __block int result = 0;
  void (^install)(void) = ^{
    iima_native_video_ensure_sessions();
    NSString *session = iima_native_video_session_key(sessionLabel);
    NSView *host = (__bridge NSView *)hostView;
    iima_native_video_hosts[session] = host;
    NSWindow *parent = host.window;
    int parentResult = iima_native_video_install_parent_result(parent);
    if (parentResult != 0) {
      [iima_native_video_hosts removeObjectForKey:session];
      result = parentResult;
      return;
    }
    // A transparent parent casts a separate shadow for every opaque WebView control. Let the
    // rounded, nonopaque video child own one shadow for the combined player surface.
    parent.hasShadow = NO;
    IIMANativeVideoView *view = iima_native_video_view_for_session(session);
    if (view == nil) {
      view = [[IIMANativeVideoView alloc]
        initWithFrame:host.bounds forceDedicatedGPU:forceDedicatedGPU != 0];
      if (view == nil) {
        [iima_native_video_hosts removeObjectForKey:session];
        result = IIMANativeVideoInstallViewCreationFailed;
        return;
      }
      iima_native_video_views[session] = view;
    }
    NSWindow *videoWindow = iima_native_video_windows[session];
    if (videoWindow == nil) {
      videoWindow = [[NSWindow alloc]
        initWithContentRect:parent.frame
                  styleMask:NSWindowStyleMaskBorderless
                    backing:NSBackingStoreBuffered
                      defer:NO];
      videoWindow.backgroundColor = NSColor.clearColor;
      videoWindow.opaque = NO;
      videoWindow.hasShadow = YES;
      videoWindow.ignoresMouseEvents = YES;
      videoWindow.releasedWhenClosed = NO;
      videoWindow.animationBehavior = NSWindowAnimationBehaviorNone;
      iima_native_video_windows[session] = videoWindow;
    }
    if (!iima_native_video_pip_active || ![iima_native_video_pip_session isEqualToString:session]) {
      if (videoWindow.contentView != view) {
        [view removeFromSuperview];
        [videoWindow setContentView:view];
      }
      iima_native_video_sync_window_shape(videoWindow, parent);
      [view updateDisplayLink];
      [view requestColorRefresh];
    }
    iima_native_video_observe_parent_window(session);
    iima_native_video_update_window_frame(session);
  };
  if ([NSThread isMainThread]) {
    install();
  } else {
    dispatch_sync(dispatch_get_main_queue(), install);
  }
  return result;
}

int iima_native_video_attach_mpv_client(void *mpvHandle, const char *libraryPath, const char *sessionLabel) {
  if (!iima_native_video_sessions_are_initialized()) {
    return -1;
  }
  NSString *session = [iima_native_video_session_key(sessionLabel) copy];
  __block int result = -1;
  void (^attach)(void) = ^{
    IIMANativeVideoView *view = iima_native_video_view_for_session(session);
    if (view == nil) {
      return;
    }
    result = [view attachMpvHandle:(mpv_handle *)mpvHandle libraryPath:libraryPath];
  };
  iima_native_video_run_on_main_sync(attach);
  return result;
}

void iima_native_video_detach_mpv_client(const char *sessionLabel) {
  if (!iima_native_video_sessions_are_initialized()) {
    return;
  }
  NSString *session = [iima_native_video_session_key(sessionLabel) copy];
  void (^detach)(void) = ^{
    IIMANativeVideoView *view = iima_native_video_view_for_session(session);
    [view detachMpvClient];
  };
  iima_native_video_run_on_main_sync(detach);
}

void iima_native_video_remove_session(const char *sessionLabel) {
  if (!iima_native_video_sessions_are_initialized()) {
    return;
  }
  NSString *session = iima_native_video_session_key(sessionLabel);
  void (^remove)(void) = ^{
    if (iima_native_video_pip_active && [iima_native_video_pip_session isEqualToString:session]) {
      iima_native_video_restore_from_pip();
    }
    // Invalidate queued blocks before detaching/closing AppKit objects. A stale main-queue block
    // then observes the missing token even if window teardown spins a nested event cycle.
    [iima_native_video_frame_retry_attempts removeObjectForKey:session];
    [iima_native_video_frame_update_generations removeObjectForKey:session];
    [iima_native_video_live_frame_update_generations removeObjectForKey:session];
    [iima_native_video_live_frame_updates removeObject:session];
    [iima_native_video_live_resize_sessions removeObject:session];
    [iima_native_video_suspended_live_frame_updates removeObject:session];
    [iima_native_video_force_surface_updates removeObject:session];
    IIMANativeVideoView *view = iima_native_video_view_for_session(session);
    [view detachMpvClient];
    [view removeFromSuperview];
    iima_native_video_remove_window_observers(session);
    NSWindow *videoWindow = iima_native_video_windows[session];
    if (videoWindow.parentWindow != nil) {
      [videoWindow.parentWindow removeChildWindow:videoWindow];
    }
    [videoWindow orderOut:nil];
    [videoWindow close];
    [iima_native_video_views removeObjectForKey:session];
    [iima_native_video_hosts removeObjectForKey:session];
    [iima_native_video_windows removeObjectForKey:session];
  };
  if ([NSThread isMainThread]) {
    remove();
  } else {
    dispatch_sync(dispatch_get_main_queue(), remove);
  }
}

void iima_native_video_configure_color(
  const char *sessionLabel,
  int loadIccProfile,
  int enableHdrSupport,
  int enableToneMapping,
  int toneMappingTargetPeak,
  const char *toneMappingAlgorithm
) {
  if (!iima_native_video_sessions_are_initialized()) {
    return;
  }
  NSString *session = [iima_native_video_session_key(sessionLabel) copy];
  void (^configure)(void) = ^{
    IIMANativeVideoView *view = iima_native_video_view_for_session(session);
    if (view == nil) {
      return;
    }
    [view configureColorLoadICC:loadIccProfile != 0
                                        enableHDR:enableHdrSupport != 0
                                 enableToneMapping:enableToneMapping != 0
                             toneMappingTargetPeak:toneMappingTargetPeak
                             toneMappingAlgorithm:toneMappingAlgorithm];
  };
  iima_native_video_run_on_main_sync(configure);
}

void iima_native_video_request_color_refresh(const char *sessionLabel) {
  if (!iima_native_video_sessions_are_initialized()) {
    return;
  }
  NSString *session = [iima_native_video_session_key(sessionLabel) copy];
  void (^refresh)(void) = ^{
    IIMANativeVideoView *view = iima_native_video_view_for_session(session);
    [view requestColorRefresh];
  };
  if ([NSThread isMainThread]) {
    refresh();
  } else {
    dispatch_async(dispatch_get_main_queue(), refresh);
  }
}

void iima_native_video_set_hdr_enabled(const char *sessionLabel, int enabled) {
  if (!iima_native_video_sessions_are_initialized()) {
    return;
  }
  NSString *session = [iima_native_video_session_key(sessionLabel) copy];
  void (^setHdrEnabled)(void) = ^{
    IIMANativeVideoView *view = iima_native_video_view_for_session(session);
    [view setHdrEnabled:enabled != 0];
  };
  iima_native_video_run_on_main_sync(setHdrEnabled);
}

int iima_native_video_toggle_pip(const char *sessionLabel,
                                 int playing,
                                 const char *title,
                                 double videoWidth,
                                 double videoHeight,
                                 int originFullscreen) {
  if (!iima_native_video_sessions_are_initialized()) {
    return -1;
  }
  NSString *session = [iima_native_video_session_key(sessionLabel) copy];
  __block int result = -1;
  void (^toggle)(void) = ^{
    IIMANativeVideoView *view = iima_native_video_view_for_session(session);
    NSWindow *videoWindow = iima_native_video_windows[session];
    if (view == nil || iima_native_video_hosts[session] == nil || videoWindow == nil) {
      return;
    }
    result = 0;
    if (iima_native_video_pip_active) {
      SEL dismissSelector = NSSelectorFromString(@"dismissViewController:");
      if ([iima_native_video_pip_controller respondsToSelector:dismissSelector]) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
        [iima_native_video_pip_controller performSelector:dismissSelector
                                               withObject:iima_native_video_pip_content_controller];
#pragma clang diagnostic pop
      } else {
        iima_native_video_prepare_for_pip_closure(iima_native_video_pip_controller);
        iima_native_video_restore_from_pip();
      }
      return;
    }
    if (!iima_native_video_load_pip_framework()) {
      result = -2;
      return;
    }
    Class pipClass = NSClassFromString(@"PIPViewController");
    SEL presentSelector = NSSelectorFromString(@"presentViewControllerAsPictureInPicture:");
    id pip = [[pipClass alloc] init];
    IIMANativeVideoPIPDelegate *delegate = [[IIMANativeVideoPIPDelegate alloc] init];
    NSViewController *contentController = [[NSViewController alloc] init];
    NSView *placeholder = [[NSView alloc] initWithFrame:videoWindow.contentView.bounds];
    placeholder.wantsLayer = YES;
    placeholder.layer.backgroundColor = NSColor.blackColor.CGColor;
    [videoWindow setContentView:placeholder];
    iima_native_video_sync_window_shape(
      videoWindow, iima_native_video_hosts[session].window);
    contentController.view = view;
    [pip setValue:delegate forKey:@"delegate"];
    [pip setValue:@(playing != 0) forKey:@"playing"];
    NSSize displaySize = isfinite(videoWidth) && isfinite(videoHeight) &&
                         videoWidth > 0 && videoHeight > 0
      ? NSMakeSize(videoWidth, videoHeight)
      : view.frame.size;
    [pip setValue:[NSValue valueWithSize:displaySize] forKey:@"aspectRatio"];
    if (title != NULL && title[0] != '\0') {
      [pip setValue:[NSString stringWithUTF8String:title] forKey:@"name"];
    }
    iima_native_video_pip_content_controller = contentController;
    iima_native_video_pip_controller = pip;
    iima_native_video_pip_delegate = delegate;
    iima_native_video_pip_session = session;
    iima_native_video_pip_display_size = displaySize;
    iima_native_video_pip_origin_fullscreen = originFullscreen != 0;
    iima_native_video_pip_closing = 0;
    iima_native_video_pip_active = 1;
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
    [pip performSelector:presentSelector withObject:contentController];
#pragma clang diagnostic pop
    [view updateDisplayLink];
    [view requestColorRefresh];
  };
  iima_native_video_run_on_main_sync(toggle);
  return result;
}

int iima_native_video_pip_is_available(void) {
  if (!iima_native_video_sessions_are_initialized()) {
    return 0;
  }
  __block int result = 0;
  iima_native_video_run_on_main_sync(^{
    result = iima_native_video_load_pip_framework() ? 1 : 0;
  });
  return result;
}

int iima_native_video_pip_is_active(void) {
  if (!iima_native_video_sessions_are_initialized()) {
    return 0;
  }
  __block int result = 0;
  iima_native_video_run_on_main_sync(^{
    result = iima_native_video_pip_active ? 1 : 0;
  });
  return result;
}

int iima_native_video_pip_is_active_for_session(const char *sessionLabel) {
  if (!iima_native_video_sessions_are_initialized()) {
    return 0;
  }
  NSString *session = [iima_native_video_session_key(sessionLabel) copy];
  __block int result = 0;
  iima_native_video_run_on_main_sync(^{
    result = iima_native_video_pip_active
      && [iima_native_video_pip_session isEqualToString:session] ? 1 : 0;
  });
  return result;
}

void iima_native_video_set_pip_will_close_callback(
  iima_native_video_pip_will_close_callback_fn callback
) {
  void (^setCallback)(void) = ^{
    iima_native_video_pip_will_close_callback = callback;
  };
  if ([NSThread isMainThread]) {
    setCallback();
  } else {
    dispatch_sync(dispatch_get_main_queue(), setCallback);
  }
}

int iima_native_video_hdr_is_available(const char *sessionLabel) {
  if (!iima_native_video_sessions_are_initialized()) {
    return 0;
  }
  NSString *session = [iima_native_video_session_key(sessionLabel) copy];
  __block int result = 0;
  iima_native_video_run_on_main_sync(^{
    result = [iima_native_video_view_for_session(session) hdrAvailable] ? 1 : 0;
  });
  return result;
}

int iima_native_video_hdr_is_enabled(const char *sessionLabel) {
  if (!iima_native_video_sessions_are_initialized()) {
    return 0;
  }
  NSString *session = [iima_native_video_session_key(sessionLabel) copy];
  __block int result = 0;
  iima_native_video_run_on_main_sync(^{
    result = [iima_native_video_view_for_session(session) hdrEnabled] ? 1 : 0;
  });
  return result;
}

int iima_native_video_is_installed(const char *sessionLabel) {
  if (!iima_native_video_sessions_are_initialized()) {
    return 0;
  }
  NSString *session = [iima_native_video_session_key(sessionLabel) copy];
  __block int result = 0;
  iima_native_video_run_on_main_sync(^{
    result = iima_native_video_view_for_session(session) != nil ? 1 : 0;
  });
  return result;
}

int iima_native_video_is_attached(const char *sessionLabel) {
  if (!iima_native_video_sessions_are_initialized()) {
    return 0;
  }
  NSString *session = [iima_native_video_session_key(sessionLabel) copy];
  __block int result = 0;
  iima_native_video_run_on_main_sync(^{
    result = [iima_native_video_view_for_session(session) hasAttachedMpvClient] ? 1 : 0;
  });
  return result;
}

int iima_native_video_render_scheduler(const char *sessionLabel) {
  if (!iima_native_video_sessions_are_initialized()) {
    return IIMANativeVideoRenderSchedulerUnavailable;
  }
  NSString *session = [iima_native_video_session_key(sessionLabel) copy];
  __block int result = IIMANativeVideoRenderSchedulerUnavailable;
  iima_native_video_run_on_main_sync(^{
    result = [iima_native_video_view_for_session(session) renderScheduler];
  });
  return result;
}

void iima_native_window_center_after_delay(void *windowPointer, uint64_t delayMilliseconds) {
  if (windowPointer == NULL) {
    return;
  }
  NSWindow *window = (__bridge NSWindow *)windowPointer;
  dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(delayMilliseconds * NSEC_PER_MSEC)),
                 dispatch_get_main_queue(), ^{
                   [window center];
                 });
}

void iima_native_configure_mini_player_window(void *windowPointer) {
  if (windowPointer == NULL) {
    return;
  }
  void (^configure)(void) = ^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    window.titleVisibility = NSWindowTitleHidden;
    window.titlebarAppearsTransparent = YES;
    window.movableByWindowBackground = YES;
    window.tabbingMode = NSWindowTabbingModeDisallowed;
    window.collectionBehavior |= NSWindowCollectionBehaviorFullScreenNone;
    for (NSNumber *buttonType in @[@(NSWindowCloseButton), @(NSWindowMiniaturizeButton),
                                    @(NSWindowZoomButton), @(NSWindowDocumentIconButton)]) {
      NSButton *button = [window standardWindowButton:(NSWindowButton)buttonType.integerValue];
      button.hidden = YES;
      button.frame = NSZeroRect;
    }
  };
  if ([NSThread isMainThread]) {
    configure();
  } else {
    dispatch_sync(dispatch_get_main_queue(), configure);
  }
}

int iima_native_path_is_on_local_volume(const char *path) {
  if (path == NULL || path[0] == '\0') {
    return 1;
  }
  NSString *filePath = [NSString stringWithUTF8String:path];
  if (filePath == nil) {
    return 1;
  }
  NSNumber *isLocal = nil;
  NSError *error = nil;
  BOOL resolved = [[NSURL fileURLWithPath:filePath] getResourceValue:&isLocal
                                                              forKey:NSURLVolumeIsLocalKey
                                                               error:&error];
  return !resolved || isLocal == nil || isLocal.boolValue ? 1 : 0;
}
