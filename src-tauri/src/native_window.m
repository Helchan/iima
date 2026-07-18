#import <Cocoa/Cocoa.h>
#import <IOKit/ps/IOPowerSources.h>
#import <IOKit/ps/IOPSKeys.h>
#import <math.h>
#import <objc/runtime.h>

typedef void (*IIMAPlayerInputCallback)(const char *windowLabel,
                                        int kind,
                                        double x,
                                        double y,
                                        double deltaX,
                                        double deltaY,
                                        int precise,
                                        int natural,
                                        unsigned long long phase,
                                        unsigned long long momentumPhase,
                                        int stage,
                                        double magnification,
                                        void *context);

typedef void (*IIMAMiniPlayerLayoutCallback)(const char *windowLabel,
                                             int videoVisible,
                                             int playlistVisible,
                                             double width,
                                             double height,
                                             double videoHeight,
                                             double playlistHeight,
                                             void *context);

static const CGFloat IIMAMiniPlayerControlHeight = 72.0;
static const CGFloat IIMAMiniPlayerDefaultPlaylistHeight = 300.0;
static const CGFloat IIMAMiniPlayerAutoHidePlaylistThreshold = 200.0;
static const CGFloat IIMAPlayerMinimumInitialDragDistance = 3.0;

static NSMapTable<NSWindow *, NSString *> *IIMAPlayerInputWindows;
static NSMapTable<NSWindow *, NSValue *> *IIMAPlayerTitleDragStarts;
static id IIMAPlayerInputMonitor;
static IIMAPlayerInputCallback IIMAPlayerInputHandler;
static void *IIMAPlayerInputContext;

@interface IIMAMiniPlayerLayoutState : NSObject
@property(nonatomic, weak) NSWindow *window;
@property(nonatomic, copy) NSString *windowLabel;
@property(nonatomic) BOOL initialized;
@property(nonatomic) BOOL videoVisible;
@property(nonatomic) BOOL playlistVisible;
@property(nonatomic) CGFloat videoAspect;
@property(nonatomic) NSRect originalWindowFrame;
@property(nonatomic) IIMAMiniPlayerLayoutCallback callback;
@property(nonatomic) void *callbackContext;
- (void)applyVideoVisible:(BOOL)videoVisible
          playlistVisible:(BOOL)playlistVisible
              videoAspect:(CGFloat)videoAspect
                   values:(double *)values;
- (void)writeValues:(double *)values;
@end

static char IIMAMiniPlayerLayoutStateKey;

static CGFloat IIMAMiniPlayerSafeAspect(CGFloat aspect) {
  if (!isfinite(aspect) || aspect <= 0) return 1.0;
  return MIN(20.0, MAX(0.05, aspect));
}

static CGFloat IIMAMiniPlayerVideoHeight(NSWindow *window,
                                         BOOL videoVisible,
                                         CGFloat videoAspect) {
  if (!videoVisible || window == nil) return 0;
  return round(window.frame.size.width / IIMAMiniPlayerSafeAspect(videoAspect));
}

int iima_native_plan_mini_player_live_resize(double originY,
                                              double currentHeight,
                                              double normalHeight,
                                              double *values) {
  if (values == NULL || !isfinite(originY) || !isfinite(currentHeight) ||
      !isfinite(normalHeight) || currentHeight <= 0 || normalHeight <= 0) return -1;
  BOOL playlistVisible = currentHeight >=
    normalHeight + IIMAMiniPlayerAutoHidePlaylistThreshold;
  double targetHeight = playlistVisible ? currentHeight : normalHeight;
  values[0] = originY + currentHeight - targetHeight;
  values[1] = targetHeight;
  values[2] = playlistVisible ? 1.0 : 0.0;
  return 0;
}

@implementation IIMAMiniPlayerLayoutState

- (instancetype)init {
  self = [super init];
  if (self != nil) {
    _videoAspect = 1.0;
    [NSNotificationCenter.defaultCenter addObserver:self
                                           selector:@selector(windowWillStartLiveResize:)
                                               name:NSWindowWillStartLiveResizeNotification
                                             object:nil];
    [NSNotificationCenter.defaultCenter addObserver:self
                                           selector:@selector(windowDidEndLiveResize:)
                                               name:NSWindowDidEndLiveResizeNotification
                                             object:nil];
  }
  return self;
}

- (void)dealloc {
  [NSNotificationCenter.defaultCenter removeObserver:self];
}

- (CGFloat)videoHeight {
  return IIMAMiniPlayerVideoHeight(self.window, self.videoVisible, self.videoAspect);
}

- (CGFloat)normalWindowHeight {
  return IIMAMiniPlayerControlHeight + self.videoHeight;
}

- (void)setTopAnchoredHeight:(CGFloat)height animate:(BOOL)animate {
  NSWindow *window = self.window;
  if (window == nil) return;
  NSRect frame = window.frame;
  CGFloat targetHeight = MAX(IIMAMiniPlayerControlHeight, height);
  frame.origin.y += frame.size.height - targetHeight;
  frame.size.height = targetHeight;
  [window setFrame:frame display:YES animate:animate];
}

- (void)writeValues:(double *)values {
  if (values == NULL) return;
  NSWindow *window = self.window;
  NSRect frame = window != nil ? window.frame : NSZeroRect;
  CGFloat videoHeight = self.videoHeight;
  CGFloat playlistHeight = self.playlistVisible
    ? MAX(0, frame.size.height - IIMAMiniPlayerControlHeight - videoHeight)
    : 0;
  values[0] = frame.size.width;
  values[1] = frame.size.height;
  values[2] = videoHeight;
  values[3] = playlistHeight;
  values[4] = self.playlistVisible ? 1.0 : 0.0;
}

- (void)emitLayout {
  if (self.callback == NULL || self.windowLabel.length == 0) return;
  double values[5] = {0};
  [self writeValues:values];
  self.callback(self.windowLabel.UTF8String,
                self.videoVisible ? 1 : 0,
                self.playlistVisible ? 1 : 0,
                values[0], values[1], values[2], values[3],
                self.callbackContext);
}

- (void)windowWillStartLiveResize:(NSNotification *)notification {
  if (notification.object != self.window) return;
  self.originalWindowFrame = self.window.frame;
}

