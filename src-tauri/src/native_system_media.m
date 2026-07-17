#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <IOKit/pwr_mgt/IOPMLib.h>
#import <MediaPlayer/MediaPlayer.h>

#include <dispatch/dispatch.h>
#include <mach/mach_error.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef int32_t (*IIMASystemMediaCommandCallback)(int32_t command,
                                                   double value,
                                                   void *context);

enum {
  IIMASystemMediaCommandPlay = 1,
  IIMASystemMediaCommandPause = 2,
  IIMASystemMediaCommandTogglePlayPause = 3,
  IIMASystemMediaCommandStop = 4,
  IIMASystemMediaCommandNextTrack = 5,
  IIMASystemMediaCommandPreviousTrack = 6,
  IIMASystemMediaCommandChangeRepeatMode = 7,
  IIMASystemMediaCommandChangePlaybackRate = 8,
  IIMASystemMediaCommandSkipForward = 9,
  IIMASystemMediaCommandSkipBackward = 10,
  IIMASystemMediaCommandChangePlaybackPosition = 11,
};

enum {
  IIMASystemMediaCommandSuccess = 0,
  IIMASystemMediaCommandNoSuchContent = 1,
};

static IIMASystemMediaCommandCallback iima_system_media_command_callback = NULL;
static void *iima_system_media_command_context = NULL;
static uint64_t iima_system_media_remote_generation = 0;
static uint64_t iima_system_media_now_playing_generation = 0;
static uint64_t iima_system_media_power_generation = 0;
static IOPMAssertionID iima_system_media_sleep_assertion = kIOPMNullAssertionID;
static BOOL iima_system_media_preventing_sleep = NO;

static char *iima_system_media_copy_utf8(NSString *value) {
  if (value == nil || value.UTF8String == NULL) {
    return NULL;
  }
  return strdup(value.UTF8String);
}

static void iima_system_media_set_error(char **error_out, NSString *message) {
  if (error_out != NULL) {
    *error_out = iima_system_media_copy_utf8(message);
  }
}

static void iima_system_media_on_main_sync(dispatch_block_t block) {
  if (NSThread.isMainThread) {
    block();
  } else {
    dispatch_sync(dispatch_get_main_queue(), block);
  }
}

static MPRemoteCommandHandlerStatus iima_system_media_dispatch_command(int32_t command,
                                                                       double value) {
  IIMASystemMediaCommandCallback callback = NULL;
  void *context = NULL;
  @synchronized(NSApplication.class) {
    callback = iima_system_media_command_callback;
    context = iima_system_media_command_context;
  }
  if (callback == NULL) {
    return MPRemoteCommandHandlerStatusCommandFailed;
  }
  int32_t status = callback(command, value, context);
  if (status == IIMASystemMediaCommandSuccess) {
    return MPRemoteCommandHandlerStatusSuccess;
  }
  if (status == IIMASystemMediaCommandNoSuchContent) {
    return MPRemoteCommandHandlerStatusNoSuchContent;
  }
  return MPRemoteCommandHandlerStatusCommandFailed;
}

static void iima_system_media_remove_remote_targets(MPRemoteCommandCenter *center) {
  [center.playCommand removeTarget:nil];
  [center.pauseCommand removeTarget:nil];
  [center.togglePlayPauseCommand removeTarget:nil];
  [center.stopCommand removeTarget:nil];
  [center.nextTrackCommand removeTarget:nil];
  [center.previousTrackCommand removeTarget:nil];
  [center.changeRepeatModeCommand removeTarget:nil];
  [center.changeShuffleModeCommand removeTarget:nil];
  [center.changePlaybackRateCommand removeTarget:nil];
  [center.skipForwardCommand removeTarget:nil];
  [center.skipBackwardCommand removeTarget:nil];
  [center.changePlaybackPositionCommand removeTarget:nil];
}

static void iima_system_media_set_remote_enabled(MPRemoteCommandCenter *center,
                                                  BOOL enabled) {
  center.playCommand.enabled = enabled;
  center.pauseCommand.enabled = enabled;
  center.togglePlayPauseCommand.enabled = enabled;
  center.stopCommand.enabled = enabled;
  center.nextTrackCommand.enabled = enabled;
  center.previousTrackCommand.enabled = enabled;
  center.changeRepeatModeCommand.enabled = enabled;
  center.changeShuffleModeCommand.enabled = NO;
  center.changePlaybackRateCommand.enabled = enabled;
  center.skipForwardCommand.enabled = enabled;
  center.skipBackwardCommand.enabled = enabled;
  center.changePlaybackPositionCommand.enabled = enabled;
}

