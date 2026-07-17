#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>

#include <dispatch/dispatch.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef int32_t (*IIMATouchBarActionCallback)(const char *session_label,
                                              int32_t action,
                                              double value,
                                              void *context);

enum {
  IIMATouchBarActionTogglePause = 1,
  IIMATouchBarActionVolumeDelta = 2,
  IIMATouchBarActionArrow = 3,
  IIMATouchBarActionSeekRelative = 4,
  IIMATouchBarActionPlaylistNavigate = 5,
  IIMATouchBarActionTogglePIP = 6,
  IIMATouchBarActionExitFullscreen = 7,
  IIMATouchBarActionSeekPercent = 8,
  IIMATouchBarActionSliderBegin = 9,
  IIMATouchBarActionSliderEnd = 10,
  IIMATouchBarActionToggleRemaining = 11,
};

@class IIMATouchBarController;

@interface IIMATouchBarSlider : NSSlider
@property(nonatomic, weak) IIMATouchBarController *iimaController;
@property(nonatomic) BOOL iimaIsTouching;
- (void)iimaSetDoubleValueSafely:(double)value;
- (void)iimaResetThumbnailCache;
@end

@interface IIMATouchBarSliderCell : NSSliderCell
@property(nonatomic) double iimaCachedThumbnailProgress;
@property(nonatomic, strong, nullable) NSImage *iimaBackgroundImage;
@end

@interface IIMATouchBarTimeField : NSTextField
@property(nonatomic, weak) IIMATouchBarController *iimaController;
@property(nonatomic) BOOL iimaCanToggle;
@end

@interface IIMATouchBarController : NSObject <NSTouchBarDelegate>
@property(nonatomic, weak) NSWindow *window;
@property(nonatomic, copy) NSString *sessionLabel;
@property(nonatomic, strong) NSDictionary<NSString *, NSString *> *labels;
@property(nonatomic, strong) NSTouchBar *touchBar;
@property(nonatomic, weak, nullable) NSButton *playPauseButton;
@property(nonatomic, strong) NSHashTable<NSButton *> *mediaButtons;
@property(nonatomic, weak, nullable) IIMATouchBarSlider *playSlider;
@property(nonatomic, weak, nullable) IIMATouchBarTimeField *currentTimeField;
@property(nonatomic, weak, nullable) IIMATouchBarTimeField *remainingTimeField;
@property(nonatomic, strong) NSLayoutConstraint *currentTimeWidth;
@property(nonatomic, strong) NSLayoutConstraint *remainingTimeWidth;
@property(nonatomic) BOOL hasMedia;
@property(nonatomic) BOOL paused;
@property(nonatomic) double position;
@property(nonatomic) double duration;
@property(nonatomic) double volume;
@property(nonatomic) NSInteger precision;
@property(nonatomic) BOOL showRemaining;
@property(nonatomic, copy) NSString *currentURL;
@property(nonatomic, copy) NSString *thumbnailSource;
@property(nonatomic) double thumbnailProgress;
@property(nonatomic, strong) NSMutableDictionary<NSNumber *, NSDictionary *> *thumbnails;
@property(nonatomic, strong) NSCache<NSString *, NSImage *> *thumbnailImages;
@property(nonatomic) IIMATouchBarActionCallback callback;
@property(nonatomic) void *callbackContext;

- (instancetype)initWithWindow:(NSWindow *)window
                   sessionLabel:(NSString *)sessionLabel
                         labels:(NSDictionary<NSString *, NSString *> *)labels
                       callback:(IIMATouchBarActionCallback)callback
                        context:(void *)context;
- (void)updateHasMedia:(BOOL)hasMedia
                paused:(BOOL)paused
              position:(double)position
              duration:(double)duration
                volume:(double)volume
             precision:(NSInteger)precision
         showRemaining:(BOOL)showRemaining
            currentURL:(NSString *)currentURL
      fullscreenEscape:(BOOL)fullscreenEscape;
- (void)updateThumbnails:(NSArray<NSDictionary *> *)thumbnails
                  source:(NSString *)source
                progress:(double)progress
                 replace:(BOOL)replace;
- (nullable NSDictionary *)thumbnailAtTime:(double)time;
- (nullable NSImage *)thumbnailImageAtTime:(double)time;
- (void)dispatchAction:(int32_t)action value:(double)value;
- (void)toggleRemainingFromTouch;
@end

static NSMutableDictionary<NSValue *, IIMATouchBarController *> *
    iima_touch_bar_controllers;

static void iima_touch_bar_on_main_sync(dispatch_block_t block) {
  if (NSThread.isMainThread) {
    block();
  } else {
    dispatch_sync(dispatch_get_main_queue(), block);
  }
}