- (void)windowDidEndLiveResize:(NSNotification *)notification {
  NSWindow *window = self.window;
  if (notification.object != window || window == nil) return;
  CGFloat normalHeight = self.normalWindowHeight;
  NSRect frame = window.frame;
  double values[3] = {0};
  if (iima_native_plan_mini_player_live_resize(frame.origin.y,
                                               frame.size.height,
                                               normalHeight,
                                               values) != 0) return;
  if (values[2] == 0.0) {
    self.playlistVisible = NO;
    frame.origin.y = values[0];
    frame.size.height = values[1];
    [window setFrame:frame display:YES animate:YES];
  } else {
    self.playlistVisible = YES;
  }
  [self emitLayout];
}

- (void)applyVideoVisible:(BOOL)videoVisible
          playlistVisible:(BOOL)playlistVisible
              videoAspect:(CGFloat)videoAspect
                   values:(double *)values {
  NSWindow *window = self.window;
  if (window == nil) return;
  CGFloat aspect = IIMAMiniPlayerSafeAspect(videoAspect);
  if (!self.initialized) {
    self.initialized = YES;
    self.videoVisible = videoVisible;
    self.playlistVisible = playlistVisible;
    self.videoAspect = aspect;
    CGFloat targetHeight = self.normalWindowHeight;
    if (playlistVisible) targetHeight += IIMAMiniPlayerDefaultPlaylistHeight;
    [self setTopAnchoredHeight:targetHeight animate:NO];
    [self writeValues:values];
    return;
  }

  BOOL playlistChanged = self.playlistVisible != playlistVisible;
  CGFloat oldVideoHeight = self.videoHeight;
  self.videoVisible = videoVisible;
  self.playlistVisible = playlistVisible;
  self.videoAspect = aspect;
  CGFloat newVideoHeight = self.videoHeight;

  if (!window.inLiveResize) {
    if (playlistChanged) {
      CGFloat targetHeight = IIMAMiniPlayerControlHeight + newVideoHeight;
      if (playlistVisible) targetHeight += IIMAMiniPlayerDefaultPlaylistHeight;
      [self setTopAnchoredHeight:targetHeight animate:YES];
    } else if (fabs(newVideoHeight - oldVideoHeight) > 0.01) {
      // MiniPlayerWindowController.toggleVideoView/updateVideoSize keep the lower edge fixed.
      NSRect frame = window.frame;
      frame.size.height = MAX(IIMAMiniPlayerControlHeight,
                              frame.size.height + newVideoHeight - oldVideoHeight);
      [window setFrame:frame display:YES animate:NO];
    }
  }
  [self writeValues:values];
}

@end

static IIMAMiniPlayerLayoutState *IIMAMiniPlayerLayoutStateForWindow(NSWindow *window) {
  IIMAMiniPlayerLayoutState *state = objc_getAssociatedObject(window,
                                                              &IIMAMiniPlayerLayoutStateKey);
  if (state == nil) {
    state = [[IIMAMiniPlayerLayoutState alloc] init];
    state.window = window;
    objc_setAssociatedObject(window, &IIMAMiniPlayerLayoutStateKey, state,
                             OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  }
  return state;
}

static void IIMARunOnMainQueueSync(dispatch_block_t block) {
  if ([NSThread isMainThread]) {
    block();
  } else {
    dispatch_sync(dispatch_get_main_queue(), block);
  }
}

void iima_native_install_mini_player_layout_observer(
  void *windowPointer,
  const char *windowLabel,
  IIMAMiniPlayerLayoutCallback callback,
  void *context
) {
  if (windowPointer == NULL || windowLabel == NULL) return;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    IIMAMiniPlayerLayoutState *state = IIMAMiniPlayerLayoutStateForWindow(window);
    state.windowLabel = [NSString stringWithUTF8String:windowLabel] ?: @"";
    state.callback = callback;
    state.callbackContext = context;
  });
}

int iima_native_apply_mini_player_layout(void *windowPointer,
                                         int videoVisible,
                                         int playlistVisible,
                                         double videoAspect,
                                         double *values) {
  if (windowPointer == NULL || values == NULL) return -1;
  __block int status = 0;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    IIMAMiniPlayerLayoutState *state = IIMAMiniPlayerLayoutStateForWindow(window);
    [state applyVideoVisible:videoVisible != 0
            playlistVisible:playlistVisible != 0
                videoAspect:videoAspect
                     values:values];
    if (!state.initialized) status = -2;
  });
  return status;
}

int iima_native_read_battery_status(int *capacity, int *charging) {
  if (capacity == NULL || charging == NULL) return -1;
  CFTypeRef powerInfo = IOPSCopyPowerSourcesInfo();
  if (powerInfo == NULL) return -2;
  CFArrayRef sources = IOPSCopyPowerSourcesList(powerInfo);
  if (sources == NULL) {
    CFRelease(powerInfo);
    return -3;
  }

  int status = -4;
  CFIndex sourceCount = CFArrayGetCount(sources);
  for (CFIndex index = 0; index < sourceCount; index += 1) {
    CFTypeRef source = CFArrayGetValueAtIndex(sources, index);
    CFDictionaryRef description = IOPSGetPowerSourceDescription(powerInfo, source);
    if (description == NULL) continue;
    CFTypeRef type = CFDictionaryGetValue(description, CFSTR(kIOPSTypeKey));
    if (type == NULL || !CFEqual(type, CFSTR(kIOPSInternalBatteryType))) continue;

    CFNumberRef capacityValue = (CFNumberRef)CFDictionaryGetValue(
      description, CFSTR(kIOPSCurrentCapacityKey));
    if (capacityValue == NULL || CFGetTypeID(capacityValue) != CFNumberGetTypeID()) continue;
    int currentCapacity = 0;
    if (!CFNumberGetValue(capacityValue, kCFNumberIntType, &currentCapacity)) continue;
    CFTypeRef chargingValue = CFDictionaryGetValue(description, CFSTR(kIOPSIsChargingKey));
    *capacity = MAX(0, MIN(100, currentCapacity));
    *charging = chargingValue != NULL && CFGetTypeID(chargingValue) == CFBooleanGetTypeID()
      ? CFBooleanGetValue((CFBooleanRef)chargingValue)
      : 0;
    status = 0;
    break;
  }

  CFRelease(sources);
  CFRelease(powerInfo);
  return status;
}

