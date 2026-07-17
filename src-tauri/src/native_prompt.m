#import <Cocoa/Cocoa.h>
#import <stdint.h>
#import <stdlib.h>
#import <string.h>

static NSString *iima_prompt_string(const char *value) {
  if (value == NULL) {
    return @"";
  }
  NSString *string = [NSString stringWithUTF8String:value];
  return string ?: @"";
}

static char *iima_prompt_text_on_main_thread(
    NSString *title,
    NSString *message,
    NSString *initialValue,
    NSString *confirmTitle,
    NSString *cancelTitle,
    BOOL multiline) {
  NSAlert *panel = [[NSAlert alloc] init];
  panel.messageText = title;
  panel.informativeText = message;

  NSTextField *input = [[NSTextField alloc]
      initWithFrame:NSMakeRect(0, 0, 240, multiline ? 60 : 24)];
  input.lineBreakMode = multiline ? NSLineBreakByWordWrapping : NSLineBreakByClipping;
  input.usesSingleLineMode = !multiline;
  input.cell.scrollable = !multiline;
  input.stringValue = initialValue;
  panel.accessoryView = input;
  [panel addButtonWithTitle:confirmTitle];
  [panel addButtonWithTitle:cancelTitle];
  panel.window.initialFirstResponder = input;

  if ([panel runModal] != NSAlertFirstButtonReturn) {
    return NULL;
  }
  return strdup(input.stringValue.UTF8String ?: "");
}

char *iima_native_prompt_text(
    const char *title,
    const char *message,
    const char *initial_value,
    const char *confirm_title,
    const char *cancel_title) {
  __block char *result = NULL;
  void (^prompt)(void) = ^{
    result = iima_prompt_text_on_main_thread(
        iima_prompt_string(title),
        iima_prompt_string(message),
        iima_prompt_string(initial_value),
        iima_prompt_string(confirm_title),
        iima_prompt_string(cancel_title),
        NO);
  };
  if ([NSThread isMainThread]) {
    prompt();
  } else {
    dispatch_sync(dispatch_get_main_queue(), prompt);
  }
  return result;
}

char *iima_native_prompt_multiline_text(
    const char *title,
    const char *message,
    const char *initial_value,
    const char *confirm_title,
    const char *cancel_title) {
  __block char *result = NULL;
  void (^prompt)(void) = ^{
    result = iima_prompt_text_on_main_thread(
        iima_prompt_string(title),
        iima_prompt_string(message),
        iima_prompt_string(initial_value),
        iima_prompt_string(confirm_title),
        iima_prompt_string(cancel_title),
        YES);
  };
  if ([NSThread isMainThread]) {
    prompt();
  } else {
    dispatch_sync(dispatch_get_main_queue(), prompt);
  }
  return result;
}

void iima_native_prompt_free(char *value) {
  free(value);
}

int32_t iima_native_confirm(
    const char *title,
    const char *confirm_title,
    const char *cancel_title) {
  __block int32_t confirmed = 0;
  void (^showAlert)(void) = ^{
    NSAlert *alert = [[NSAlert alloc] init];
    alert.messageText = iima_prompt_string(title);
    [alert addButtonWithTitle:iima_prompt_string(confirm_title)];
    [alert addButtonWithTitle:iima_prompt_string(cancel_title)];
    confirmed = [alert runModal] == NSAlertFirstButtonReturn ? 1 : 0;
  };
  if ([NSThread isMainThread]) {
    showAlert();
  } else {
    dispatch_sync(dispatch_get_main_queue(), showAlert);
  }
  return confirmed;
}

static void iima_show_error_on_main_thread(NSString *title, NSString *message) {
  NSAlert *alert = [[NSAlert alloc] init];
  alert.messageText = title;
  alert.informativeText = message;
  alert.alertStyle = NSAlertStyleCritical;
  [alert runModal];
}

void iima_native_show_error(const char *title, const char *message) {
  void (^showAlert)(void) = ^{
    iima_show_error_on_main_thread(iima_prompt_string(title), iima_prompt_string(message));
  };
  if ([NSThread isMainThread]) {
    showAlert();
  } else {
    dispatch_sync(dispatch_get_main_queue(), showAlert);
  }
}