static NSDictionary<NSString *, NSString *> *
iima_touch_bar_parse_labels(const char *labels_json) {
  if (labels_json == NULL) {
    return @{};
  }
  NSData *data = [NSData dataWithBytes:labels_json length:strlen(labels_json)];
  id value = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
  return [value isKindOfClass:NSDictionary.class] ? value : @{};
}

static NSString *iima_touch_bar_label(NSDictionary<NSString *, NSString *> *labels,
                                      NSString *key,
                                      NSString *fallback) {
  id value = labels[key];
  return [value isKindOfClass:NSString.class] && [value length] > 0 ? value
                                                                   : fallback;
}

static NSString *iima_touch_bar_format_time(double seconds,
                                             NSInteger precision) {
  if (!isfinite(seconds) || seconds < 0.0) {
    seconds = 0.0;
  }
  precision = MAX(0, MIN(3, precision));
  NSInteger total = (NSInteger)floor(seconds);
  NSInteger hours = total / 3600;
  NSInteger minutes = (total % 3600) / 60;
  NSString *secondText;
  if (precision > 0) {
    NSInteger width = precision + 3;
    secondText = [NSString stringWithFormat:@"%0*.*f", (int)width,
                                                (int)precision, fmod(seconds, 60.0)];
  } else {
    secondText = [NSString stringWithFormat:@"%02ld", (long)(total % 60)];
  }
  if (hours > 0) {
    return [NSString stringWithFormat:@"%ld:%02ld:%@", (long)hours,
                                      (long)minutes, secondText];
  }
  return [NSString stringWithFormat:@"%02ld:%@", (long)minutes, secondText];
}

static NSImage *iima_touch_bar_pip_image(void) {
  if (@available(macOS 11.0, *)) {
    NSImage *symbol = [NSImage imageWithSystemSymbolName:@"pip"
                                accessibilityDescription:@"Picture in Picture"];
    if (symbol != nil) {
      return symbol;
    }
  }
  return [NSImage imageNamed:NSImageNameTouchBarIconViewTemplate];
}

@implementation IIMATouchBarController

- (instancetype)initWithWindow:(NSWindow *)window
                   sessionLabel:(NSString *)sessionLabel
                         labels:(NSDictionary<NSString *, NSString *> *)labels
                       callback:(IIMATouchBarActionCallback)callback
                        context:(void *)context {
  self = [super init];
  if (self == nil) {
    return nil;
  }
  _window = window;
  _sessionLabel = [sessionLabel copy];
  _labels = labels;
  _callback = callback;
  _callbackContext = context;
  _showRemaining = YES;
  _currentURL = @"";
  _thumbnailSource = @"";
  _thumbnailProgress = -1.0;
  _thumbnails = [NSMutableDictionary dictionary];
  _thumbnailImages = [[NSCache alloc] init];
  _mediaButtons = [NSHashTable weakObjectsHashTable];

  NSString *bundleIdentifier = NSBundle.mainBundle.bundleIdentifier;
  if (bundleIdentifier.length == 0) {
    bundleIdentifier = @"io.iima.player";
  }
  NSString *(^identifier)(NSString *) = ^NSString *(NSString *suffix) {
    return [NSString stringWithFormat:@"%@.TouchBarItem.%@", bundleIdentifier,
                                      suffix];
  };
  NSTouchBar *touchBar = [[NSTouchBar alloc] init];
  touchBar.delegate = self;
  touchBar.customizationIdentifier =
      [NSString stringWithFormat:@"%@.windowTouchBar", bundleIdentifier];
  touchBar.defaultItemIdentifiers = @[
    identifier(@"playPause"), identifier(@"time"), identifier(@"slider"),
    identifier(@"remainingTimeOrTotalDuration")
  ];
  touchBar.customizationAllowedItemIdentifiers = @[
    identifier(@"playPause"), identifier(@"slider"), identifier(@"voUp"),
    identifier(@"voDn"), identifier(@"rewind"), identifier(@"forward"),
    identifier(@"time"), identifier(@"remainingTimeOrTotalDuration"),
    identifier(@"ahead15Sec"), identifier(@"ahead30Sec"),
    identifier(@"back15Sec"), identifier(@"back30Sec"), identifier(@"next"),
    identifier(@"prev"), identifier(@"togglePIP"),
    NSTouchBarItemIdentifierFixedSpaceLarge
  ];
  _touchBar = touchBar;
  window.touchBar = touchBar;
  return self;
}

- (NSString *)suffixForIdentifier:(NSTouchBarItemIdentifier)identifier {
  return [identifier componentsSeparatedByString:@"."].lastObject ?: @"";
}