int iima_native_read_player_window_context(void *windowPointer,
                                           double videoWidth,
                                           double videoHeight,
                                           int usePhysicalResolution,
                                           double *values) {
  if (windowPointer == NULL || values == NULL) return -1;
  __block int status = 0;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    NSScreen *screen = window.screen ?: NSScreen.mainScreen;
    if (screen == nil) {
      status = -2;
      return;
    }
    NSRect frame = window.frame;
    NSRect visibleFrame = screen.visibleFrame;
    NSSize videoSize = NSMakeSize(videoWidth, videoHeight);
    if (usePhysicalResolution && videoWidth > 0 && videoHeight > 0) {
      NSRect backingVideoRect = NSMakeRect(frame.origin.x, frame.origin.y, videoWidth, videoHeight);
      videoSize = [window convertRectFromBacking:backingVideoRect].size;
    }
    NSSize aspect = window.aspectRatio;
    double aspectRatio = aspect.width > 0 && aspect.height > 0
      ? aspect.width / aspect.height
      : (videoWidth > 0 && videoHeight > 0 ? videoWidth / videoHeight : frame.size.width / frame.size.height);

    values[0] = frame.origin.x;
    values[1] = frame.origin.y;
    values[2] = frame.size.width;
    values[3] = frame.size.height;
    values[4] = visibleFrame.origin.x;
    values[5] = visibleFrame.origin.y;
    values[6] = visibleFrame.size.width;
    values[7] = visibleFrame.size.height;
    values[8] = videoSize.width;
    values[9] = videoSize.height;
    values[10] = aspectRatio;
    values[11] = (window.styleMask & NSWindowStyleMaskFullScreen) != 0 ? 1.0 : 0.0;
  });
  return status;
}

int iima_native_set_player_window_frame(void *windowPointer,
                                        double x,
                                        double y,
                                        double width,
                                        double height) {
  if (windowPointer == NULL || width <= 0 || height <= 0) return -1;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    [window setFrame:NSMakeRect(x, y, width, height) display:YES animate:YES];
  });
  return 0;
}

int iima_native_set_player_window_frame_immediate(void *windowPointer,
                                                  double x,
                                                  double y,
                                                  double width,
                                                  double height) {
  if (windowPointer == NULL || width <= 0 || height <= 0) return -1;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    [window setFrame:NSMakeRect(x, y, width, height) display:YES animate:NO];
  });
  return 0;
}

void iima_native_configure_plugin_window(void *windowPointer,
                                         int fullSizeContentView,
                                         int hideTitleBar) {
  if (windowPointer == NULL) return;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    if (fullSizeContentView >= 0) {
      NSWindowStyleMask mask = window.styleMask;
      if (fullSizeContentView != 0) {
        mask |= NSWindowStyleMaskFullSizeContentView;
      } else {
        mask &= ~NSWindowStyleMaskFullSizeContentView;
      }
      window.styleMask = mask;
    }
    if (hideTitleBar >= 0) {
      BOOL hidden = hideTitleBar != 0;
      window.titlebarAppearsTransparent = hidden;
      window.titleVisibility = hidden ? NSWindowTitleHidden : NSWindowTitleVisible;
      window.movableByWindowBackground = hidden;
    }
  });
}

int iima_native_set_plugin_window_frame(void *windowPointer,
                                        int hasWidth,
                                        double width,
                                        int hasHeight,
                                        double height,
                                        int hasX,
                                        double x,
                                        int hasY,
                                        double y) {
  if (windowPointer == NULL) return -1;
  __block int status = 0;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    NSRect frame = window.frame;
    if (hasWidth) frame.size.width = width;
    if (hasHeight) frame.size.height = height;
    if (hasX) frame.origin.x = x;
    if (hasY) frame.origin.y = y;
    if (frame.size.width <= 0 || frame.size.height <= 0) {
      status = -2;
      return;
    }
    [window setFrame:frame display:YES animate:NO];
  });
  return status;
}

@interface IIMALegacyFullscreenState : NSObject
@property(nonatomic) NSRect frame;
@property(nonatomic) NSWindowStyleMask styleMask;
@property(nonatomic) NSWindowCollectionBehavior collectionBehavior;
@property(nonatomic) NSWindowLevel level;
@property(nonatomic) NSSize resizeIncrements;
@property(nonatomic) NSSize aspectRatio;
@property(nonatomic) NSRect contentViewFrame;
@property(nonatomic) NSApplicationPresentationOptions presentationOptions;
@end

@implementation IIMALegacyFullscreenState
@end

@interface IIMAPlayerWindowChromeState : NSObject
@property(nonatomic) BOOL initialized;
@property(nonatomic) BOOL visible;
@property(nonatomic) BOOL animating;
@property(nonatomic) NSUInteger generation;
@end

@implementation IIMAPlayerWindowChromeState
@end

static char IIMALegacyFullscreenStateKey;
static char IIMALegacyScreenObserverKey;
static char IIMABlackoutWindowsKey;
static char IIMABlackoutScreenKey;
static char IIMABlackoutScreenCountKey;
static char IIMAScreenParametersObserverKey;
static char IIMAPlayerWindowTitleFingerprintKey;
static char IIMAPlayerWindowChromeStateKey;

void iima_native_set_blackout_other_monitors(void *windowPointer, int enabled);

static IIMALegacyFullscreenState *IIMALegacyState(NSWindow *window) {
  return objc_getAssociatedObject(window, &IIMALegacyFullscreenStateKey);
}

