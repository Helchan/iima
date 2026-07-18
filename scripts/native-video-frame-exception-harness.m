#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>

#include <stdio.h>
#include <stdlib.h>

// Include the production bridge in this translation unit so the harness exercises the exact
// exception boundary and retry scheduler shipped in IINA.app. The fake AppKit objects below only
// replace the window/view instances held by the production session dictionaries.
#include "../src-tauri/src/native_video.m"

@interface IIMAFakeFrameVideoWindow : NSObject
@property(nonatomic) NSRect frame;
@property(nonatomic, strong) id fakeParentWindow;
@property(nonatomic) NSUInteger setFrameCalls;
@property(nonatomic) NSUInteger orderCalls;
@property(nonatomic) NSUInteger closeCalls;
@property(nonatomic) NSUInteger remainingThrows;
@property(nonatomic) BOOL commitFrameBeforeThrow;
@property(nonatomic) BOOL lastDisplayArgument;
@property(nonatomic, getter=isVisible) BOOL visible;
@property(nonatomic, strong) NSView *contentView;
@property(nonatomic) NSUInteger invalidateShadowCalls;
@end

@interface IIMAFakeFrameParentWindow : NSObject
@property(nonatomic) NSRect frame;
@property(nonatomic) BOOL visible;
@property(nonatomic) NSInteger number;
@property(nonatomic) NSWindowStyleMask styleMask;
@property(nonatomic, strong) IIMAFakeFrameVideoWindow *childWindow;
@end

@interface IIMAFakeFrameHostView : NSObject
@property(nonatomic, strong) IIMAFakeFrameParentWindow *fakeWindow;
@end

@interface IIMAFakeFrameOpenGLContext : NSObject
@property(nonatomic) NSUInteger updateCalls;
@end

@interface IIMAFakeFrameVideoView : NSObject
@property(nonatomic, strong) IIMAFakeFrameOpenGLContext *fakeContext;
@property(nonatomic) NSUInteger renderRequests;
@property(nonatomic) NSUInteger detachCalls;
@property(nonatomic) NSUInteger removalCalls;
@end

@implementation IIMAFakeFrameVideoWindow
- (NSWindow *)parentWindow {
  return (NSWindow *)self.fakeParentWindow;
}

- (void)setFrame:(NSRect)frameRect display:(BOOL)display {
  self.setFrameCalls += 1;
  self.lastDisplayArgument = display;
  BOOL shouldThrow = self.remainingThrows > 0;
  if (shouldThrow && self.remainingThrows != NSUIntegerMax) {
    self.remainingThrows -= 1;
  }
  if (!shouldThrow || self.commitFrameBeforeThrow) {
    self.frame = frameRect;
  }
  if (shouldThrow) {
    @throw [NSException
      exceptionWithName:NSInternalInconsistencyException
                 reason:@"CGSSetSurfaceColorSpace failed with 1000"
               userInfo:nil];
  }
}

- (void)orderWindow:(NSWindowOrderingMode)place relativeTo:(NSInteger)otherWindowNumber {
  (void)place;
  (void)otherWindowNumber;
  self.orderCalls += 1;
}

- (void)orderOut:(id)sender {
  (void)sender;
}

- (void)close {
  self.closeCalls += 1;
}

- (void)invalidateShadow {
  self.invalidateShadowCalls += 1;
}
@end

@implementation IIMAFakeFrameParentWindow
- (BOOL)isVisible {
  return self.visible;
}

- (NSInteger)windowNumber {
  return self.number;
}

- (void)addChildWindow:(NSWindow *)childWindow ordered:(NSWindowOrderingMode)place {
  (void)place;
  IIMAFakeFrameVideoWindow *child = (IIMAFakeFrameVideoWindow *)childWindow;
  self.childWindow = child;
  child.fakeParentWindow = self;
}

- (void)removeChildWindow:(NSWindow *)childWindow {
  IIMAFakeFrameVideoWindow *child = (IIMAFakeFrameVideoWindow *)childWindow;
  if (self.childWindow == child) {
    self.childWindow = nil;
  }
  child.fakeParentWindow = nil;
}
@end

@implementation IIMAFakeFrameHostView
- (NSWindow *)window {
  return (NSWindow *)self.fakeWindow;
}
@end

@implementation IIMAFakeFrameOpenGLContext
- (void)update {
  self.updateCalls += 1;
}
@end

