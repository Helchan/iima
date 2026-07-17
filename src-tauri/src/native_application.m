#import <AppKit/AppKit.h>
#import <dispatch/dispatch.h>
#import <objc/runtime.h>
#import <stddef.h>
#import <stdint.h>
#import <string.h>

typedef void (*IIMADockOpenCallback)(void);
typedef void (*IIMAServiceOpenURLCallback)(const char *url, void *mainWindow);

@interface IIMAApplicationBridge : NSObject

@property(nonatomic, assign) IIMADockOpenCallback dockOpenCallback;
@property(nonatomic, assign) IIMAServiceOpenURLCallback serviceOpenURLCallback;
@property(nonatomic, strong) NSMenu *dockMenu;
@property(nonatomic, strong) id delegateObject;
@property(nonatomic, assign) Class originalDelegateClass;
@property(nonatomic, assign) Class delegateSubclass;
@property(nonatomic, strong) id previousServicesProvider;

- (instancetype)initWithDockOpenTitle:(NSString *)title
                      dockOpenCallback:(IIMADockOpenCallback)dockOpenCallback
                   serviceURLCallback:
                       (IIMAServiceOpenURLCallback)serviceOpenURLCallback;
- (BOOL)install;
- (void)uninstall;
- (void)updateDockOpenTitle:(NSString *)title
           dockOpenCallback:(IIMADockOpenCallback)dockOpenCallback
        serviceURLCallback:
            (IIMAServiceOpenURLCallback)serviceOpenURLCallback;
- (NSMenu *)applicationDockMenu:(NSApplication *)sender;

@end

static IIMAApplicationBridge *IIMAApplicationBridgeInstance = nil;

// Mirrors PlayerCore.openURLString rather than handing an arbitrary pasteboard string directly to
// mpv. IINA 1.3.5 deliberately uses Foundation here: absolute paths stay decoded file paths,
// file:// URLs are converted back to paths, and network/relative URL strings are serialized to the
// percent-encoded absoluteString accepted by mpv. Xcode 15's URL initializer only gained automatic
// encoding on macOS 14, so older systems retain the reference explicit CharacterSet path.
static NSString *IIMANormalizedOpenURLString(NSString *raw) {
  if (raw == nil) return nil;
  if ([raw isEqualToString:@"-"]) return raw;

  NSURL *url = nil;
  if ([raw hasPrefix:@"/"]) {
    url = [NSURL fileURLWithPath:raw];
  } else {
    // Swift's URL(string: "") is nil, whereas NSURL's Objective-C initializer produces an empty
    // relative URL. Preserve the Swift contract explicitly.
    if (raw.length == 0) return nil;
    NSString *candidate = raw;
    if (@available(macOS 14.0, *)) {
      // Foundation performs the same automatic encoding as URLComponents on Sonoma and later.
    } else {
      NSMutableCharacterSet *allowed =
          [[NSCharacterSet URLHostAllowedCharacterSet] mutableCopy];
      [allowed formUnionWithCharacterSet:[NSCharacterSet URLUserAllowedCharacterSet]];
      [allowed formUnionWithCharacterSet:[NSCharacterSet URLPasswordAllowedCharacterSet]];
      [allowed formUnionWithCharacterSet:[NSCharacterSet URLPathAllowedCharacterSet]];
      [allowed formUnionWithCharacterSet:[NSCharacterSet URLQueryAllowedCharacterSet]];
      [allowed formUnionWithCharacterSet:[NSCharacterSet URLFragmentAllowedCharacterSet]];
      [allowed addCharactersInString:@"%"];
      candidate = [raw stringByAddingPercentEncodingWithAllowedCharacters:allowed];
      if (candidate == nil) return nil;
    }
    url = [NSURL URLWithString:candidate];
  }

  if (url == nil) return nil;
  return url.isFileURL ? url.path : url.absoluteString;
}

// Returns the UTF-8 byte count, -1 when Swift URL(string:) would reject the value, or -2 when a
// supplied output buffer is too small. Calling once with a NULL buffer and then with length + 1
// keeps one Foundation implementation authoritative for Services and any future openURLString
// callers on the Rust side.
intptr_t iima_native_normalize_open_url_string(const char *raw,
                                                char *output,
                                                size_t outputCapacity) {
  if (raw == NULL) return -1;
  NSString *rawString = [NSString stringWithUTF8String:raw];
  NSString *normalized = IIMANormalizedOpenURLString(rawString);
  if (normalized == nil) return -1;
  NSData *bytes = [normalized dataUsingEncoding:NSUTF8StringEncoding];
  if (bytes == nil) return -1;
  size_t length = bytes.length;
  if (output != NULL) {
    if (outputCapacity <= length) return -2;
    if (length > 0) memcpy(output, bytes.bytes, length);
    output[length] = '\0';
  }
  return (intptr_t)length;
}