- (NSCustomTouchBarItem *)buttonItem:(NSTouchBarItemIdentifier)identifier
                               image:(NSImage *)image
                                 tag:(NSInteger)tag
                               label:(NSString *)label
                              action:(SEL)action {
  NSCustomTouchBarItem *item =
      [[NSCustomTouchBarItem alloc] initWithIdentifier:identifier];
  NSButton *button = [NSButton buttonWithImage:image target:self action:action];
  button.tag = tag;
  button.enabled = self.hasMedia;
  [self.mediaButtons addObject:button];
  item.view = button;
  item.customizationLabel = label;
  return item;
}

- (nullable NSTouchBarItem *)touchBar:(NSTouchBar *)touchBar
                makeItemForIdentifier:(NSTouchBarItemIdentifier)identifier {
  (void)touchBar;
  NSString *suffix = [self suffixForIdentifier:identifier];
  if ([suffix isEqualToString:@"playPause"]) {
    NSCustomTouchBarItem *item =
        [[NSCustomTouchBarItem alloc] initWithIdentifier:identifier];
    NSButton *button = [NSButton
        buttonWithImage:[NSImage imageNamed:self.paused ? NSImageNameTouchBarPlayTemplate
                                                       : NSImageNameTouchBarPauseTemplate]
                 target:self
                 action:@selector(togglePause:)];
    button.enabled = self.hasMedia;
    [self.mediaButtons addObject:button];
    item.view = button;
    item.customizationLabel = iima_touch_bar_label(
        self.labels, @"playPause", @"Play / Pause");
    self.playPauseButton = button;
    return item;
  }
  if ([suffix isEqualToString:@"slider"]) {
    NSSliderTouchBarItem *item =
        [[NSSliderTouchBarItem alloc] initWithIdentifier:identifier];
    IIMATouchBarSlider *slider = [[IIMATouchBarSlider alloc] init];
    IIMATouchBarSliderCell *cell = [[IIMATouchBarSliderCell alloc] init];
    cell.iimaCachedThumbnailProgress = -1.0;
    slider.cell = cell;
    slider.iimaController = self;
    slider.minValue = 0.0;
    slider.maxValue = 100.0;
    slider.target = self;
    slider.action = @selector(sliderChanged:);
    slider.enabled = self.hasMedia && self.duration > 0.0;
    item.slider = slider;
    item.customizationLabel = iima_touch_bar_label(self.labels, @"seek", @"Seek");
    self.playSlider = slider;
    return item;
  }
  if ([suffix isEqualToString:@"voUp"] || [suffix isEqualToString:@"voDn"]) {
    BOOL up = [suffix isEqualToString:@"voUp"];
    return [self buttonItem:identifier
                      image:[NSImage imageNamed:up ? NSImageNameTouchBarVolumeUpTemplate
                                                   : NSImageNameTouchBarVolumeDownTemplate]
                        tag:up ? 1 : -1
                      label:iima_touch_bar_label(
                                self.labels, up ? @"volumeUp" : @"volumeDown",
                                up ? @"Volume +" : @"Volume -")
                     action:@selector(adjustVolume:)];
  }
  if ([suffix isEqualToString:@"rewind"] || [suffix isEqualToString:@"forward"]) {
    BOOL forward = [suffix isEqualToString:@"forward"];
    return [self buttonItem:identifier
                      image:[NSImage imageNamed:forward ? NSImageNameTouchBarFastForwardTemplate
                                                        : NSImageNameTouchBarRewindTemplate]
                        tag:forward ? 1 : -1
                      label:iima_touch_bar_label(
                                self.labels, forward ? @"fastForward" : @"rewind",
                                forward ? @"Fast Forward" : @"Rewind")
                     action:@selector(arrow:)];
  }
  if ([suffix isEqualToString:@"time"] ||
      [suffix isEqualToString:@"remainingTimeOrTotalDuration"]) {
    BOOL canToggle = [suffix isEqualToString:@"remainingTimeOrTotalDuration"];
    NSCustomTouchBarItem *item =
        [[NSCustomTouchBarItem alloc] initWithIdentifier:identifier];
    IIMATouchBarTimeField *field =
        [IIMATouchBarTimeField labelWithString:@"00:00"];
    field.iimaController = self;
    field.iimaCanToggle = canToggle;
    field.alignment = NSTextAlignmentCenter;
    field.font = [NSFont monospacedDigitSystemFontOfSize:0.0
                                                 weight:NSFontWeightRegular];
    NSLayoutConstraint *width = [field.widthAnchor constraintEqualToConstant:56.0];
    width.active = YES;
    item.view = field;
    item.customizationLabel = iima_touch_bar_label(
        self.labels, canToggle ? @"remaining" : @"time",
        canToggle ? @"Show Remaining Time or Total Duration" : @"Time Position");
    if (canToggle) {
      self.remainingTimeField = field;
      self.remainingTimeWidth = width;
    } else {
      self.currentTimeField = field;
      self.currentTimeWidth = width;
    }
    [self refreshControls];
    return item;
  }
  NSDictionary<NSString *, NSDictionary *> *seekItems = @{
    @"ahead15Sec" : @{
      @"image" : NSImageNameTouchBarSkipAhead15SecondsTemplate,
      @"value" : @15,
      @"label" : @"ahead15",
      @"fallback" : @"15sec Ahead"
    },
    @"ahead30Sec" : @{
      @"image" : NSImageNameTouchBarSkipAhead30SecondsTemplate,
      @"value" : @30,
      @"label" : @"ahead30",
      @"fallback" : @"30sec Ahead"
    },
    @"back15Sec" : @{
      @"image" : NSImageNameTouchBarSkipBack15SecondsTemplate,
      @"value" : @-15,
      @"label" : @"back15",
      @"fallback" : @"15sec Back"
    },
    @"back30Sec" : @{
      @"image" : NSImageNameTouchBarSkipBack30SecondsTemplate,
      @"value" : @-30,
      @"label" : @"back30",
      @"fallback" : @"30sec Back"
    }
  };
  NSDictionary *seek = seekItems[suffix];
  if (seek != nil) {
    return [self buttonItem:identifier
                      image:[NSImage imageNamed:seek[@"image"]]
                        tag:[seek[@"value"] integerValue]
                      label:iima_touch_bar_label(self.labels, seek[@"label"],
                                                 seek[@"fallback"])
                     action:@selector(seekRelative:)];
  }
  if ([suffix isEqualToString:@"next"] || [suffix isEqualToString:@"prev"]) {
    BOOL next = [suffix isEqualToString:@"next"];
    return [self buttonItem:identifier
                      image:[NSImage imageNamed:next ? NSImageNameTouchBarSkipAheadTemplate
                                                     : NSImageNameTouchBarSkipBackTemplate]
                        tag:next ? 1 : -1
                      label:iima_touch_bar_label(self.labels, next ? @"next" : @"previous",
                                                 next ? @"Next Video" : @"Previous Video")
                     action:@selector(navigatePlaylist:)];
  }
  if ([suffix isEqualToString:@"exitFullScr"]) {
    return [self buttonItem:identifier
                      image:[NSImage imageNamed:NSImageNameTouchBarExitFullScreenTemplate]
                        tag:0
                      label:@"Exit Full Screen"
                     action:@selector(exitFullscreen:)];
  }
  if ([suffix isEqualToString:@"togglePIP"]) {
    return [self buttonItem:identifier
                      image:iima_touch_bar_pip_image()
                        tag:0
                      label:iima_touch_bar_label(self.labels, @"togglePip",
                                                 @"Toggle Picture-in-Picture")
                     action:@selector(togglePIP:)];
  }
  return nil;
}