@implementation IIMAFakeFrameVideoView
- (NSOpenGLContext *)openGLContext {
  return (NSOpenGLContext *)self.fakeContext;
}

- (void)requestRender {
  self.renderRequests += 1;
}

- (void)detachMpvClient {
  self.detachCalls += 1;
}

- (void)removeFromSuperview {
  self.removalCalls += 1;
}
@end

static void IIMAFail(NSString *message) {
  fprintf(stderr, "native video frame exception harness: %s\n", message.UTF8String);
  exit(1);
}

static void IIMARequire(BOOL condition, NSString *message) {
  if (!condition) {
    IIMAFail(message);
  }
}

static void IIMAPumpMainQueueInMode(NSRunLoopMode mode, NSTimeInterval seconds) {
  NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:seconds];
  while ([deadline timeIntervalSinceNow] > 0) {
    [[NSRunLoop currentRunLoop]
      runMode:mode
      beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
  }
}

static void IIMAPumpMainQueue(NSTimeInterval seconds) {
  IIMAPumpMainQueueInMode(NSDefaultRunLoopMode, seconds);
}

static void IIMAPumpEventTrackingMainQueue(NSTimeInterval seconds) {
  IIMAPumpMainQueueInMode(NSEventTrackingRunLoopMode, seconds);
}

static void IIMARemoveFakeSession(NSString *session) {
  [iima_native_video_frame_retry_attempts removeObjectForKey:session];
  [iima_native_video_frame_update_generations removeObjectForKey:session];
  [iima_native_video_live_frame_update_generations removeObjectForKey:session];
  [iima_native_video_live_frame_updates removeObject:session];
  [iima_native_video_live_resize_sessions removeObject:session];
  [iima_native_video_suspended_live_frame_updates removeObject:session];
  [iima_native_video_force_surface_updates removeObject:session];
  iima_native_video_remove_window_observers(session);
  [iima_native_video_views removeObjectForKey:session];
  [iima_native_video_hosts removeObjectForKey:session];
  [iima_native_video_windows removeObjectForKey:session];
}

static void IIMAInstallFakeSession(
  NSString *session,
  IIMAFakeFrameParentWindow *parent,
  IIMAFakeFrameVideoWindow *video,
  IIMAFakeFrameVideoView *view
) {
  IIMAFakeFrameHostView *host = [IIMAFakeFrameHostView new];
  host.fakeWindow = parent;
  iima_native_video_hosts[session] = (NSView *)host;
  iima_native_video_windows[session] = (NSWindow *)video;
  iima_native_video_views[session] = (IIMANativeVideoView *)view;
}

static void IIMATestExceptionIsCaughtAndRetried(void) {
  NSString *session = @"throw-before-commit";
  IIMAFakeFrameParentWindow *parent = [IIMAFakeFrameParentWindow new];
  parent.frame = NSMakeRect(40, 80, 960, 540);
  parent.visible = YES;
  parent.number = 42;
  IIMAFakeFrameVideoWindow *video = [IIMAFakeFrameVideoWindow new];
  video.frame = NSMakeRect(0, 0, 640, 360);
  video.remainingThrows = 1;
  IIMAFakeFrameVideoView *view = [IIMAFakeFrameVideoView new];
  view.fakeContext = [IIMAFakeFrameOpenGLContext new];
  IIMAInstallFakeSession(session, parent, video, view);

  @try {
    iima_native_video_update_window_frame(session);
  } @catch (NSException *exception) {
    IIMAFail([NSString stringWithFormat:@"frame exception leaked: %@", exception]);
  }
  IIMARequire(video.setFrameCalls == 1, @"initial frame update was not attempted exactly once");
  IIMARequire(iima_native_video_frame_retry_attempts[session] != nil, @"retry was not scheduled");

  // The bounded recovery must remain available independently of live-resize coalescing. A
  // transient WindowServer failure is retried after its short backoff and still performs the
  // stronger surface refresh needed after a possible partial AppKit commit.
  IIMAPumpMainQueue(0.15);
  IIMARequire(video.setFrameCalls == 2, @"frame recovery did not run exactly once");
  IIMARequire(!video.lastDisplayArgument, @"frame recovery requested synchronous display");
  IIMARequire(NSEqualRects(video.frame, parent.frame), @"frame recovery did not apply target frame");
  IIMARequire(parent.childWindow == video, @"frame recovery did not restore child ownership");
  IIMARequire(video.orderCalls == 1, @"frame recovery did not restore child ordering");
  IIMARequire(iima_native_video_frame_retry_attempts[session] == nil, @"successful recovery retained retry state");
  IIMARequire(![iima_native_video_force_surface_updates containsObject:session], @"successful recovery retained force-refresh state");
  IIMARemoveFakeSession(session);
}