static IIMAPlayerWindowChromeState *IIMAPlayerChromeState(NSWindow *window) {
  IIMAPlayerWindowChromeState *state = objc_getAssociatedObject(
    window, &IIMAPlayerWindowChromeStateKey);
  if (state == nil) {
    state = [[IIMAPlayerWindowChromeState alloc] init];
    objc_setAssociatedObject(window, &IIMAPlayerWindowChromeStateKey, state,
                             OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  }
  return state;
}

static NSTextField *IIMAPlayerWindowTitleTextField(NSWindow *window) {
  NSButton *closeButton = [window standardWindowButton:NSWindowCloseButton];
  for (NSView *view in closeButton.superview.subviews) {
    if ([view isKindOfClass:NSTextField.class]) {
      NSTextField *titleTextField = (NSTextField *)view;
      titleTextField.editable = NO;
      titleTextField.selectable = NO;
      titleTextField.refusesFirstResponder = YES;
      return titleTextField;
    }
  }
  return nil;
}

static BOOL IIMAPlayerTitleContainsEvent(NSWindow *window, NSEvent *event) {
  if (window == nil || event.window != window ||
      (window.styleMask & NSWindowStyleMaskFullScreen) != 0) return NO;
  NSTextField *titleTextField = IIMAPlayerWindowTitleTextField(window);
  if (titleTextField == nil || titleTextField.hidden || titleTextField.alphaValue <= 0.01) return NO;
  NSRect titleRect = [titleTextField convertRect:titleTextField.bounds toView:nil];
  return NSPointInRect(event.locationInWindow, titleRect);
}

static NSArray<NSButton *> *IIMAPlayerWindowStandardButtons(NSWindow *window) {
  NSMutableArray<NSButton *> *buttons = [NSMutableArray arrayWithCapacity:4];
  const NSWindowButton types[] = {
    NSWindowCloseButton,
    NSWindowMiniaturizeButton,
    NSWindowZoomButton,
    NSWindowDocumentIconButton,
  };
  for (NSUInteger index = 0; index < sizeof(types) / sizeof(types[0]); index += 1) {
    NSButton *button = [window standardWindowButton:types[index]];
    if (button != nil) [buttons addObject:button];
  }
  return buttons;
}

static void IIMAApplyPlayerWindowChromeAlpha(NSWindow *window, CGFloat alpha) {
  for (NSButton *button in IIMAPlayerWindowStandardButtons(window)) {
    button.hidden = NO;
    button.enabled = YES;
    button.alphaValue = alpha;
  }
  NSTextField *titleTextField = IIMAPlayerWindowTitleTextField(window);
  titleTextField.hidden = NO;
  titleTextField.alphaValue = alpha;
}

static void IIMAPreparePlayerWindowChromeForAnimation(NSWindow *window) {
  for (NSButton *button in IIMAPlayerWindowStandardButtons(window)) {
    button.hidden = NO;
    button.enabled = YES;
  }
  IIMAPlayerWindowTitleTextField(window).hidden = NO;
}

// Mirrors MainWindowController.updateTitle: local absolute files participate in AppKit's
// represented-document surface, while network media and the retained Initial surface use a plain
// title and must not retain a stale proxy icon. setTitleWithRepresentedFilename is unsafe after
// legacy fullscreen removes NSWindowStyleMaskTitled, so that state uses lastPathComponent exactly
// like IINA's Big Sur workaround.
int iima_native_sync_player_window_title(void *windowPointer,
                                          const char *representedPath,
                                          const char *plainTitle) {
  if (windowPointer == NULL || plainTitle == NULL) return -1;
  NSString *title = [NSString stringWithUTF8String:plainTitle];
  if (title == nil) return -2;

  __block int status = 0;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    if (representedPath != NULL) {
      NSString *path = [NSString stringWithUTF8String:representedPath];
      if (path == nil || !path.isAbsolutePath) {
        status = -3;
        return;
      }
      BOOL useFilenameFallback = IIMALegacyState(window) != nil ||
        (window.styleMask & NSWindowStyleMaskTitled) == 0;
      NSString *fingerprint = [NSString stringWithFormat:@"local:%d:%@",
                                                        useFilenameFallback, path];
      NSString *priorFingerprint = objc_getAssociatedObject(
        window, &IIMAPlayerWindowTitleFingerprintKey);
      if ([priorFingerprint isEqualToString:fingerprint]) return;

      NSURL *representedURL = [NSURL fileURLWithPath:path];
      window.representedURL = representedURL;
      if (useFilenameFallback) {
        window.title = path.lastPathComponent ?: @"";
      } else {
        [window setTitleWithRepresentedFilename:path];
      }
      objc_setAssociatedObject(window, &IIMAPlayerWindowTitleFingerprintKey, fingerprint,
                               OBJC_ASSOCIATION_COPY_NONATOMIC);
      return;
    }

    NSString *fingerprint = [@"plain:" stringByAppendingString:title];
    NSString *priorFingerprint = objc_getAssociatedObject(
      window, &IIMAPlayerWindowTitleFingerprintKey);
    if ([priorFingerprint isEqualToString:fingerprint]) return;
    window.representedURL = nil;
    window.title = title;
    objc_setAssociatedObject(window, &IIMAPlayerWindowTitleFingerprintKey, fingerprint,
                             OBJC_ASSOCIATION_COPY_NONATOMIC);
  });
  return status;
}

// MainWindowController has a single AppKit-owned title. Its traffic lights, document icon and
// title text participate in the same fadeable UI state as titleBarView and the OSC. The WebView
// supplies only the titlebar material/accessory surface; it must never draw a second filename.
int iima_native_set_player_window_chrome_visible(void *windowPointer,
                                                  int visible,
                                                  int animated) {
  if (windowPointer == NULL) return -1;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    IIMAPlayerWindowChromeState *state = IIMAPlayerChromeState(window);
    BOOL shouldShow = visible != 0;
    if (state.initialized && state.visible == shouldShow) {
      if (!state.animating) {
        IIMAApplyPlayerWindowChromeAlpha(window, shouldShow ? 1.0 : 1e-100);
        IIMAPlayerWindowTitleTextField(window).alphaValue = shouldShow ? 1.0 : 0.0;
      }
      return;
    }

    state.initialized = YES;
    state.visible = shouldShow;
    state.animating = animated != 0;
    state.generation += 1;
    NSUInteger generation = state.generation;

    void (^finish)(void) = ^{
      IIMAPlayerWindowChromeState *current = IIMAPlayerChromeState(window);
      if (current.generation != generation || current.visible != shouldShow) return;
      current.animating = NO;
      IIMAApplyPlayerWindowChromeAlpha(window, shouldShow ? 1.0 : 1e-100);
      IIMAPlayerWindowTitleTextField(window).alphaValue = shouldShow ? 1.0 : 0.0;
    };

    if (animated == 0) {
      finish();
      return;
    }
    // Do not force the opposite endpoint before animating. When the pointer reverses direction
    // during the 250 ms fade, AppKit must retarget from the current presentation alpha instead of
    // flashing through fully hidden or fully visible chrome first.
    IIMAPreparePlayerWindowChromeForAnimation(window);
    [NSAnimationContext runAnimationGroup:^(NSAnimationContext *context) {
      context.duration = 0.25;
      for (NSButton *button in IIMAPlayerWindowStandardButtons(window)) {
        button.animator.alphaValue = shouldShow ? 1.0 : 0.0;
      }
      IIMAPlayerWindowTitleTextField(window).animator.alphaValue = shouldShow ? 1.0 : 0.0;
    } completionHandler:finish];
  });
  return 0;
}