static Class iima_application_delegate_subclass(Class originalClass) {
  const char *subclassName = "IIMAApplicationDockDelegateBridge";
  Class subclass = objc_getClass(subclassName);
  if (subclass != Nil) {
    return class_getSuperclass(subclass) == originalClass ? subclass : Nil;
  }

  subclass = objc_allocateClassPair(originalClass, subclassName, 0);
  if (subclass == Nil) {
    return Nil;
  }
  Method dockMenuMethod = class_getInstanceMethod(
      IIMAApplicationBridge.class, @selector(applicationDockMenu:));
  if (dockMenuMethod == NULL ||
      !class_addMethod(subclass, @selector(applicationDockMenu:),
                       method_getImplementation(dockMenuMethod),
                       method_getTypeEncoding(dockMenuMethod))) {
    objc_disposeClassPair(subclass);
    return Nil;
  }
  objc_registerClassPair(subclass);
  return subclass;
}

@implementation IIMAApplicationBridge

- (instancetype)initWithDockOpenTitle:(NSString *)title
                      dockOpenCallback:(IIMADockOpenCallback)dockOpenCallback
                   serviceURLCallback:
                       (IIMAServiceOpenURLCallback)serviceOpenURLCallback {
  self = [super init];
  if (self != nil) {
    [self updateDockOpenTitle:title
             dockOpenCallback:dockOpenCallback
          serviceURLCallback:serviceOpenURLCallback];
  }
  return self;
}

- (void)updateDockOpenTitle:(NSString *)title
           dockOpenCallback:(IIMADockOpenCallback)dockOpenCallback
        serviceURLCallback:
            (IIMAServiceOpenURLCallback)serviceOpenURLCallback {
  self.dockOpenCallback = dockOpenCallback;
  self.serviceOpenURLCallback = serviceOpenURLCallback;

  NSMenu *menu = [[NSMenu alloc] initWithTitle:@""];
  NSMenuItem *openItem =
      [[NSMenuItem alloc] initWithTitle:title
                                action:@selector(openFile:)
                         keyEquivalent:@""];
  openItem.target = self;
  [menu addItem:openItem];
  self.dockMenu = menu;
}

- (BOOL)install {
  id delegate = NSApp.delegate;
  if (delegate == nil) {
    return NO;
  }
  Class originalClass = object_getClass(delegate);
  Class subclass = iima_application_delegate_subclass(originalClass);
  if (subclass == Nil) {
    return NO;
  }

  self.delegateObject = delegate;
  self.originalDelegateClass = originalClass;
  self.delegateSubclass = subclass;
  self.previousServicesProvider = NSApp.servicesProvider;

  object_setClass(delegate, subclass);
  NSApp.servicesProvider = self;
  return YES;
}

- (void)uninstall {
  if (NSApp.servicesProvider == self) {
    NSApp.servicesProvider = self.previousServicesProvider;
  }
  id delegate = self.delegateObject;
  if (delegate != nil && object_getClass(delegate) == self.delegateSubclass) {
    object_setClass(delegate, self.originalDelegateClass);
  }
  self.delegateObject = nil;
  self.previousServicesProvider = nil;
}

- (NSMenu *)applicationDockMenu:(NSApplication *)sender {
  (void)sender;
  return IIMAApplicationBridgeInstance.dockMenu;
}

- (void)openFile:(id)sender {
  (void)sender;
  if (self.dockOpenCallback != NULL) {
    self.dockOpenCallback();
  }
}

- (void)droppedText:(NSPasteboard *)pasteboard
            userData:(NSString *)userData
               error:(NSString **)error {
  (void)userData;
  (void)error;
  NSString *url = [pasteboard stringForType:NSPasteboardTypeString];
  if (url != nil && self.serviceOpenURLCallback != NULL) {
    self.serviceOpenURLCallback(url.UTF8String, (__bridge void *)NSApp.mainWindow);
  }
}

@end

int32_t iima_native_application_bridge_install(
    const char *dock_open_title, IIMADockOpenCallback dock_open_callback,
    IIMAServiceOpenURLCallback service_open_url_callback) {
  if (dock_open_title == NULL || dock_open_callback == NULL ||
      service_open_url_callback == NULL) {
    return 0;
  }
  NSString *title = [NSString stringWithUTF8String:dock_open_title];
  if (title == nil) {
    return 0;
  }

  __block int32_t installed = 0;
  void (^installBridge)(void) = ^{
    if (IIMAApplicationBridgeInstance != nil) {
      [IIMAApplicationBridgeInstance
          updateDockOpenTitle:title
             dockOpenCallback:dock_open_callback
          serviceURLCallback:service_open_url_callback];
      NSApp.servicesProvider = IIMAApplicationBridgeInstance;
      installed = 1;
      return;
    }

    IIMAApplicationBridge *bridge = [[IIMAApplicationBridge alloc]
        initWithDockOpenTitle:title
            dockOpenCallback:dock_open_callback
         serviceURLCallback:service_open_url_callback];
    IIMAApplicationBridgeInstance = bridge;
    if (![bridge install]) {
      IIMAApplicationBridgeInstance = nil;
      return;
    }
    installed = 1;
  };
  if (NSThread.isMainThread) {
    installBridge();
  } else {
    dispatch_sync(dispatch_get_main_queue(), installBridge);
  }
  return installed;
}

void iima_native_application_bridge_shutdown(void) {
  void (^shutdownBridge)(void) = ^{
    [IIMAApplicationBridgeInstance uninstall];
    IIMAApplicationBridgeInstance = nil;
  };
  if (NSThread.isMainThread) {
    shutdownBridge();
  } else {
    dispatch_sync(dispatch_get_main_queue(), shutdownBridge);
  }
}