- (void)refreshControls {
  for (NSButton *button in self.mediaButtons) {
    button.enabled = self.hasMedia;
  }
  self.playPauseButton.image =
      [NSImage imageNamed:self.paused ? NSImageNameTouchBarPlayTemplate
                                      : NSImageNameTouchBarPauseTemplate];
  self.playPauseButton.enabled = self.hasMedia;
  self.playSlider.enabled = self.hasMedia && self.duration > 0.0;
  double percent = self.duration > 0.0 ? self.position * 100.0 / self.duration : 0.0;
  [self.playSlider iimaSetDoubleValueSafely:MAX(0.0, MIN(100.0, percent))];
  self.currentTimeField.stringValue =
      iima_touch_bar_format_time(self.position, self.precision);
  double remaining = MAX(0.0, self.duration - self.position);
  self.remainingTimeField.stringValue = self.showRemaining
      ? [@"-" stringByAppendingString:iima_touch_bar_format_time(remaining, self.precision)]
      : iima_touch_bar_format_time(self.duration, self.precision);

  NSString *sizing = iima_touch_bar_format_time(self.duration, self.precision);
  if (self.showRemaining) {
    sizing = [@"-" stringByAppendingString:sizing];
  }
  NSDictionary *attributes = @{NSFontAttributeName :
      [NSFont monospacedDigitSystemFontOfSize:0.0 weight:NSFontWeightRegular]};
  CGFloat width = ceil([sizing sizeWithAttributes:attributes].width) + 16.0;
  self.currentTimeWidth.constant = width;
  self.remainingTimeWidth.constant = width;
  self.playSlider.needsDisplay = YES;
}