static void IIMATestLiveResizeExceptionFallsBackToBoundedRetry(void) {
  NSString *session = @"live-throw-before-commit";
  IIMAFakeFrameParentWindow *parent = [IIMAFakeFrameParentWindow new];
  parent.frame = NSMakeRect(40, 80, 960, 540);
  IIMAFakeFrameVideoWindow *video = [IIMAFakeFrameVideoWindow new];
  video.frame = NSMakeRect(0, 0, 640, 360);
  video.remainingThrows = 1;
  IIMAFakeFrameVideoView *view = [IIMAFakeFrameVideoView new];
  view.fakeContext = [IIMAFakeFrameOpenGLContext new];
  IIMAInstallFakeSession(session, parent, video, view);
  iima_native_video_observe_parent_window(session);

  [[NSNotificationCenter defaultCenter]
    postNotificationName:NSWindowWillStartLiveResizeNotification
                  object:parent];
  IIMAPumpMainQueue(0.02);
  IIMARequire(video.setFrameCalls == 1,
              @"live fast path did not attempt the frame that throws");
  IIMARequire(iima_native_video_frame_retry_attempts[session] != nil,
              @"live fast-path exception did not enter bounded recovery");
  IIMARequire([iima_native_video_force_surface_updates containsObject:session],
              @"live fast-path exception lost the final surface-refresh requirement");

  IIMAPumpMainQueue(0.15);
  IIMARequire(video.setFrameCalls == 2,
              @"live fast-path exception was not retried exactly once");
  IIMARequire(NSEqualRects(video.frame, parent.frame),
              @"live fast-path recovery did not apply the current parent frame");
  IIMARequire(iima_native_video_frame_retry_attempts[session] == nil,
              @"successful live fast-path recovery retained retry state");
  IIMARemoveFakeSession(session);
}

static void IIMATestPersistentLiveResizeExceptionPreservesBackoff(void) {
  NSString *session = @"live-persistent-throw";
  IIMAFakeFrameParentWindow *parent = [IIMAFakeFrameParentWindow new];
  parent.frame = NSMakeRect(40, 80, 640, 360);
  IIMAFakeFrameVideoWindow *video = [IIMAFakeFrameVideoWindow new];
  video.frame = NSMakeRect(0, 0, 320, 180);
  video.remainingThrows = NSUIntegerMax;
  IIMAFakeFrameVideoView *view = [IIMAFakeFrameVideoView new];
  view.fakeContext = [IIMAFakeFrameOpenGLContext new];
  IIMAInstallFakeSession(session, parent, video, view);
  iima_native_video_observe_parent_window(session);

  [[NSNotificationCenter defaultCenter]
    postNotificationName:NSWindowWillStartLiveResizeNotification
                  object:parent];
  IIMAPumpMainQueue(0.02);
  IIMARequire(video.setFrameCalls == 1,
              @"persistent live exception did not begin with one fast-path attempt");
  IIMARequire(iima_native_video_frame_retry_attempts[session].unsignedIntegerValue == 1,
              @"persistent live exception did not retain retry attempt one");

  // Keep emitting resize events beyond the complete 50/100/200 ms retry ladder. They must use
  // the pending backoff instead of cancelling it and starting a new attempt for every event.
  for (NSUInteger index = 0; index < 45; index += 1) {
    parent.frame = NSMakeRect(40, 80, 641 + index, 361 + index);
    [[NSNotificationCenter defaultCenter]
      postNotificationName:NSWindowDidResizeNotification
                    object:parent];
    IIMAPumpMainQueue(0.01);
  }
  IIMARequire(video.setFrameCalls == 4,
              @"continuous resize notifications bypassed the bounded retry ladder");
  IIMARequire(iima_native_video_frame_retry_attempts[session] == nil,
              @"persistent live exception retained a retry after the bounded ladder");
  IIMARequire([iima_native_video_suspended_live_frame_updates containsObject:session],
              @"persistent live exception did not suspend event-frequency fast updates");
  IIMARemoveFakeSession(session);
}

