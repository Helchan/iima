#import <Cocoa/Cocoa.h>
#import <stdlib.h>
#import <string.h>

@interface IIMAFontPickerController : NSObject <NSTableViewDataSource, NSTableViewDelegate, NSSearchFieldDelegate, NSWindowDelegate>
@property(nonatomic, strong) NSPanel *panel;
@property(nonatomic, strong) NSSearchField *searchField;
@property(nonatomic, strong) NSTableView *familyTable;
@property(nonatomic, strong) NSTableView *faceTable;
@property(nonatomic, strong) NSTextField *previewField;
@property(nonatomic, strong) NSTextField *otherField;
@property(nonatomic, strong) NSArray<NSString *> *fontFamilies;
@property(nonatomic, strong) NSArray<NSString *> *filteredFontFamilies;
@property(nonatomic, strong) NSArray<NSArray *> *fontMembers;
@property(nonatomic, copy) NSString *chosenFace;
@property(nonatomic, copy) NSString *chosenFont;
@property(nonatomic, strong) NSDictionary<NSString *, NSString *> *labels;
@end

@implementation IIMAFontPickerController

- (instancetype)initWithLabels:(NSDictionary<NSString *, NSString *> *)labels {
  self = [super init];
  if (self) {
    _labels = labels ?: @{};
    NSFontManager *manager = [NSFontManager sharedFontManager];
    NSArray<NSString *> *families = [manager.availableFontFamilies filteredArrayUsingPredicate:[NSPredicate predicateWithBlock:^BOOL(NSString *family, NSDictionary *bindings) {
      (void)bindings;
      return ![family hasPrefix:@"."] && [[family stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceCharacterSet] length] > 0;
    }]];
    _fontFamilies = [families sortedArrayUsingComparator:^NSComparisonResult(NSString *left, NSString *right) {
      NSString *leftName = [manager localizedNameForFamily:left face:nil] ?: left;
      NSString *rightName = [manager localizedNameForFamily:right face:nil] ?: right;
      return [leftName localizedCaseInsensitiveCompare:rightName];
    }];
    _filteredFontFamilies = _fontFamilies;
    _fontMembers = @[];
    [self buildPanel];
  }
  return self;
}

- (NSString *)label:(NSString *)key fallback:(NSString *)fallback {
  NSString *value = self.labels[key];
  return [value isKindOfClass:NSString.class] && value.length > 0 ? value : fallback;
}

- (NSTableView *)newTableView {
  NSTableView *table = [[NSTableView alloc] initWithFrame:NSZeroRect];
  NSTableColumn *column = [[NSTableColumn alloc] initWithIdentifier:@"font"];
  column.width = 203;
  [table addTableColumn:column];
  table.headerView = nil;
  table.dataSource = self;
  table.delegate = self;
  table.usesAlternatingRowBackgroundColors = YES;
  table.allowsMultipleSelection = NO;
  return table;
}

- (NSScrollView *)scrollViewForTable:(NSTableView *)table frame:(NSRect)frame {
  NSScrollView *scrollView = [[NSScrollView alloc] initWithFrame:frame];
  scrollView.hasVerticalScroller = YES;
  scrollView.autohidesScrollers = YES;
  scrollView.borderType = NSBezelBorder;
  scrollView.documentView = table;
  return scrollView;
}

- (void)buildPanel {
  _panel = [[NSPanel alloc] initWithContentRect:NSMakeRect(0, 0, 458, 524)
                                      styleMask:(NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskMiniaturizable)
                                        backing:NSBackingStoreBuffered
                                          defer:NO];
  _panel.title = [self label:@"windowTitle" fallback:@"Choose a Font"];
  _panel.releasedWhenClosed = NO;
  _panel.delegate = self;

  NSView *content = _panel.contentView;
  NSTextField *chooseLabel = [NSTextField labelWithString:[self label:@"chooseLabel" fallback:@"Choose a font:"]];
  chooseLabel.frame = NSMakeRect(12, 474, 118, 18);
  [content addSubview:chooseLabel];

  _searchField = [[NSSearchField alloc] initWithFrame:NSMakeRect(12, 444, 434, 24)];
  _searchField.placeholderString = [self label:@"searchPlaceholder" fallback:@"Type to filter…"];
  _searchField.delegate = self;
  [content addSubview:_searchField];

  _familyTable = [self newTableView];
  _faceTable = [self newTableView];
  [content addSubview:[self scrollViewForTable:_familyTable frame:NSMakeRect(12, 229, 216, 207)]];
  [content addSubview:[self scrollViewForTable:_faceTable frame:NSMakeRect(230, 229, 216, 207)]];

  _previewField = [NSTextField labelWithString:@"The quick brown fox jumps over the lazy dog."];
  _previewField.frame = NSMakeRect(12, 125, 434, 88);
  _previewField.alignment = NSTextAlignmentCenter;
  _previewField.lineBreakMode = NSLineBreakByWordWrapping;
  _previewField.maximumNumberOfLines = 3;
  _previewField.font = [NSFont systemFontOfSize:24];
  [content addSubview:_previewField];

  NSBox *separator = [[NSBox alloc] initWithFrame:NSMakeRect(0, 107, 458, 1)];
  separator.boxType = NSBoxSeparator;
  [content addSubview:separator];

  NSTextField *otherLabel = [NSTextField labelWithString:[self label:@"otherLabel" fallback:@"Or enter the font name:"]];
  otherLabel.frame = NSMakeRect(12, 80, 180, 18);
  [content addSubview:otherLabel];

  _otherField = [[NSTextField alloc] initWithFrame:NSMakeRect(12, 49, 434, 22)];
  _otherField.placeholderString = @"sans-serif";
  [content addSubview:_otherField];

  NSButton *cancel = [NSButton buttonWithTitle:[self label:@"cancel" fallback:@"Cancel"]
                                          target:self
                                          action:@selector(cancelPressed:)];
  cancel.frame = NSMakeRect(268, 8, 92, 28);
  cancel.keyEquivalent = @"\e";
  [content addSubview:cancel];

  NSButton *confirm = [NSButton buttonWithTitle:[self label:@"confirm" fallback:@"OK"]
                                           target:self
                                           action:@selector(confirmPressed:)];
  confirm.frame = NSMakeRect(372, 8, 74, 28);
  confirm.keyEquivalent = @"\r";
  [content addSubview:confirm];
}