- (void)updateHasMedia:(BOOL)hasMedia
                paused:(BOOL)paused
              position:(double)position
              duration:(double)duration
                volume:(double)volume
             precision:(NSInteger)precision
         showRemaining:(BOOL)showRemaining
            currentURL:(NSString *)currentURL
      fullscreenEscape:(BOOL)fullscreenEscape {
  self.hasMedia = hasMedia;
  self.paused = paused;
  self.position = isfinite(position) ? MAX(0.0, position) : 0.0;
  self.duration = isfinite(duration) ? MAX(0.0, duration) : 0.0;
  self.volume = isfinite(volume) ? MAX(0.0, volume) : 0.0;
  self.precision = MAX(0, MIN(3, precision));
  self.showRemaining = showRemaining;
  currentURL = currentURL ?: @"";
  if (![self.currentURL isEqualToString:currentURL]) {
    self.currentURL = currentURL;
    self.thumbnailSource = @"";
    self.thumbnailProgress = -1.0;
    [self.thumbnails removeAllObjects];
    [self.thumbnailImages removeAllObjects];
    [self.playSlider iimaResetThumbnailCache];
  }
  self.touchBar.escapeKeyReplacementItemIdentifier = fullscreenEscape
      ? [NSString stringWithFormat:@"%@.TouchBarItem.exitFullScr",
                                   NSBundle.mainBundle.bundleIdentifier ?: @"io.iima.player"]
      : nil;
  [self refreshControls];
}

- (void)updateThumbnails:(NSArray<NSDictionary *> *)thumbnails
                  source:(NSString *)source
                progress:(double)progress
                 replace:(BOOL)replace {
  if (source.length == 0) {
    if (replace) {
      self.thumbnailSource = @"";
      self.thumbnailProgress = -1.0;
      [self.thumbnails removeAllObjects];
      [self.thumbnailImages removeAllObjects];
      [self.playSlider iimaResetThumbnailCache];
      self.playSlider.needsDisplay = YES;
    }
    return;
  }
  if (self.currentURL.length > 0 && ![self.currentURL isEqualToString:source]) {
    return;
  }
  if (replace || ![self.thumbnailSource isEqualToString:source]) {
    [self.thumbnails removeAllObjects];
    [self.thumbnailImages removeAllObjects];
  }
  self.thumbnailSource = source;
  for (NSDictionary *thumbnail in thumbnails) {
    NSNumber *index = thumbnail[@"index"];
    NSNumber *time = thumbnail[@"time_seconds"];
    NSString *path = thumbnail[@"path"];
    if ([index isKindOfClass:NSNumber.class] && [time isKindOfClass:NSNumber.class] &&
        [path isKindOfClass:NSString.class] && path.length > 0) {
      self.thumbnails[index] = thumbnail;
    }
  }
  self.thumbnailProgress = isfinite(progress) ? MAX(0.0, MIN(1.0, progress)) : 0.0;
  [self.playSlider iimaResetThumbnailCache];
  self.playSlider.needsDisplay = YES;
}

- (nullable NSDictionary *)thumbnailAtTime:(double)time {
  NSArray<NSDictionary *> *ordered = [self.thumbnails.allValues
      sortedArrayUsingComparator:^NSComparisonResult(NSDictionary *left,
                                                       NSDictionary *right) {
        return [left[@"time_seconds"] compare:right[@"time_seconds"]];
      }];
  if (ordered.count == 0) {
    return nil;
  }
  NSDictionary *selected = ordered.lastObject;
  for (NSUInteger index = 0; index < ordered.count; index += 1) {
    if ([ordered[index][@"time_seconds"] doubleValue] >= time) {
      selected = ordered[index == 0 ? 0 : index - 1];
      break;
    }
  }
  return selected;
}

- (nullable NSImage *)thumbnailImageAtTime:(double)time {
  NSDictionary *thumbnail = [self thumbnailAtTime:time];
  NSString *path = thumbnail[@"path"];
  if (path.length == 0) {
    return nil;
  }
  NSImage *cached = [self.thumbnailImages objectForKey:path];
  if (cached != nil) {
    return cached;
  }
  NSImage *image = [[NSImage alloc] initWithContentsOfFile:path];
  if (image != nil) {
    [self.thumbnailImages setObject:image forKey:path];
  }
  return image;
}

- (void)dispatchAction:(int32_t)action value:(double)value {
  if (self.callback != NULL && self.sessionLabel.UTF8String != NULL) {
    self.callback(self.sessionLabel.UTF8String, action, value,
                  self.callbackContext);
  }
}

- (void)toggleRemainingFromTouch {
  self.showRemaining = !self.showRemaining;
  [self refreshControls];
  [self dispatchAction:IIMATouchBarActionToggleRemaining
                 value:self.showRemaining ? 1.0 : 0.0];
}