static void IIMATestChildWindowShapeTracksParentPresentation(void) {
  NSString *session = @"window-shape";
  IIMAFakeFrameParentWindow *parent = [IIMAFakeFrameParentWindow new];
  parent.frame = NSMakeRect(20, 40, 960, 540);
  parent.styleMask = NSWindowStyleMaskTitled | NSWindowStyleMaskResizable;

  IIMAFakeFrameVideoWindow *video = [IIMAFakeFrameVideoWindow new];
  video.frame = parent.frame;
  NSView *frameView = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 960, 540)];
  NSView *contentView = [[NSView alloc] initWithFrame:frameView.bounds];
  [frameView addSubview:contentView];
  video.contentView = contentView;

  IIMAFakeFrameVideoView *view = [IIMAFakeFrameVideoView new];
  view.fakeContext = [IIMAFakeFrameOpenGLContext new];
  IIMAInstallFakeSession(session, parent, video, view);

  IIMARequire(iima_native_video_apply_window_frame(session, NO),
              @"titled window shape synchronization failed");
  IIMARequire(frameView.wantsLayer && frameView.layer != nil,
              @"video frame container did not become layer-backed");
  IIMARequire(frameView.layer.cornerRadius == 10.0,
              @"ordinary titled parent did not apply the player corner radius");
  IIMARequire(frameView.layer.masksToBounds,
              @"video frame container does not clip its child OpenGL surface");
  IIMARequire(CGColorEqualToColor(frameView.layer.backgroundColor, NSColor.blackColor.CGColor),
              @"video frame container does not own the black background");
  if (@available(macOS 10.15, *)) {
    IIMARequire([frameView.layer.cornerCurve isEqualToString:kCACornerCurveContinuous],
                @"ordinary player corners are not continuous AppKit-style corners");
  }

  parent.styleMask |= NSWindowStyleMaskFullScreen;
  IIMARequire(iima_native_video_apply_window_frame(session, NO),
              @"native fullscreen shape synchronization failed");
  IIMARequire(frameView.layer.cornerRadius == 0.0,
              @"native fullscreen retained rounded child-window corners");
  IIMARequire(frameView.layer.masksToBounds,
              @"native fullscreen discarded the outer clipping boundary");

  parent.styleMask = NSWindowStyleMaskBorderless;
  IIMARequire(iima_native_video_apply_window_frame(session, NO),
              @"legacy fullscreen shape synchronization failed");
  IIMARequire(frameView.layer.cornerRadius == 0.0,
              @"legacy untitled fullscreen retained rounded child-window corners");

  parent.styleMask = NSWindowStyleMaskTitled | NSWindowStyleMaskResizable;
  IIMARequire(iima_native_video_apply_window_frame(session, NO),
              @"windowed shape restoration failed");
  IIMARequire(frameView.layer.cornerRadius == 10.0,
              @"leaving fullscreen did not restore rounded player corners");
  IIMARequire(video.invalidateShadowCalls == 4,
              @"player shadow was not refreshed for every shape transition");
  IIMARemoveFakeSession(session);
}