int iima_native_plan_legacy_exit_aspect_frame(double frameX,
                                               double frameY,
                                               double frameWidth,
                                               double frameHeight,
                                               double videoWidth,
                                               double videoHeight,
                                               double *values) {
  if (values == NULL || !isfinite(frameX) || !isfinite(frameY) ||
      !isfinite(frameWidth) || !isfinite(frameHeight) ||
      !isfinite(videoWidth) || !isfinite(videoHeight) ||
      frameWidth <= 0 || frameHeight <= 0 || videoWidth <= 0 || videoHeight <= 0) {
    return -1;
  }
  double aspect = videoWidth / videoHeight;
  double targetWidth = frameWidth;
  double targetHeight = targetWidth / aspect;
  if (targetHeight > frameHeight) {
    targetHeight = frameHeight;
    targetWidth = targetHeight * aspect;
  }
  values[0] = frameX + (frameWidth - targetWidth) / 2.0;
  values[1] = frameY + (frameHeight - targetHeight) / 2.0;
  values[2] = targetWidth;
  values[3] = targetHeight;
  return 0;
}

static BOOL IIMALegacyFullscreenShouldAnimate(void) {
  return !NSWorkspace.sharedWorkspace.accessibilityDisplayShouldReduceMotion;
}

static void IIMAApplyLegacyFullscreenFrame(NSWindow *window, BOOL animate) {
  NSScreen *screen = window.screen ?: NSScreen.mainScreen;
  if (screen == nil) return;
  [window setFrame:screen.frame display:YES animate:animate];
  if (@available(macOS 12.0, *)) {
    CGFloat cameraHousingHeight = screen.safeAreaInsets.top;
    if (cameraHousingHeight > 0 && window.contentView != nil) {
      NSRect contentFrame = window.contentView.frame;
      contentFrame.size.height = MAX(0, contentFrame.size.height - cameraHousingHeight);
      contentFrame.origin.y = 0;
      window.contentView.frame = contentFrame;
    }
  }
}

void iima_native_configure_fullscreen_mode(void *windowPointer, int useLegacy) {
  if (windowPointer == NULL) return;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    if (IIMALegacyState(window) != nil ||
        (window.styleMask & NSWindowStyleMaskFullScreen) != 0) {
      return;
    }
    NSWindowCollectionBehavior behavior = window.collectionBehavior;
    if (useLegacy) {
      behavior &= ~NSWindowCollectionBehaviorFullScreenPrimary;
      behavior |= NSWindowCollectionBehaviorFullScreenAuxiliary;
    } else {
      behavior &= ~NSWindowCollectionBehaviorFullScreenAuxiliary;
      behavior |= NSWindowCollectionBehaviorFullScreenPrimary;
    }
    window.collectionBehavior = behavior;
  });
}

void iima_native_set_window_theme(void *windowPointer, int theme) {
  if (windowPointer == NULL) return;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    if (@available(macOS 10.14, *)) {
      switch (theme) {
        case 2:
          window.appearance = [NSAppearance appearanceNamed:NSAppearanceNameAqua];
          break;
        case 4:
          window.appearance = nil;
          break;
        default:
          window.appearance = [NSAppearance appearanceNamed:NSAppearanceNameDarkAqua];
          break;
      }
    }
  });
}

int iima_native_window_is_legacy_fullscreen(void *windowPointer) {
  if (windowPointer == NULL) return 0;
  __block int active = 0;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    active = IIMALegacyState(window) != nil ? 1 : 0;
  });
  return active;
}

