#import <Cocoa/Cocoa.h>

int iima_native_configure_inspector_panel(void *windowPointer) {
  if (windowPointer == NULL) return -1;
  void (^configure)(void) = ^{
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    window.styleMask = window.styleMask |
      NSWindowStyleMaskTitled |
      NSWindowStyleMaskClosable |
      NSWindowStyleMaskMiniaturizable |
      NSWindowStyleMaskResizable |
      NSWindowStyleMaskUtilityWindow |
      NSWindowStyleMaskHUDWindow;
    window.hidesOnDeactivate = YES;
    window.releasedWhenClosed = NO;
    [window setFrameAutosaveName:@"IINAInspectorPanel"];
    window.level = NSFloatingWindowLevel;
    window.collectionBehavior = window.collectionBehavior |
      NSWindowCollectionBehaviorFullScreenAuxiliary;
  };
  if (NSThread.isMainThread) {
    configure();
  } else {
    dispatch_sync(dispatch_get_main_queue(), configure);
  }
  return 0;
}
