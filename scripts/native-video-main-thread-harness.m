#import <Cocoa/Cocoa.h>
#import <stdatomic.h>
#import <stdio.h>

extern int iima_native_video_attach_mpv_client(
  void *mpvHandle,
  const char *libraryPath,
  const char *sessionLabel
);
extern void iima_native_video_detach_mpv_client(const char *sessionLabel);
extern void iima_native_video_remove_session(const char *sessionLabel);
extern void iima_native_video_configure_color(
  const char *sessionLabel,
  int loadIccProfile,
  int enableHdrSupport,
  int enableToneMapping,
  int toneMappingTargetPeak,
  const char *toneMappingAlgorithm
);
extern void iima_native_video_request_color_refresh(const char *sessionLabel);
extern void iima_native_video_set_hdr_enabled(const char *sessionLabel, int enabled);
extern int iima_native_video_toggle_pip(
  const char *sessionLabel,
  int playing,
  const char *title,
  double videoWidth,
  double videoHeight,
  int originFullscreen
);
extern int iima_native_video_pip_is_active(void);
extern int iima_native_video_pip_is_active_for_session(const char *sessionLabel);
extern int iima_native_video_hdr_is_available(const char *sessionLabel);
extern int iima_native_video_hdr_is_enabled(const char *sessionLabel);
extern int iima_native_video_is_installed(const char *sessionLabel);
extern int iima_native_video_is_attached(const char *sessionLabel);
extern int iima_native_video_render_scheduler(const char *sessionLabel);
extern void iima_native_video_test_initialize_empty_sessions(void);

int main(void) {
  @autoreleasepool {
    static const char *missingSession = "main-thread-harness-missing";
    __block atomic_int finished;
    __block atomic_int failures;
    atomic_init(&finished, 0);
    atomic_init(&failures, 0);
    iima_native_video_test_initialize_empty_sessions();

    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
      @autoreleasepool {
        if (iima_native_video_attach_mpv_client(NULL, NULL, missingSession) != -1) {
          atomic_fetch_add(&failures, 1);
        }
        iima_native_video_detach_mpv_client(missingSession);
        iima_native_video_configure_color(missingSession, 1, 1, 0, 0, "auto");
        iima_native_video_request_color_refresh(missingSession);
        iima_native_video_set_hdr_enabled(missingSession, 1);
        if (iima_native_video_toggle_pip(missingSession, 0, "", 0, 0, 0) != -1) {
          atomic_fetch_add(&failures, 1);
        }
        if (iima_native_video_pip_is_active() != 0
            || iima_native_video_pip_is_active_for_session(missingSession) != 0
            || iima_native_video_hdr_is_available(missingSession) != 0
            || iima_native_video_hdr_is_enabled(missingSession) != 0
            || iima_native_video_is_installed(missingSession) != 0
            || iima_native_video_is_attached(missingSession) != 0
            || iima_native_video_render_scheduler(missingSession) != 0) {
          atomic_fetch_add(&failures, 1);
        }
        iima_native_video_remove_session(missingSession);
        atomic_store(&finished, 1);
      }
    });

    NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:10.0];
    while (!atomic_load(&finished) && deadline.timeIntervalSinceNow > 0) {
      @autoreleasepool {
        [[NSRunLoop mainRunLoop]
          runMode:NSDefaultRunLoopMode
          beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
      }
    }
    if (!atomic_load(&finished)) {
      fprintf(stderr, "native video main-thread harness timed out\n");
      return 2;
    }
    if (atomic_load(&failures) != 0) {
      fprintf(stderr, "native video main-thread harness reported %d failure(s)\n",
              atomic_load(&failures));
      return 1;
    }
    printf("native video main-thread harness: background C ABI checks passed\n");
    return 0;
  }
}