- (void)togglePause:(id)sender {
  (void)sender;
  [self dispatchAction:IIMATouchBarActionTogglePause value:0.0];
}
- (void)adjustVolume:(NSButton *)sender {
  [self dispatchAction:IIMATouchBarActionVolumeDelta value:sender.tag * 5.0];
}
- (void)arrow:(NSButton *)sender {
  [self dispatchAction:IIMATouchBarActionArrow value:sender.tag];
}
- (void)seekRelative:(NSButton *)sender {
  [self dispatchAction:IIMATouchBarActionSeekRelative value:sender.tag];
}
- (void)navigatePlaylist:(NSButton *)sender {
  [self dispatchAction:IIMATouchBarActionPlaylistNavigate value:sender.tag];
}
- (void)togglePIP:(id)sender {
  (void)sender;
  [self dispatchAction:IIMATouchBarActionTogglePIP value:0.0];
}
- (void)exitFullscreen:(id)sender {
  (void)sender;
  [self dispatchAction:IIMATouchBarActionExitFullscreen value:0.0];
}
- (void)sliderChanged:(NSSlider *)sender {
  double percent = sender.maxValue > 0.0
      ? sender.doubleValue * 100.0 / sender.maxValue
      : 0.0;
  [self dispatchAction:IIMATouchBarActionSeekPercent value:percent];
}

@end

@implementation IIMATouchBarSlider

- (void)touchesBeganWithEvent:(NSEvent *)event {
  if (!self.iimaIsTouching) {
    self.iimaIsTouching = YES;
    [self.iimaController dispatchAction:IIMATouchBarActionSliderBegin value:0.0];
  }
  [super touchesBeganWithEvent:event];
}

- (void)touchesEndedWithEvent:(NSEvent *)event {
  if (self.iimaIsTouching) {
    self.iimaIsTouching = NO;
    [self.iimaController dispatchAction:IIMATouchBarActionSliderEnd value:0.0];
  }
  [super touchesEndedWithEvent:event];
}

- (void)touchesCancelledWithEvent:(NSEvent *)event {
  if (self.iimaIsTouching) {
    self.iimaIsTouching = NO;
    [self.iimaController dispatchAction:IIMATouchBarActionSliderEnd value:0.0];
  }
  [super touchesCancelledWithEvent:event];
}

- (void)iimaSetDoubleValueSafely:(double)value {
  if (!self.iimaIsTouching) {
    self.doubleValue = value;
  }
}

- (void)iimaResetThumbnailCache {
  IIMATouchBarSliderCell *cell = (IIMATouchBarSliderCell *)self.cell;
  if ([cell isKindOfClass:IIMATouchBarSliderCell.class]) {
    cell.iimaCachedThumbnailProgress = -1.0;
    cell.iimaBackgroundImage = nil;
  }
}

@end

@implementation IIMATouchBarSliderCell

- (IIMATouchBarController *)iimaController {
  IIMATouchBarSlider *slider = (IIMATouchBarSlider *)self.controlView;
  return [slider isKindOfClass:IIMATouchBarSlider.class]
             ? slider.iimaController
             : nil;
}

- (BOOL)iimaIsTouching {
  IIMATouchBarSlider *slider = (IIMATouchBarSlider *)self.controlView;
  return [slider isKindOfClass:IIMATouchBarSlider.class] &&
         slider.iimaIsTouching;
}

- (CGFloat)knobThickness {
  return 4.0;
}

- (NSRect)barRectFlipped:(BOOL)flipped {
  self.controlView.superview.layer.backgroundColor = NSColor.blackColor.CGColor;
  NSRect rect = [super barRectFlipped:flipped];
  return NSMakeRect(rect.origin.x, 2.0, rect.size.width,
                    MAX(0.0, self.controlView.frame.size.height - 4.0));
}

- (NSRect)knobRectFlipped:(BOOL)flipped {
  NSRect knob = [super knobRectFlipped:flipped];
  IIMATouchBarController *controller = [self iimaController];
  if ([self iimaIsTouching] && controller.thumbnails.count > 0) {
    CGFloat barWidth = [self barRectFlipped:flipped].size.width;
    CGFloat imageWidth = 60.0;
    if (barWidth > 0.0) {
      knob.origin.x = knob.origin.x *
          (barWidth - (imageWidth - knob.size.width)) / barWidth;
      knob.size.width = imageWidth;
    }
    return knob;
  }
  CGFloat remaining = knob.size.width - self.knobThickness;
  knob.origin.x += remaining * (self.doubleValue / 100.0);
  knob.size.width = self.knobThickness;
  return knob;
}