static void IIMATestLiveResizeTracksEveryMainQueueTurnAndFinalizes(void) {
  NSString *session = @"live-resize";
  IIMAFakeFrameParentWindow *parent = [IIMAFakeFrameParentWindow new];
  parent.frame = NSMakeRect(50, 60, 640, 360);
  parent.visible = YES;
  parent.number = 73;
  parent.styleMask = NSWindowStyleMaskTitled | NSWindowStyleMaskResizable;
  IIMAFakeFrameVideoWindow *video = [IIMAFakeFrameVideoWindow new];
  video.frame = parent.frame;
  NSView *frameView = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 640, 360)];
  NSView *contentView = [[NSView alloc] initWithFrame:frameView.bounds];
  [frameView addSubview:contentView];
  video.contentView = contentView;
  IIMAFakeFrameVideoView *view = [IIMAFakeFrameVideoView new];
  view.fakeContext = [IIMAFakeFrameOpenGLContext new];
  IIMAInstallFakeSession(session, parent, video, view);
  iima_native_video_observe_parent_window(session);

  [[NSNotificationCenter defaultCenter]
    postNotificationName:NSWindowWillStartLiveResizeNotification
                  object:parent];
  IIMARequire([iima_native_video_live_resize_sessions containsObject:session],
              @"live-resize start notification did not activate the session");

  // Multiple resize notifications in one AppKit turn are coalesced, without synchronously
  // mutating a child NSWindow from the notification stack.
  parent.frame = NSMakeRect(50, 60, 800, 450);
  [[NSNotificationCenter defaultCenter]
    postNotificationName:NSWindowDidResizeNotification
                  object:parent];
  [[NSNotificationCenter defaultCenter]
    postNotificationName:NSWindowDidResizeNotification
                  object:parent];
  IIMARequire(video.setFrameCalls == 0,
              @"live resize mutated the child window on the notification stack");
  IIMARequire([iima_native_video_live_frame_updates containsObject:session],
              @"live resize did not enqueue a main-turn update");
  IIMAPumpMainQueue(0.02);
  IIMARequire(video.setFrameCalls == 1,
              @"same-turn resize notifications were not coalesced into one update");
  IIMARequire(NSEqualRects(video.frame, parent.frame),
              @"first live-resize turn did not apply the current parent frame");
  IIMARequire(view.renderRequests == 1,
              @"first live-resize turn did not request a renderer-owned paint");
  IIMARequire(video.invalidateShadowCalls == 0,
              @"live fast path performed expensive shape synchronization");

  // A resize arriving on the next run-loop turn must advance immediately instead of restarting a
  // trailing 75 ms debounce. This is the contract required for video to track the drag smoothly.
  parent.frame = NSMakeRect(50, 60, 960, 540);
  [[NSNotificationCenter defaultCenter]
    postNotificationName:NSWindowDidResizeNotification
                  object:parent];
  IIMAPumpMainQueue(0.02);
  IIMARequire(video.setFrameCalls == 2,
              @"next-turn resize did not advance the child window immediately");
  IIMARequire(NSEqualRects(video.frame, parent.frame),
              @"next live-resize turn did not use the latest parent frame");
  IIMARequire(view.renderRequests == 2,
              @"next live-resize turn did not request a paint");

  // End-live-resize must run the full synchronization path even when the frame is already equal:
  // refresh the OpenGL surface, restore shape/shadow, parent ownership, and ordering.
  [[NSNotificationCenter defaultCenter]
    postNotificationName:NSWindowDidEndLiveResizeNotification
                  object:parent];
  IIMARequire(![iima_native_video_live_resize_sessions containsObject:session],
              @"live-resize end notification retained the active session");
  IIMAPumpMainQueue(0.02);
  IIMARequire(video.setFrameCalls == 2,
              @"final full synchronization redundantly changed an equal frame");
  IIMARequire(view.fakeContext.updateCalls == 1,
              @"end-live-resize did not refresh the OpenGL context");
  IIMARequire(view.renderRequests == 3,
              @"end-live-resize did not request a final paint");
  IIMARequire(video.invalidateShadowCalls == 1,
              @"end-live-resize did not perform final shape synchronization");
  IIMARequire(parent.childWindow == video,
              @"end-live-resize did not restore child ownership");
  IIMARequire(video.orderCalls == 1,
              @"end-live-resize did not restore child ordering");
  IIMARemoveFakeSession(session);
}