int iima_native_set_legacy_fullscreen(void *windowPointer,
                                      int enabled,
                                      int animateExit,
                                      double videoWidth,
                                      double videoHeight) {
  if (windowPointer == NULL) return -1;
  __block int status = 0;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    IIMALegacyFullscreenState *state = IIMALegacyState(window);
    if (enabled) {
      if (state != nil) return;
      if ((window.styleMask & NSWindowStyleMaskFullScreen) != 0) {
        status = -2;
        return;
      }
      NSScreen *screen = window.screen ?: NSScreen.mainScreen;
      if (screen == nil) {
        status = -3;
        return;
      }
      state = [[IIMALegacyFullscreenState alloc] init];
      state.frame = window.frame;
      state.styleMask = window.styleMask;
      state.collectionBehavior = window.collectionBehavior;
      state.level = window.level;
      state.resizeIncrements = window.resizeIncrements;
      state.aspectRatio = window.aspectRatio;
      state.contentViewFrame = window.contentView.frame;
      state.presentationOptions = NSApp.presentationOptions;
      objc_setAssociatedObject(window, &IIMALegacyFullscreenStateKey, state,
                               OBJC_ASSOCIATION_RETAIN_NONATOMIC);

      NSWindowStyleMask styleMask = window.styleMask | NSWindowStyleMaskBorderless;
      styleMask &= ~NSWindowStyleMaskTitled;
      window.styleMask = styleMask;
      window.collectionBehavior = (window.collectionBehavior &
                                   ~NSWindowCollectionBehaviorFullScreenPrimary) |
                                  NSWindowCollectionBehaviorFullScreenAuxiliary;
      window.level = NSFloatingWindowLevel;
      window.resizeIncrements = NSMakeSize(1, 1);
      NSApp.presentationOptions = NSApp.presentationOptions |
                                  NSApplicationPresentationAutoHideMenuBar |
                                  NSApplicationPresentationAutoHideDock;
      IIMAApplyLegacyFullscreenFrame(window, IIMALegacyFullscreenShouldAnimate());
      [window makeKeyAndOrderFront:nil];
      __weak NSWindow *weakWindow = window;
      id observer = [NSNotificationCenter.defaultCenter
        addObserverForName:NSApplicationDidChangeScreenParametersNotification
                    object:nil
                     queue:NSOperationQueue.mainQueue
                usingBlock:^(__unused NSNotification *notification) {
                  NSWindow *strongWindow = weakWindow;
                  if (strongWindow == nil || IIMALegacyState(strongWindow) == nil) return;
                  IIMAApplyLegacyFullscreenFrame(strongWindow,
                                                 IIMALegacyFullscreenShouldAnimate());
                }];
      objc_setAssociatedObject(window, &IIMALegacyScreenObserverKey, observer,
                               OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    } else {
      if (state == nil) return;
      id observer = objc_getAssociatedObject(window, &IIMALegacyScreenObserverKey);
      if (observer != nil) {
        [NSNotificationCenter.defaultCenter removeObserver:observer];
        objc_setAssociatedObject(window, &IIMALegacyScreenObserverKey, nil,
                                 OBJC_ASSOCIATION_RETAIN_NONATOMIC);
      }
      window.styleMask = state.styleMask;
      window.collectionBehavior = state.collectionBehavior;
      window.level = state.level;
      window.resizeIncrements = state.resizeIncrements;
      NSApp.presentationOptions = state.presentationOptions;
      BOOL hasVideoAspect = isfinite(videoWidth) && isfinite(videoHeight) &&
                            videoWidth > 0 && videoHeight > 0;
      BOOL shouldAnimate = animateExit != 0;
      if (shouldAnimate && hasVideoAspect) {
        double values[4] = {0};
        NSRect fullscreenFrame = window.frame;
        if (iima_native_plan_legacy_exit_aspect_frame(fullscreenFrame.origin.x,
                                                       fullscreenFrame.origin.y,
                                                       fullscreenFrame.size.width,
                                                       fullscreenFrame.size.height,
                                                       videoWidth,
                                                       videoHeight,
                                                       values) == 0) {
          NSRect aspectFrame = NSMakeRect(values[0], values[1], values[2], values[3]);
          [window setFrame:aspectFrame display:YES animate:NO];
        }
      }
      [window setFrame:state.frame display:YES animate:shouldAnimate];
      window.aspectRatio = hasVideoAspect
        ? NSMakeSize(videoWidth, videoHeight)
        : state.aspectRatio;
      window.contentView.frame = state.contentViewFrame;
      objc_setAssociatedObject(window, &IIMALegacyFullscreenStateKey, nil,
                               OBJC_ASSOCIATION_RETAIN_NONATOMIC);
      [window makeKeyAndOrderFront:nil];
    }
  });
  return status;
}

void iima_native_prepare_player_window_close(void *windowPointer) {
  if (windowPointer == NULL) return;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    IIMALegacyFullscreenState *state = IIMALegacyState(window);
    if (state == nil) return;

    // IINA restores the Dock/menu-bar presentation immediately when a legacy
    // fullscreen player closes. The closing window does not need the normal
    // geometry animation, but its screen observer must no longer retain any
    // fullscreen behavior after CloseRequested.
    NSApp.presentationOptions = state.presentationOptions;
    id observer = objc_getAssociatedObject(window, &IIMALegacyScreenObserverKey);
    if (observer != nil) {
      [NSNotificationCenter.defaultCenter removeObserver:observer];
      objc_setAssociatedObject(window, &IIMALegacyScreenObserverKey, nil,
                               OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    }
    objc_setAssociatedObject(window, &IIMALegacyFullscreenStateKey, nil,
                             OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  });
}