- (void)drawKnob:(NSRect)knobRect {
  IIMATouchBarController *controller = [self iimaController];
  if (!controller.hasMedia) {
    return;
  }
  NSImage *image = nil;
  if ([self iimaIsTouching] && controller.duration > 0.0) {
    image = [controller thumbnailImageAtTime:self.doubleValue *
                                             controller.duration / 100.0];
  }
  if (image == nil) {
    [NSColor.labelColor setFill];
    [[NSBezierPath bezierPathWithRoundedRect:knobRect xRadius:2.0 yRadius:2.0]
        fill];
    return;
  }
  [NSGraphicsContext saveGraphicsState];
  [[NSBezierPath bezierPathWithRoundedRect:knobRect xRadius:3.0 yRadius:3.0]
      addClip];
  CGFloat sourceAspect = image.size.width / MAX(1.0, image.size.height);
  CGFloat targetAspect = knobRect.size.width / MAX(1.0, knobRect.size.height);
  NSRect source = NSMakeRect(0.0, 0.0, image.size.width, image.size.height);
  if (sourceAspect > targetAspect) {
    CGFloat width = image.size.height * targetAspect;
    source.origin.x = (image.size.width - width) / 2.0;
    source.size.width = width;
  } else {
    CGFloat height = image.size.width / targetAspect;
    source.origin.y = (image.size.height - height) / 2.0;
    source.size.height = height;
  }
  [image drawInRect:knobRect
           fromRect:source
          operation:NSCompositingOperationCopy
           fraction:1.0
     respectFlipped:YES
              hints:nil];
  [NSColor.whiteColor setStroke];
  NSBezierPath *outer = [NSBezierPath
      bezierPathWithRoundedRect:NSInsetRect(knobRect, 1.0, 1.0)
                        xRadius:3.0
                        yRadius:3.0];
  outer.lineWidth = 1.0;
  [outer stroke];
  [NSColor.blackColor setStroke];
  NSBezierPath *inner = [NSBezierPath
      bezierPathWithRoundedRect:NSInsetRect(knobRect, 2.0, 2.0)
                        xRadius:2.0
                        yRadius:2.0];
  inner.lineWidth = 1.0;
  [inner stroke];
  [NSGraphicsContext restoreGraphicsState];
}

- (void)drawBarInside:(NSRect)rect flipped:(BOOL)flipped {
  (void)rect;
  IIMATouchBarController *controller = [self iimaController];
  if (!controller.hasMedia) {
    return;
  }
  NSRect barRect = [self barRectFlipped:flipped];
  if (self.iimaBackgroundImage != nil &&
      self.iimaCachedThumbnailProgress == controller.thumbnailProgress) {
    [self.iimaBackgroundImage drawInRect:barRect];
    return;
  }
  NSImage *background = [[NSImage alloc] initWithSize:barRect.size];
  [background lockFocus];
  [NSGraphicsContext saveGraphicsState];
  NSRect imageRect = NSMakeRect(0.0, 0.0, barRect.size.width,
                                barRect.size.height);
  [[NSBezierPath bezierPathWithRoundedRect:imageRect xRadius:2.5 yRadius:2.5]
      addClip];
  [NSColor.labelColor setFill];
  for (CGFloat x = 0.0; x < imageRect.size.width + 3.0; x += 3.0) {
    double percent = imageRect.size.width > 0.0 ? x / imageRect.size.width : 0.0;
    NSRect destination = NSMakeRect(x, 0.0, 2.0, imageRect.size.height);
    NSImage *thumbnail = controller.duration > 0.0 &&
                                 controller.thumbnailProgress >= percent
                             ? [controller thumbnailImageAtTime:percent *
                                                                  controller.duration]
                             : nil;
    if (thumbnail != nil) {
      [thumbnail drawInRect:destination
                   fromRect:NSMakeRect(0.0, 0.0, thumbnail.size.width,
                                       thumbnail.size.height)
                  operation:NSCompositingOperationCopy
                   fraction:1.0
             respectFlipped:YES
                      hints:nil];
    } else {
      [[NSBezierPath bezierPathWithRect:destination] fill];
    }
  }
  [NSGraphicsContext restoreGraphicsState];
  [background unlockFocus];
  self.iimaBackgroundImage = background;
  self.iimaCachedThumbnailProgress = controller.thumbnailProgress;
  [background drawInRect:barRect];
}

@end

@implementation IIMATouchBarTimeField

- (void)touchesBeganWithEvent:(NSEvent *)event {
  [super touchesBeganWithEvent:event];
  if (self.iimaCanToggle) {
    [self.iimaController toggleRemainingFromTouch];
  }
}

- (void)scrollWheel:(NSEvent *)event {
  (void)event;
}

@end

int32_t iima_touch_bar_set_automatic_customization_enabled(int32_t enabled) {
  if (@available(macOS 10.12.2, *)) {
    iima_touch_bar_on_main_sync(^{
      NSApp.automaticCustomizeTouchBarMenuItemEnabled = enabled != 0;
    });
    return 1;
  } else {
    return 0;
  }
}