static void IIMATestLiveResizeTracksInsideEventTrackingMode(void) {
  NSString *session = @"live-resize-event-tracking";
  IIMAFakeFrameParentWindow *parent = [IIMAFakeFrameParentWindow new];
  parent.frame = NSMakeRect(50, 60, 640, 360);
  parent.visible = YES;
  parent.number = 74;
  parent.styleMask = NSWindowStyleMaskTitled | NSWindowStyleMaskResizable;
  IIMAFakeFrameVideoWindow *video = [IIMAFakeFrameVideoWindow new];
  video.frame = parent.frame;
  IIMAFakeFrameVideoView *view = [IIMAFakeFrameVideoView new];
  view.fakeContext = [IIMAFakeFrameOpenGLContext new];
  IIMAInstallFakeSession(session, parent, video, view);
  iima_native_video_observe_parent_window(session);

  [[NSNotificationCenter defaultCenter]
    postNotificationName:NSWindowWillStartLiveResizeNotification
                  object:parent];

  // AppKit keeps the main thread in NSEventTrackingRunLoopMode while the resize button remains
  // held. The production dispatch_async(main) fast path must therefore advance in that mode, not
  // only after AppKit returns to NSDefaultRunLoopMode when the mouse is released.
  parent.frame = NSMakeRect(50, 60, 800, 450);
  [[NSNotificationCenter defaultCenter]
    postNotificationName:NSWindowDidResizeNotification
                  object:parent];
  IIMARequire(video.setFrameCalls == 0,
              @"event-tracking resize mutated the child on the notification stack");
  IIMAPumpEventTrackingMainQueue(0.02);
  IIMARequire(video.setFrameCalls == 1,
              @"live child update did not execute in NSEventTrackingRunLoopMode");
  IIMARequire(NSEqualRects(video.frame, parent.frame),
              @"event-tracking live update did not apply the held-drag frame");
  IIMARequire(view.renderRequests == 1,
              @"event-tracking live update did not request a paint");

  parent.frame = NSMakeRect(50, 60, 960, 540);
  [[NSNotificationCenter defaultCenter]
    postNotificationName:NSWindowDidResizeNotification
                  object:parent];
  IIMAPumpEventTrackingMainQueue(0.02);
  IIMARequire(video.setFrameCalls == 2,
              @"second held-drag turn did not advance in event-tracking mode");
  IIMARequire(NSEqualRects(video.frame, parent.frame),
              @"second event-tracking turn did not apply the latest frame");
  IIMARequire(view.renderRequests == 2,
              @"second event-tracking turn did not request a paint");

  [[NSNotificationCenter defaultCenter]
    postNotificationName:NSWindowDidEndLiveResizeNotification
                  object:parent];
  IIMAPumpMainQueue(0.02);
  IIMARequire(view.fakeContext.updateCalls == 1,
              @"event-tracking resize did not finalize the OpenGL surface");
  IIMARequire(view.renderRequests == 3,
              @"event-tracking resize did not request the final paint");
  IIMARemoveFakeSession(session);
}

static void IIMATestLiveResizeGenerationRejectsReinstalledSessionABA(void) {
  NSString *session = @"live-resize-aba";
  IIMAFakeFrameParentWindow *oldParent = [IIMAFakeFrameParentWindow new];
  oldParent.frame = NSMakeRect(10, 20, 640, 360);
  IIMAFakeFrameVideoWindow *oldVideo = [IIMAFakeFrameVideoWindow new];
  oldVideo.frame = oldParent.frame;
  IIMAFakeFrameVideoView *oldView = [IIMAFakeFrameVideoView new];
  oldView.fakeContext = [IIMAFakeFrameOpenGLContext new];
  IIMAInstallFakeSession(session, oldParent, oldVideo, oldView);

  // Queue a live update for the old session, then remove and reinstall the same label before the
  // main queue can execute it. A membership-only token lets this stale block consume the new
  // session's token and mutate its window; the generation must distinguish the two lifetimes.
  iima_native_video_begin_live_resize(session);
  NSNumber *oldGeneration = iima_native_video_live_frame_update_generations[session];
  IIMARequire(oldGeneration != nil, @"old live-resize session did not install a generation");
  IIMARemoveFakeSession(session);
  IIMARequire(iima_native_video_live_frame_update_generations[session] == nil,
              @"fake-session removal retained the old live generation");

  IIMAFakeFrameParentWindow *newParent = [IIMAFakeFrameParentWindow new];
  newParent.frame = NSMakeRect(30, 40, 960, 540);
  IIMAFakeFrameVideoWindow *newVideo = [IIMAFakeFrameVideoWindow new];
  newVideo.frame = NSMakeRect(0, 0, 320, 180);
  IIMAFakeFrameVideoView *newView = [IIMAFakeFrameVideoView new];
  newView.fakeContext = [IIMAFakeFrameOpenGLContext new];
  IIMAInstallFakeSession(session, newParent, newVideo, newView);

  __block NSUInteger callsObservedBetweenQueuedBlocks = NSUIntegerMax;
  dispatch_async(dispatch_get_main_queue(), ^{
    callsObservedBetweenQueuedBlocks = newVideo.setFrameCalls;
  });
  iima_native_video_begin_live_resize(session);
  NSNumber *newGeneration = iima_native_video_live_frame_update_generations[session];
  IIMARequire(newGeneration != nil, @"reinstalled live-resize session did not install a generation");
  IIMARequire(![newGeneration isEqualToNumber:oldGeneration],
              @"reinstalled live-resize session reused the stale generation");

  IIMAPumpMainQueue(0.02);
  IIMARequire(callsObservedBetweenQueuedBlocks == 0,
              @"stale queued block consumed the reinstalled session's live token");
  IIMARequire(newVideo.setFrameCalls == 1,
              @"reinstalled session's own live block did not execute exactly once");
  IIMARequire(NSEqualRects(newVideo.frame, newParent.frame),
              @"reinstalled session's live block did not apply its parent frame");
  IIMARequire(newView.renderRequests == 1,
              @"reinstalled session's live block did not request exactly one paint");
  IIMARequire(iima_native_video_live_frame_update_generations[session] == nil,
              @"completed reinstalled-session update retained its generation");
  IIMARequire(![iima_native_video_live_frame_updates containsObject:session],
              @"completed reinstalled-session update retained its membership token");
  IIMARemoveFakeSession(session);
}