- (NSArray<NSString *> *)visibleFamilies {
  return self.filteredFontFamilies ?: @[];
}

- (NSInteger)numberOfRowsInTableView:(NSTableView *)tableView {
  return tableView == self.familyTable ? self.visibleFamilies.count : self.fontMembers.count;
}

- (id)tableView:(NSTableView *)tableView objectValueForTableColumn:(NSTableColumn *)tableColumn row:(NSInteger)row {
  (void)tableColumn;
  if (tableView == self.familyTable) {
    NSString *family = self.visibleFamilies[(NSUInteger)row];
    return [[NSFontManager sharedFontManager] localizedNameForFamily:family face:nil] ?: family;
  }
  NSArray *member = self.fontMembers[(NSUInteger)row];
  return member.count > 1 ? member[1] : @"";
}

- (void)tableViewSelectionDidChange:(NSNotification *)notification {
  NSTableView *table = notification.object;
  if (table == self.familyTable) {
    NSInteger row = table.selectedRow;
    if (row < 0 || (NSUInteger)row >= self.visibleFamilies.count) {
      return;
    }
    NSString *family = self.visibleFamilies[(NSUInteger)row];
    self.fontMembers = [[NSFontManager sharedFontManager] availableMembersOfFontFamily:family] ?: @[];
    [self.faceTable reloadData];
    if (self.fontMembers.count > 0) {
      [self.faceTable selectRowIndexes:[NSIndexSet indexSetWithIndex:0] byExtendingSelection:NO];
    }
    return;
  }
  if (table == self.faceTable) {
    NSInteger row = table.selectedRow;
    if (row < 0 || (NSUInteger)row >= self.fontMembers.count) {
      return;
    }
    NSArray *member = self.fontMembers[(NSUInteger)row];
    self.chosenFace = member.count > 1 ? member[1] : @"";
    self.chosenFont = member.count > 0 ? member[0] : @"";
    self.previewField.font = [NSFont fontWithName:self.chosenFont size:24] ?: [NSFont systemFontOfSize:24];
  }
}

- (void)controlTextDidChange:(NSNotification *)notification {
  if (notification.object != self.searchField) {
    return;
  }
  NSString *query = self.searchField.stringValue;
  if (query.length == 0) {
    self.filteredFontFamilies = self.fontFamilies;
  } else {
    NSFontManager *manager = [NSFontManager sharedFontManager];
    self.filteredFontFamilies = [self.fontFamilies filteredArrayUsingPredicate:[NSPredicate predicateWithBlock:^BOOL(NSString *family, NSDictionary *bindings) {
      (void)bindings;
      NSString *name = [manager localizedNameForFamily:family face:nil] ?: family;
      return [name rangeOfString:query options:NSCaseInsensitiveSearch].location != NSNotFound;
    }]];
  }
  self.fontMembers = @[];
  self.chosenFace = nil;
  self.chosenFont = nil;
  [self.familyTable deselectAll:nil];
  [self.familyTable reloadData];
  [self.faceTable reloadData];
}

- (void)confirmPressed:(id)sender {
  (void)sender;
  self.chosenFont = self.otherField.stringValue.length > 0 ? self.otherField.stringValue : self.chosenFont;
  [NSApp stopModalWithCode:NSModalResponseOK];
  [self.panel orderOut:nil];
}

- (void)cancelPressed:(id)sender {
  (void)sender;
  self.chosenFont = nil;
  [NSApp abortModal];
  [self.panel orderOut:nil];
}

- (void)windowWillClose:(NSNotification *)notification {
  (void)notification;
  self.chosenFont = nil;
  [NSApp abortModal];
}

@end

static char *iima_pick_font_on_main_thread(NSDictionary<NSString *, NSString *> *labels) {
  IIMAFontPickerController *controller = [[IIMAFontPickerController alloc] initWithLabels:labels];
  [controller.panel center];
  NSInteger response = [NSApp runModalForWindow:controller.panel];
  [controller.panel orderOut:nil];
  if (response != NSModalResponseOK) {
    return NULL;
  }
  return strdup((controller.chosenFont ?: @"").UTF8String);
}

char *iima_native_font_picker_choose(const char *localizations_json) {
  NSDictionary<NSString *, NSString *> *labels = @{};
  if (localizations_json != NULL) {
    NSData *data = [[NSData alloc] initWithBytes:localizations_json
                                         length:strlen(localizations_json)];
    id value = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
    if ([value isKindOfClass:NSDictionary.class]) {
      labels = value;
    }
  }
  __block char *font = NULL;
  void (^pick)(void) = ^{
    font = iima_pick_font_on_main_thread(labels);
  };
  if ([NSThread isMainThread]) {
    pick();
  } else {
    dispatch_sync(dispatch_get_main_queue(), pick);
  }
  return font;
}

void iima_native_font_picker_free(char *font) {
  free(font);
}