void iima_system_media_set_remote_commands_enabled(
    int32_t enabled,
    uint64_t generation,
    IIMASystemMediaCommandCallback callback,
    void *context) {
  iima_system_media_on_main_sync(^{
    MPRemoteCommandCenter *center = MPRemoteCommandCenter.sharedCommandCenter;
    @synchronized(NSApplication.class) {
      if (generation <= iima_system_media_remote_generation) {
        return;
      }
      iima_system_media_remote_generation = generation;
      if (enabled == 0 && generation > iima_system_media_now_playing_generation) {
        iima_system_media_now_playing_generation = generation;
      }
      iima_system_media_command_callback = enabled != 0 ? callback : NULL;
      iima_system_media_command_context = enabled != 0 ? context : NULL;
    }
    iima_system_media_remove_remote_targets(center);
    iima_system_media_set_remote_enabled(center, enabled != 0);

    if (enabled == 0) {
      MPNowPlayingInfoCenter.defaultCenter.nowPlayingInfo = nil;
      MPNowPlayingInfoCenter.defaultCenter.playbackState = MPNowPlayingPlaybackStateUnknown;
      return;
    }

    [center.playCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
      (void)event;
      return iima_system_media_dispatch_command(IIMASystemMediaCommandPlay, 0.0);
    }];
    [center.pauseCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
      (void)event;
      return iima_system_media_dispatch_command(IIMASystemMediaCommandPause, 0.0);
    }];
    [center.togglePlayPauseCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
      (void)event;
      return iima_system_media_dispatch_command(IIMASystemMediaCommandTogglePlayPause, 0.0);
    }];
    [center.stopCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
      (void)event;
      return iima_system_media_dispatch_command(IIMASystemMediaCommandStop, 0.0);
    }];
    [center.nextTrackCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
      (void)event;
      return iima_system_media_dispatch_command(IIMASystemMediaCommandNextTrack, 0.0);
    }];
    [center.previousTrackCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
      (void)event;
      return iima_system_media_dispatch_command(IIMASystemMediaCommandPreviousTrack, 0.0);
    }];
    [center.changeRepeatModeCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
      (void)event;
      return iima_system_media_dispatch_command(IIMASystemMediaCommandChangeRepeatMode, 0.0);
    }];

    center.changePlaybackRateCommand.supportedPlaybackRates = @[@0.5, @1.0, @1.5, @2.0];
    [center.changePlaybackRateCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
      MPChangePlaybackRateCommandEvent *rateEvent =
          (MPChangePlaybackRateCommandEvent *)event;
      return iima_system_media_dispatch_command(IIMASystemMediaCommandChangePlaybackRate,
                                                rateEvent.playbackRate);
    }];

    center.skipForwardCommand.preferredIntervals = @[@15.0];
    [center.skipForwardCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
      MPSkipIntervalCommandEvent *skipEvent = (MPSkipIntervalCommandEvent *)event;
      return iima_system_media_dispatch_command(IIMASystemMediaCommandSkipForward,
                                                skipEvent.interval);
    }];
    center.skipBackwardCommand.preferredIntervals = @[@15.0];
    [center.skipBackwardCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
      MPSkipIntervalCommandEvent *skipEvent = (MPSkipIntervalCommandEvent *)event;
      return iima_system_media_dispatch_command(IIMASystemMediaCommandSkipBackward,
                                                skipEvent.interval);
    }];
    [center.changePlaybackPositionCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent *event) {
      MPChangePlaybackPositionCommandEvent *positionEvent =
          (MPChangePlaybackPositionCommandEvent *)event;
      return iima_system_media_dispatch_command(IIMASystemMediaCommandChangePlaybackPosition,
                                                positionEvent.positionTime);
    }];
  });
}

static NSString *iima_system_media_optional_string(id value) {
  if (![value isKindOfClass:NSString.class] || [(NSString *)value length] == 0) {
    return nil;
  }
  return value;
}