void iima_native_set_blackout_other_monitors(void *windowPointer, int enabled) {
  if (windowPointer == NULL) return;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    NSArray<NSWindow *> *existing = objc_getAssociatedObject(window, &IIMABlackoutWindowsKey);
    NSScreen *activeScreen = window.screen;
    NSScreen *previousScreen = objc_getAssociatedObject(window, &IIMABlackoutScreenKey);
    NSNumber *previousScreenCount = objc_getAssociatedObject(window, &IIMABlackoutScreenCountKey);
    if (enabled && existing != nil && previousScreen == activeScreen &&
        previousScreenCount.unsignedIntegerValue == NSScreen.screens.count) return;
    for (NSWindow *blackWindow in existing) {
      [blackWindow orderOut:nil];
      [blackWindow close];
    }
    objc_setAssociatedObject(window, &IIMABlackoutWindowsKey, nil,
                             OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    objc_setAssociatedObject(window, &IIMABlackoutScreenKey, nil,
                             OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    objc_setAssociatedObject(window, &IIMABlackoutScreenCountKey, nil,
                             OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    if (!enabled) {
      id observer = objc_getAssociatedObject(window, &IIMAScreenParametersObserverKey);
      if (observer != nil) {
        [NSNotificationCenter.defaultCenter removeObserver:observer];
        objc_setAssociatedObject(window, &IIMAScreenParametersObserverKey, nil,
                                 OBJC_ASSOCIATION_RETAIN_NONATOMIC);
      }
      return;
    }

    NSMutableArray<NSWindow *> *blackWindows = [[NSMutableArray alloc] init];
    for (NSScreen *screen in NSScreen.screens) {
      if (screen == activeScreen) continue;
      NSRect screenRect = screen.frame;
      screenRect.origin = NSZeroPoint;
      NSWindow *blackWindow = [[NSWindow alloc]
        initWithContentRect:screenRect
                  styleMask:NSWindowStyleMaskBorderless
                    backing:NSBackingStoreBuffered
                      defer:NO
                     screen:screen];
      blackWindow.backgroundColor = NSColor.blackColor;
      blackWindow.opaque = YES;
      blackWindow.hasShadow = NO;
      blackWindow.ignoresMouseEvents = YES;
      blackWindow.level = NSMainMenuWindowLevel + 1;
      blackWindow.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces |
                                       NSWindowCollectionBehaviorStationary;
      [blackWindow orderFront:nil];
      [blackWindows addObject:blackWindow];
    }
    objc_setAssociatedObject(window, &IIMABlackoutWindowsKey, blackWindows,
                             OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    objc_setAssociatedObject(window, &IIMABlackoutScreenKey, activeScreen,
                             OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    objc_setAssociatedObject(window, &IIMABlackoutScreenCountKey, @(NSScreen.screens.count),
                             OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    if (objc_getAssociatedObject(window, &IIMAScreenParametersObserverKey) == nil) {
      __weak NSWindow *weakWindow = window;
      id observer = [NSNotificationCenter.defaultCenter
        addObserverForName:NSApplicationDidChangeScreenParametersNotification
                    object:nil
                     queue:NSOperationQueue.mainQueue
                usingBlock:^(__unused NSNotification *notification) {
                  NSWindow *strongWindow = weakWindow;
                  if (strongWindow == nil) return;
                  IIMALegacyFullscreenState *legacyState = IIMALegacyState(strongWindow);
                  if (legacyState != nil) {
                    IIMAApplyLegacyFullscreenFrame(strongWindow,
                                                   IIMALegacyFullscreenShouldAnimate());
                  }
                  iima_native_set_blackout_other_monitors((__bridge void *)strongWindow, 1);
                }];
      objc_setAssociatedObject(window, &IIMAScreenParametersObserverKey, observer,
                               OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    }
  });
}

int iima_native_application_is_active(void) {
  __block int active = 0;
  // A native utility panel keeps the app active and owns keyWindow, so the
  // player must not pause. Switching applications clears keyWindow; switching
  // players is identified separately by the Rust Tauri window registry.
  IIMARunOnMainQueueSync(^{ active = NSApp.isActive && NSApp.keyWindow != nil ? 1 : 0; });
  return active;
}

typedef void (*IIMASystemSleepCallback)(void *context);
static id IIMASystemSleepObserver;

void iima_native_install_system_sleep_observer(IIMASystemSleepCallback callback, void *context) {
  IIMARunOnMainQueueSync(^{
    if (IIMASystemSleepObserver != nil) {
      [NSWorkspace.sharedWorkspace.notificationCenter removeObserver:IIMASystemSleepObserver];
      IIMASystemSleepObserver = nil;
    }
    if (callback == NULL) return;
    IIMASystemSleepObserver = [NSWorkspace.sharedWorkspace.notificationCenter
      addObserverForName:NSWorkspaceWillSleepNotification
                  object:nil
                   queue:NSOperationQueue.mainQueue
              usingBlock:^(__unused NSNotification *notification) {
                callback(context);
              }];
  });
}

static void IIMAEmitPlayerInput(NSEvent *event, NSString *label) {
  IIMAPlayerInputCallback callback = IIMAPlayerInputHandler;
  if (callback == NULL || label.length == 0) return;
  NSWindow *window = event.window;
  NSView *contentView = window.contentView;
  NSPoint point = event.locationInWindow;
  if (contentView != nil) {
    point = [contentView convertPoint:point fromView:nil];
    point.y = NSHeight(contentView.bounds) - point.y;
  }
  int kind = 0;
  double deltaX = 0;
  double deltaY = 0;
  int precise = 0;
  int natural = 0;
  int stage = 0;
  double magnification = 0;
  switch (event.type) {
    case NSEventTypeScrollWheel:
      kind = 1;
      deltaX = event.scrollingDeltaX;
      deltaY = event.scrollingDeltaY;
      precise = event.hasPreciseScrollingDeltas ? 1 : 0;
      natural = event.isDirectionInvertedFromDevice ? 1 : 0;
      break;
    case NSEventTypePressure:
      kind = 2;
      stage = (int)event.stage;
      break;
    case NSEventTypeMagnify:
      kind = 3;
      magnification = event.magnification;
      break;
    default:
      return;
  }
  callback(label.UTF8String,
           kind,
           point.x,
           point.y,
           deltaX,
           deltaY,
           precise,
           natural,
           (unsigned long long)event.phase,
           (unsigned long long)event.momentumPhase,
           stage,
           magnification,
           IIMAPlayerInputContext);
}

void iima_native_install_player_input_monitor(void *windowPointer,
                                              const char *windowLabel,
                                              IIMAPlayerInputCallback callback,
                                              void *context) {
  if (windowPointer == NULL || windowLabel == NULL || callback == NULL) return;
  NSString *label = [NSString stringWithUTF8String:windowLabel];
  if (label.length == 0) return;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    if (IIMAPlayerInputWindows == nil) {
      IIMAPlayerInputWindows = [NSMapTable weakToStrongObjectsMapTable];
    }
    if (IIMAPlayerTitleDragStarts == nil) {
      IIMAPlayerTitleDragStarts = [NSMapTable weakToStrongObjectsMapTable];
    }
    IIMAPlayerInputHandler = callback;
    IIMAPlayerInputContext = context;
    [IIMAPlayerInputWindows setObject:label forKey:window];
    if (IIMAPlayerInputMonitor != nil) return;
    NSEventMask mask = NSEventMaskLeftMouseDown |
      NSEventMaskLeftMouseDragged |
      NSEventMaskLeftMouseUp |
      NSEventMaskScrollWheel |
      NSEventMaskPressure |
      NSEventMaskMagnify;
    IIMAPlayerInputMonitor = [NSEvent addLocalMonitorForEventsMatchingMask:mask
      handler:^NSEvent * _Nullable(NSEvent *event) {
        NSString *targetLabel = [IIMAPlayerInputWindows objectForKey:event.window];
        if (targetLabel == nil) return event;
        if (event.type == NSEventTypeLeftMouseDown) {
          // AppKit owns the represented filename and proxy icon, so WebKit's Tauri drag region
          // never sees a press over the native title text. Match IINA's MainWindowController:
          // remember the press and wait for a real drag beyond its 3 pt sensitivity threshold.
          if (IIMAPlayerTitleContainsEvent(event.window, event)) {
            [IIMAPlayerTitleDragStarts setObject:[NSValue valueWithPoint:event.locationInWindow]
                                           forKey:event.window];
          } else {
            [IIMAPlayerTitleDragStarts removeObjectForKey:event.window];
          }
          return event;
        }
        if (event.type == NSEventTypeLeftMouseDragged) {
          NSValue *startValue = [IIMAPlayerTitleDragStarts objectForKey:event.window];
          if (startValue == nil) return event;
          NSPoint start = startValue.pointValue;
          CGFloat distance = hypot(event.locationInWindow.x - start.x,
                                   event.locationInWindow.y - start.y);
          if (distance <= IIMAPlayerMinimumInitialDragDistance) return event;
          [IIMAPlayerTitleDragStarts removeObjectForKey:event.window];
          [event.window performWindowDragWithEvent:event];
          return nil;
        }
        if (event.type == NSEventTypeLeftMouseUp) {
          [IIMAPlayerTitleDragStarts removeObjectForKey:event.window];
          return event;
        }
        IIMAEmitPlayerInput(event, targetLabel);
        // Scroll and magnify are rerouted through the Tauri event so the WebView
        // can apply IINA's hit-test rules without losing AppKit phase metadata.
        if (event.type == NSEventTypeScrollWheel || event.type == NSEventTypeMagnify) return nil;
        return event;
      }];
  });
}

void iima_native_remove_player_input_monitor(void *windowPointer) {
  if (windowPointer == NULL) return;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    [IIMAPlayerInputWindows removeObjectForKey:window];
    [IIMAPlayerTitleDragStarts removeObjectForKey:window];
  });
}

void iima_native_remove_all_player_input_monitors(void) {
  IIMARunOnMainQueueSync(^{
    [IIMAPlayerInputWindows removeAllObjects];
    [IIMAPlayerTitleDragStarts removeAllObjects];
    if (IIMAPlayerInputMonitor != nil) {
      [NSEvent removeMonitor:IIMAPlayerInputMonitor];
      IIMAPlayerInputMonitor = nil;
    }
    IIMAPlayerInputHandler = NULL;
    IIMAPlayerInputContext = NULL;
  });
}

static BOOL IIMAConfigureFrameAutosave(NSWindow *window, NSString *autosaveName) {
  if (window == nil || autosaveName.length == 0) return NO;
  BOOL hasSavedFrame = [NSUserDefaults.standardUserDefaults
    objectForKey:[NSString stringWithFormat:@"NSWindow Frame %@", autosaveName]] != nil;
  if (hasSavedFrame) {
    [window setFrameUsingName:autosaveName force:NO];
  }
  [window setFrameAutosaveName:autosaveName];
  return hasSavedFrame;
}

// Applies the native-only parts of IINA 1.3.5's reusable auxiliary window XIBs.
// kind: 1 = Open URL, 2 = Preferences, 3 = Audio/Video Filters.
int iima_native_configure_auxiliary_window(void *windowPointer, int kind) {
  if (windowPointer == NULL || kind < 1 || kind > 3) return -1;
  __block int status = 0;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    window.releasedWhenClosed = NO;
    window.styleMask = window.styleMask |
      NSWindowStyleMaskTitled |
      NSWindowStyleMaskClosable |
      NSWindowStyleMaskMiniaturizable |
      NSWindowStyleMaskResizable;

    if (kind == 1 || kind == 2) {
      window.styleMask = window.styleMask | NSWindowStyleMaskFullSizeContentView;
      window.titlebarAppearsTransparent = YES;
      window.titleVisibility = NSWindowTitleHidden;
      window.movableByWindowBackground = YES;
    }

    if (kind == 1) {
      window.collectionBehavior = window.collectionBehavior |
        NSWindowCollectionBehaviorFullScreenNone;
      for (NSNumber *buttonType in @[
        @(NSWindowCloseButton),
        @(NSWindowMiniaturizeButton),
        @(NSWindowZoomButton)
      ]) {
        [window standardWindowButton:buttonType.unsignedIntegerValue].hidden = YES;
      }
    } else if (kind == 2) {
      IIMAConfigureFrameAutosave(window, @"IINAPreferenceWindow");
    }
  });
  return status;
}

