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
@end

@interface IIMAFakeFrameParentWindow : NSObject
@property(nonatomic) NSRect frame;
@property(nonatomic) BOOL visible;
@property(nonatomic) NSInteger number;
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

static void IIMAPumpMainQueue(NSTimeInterval seconds) {
  NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:seconds];
  while ([deadline timeIntervalSinceNow] > 0) {
    [[NSRunLoop currentRunLoop]
      runMode:NSDefaultRunLoopMode
      beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
  }
}

static void IIMARemoveFakeSession(NSString *session) {
  [iima_native_video_frame_retry_attempts removeObjectForKey:session];
  [iima_native_video_frame_update_generations removeObjectForKey:session];
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

  // Simulate more DidResize notifications arriving during the same animation. They must cancel
  // the retry against the intermediate frame, retain the force-refresh requirement, and wait for
  // a full quiet period before touching the child surface.
  iima_native_video_schedule_window_frame_update(session);
  iima_native_video_schedule_window_frame_update(session);
  IIMAPumpMainQueue(0.02);
  IIMARequire(video.setFrameCalls == 1, @"resize notification bypassed the quiet-period debounce");
  IIMARequire(iima_native_video_frame_retry_attempts[session] == nil, @"superseded retry remained pending");
  IIMARequire(iima_native_video_frame_update_generations[session] != nil, @"debounced recovery was not pending");
  IIMARequire([iima_native_video_force_surface_updates containsObject:session], @"debounced recovery lost its force-refresh requirement");

  IIMAPumpMainQueue(0.15);
  IIMARequire(video.setFrameCalls == 2, @"debounced frame recovery did not run exactly once");
  IIMARequire(!video.lastDisplayArgument, @"debounced frame recovery requested synchronous display");
  IIMARequire(NSEqualRects(video.frame, parent.frame), @"debounced recovery did not apply target frame");
  IIMARequire(parent.childWindow == video, @"debounced recovery did not restore child ownership");
  IIMARequire(video.orderCalls == 1, @"debounced recovery did not restore child ordering");
  IIMARequire(iima_native_video_frame_retry_attempts[session] == nil, @"successful recovery retained retry state");
  IIMARequire(iima_native_video_frame_update_generations[session] == nil, @"successful recovery retained debounce state");
  IIMARequire(![iima_native_video_force_surface_updates containsObject:session], @"successful recovery retained force-refresh state");
  IIMARemoveFakeSession(session);
}

static void IIMATestNotificationBurstRestartsQuietPeriod(void) {
  NSString *session = @"notification-debounce";
  IIMAFakeFrameParentWindow *parent = [IIMAFakeFrameParentWindow new];
  parent.frame = NSMakeRect(50, 60, 1024, 576);
  IIMAFakeFrameVideoWindow *video = [IIMAFakeFrameVideoWindow new];
  video.frame = NSMakeRect(0, 0, 320, 180);
  IIMAFakeFrameVideoView *view = [IIMAFakeFrameVideoView new];
  view.fakeContext = [IIMAFakeFrameOpenGLContext new];
  IIMAInstallFakeSession(session, parent, video, view);
  iima_native_video_observe_parent_window(session);

  NSArray<NSNotificationName> *names = @[
    NSWindowDidMoveNotification,
    NSWindowDidResizeNotification,
    NSWindowDidEnterFullScreenNotification,
    NSWindowDidExitFullScreenNotification,
    NSWindowDidChangeScreenNotification,
  ];
  uint64_t previousGeneration = 0;
  for (NSNotificationName name in names) {
    [[NSNotificationCenter defaultCenter] postNotificationName:name object:parent];
    NSNumber *generation = iima_native_video_frame_update_generations[session];
    IIMARequire(generation != nil, @"parent-window notification did not schedule a debounce");
    IIMARequire(generation.unsignedLongLongValue > previousGeneration, @"parent-window notification did not advance the generation");
    previousGeneration = generation.unsignedLongLongValue;
    IIMARequire(video.setFrameCalls == 0, @"parent-window notification mutated the frame on its notification stack");
  }

  // Let the first generation age most of the way to its deadline, then post one more resize. The
  // old blocks must expire harmlessly while the new generation receives a fresh full 75 ms.
  IIMAPumpMainQueue(0.04);
  [[NSNotificationCenter defaultCenter]
    postNotificationName:NSWindowDidResizeNotification
                  object:parent];
  uint64_t finalGeneration =
    iima_native_video_frame_update_generations[session].unsignedLongLongValue;
  IIMARequire(finalGeneration > previousGeneration, @"late burst event did not restart debounce generation");
  IIMARequire(video.setFrameCalls == 0, @"late burst event mutated the frame on its notification stack");

  IIMAPumpMainQueue(0.05);
  IIMARequire(video.setFrameCalls == 0, @"stale generation applied before the latest quiet period elapsed");
  IIMARequire(iima_native_video_frame_update_generations[session] != nil, @"latest debounce was cleared by a stale generation");

  IIMAPumpMainQueue(0.06);
  IIMARequire(video.setFrameCalls == 1, @"notification burst was not debounced into one final update");
  IIMARequire(!video.lastDisplayArgument, @"debounced update requested synchronous display");
  IIMARequire(view.renderRequests == 1, @"debounced frame update did not request a renderer-owned paint");
  IIMARequire(NSEqualRects(video.frame, parent.frame), @"debounced update used the wrong frame");
  IIMARequire(iima_native_video_frame_update_generations[session] == nil, @"debounced update retained its generation");
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

  iima_native_video_schedule_window_frame_update(updateSession);
  IIMARequire(iima_native_video_frame_update_generations[updateSession] != nil, @"cleanup update was not pending");
  iima_native_video_remove_session("cleanup-update");
  IIMARequire(iima_native_video_frame_update_generations[updateSession] == nil, @"session removal retained pending update");
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
  IIMARequire(![iima_native_video_force_surface_updates containsObject:retrySession], @"session removal retained force-refresh state");
  IIMAPumpMainQueue(0.15);
  IIMARequire(retryVideo.setFrameCalls == 1, @"cancelled delayed retry ran after session removal");
}

int main(void) {
  @autoreleasepool {
    iima_native_video_ensure_sessions();
    IIMATestNotificationBurstRestartsQuietPeriod();
    IIMATestExceptionIsCaughtAndRetried();
    IIMATestPartialFrameCommitRefreshesOpenGLSurface();
    IIMATestPersistentExceptionIsBounded();
    IIMATestSessionRemovalCancelsPendingFrameWork();
    printf("native video frame exception harness: 5 scenarios passed\n");
  }
  return 0;
}