static void IIMATestPartialFrameCommitRefreshesOpenGLSurface(void) {
  NSString *session = @"throw-after-commit";
  IIMAFakeFrameParentWindow *parent = [IIMAFakeFrameParentWindow new];
  parent.frame = NSMakeRect(20, 30, 1280, 720);
  IIMAFakeFrameVideoWindow *video = [IIMAFakeFrameVideoWindow new];
  video.frame = NSMakeRect(0, 0, 640, 360);
  video.remainingThrows = 1;
  video.commitFrameBeforeThrow = YES;
  IIMAFakeFrameVideoView *view = [IIMAFakeFrameVideoView new];
  view.fakeContext = [IIMAFakeFrameOpenGLContext new];
  IIMAInstallFakeSession(session, parent, video, view);

  iima_native_video_update_window_frame(session);
  IIMARequire(NSEqualRects(video.frame, parent.frame), @"fake partial commit did not occur");
  IIMAPumpMainQueue(0.15);

  IIMARequire(video.setFrameCalls == 1, @"equal-frame recovery redundantly resized the window");
  IIMARequire(!video.lastDisplayArgument, @"partial commit attempted synchronous display");
  IIMARequire(view.fakeContext.updateCalls == 1, @"partial commit did not refresh OpenGL context");
  IIMARequire(view.renderRequests == 1, @"partial commit did not request a recovery render");
  IIMARequire(parent.childWindow == video, @"partial-commit recovery did not restore child ownership");
  IIMARequire(iima_native_video_frame_retry_attempts[session] == nil, @"partial-commit retry remained pending");
  IIMARemoveFakeSession(session);
}

static void IIMATestPersistentExceptionIsBounded(void) {
  NSString *session = @"persistent-throw";
  IIMAFakeFrameParentWindow *parent = [IIMAFakeFrameParentWindow new];
  parent.frame = NSMakeRect(10, 10, 800, 450);
  IIMAFakeFrameVideoWindow *video = [IIMAFakeFrameVideoWindow new];
  video.remainingThrows = NSUIntegerMax;
  IIMAFakeFrameVideoView *view = [IIMAFakeFrameVideoView new];
  view.fakeContext = [IIMAFakeFrameOpenGLContext new];
  IIMAInstallFakeSession(session, parent, video, view);

  iima_native_video_update_window_frame(session);
  IIMAPumpMainQueue(0.50);

  IIMARequire(video.setFrameCalls == 4, @"persistent failure did not stop after three retries");
  IIMARequire(iima_native_video_frame_retry_attempts[session] == nil, @"bounded retries remained pending");
  IIMAPumpMainQueue(0.25);
  IIMARequire(video.setFrameCalls == 4, @"persistent failure kept retrying after the bound");
  IIMARemoveFakeSession(session);
}