int32_t iima_system_media_update_now_playing_json(const char *json_utf8,
                                                   uint64_t generation,
                                                   char **error_out) {
  if (error_out != NULL) {
    *error_out = NULL;
  }
  if (json_utf8 == NULL) {
    iima_system_media_set_error(error_out, @"Now Playing metadata is missing");
    return -1;
  }
  NSData *data = [NSData dataWithBytes:json_utf8 length:strlen(json_utf8)];
  NSError *parseError = nil;
  id decoded = [NSJSONSerialization JSONObjectWithData:data options:0 error:&parseError];
  if (![decoded isKindOfClass:NSDictionary.class]) {
    iima_system_media_set_error(
        error_out,
        parseError.localizedDescription ?: @"Now Playing metadata is not a dictionary");
    return -1;
  }
  NSDictionary *projection = decoded;
  __block NSString *applyError = nil;
  iima_system_media_on_main_sync(^{
    @synchronized(NSApplication.class) {
      if (generation <= iima_system_media_now_playing_generation) {
        return;
      }
      iima_system_media_now_playing_generation = generation;
    }
    NSNumber *duration = [projection[@"duration"] isKindOfClass:NSNumber.class]
        ? projection[@"duration"] : @0.0;
    NSNumber *elapsed = [projection[@"elapsed"] isKindOfClass:NSNumber.class]
        ? projection[@"elapsed"] : @0.0;
    NSNumber *rate = [projection[@"rate"] isKindOfClass:NSNumber.class]
        ? projection[@"rate"] : @1.0;
    NSNumber *defaultRate = [projection[@"default_rate"] isKindOfClass:NSNumber.class]
        ? projection[@"default_rate"] : @1.0;

    NSMutableDictionary<NSString *, id> *info = [NSMutableDictionary dictionary];
    NSString *mediaType = iima_system_media_optional_string(projection[@"media_type"]);
    if ([mediaType isEqualToString:@"audio"]) {
      info[MPMediaItemPropertyMediaType] = @(MPNowPlayingInfoMediaTypeAudio);
    } else if ([mediaType isEqualToString:@"video"]) {
      info[MPMediaItemPropertyMediaType] = @(MPNowPlayingInfoMediaTypeVideo);
    }
    NSString *title = iima_system_media_optional_string(projection[@"title"]);
    NSString *album = iima_system_media_optional_string(projection[@"album"]);
    NSString *artist = iima_system_media_optional_string(projection[@"artist"]);
    if (title != nil) {
      info[MPMediaItemPropertyTitle] = title;
    }
    if (album != nil) {
      info[MPMediaItemPropertyAlbumTitle] = album;
    }
    if (artist != nil) {
      info[MPMediaItemPropertyArtist] = artist;
    }
    info[MPMediaItemPropertyPlaybackDuration] = duration;
    info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = elapsed;
    info[MPNowPlayingInfoPropertyPlaybackRate] = rate;
    info[MPNowPlayingInfoPropertyDefaultPlaybackRate] = defaultRate;

    MPNowPlayingInfoCenter *center = MPNowPlayingInfoCenter.defaultCenter;
    center.nowPlayingInfo = info;
    NSString *state = iima_system_media_optional_string(projection[@"playback_state"]);
    if ([state isEqualToString:@"playing"]) {
      center.playbackState = MPNowPlayingPlaybackStatePlaying;
    } else if ([state isEqualToString:@"paused"]) {
      center.playbackState = MPNowPlayingPlaybackStatePaused;
    } else if ([state isEqualToString:@"stopped"]) {
      center.playbackState = MPNowPlayingPlaybackStateStopped;
    } else if ([state isEqualToString:@"unknown"] || state == nil) {
      center.playbackState = MPNowPlayingPlaybackStateUnknown;
    } else {
      applyError = [NSString stringWithFormat:@"Unknown Now Playing state: %@", state];
    }
  });
  if (applyError != nil) {
    iima_system_media_set_error(error_out, applyError);
    return -1;
  }
  return 0;
}

int32_t iima_system_media_set_prevent_display_sleep(int32_t prevent,
                                                     uint64_t generation,
                                                     char **error_out) {
  if (error_out != NULL) {
    *error_out = NULL;
  }
  @synchronized(NSApplication.class) {
    if (generation <= iima_system_media_power_generation) {
      return kIOReturnSuccess;
    }
    iima_system_media_power_generation = generation;
    if (prevent != 0) {
      if (iima_system_media_preventing_sleep) {
        return kIOReturnSuccess;
      }
      IOReturn status = IOPMAssertionCreateWithName(
          kIOPMAssertionTypeNoDisplaySleep,
          kIOPMAssertionLevelOn,
          CFSTR("IINA is playing video"),
          &iima_system_media_sleep_assertion);
      if (status == kIOReturnSuccess) {
        iima_system_media_preventing_sleep = YES;
        return status;
      }
      const char *description = mach_error_string(status);
      iima_system_media_set_error(
          error_out,
          [NSString stringWithFormat:
              @"IOPMAssertionCreateWithName returned 0x%08X (%s); cannot prevent display sleep",
              (uint32_t)status,
              description != NULL ? description : "unknown IOKit error"]);
      return status;
    }

    if (!iima_system_media_preventing_sleep) {
      return kIOReturnSuccess;
    }
    IOReturn status = IOPMAssertionRelease(iima_system_media_sleep_assertion);
    if (status == kIOReturnSuccess) {
      iima_system_media_preventing_sleep = NO;
      iima_system_media_sleep_assertion = kIOPMNullAssertionID;
      return status;
    }
    const char *description = mach_error_string(status);
    iima_system_media_set_error(
        error_out,
        [NSString stringWithFormat:
            @"IOPMAssertionRelease returned 0x%08X (%s); cannot release display-sleep assertion",
            (uint32_t)status,
            description != NULL ? description : "unknown IOKit error"]);
    return status;
  }
}

int32_t iima_system_media_show_sleep_failure_alert(const char *message_utf8) {
  NSString *message = message_utf8 != NULL
      ? [NSString stringWithUTF8String:message_utf8]
      : @"Cannot prevent display sleep";
  __block int32_t suppressed = 0;
  iima_system_media_on_main_sync(^{
    NSAlert *alert = [[NSAlert alloc] init];
    alert.alertStyle = NSAlertStyleWarning;
    alert.messageText = @"Cannot prevent display sleep!";
    alert.informativeText = message ?: @"macOS power management returned an unknown error.";
    alert.showsSuppressionButton = YES;
    alert.suppressionButton.title = @"Do not show this message again";
    [alert addButtonWithTitle:@"OK"];
    [alert runModal];
    suppressed = alert.suppressionButton.state == NSControlStateValueOn ? 1 : 0;
  });
  return suppressed;
}

void iima_system_media_free_string(char *value) {
  free(value);
}