int32_t iima_touch_bar_install(void *ns_window,
                               const char *session_label,
                               const char *labels_json,
                               IIMATouchBarActionCallback callback,
                               void *context) {
  if (@available(macOS 10.12.2, *)) {
    // Continue below while guarded by the runtime availability check.
  } else {
    return 0;
  }
  if (ns_window == NULL || session_label == NULL || callback == NULL) {
    return -1;
  }
  NSWindow *window = (__bridge NSWindow *)ns_window;
  NSString *sessionLabel = [NSString stringWithUTF8String:session_label];
  NSDictionary *labels = iima_touch_bar_parse_labels(labels_json);
  if (sessionLabel.length == 0) {
    return -1;
  }
  __block int32_t status = 0;
  iima_touch_bar_on_main_sync(^{
    if (iima_touch_bar_controllers == nil) {
      iima_touch_bar_controllers = [NSMutableDictionary dictionary];
    }
    NSValue *key = [NSValue valueWithNonretainedObject:window];
    IIMATouchBarController *controller = iima_touch_bar_controllers[key];
    if (controller == nil || ![controller.sessionLabel isEqualToString:sessionLabel]) {
      controller = [[IIMATouchBarController alloc]
          initWithWindow:window
            sessionLabel:sessionLabel
                  labels:labels
                callback:callback
                 context:context];
      iima_touch_bar_controllers[key] = controller;
    } else {
      controller.window = window;
      controller.callback = callback;
      controller.callbackContext = context;
      controller.labels = labels;
      window.touchBar = controller.touchBar;
    }
    status = controller != nil ? 1 : -1;
  });
  return status;
}

void iima_touch_bar_update(const char *session_label,
                           int32_t has_media,
                           int32_t paused,
                           double position,
                           double duration,
                           double volume,
                           int32_t precision,
                           int32_t show_remaining,
                           const char *current_url,
                           int32_t fullscreen_escape) {
  if (session_label == NULL) {
    return;
  }
  NSString *sessionLabel = [NSString stringWithUTF8String:session_label];
  NSString *currentURL = current_url != NULL
      ? [NSString stringWithUTF8String:current_url]
      : @"";
  iima_touch_bar_on_main_sync(^{
    for (IIMATouchBarController *controller in
         iima_touch_bar_controllers.allValues) {
      if ([controller.sessionLabel isEqualToString:sessionLabel]) {
        [controller updateHasMedia:has_media != 0
                            paused:paused != 0
                          position:position
                          duration:duration
                            volume:volume
                         precision:precision
                     showRemaining:show_remaining != 0
                        currentURL:currentURL
                  fullscreenEscape:fullscreen_escape != 0];
      }
    }
  });
}

void iima_touch_bar_update_thumbnails_json(const char *session_label,
                                           const char *source,
                                           const char *thumbnails_json,
                                           double progress,
                                           int32_t replace) {
  if (session_label == NULL || source == NULL || thumbnails_json == NULL) {
    return;
  }
  NSString *sessionLabel = [NSString stringWithUTF8String:session_label];
  NSString *sourcePath = [NSString stringWithUTF8String:source];
  NSData *data = [NSData dataWithBytes:thumbnails_json
                                length:strlen(thumbnails_json)];
  id parsed = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
  NSArray *thumbnails = [parsed isKindOfClass:NSArray.class] ? parsed : @[];
  iima_touch_bar_on_main_sync(^{
    for (IIMATouchBarController *controller in
         iima_touch_bar_controllers.allValues) {
      if ([controller.sessionLabel isEqualToString:sessionLabel]) {
        [controller updateThumbnails:thumbnails
                              source:sourcePath
                            progress:progress
                             replace:replace != 0];
      }
    }
  });
}

void iima_touch_bar_remove_session(const char *session_label) {
  if (session_label == NULL) {
    return;
  }
  NSString *sessionLabel = [NSString stringWithUTF8String:session_label];
  iima_touch_bar_on_main_sync(^{
    NSArray<NSValue *> *keys = iima_touch_bar_controllers.allKeys.copy;
    for (NSValue *key in keys) {
      IIMATouchBarController *controller = iima_touch_bar_controllers[key];
      if ([controller.sessionLabel isEqualToString:sessionLabel]) {
        if (controller.window.touchBar == controller.touchBar) {
          controller.window.touchBar = nil;
        }
        [iima_touch_bar_controllers removeObjectForKey:key];
      }
    }
  });
}

void iima_touch_bar_remove_all(void) {
  iima_touch_bar_on_main_sync(^{
    for (IIMATouchBarController *controller in
         iima_touch_bar_controllers.allValues) {
      if (controller.window.touchBar == controller.touchBar) {
        controller.window.touchBar = nil;
      }
    }
    [iima_touch_bar_controllers removeAllObjects];
  });
}