static void IIMATestSessionRemovalCancelsPendingFrameWork(void) {
  NSString *updateSession = @"cleanup-update";
  IIMAFakeFrameParentWindow *updateParent = [IIMAFakeFrameParentWindow new];
  updateParent.frame = NSMakeRect(0, 0, 900, 500);
  IIMAFakeFrameVideoWindow *updateVideo = [IIMAFakeFrameVideoWindow new];
  IIMAFakeFrameVideoView *updateView = [IIMAFakeFrameVideoView new];
  updateView.fakeContext = [IIMAFakeFrameOpenGLContext new];
  IIMAInstallFakeSession(updateSession, updateParent, updateVideo, updateView);

  iima_native_video_begin_live_resize(updateSession);
  iima_native_video_schedule_window_frame_update(updateSession);
  IIMARequire(iima_native_video_frame_update_generations[updateSession] != nil, @"cleanup update was not pending");
  IIMARequire(iima_native_video_live_frame_update_generations[updateSession] != nil,
              @"cleanup live update did not own a generation");
  iima_native_video_remove_session("cleanup-update");
  IIMARequire(iima_native_video_frame_update_generations[updateSession] == nil, @"session removal retained pending update");
  IIMARequire(iima_native_video_live_frame_update_generations[updateSession] == nil,
              @"session removal retained pending live generation");
  IIMARequire(![iima_native_video_live_frame_updates containsObject:updateSession], @"session removal retained pending live update");
  IIMARequire(![iima_native_video_live_resize_sessions containsObject:updateSession], @"session removal retained live-resize state");
  IIMARequire(![iima_native_video_force_surface_updates containsObject:updateSession], @"session removal retained force-refresh state");
  IIMARequire(iima_native_video_frame_retry_attempts[updateSession] == nil, @"session removal retained retry state");
  IIMARequire(updateView.detachCalls == 1 && updateView.removalCalls == 1, @"session removal did not clean fake view");
  IIMARequire(updateVideo.closeCalls == 1, @"session removal did not close fake video window");
  IIMAPumpMainQueue(0.10);
  IIMARequire(updateVideo.setFrameCalls == 0, @"cancelled pending update ran after session removal");

  NSString *retrySession = @"cleanup-retry";
  IIMAFakeFrameParentWindow *retryParent = [IIMAFakeFrameParentWindow new];
  retryParent.frame = NSMakeRect(0, 0, 1100, 620);
  IIMAFakeFrameVideoWindow *retryVideo = [IIMAFakeFrameVideoWindow new];
  retryVideo.remainingThrows = 1;
  IIMAFakeFrameVideoView *retryView = [IIMAFakeFrameVideoView new];
  retryView.fakeContext = [IIMAFakeFrameOpenGLContext new];
  IIMAInstallFakeSession(retrySession, retryParent, retryVideo, retryView);

  iima_native_video_update_window_frame(retrySession);
  IIMARequire(iima_native_video_frame_retry_attempts[retrySession] != nil, @"cleanup retry was not pending");
  iima_native_video_remove_session("cleanup-retry");
  IIMARequire(iima_native_video_frame_retry_attempts[retrySession] == nil, @"session removal retained delayed retry");
  IIMARequire(iima_native_video_frame_update_generations[retrySession] == nil, @"session removal retained update state");
  IIMARequire(iima_native_video_live_frame_update_generations[retrySession] == nil,
              @"session removal retained live generation state");
  IIMARequire(![iima_native_video_live_frame_updates containsObject:retrySession], @"session removal retained pending live update");
  IIMARequire(![iima_native_video_live_resize_sessions containsObject:retrySession], @"session removal retained live-resize state");
  IIMARequire(![iima_native_video_force_surface_updates containsObject:retrySession], @"session removal retained force-refresh state");
  IIMAPumpMainQueue(0.15);
  IIMARequire(retryVideo.setFrameCalls == 1, @"cancelled delayed retry ran after session removal");
}

int main(void) {
  @autoreleasepool {
    iima_native_video_ensure_sessions();
    IIMATestChildWindowShapeTracksParentPresentation();
    IIMATestLiveResizeTracksEveryMainQueueTurnAndFinalizes();
    IIMATestLiveResizeTracksInsideEventTrackingMode();
    IIMATestLiveResizeGenerationRejectsReinstalledSessionABA();
    IIMATestExceptionIsCaughtAndRetried();
    IIMATestLiveResizeExceptionFallsBackToBoundedRetry();
    IIMATestPersistentLiveResizeExceptionPreservesBackoff();
    IIMATestPartialFrameCommitRefreshesOpenGLSurface();
    IIMATestPersistentExceptionIsBounded();
    IIMATestSessionRemovalCancelsPendingFrameWork();
    printf("native video frame exception/live-resize harness: 10 scenarios passed\n");
  }
  return 0;
}