// PlaybackHistoryWindowController and LogWindowController both retain their XIB
// windows after close and use AppKit frame autosave names. Keeping this native
// avoids replacing AppKit's established persistence semantics with a web store.
int iima_native_configure_retained_window(void *windowPointer,
                                          const char *frameAutosaveName) {
  if (windowPointer == NULL || frameAutosaveName == NULL) return -1;
  NSString *autosaveName = [NSString stringWithUTF8String:frameAutosaveName];
  if (autosaveName.length == 0) return -2;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    window.releasedWhenClosed = NO;
    IIMAConfigureFrameAutosave(window, autosaveName);
  });
  return 0;
}

// Configures the one retained Tauri player window to emulate IINA's separate
// InitialWindowController/MainWindowController pair. Return 1 when AppKit had a saved welcome
// frame to restore, 0 when the caller should center the first welcome presentation, and < 0 on
// invalid input.
int iima_native_configure_player_presentation(void *windowPointer, int initial) {
  if (windowPointer == NULL) return -1;
  __block int status = 0;
  IIMARunOnMainQueueSync(^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    window.releasedWhenClosed = NO;
    window.styleMask = window.styleMask |
      NSWindowStyleMaskTitled |
      NSWindowStyleMaskClosable |
      NSWindowStyleMaskMiniaturizable |
      NSWindowStyleMaskFullSizeContentView;
    window.titlebarAppearsTransparent = YES;
    window.titleVisibility = initial != 0 ? NSWindowTitleHidden : NSWindowTitleVisible;

    if (initial != 0) {
      window.styleMask = window.styleMask & ~NSWindowStyleMaskResizable;
      window.movableByWindowBackground = YES;
      NSString *autosaveName = @"IINAWelcomeWindow";
      if (IIMAConfigureFrameAutosave(window, autosaveName)) {
        status = 1;
      }
    } else {
      window.styleMask = window.styleMask | NSWindowStyleMaskResizable;
      window.movableByWindowBackground = NO;
      [window setFrameAutosaveName:@""];
    }
  });
  return status;
}
