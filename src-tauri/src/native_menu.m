#import <AppKit/AppKit.h>
#import <dispatch/dispatch.h>
#import <stdint.h>

enum {
  IIMAKeyModifierCommand = 1 << 0,
  IIMAKeyModifierControl = 1 << 1,
  IIMAKeyModifierOption = 1 << 2,
  IIMAKeyModifierShift = 1 << 3,
};

static NSEventModifierFlags iima_modifier_flags(uint32_t mask) {
  NSEventModifierFlags flags = 0;
  if ((mask & IIMAKeyModifierCommand) != 0) {
    flags |= NSEventModifierFlagCommand;
  }
  if ((mask & IIMAKeyModifierControl) != 0) {
    flags |= NSEventModifierFlagControl;
  }
  if ((mask & IIMAKeyModifierOption) != 0) {
    flags |= NSEventModifierFlagOption;
  }
  if ((mask & IIMAKeyModifierShift) != 0) {
    flags |= NSEventModifierFlagShift;
  }
  return flags;
}

static void iima_apply_key_equivalent(NSMenuItem *item,
                                      NSString *keyEquivalent,
                                      uint32_t modifierMask) {
  item.keyEquivalent = keyEquivalent;
  item.keyEquivalentModifierMask = iima_modifier_flags(modifierMask);
}

static int32_t iima_set_menu_item_key_equivalent_on_main(
    NSString *menuTitle, NSString *itemTitle, NSString *keyEquivalent,
    uint32_t modifierMask) {
  NSMenu *submenu = [NSApp.mainMenu itemWithTitle:menuTitle].submenu;
  if (submenu == nil) {
    return 0;
  }
  for (NSMenuItem *item in submenu.itemArray) {
    if ([item.title isEqualToString:itemTitle]) {
      iima_apply_key_equivalent(item, keyEquivalent, modifierMask);
      return 1;
    }
  }
  return 0;
}

static int32_t iima_set_submenu_item_key_equivalent_on_main(
    NSString *menuTitle, NSString *submenuTitle, NSUInteger itemIndex,
    NSString *keyEquivalent, uint32_t modifierMask) {
  NSMenu *topMenu = [NSApp.mainMenu itemWithTitle:menuTitle].submenu;
  NSMenu *submenu = [topMenu itemWithTitle:submenuTitle].submenu;
  if (submenu == nil || itemIndex >= (NSUInteger)submenu.numberOfItems) {
    return 0;
  }
  iima_apply_key_equivalent([submenu itemAtIndex:itemIndex], keyEquivalent,
                            modifierMask);
  return 1;
}

static int32_t iima_set_menu_item_key_equivalent_at_path_on_main(
    NSString *menuTitle, const uintptr_t *itemPath, uintptr_t itemPathLength,
    NSString *keyEquivalent, uint32_t modifierMask) {
  NSMenu *menu = [NSApp.mainMenu itemWithTitle:menuTitle].submenu;
  if (menu == nil || itemPath == NULL || itemPathLength == 0) {
    return 0;
  }
  NSMenuItem *item = nil;
  for (uintptr_t depth = 0; depth < itemPathLength; depth++) {
    NSUInteger index = (NSUInteger)itemPath[depth];
    if (index >= (NSUInteger)menu.numberOfItems) {
      return 0;
    }
    item = [menu itemAtIndex:index];
    if (depth + 1 < itemPathLength) {
      menu = item.submenu;
      if (menu == nil) {
        return 0;
      }
    }
  }
  iima_apply_key_equivalent(item, keyEquivalent, modifierMask);
  return 1;
}

int32_t iima_native_set_menu_item_key_equivalent(
    const char *menu_title, const char *item_title, const char *key_equivalent,
    uint32_t modifier_mask) {
  if (menu_title == NULL || item_title == NULL || key_equivalent == NULL) {
    return 0;
  }
  NSString *menuTitle = [NSString stringWithUTF8String:menu_title];
  NSString *itemTitle = [NSString stringWithUTF8String:item_title];
  NSString *keyEquivalent = [NSString stringWithUTF8String:key_equivalent];
  if (menuTitle == nil || itemTitle == nil || keyEquivalent == nil) {
    return 0;
  }
  __block int32_t configured = 0;
  void (^configure)(void) = ^{
    configured = iima_set_menu_item_key_equivalent_on_main(
        menuTitle, itemTitle, keyEquivalent, modifier_mask);
  };
  if (NSThread.isMainThread) {
    configure();
  } else {
    dispatch_sync(dispatch_get_main_queue(), configure);
  }
  return configured;
}

int32_t iima_native_set_submenu_item_key_equivalent(
    const char *menu_title, const char *submenu_title, uintptr_t item_index,
    const char *key_equivalent, uint32_t modifier_mask) {
  if (menu_title == NULL || submenu_title == NULL || key_equivalent == NULL) {
    return 0;
  }
  NSString *menuTitle = [NSString stringWithUTF8String:menu_title];
  NSString *submenuTitle = [NSString stringWithUTF8String:submenu_title];
  NSString *keyEquivalent = [NSString stringWithUTF8String:key_equivalent];
  if (menuTitle == nil || submenuTitle == nil || keyEquivalent == nil) {
    return 0;
  }
  __block int32_t configured = 0;
  void (^configure)(void) = ^{
    configured = iima_set_submenu_item_key_equivalent_on_main(
        menuTitle, submenuTitle, (NSUInteger)item_index, keyEquivalent,
        modifier_mask);
  };
  if (NSThread.isMainThread) {
    configure();
  } else {
    dispatch_sync(dispatch_get_main_queue(), configure);
  }
  return configured;
}

int32_t iima_native_set_menu_item_key_equivalent_at_path(
    const char *menu_title, const uintptr_t *item_path,
    uintptr_t item_path_length, const char *key_equivalent,
    uint32_t modifier_mask) {
  if (menu_title == NULL || item_path == NULL || item_path_length == 0 ||
      key_equivalent == NULL) {
    return 0;
  }
  NSString *menuTitle = [NSString stringWithUTF8String:menu_title];
  NSString *keyEquivalent = [NSString stringWithUTF8String:key_equivalent];
  if (menuTitle == nil || keyEquivalent == nil) {
    return 0;
  }
  __block int32_t configured = 0;
  void (^configure)(void) = ^{
    configured = iima_set_menu_item_key_equivalent_at_path_on_main(
        menuTitle, item_path, item_path_length, keyEquivalent, modifier_mask);
  };
  if (NSThread.isMainThread) {
    configure();
  } else {
    dispatch_sync(dispatch_get_main_queue(), configure);
  }
  return configured;
}

int32_t iima_native_plugin_developer_tool_available(void) {
  if (@available(macOS 12.0, *)) {
    return 1;
  }
  return 0;
}

static int32_t iima_set_menu_item_state_at_path_on_main(
    NSString *menuTitle, const uintptr_t *itemPath, uintptr_t itemPathLength,
    BOOL selected) {
  NSMenu *menu = [NSApp.mainMenu itemWithTitle:menuTitle].submenu;
  if (menu == nil || itemPath == NULL || itemPathLength == 0) {
    return 0;
  }
  NSMenuItem *item = nil;
  for (uintptr_t depth = 0; depth < itemPathLength; depth++) {
    NSUInteger index = (NSUInteger)itemPath[depth];
    if (index >= (NSUInteger)menu.numberOfItems) {
      return 0;
    }
    item = [menu itemAtIndex:index];
    if (depth + 1 < itemPathLength) {
      menu = item.submenu;
      if (menu == nil) {
        return 0;
      }
    }
  }
  item.state = selected ? NSControlStateValueOn : NSControlStateValueOff;
  return 1;
}

int32_t iima_native_set_menu_item_state_at_path(
    const char *menu_title, const uintptr_t *item_path,
    uintptr_t item_path_length, int32_t selected) {
  if (menu_title == NULL || item_path == NULL || item_path_length == 0) {
    return 0;
  }
  NSString *menuTitle = [NSString stringWithUTF8String:menu_title];
  if (menuTitle == nil) {
    return 0;
  }
  __block int32_t configured = 0;
  void (^configure)(void) = ^{
    configured = iima_set_menu_item_state_at_path_on_main(
        menuTitle, item_path, item_path_length, selected != 0);
  };
  if (NSThread.isMainThread) {
    configure();
  } else {
    dispatch_sync(dispatch_get_main_queue(), configure);
  }
  return configured;
}

static int32_t iima_mark_menu_item_alternate_on_main(NSString *menuTitle,
                                                      NSString *itemTitle,
                                                      BOOL requireOptionAccelerator) {
  NSMenu *mainMenu = NSApp.mainMenu;
  NSMenu *submenu = [mainMenu itemWithTitle:menuTitle].submenu;
  if (submenu == nil) {
    return 0;
  }

  for (NSMenuItem *item in submenu.itemArray) {
    if (![item.title isEqualToString:itemTitle]) {
      continue;
    }
    NSEventModifierFlags modifiers = item.keyEquivalentModifierMask;
    if (requireOptionAccelerator &&
        (modifiers & NSEventModifierFlagOption) == 0) {
      continue;
    }
    item.alternate = YES;
    item.keyEquivalentModifierMask = modifiers | NSEventModifierFlagOption;
    return 1;
  }
  return 0;
}

int32_t iima_native_mark_menu_item_alternate(
    const char *menu_title, const char *item_title,
    int32_t require_option_accelerator) {
  if (menu_title == NULL || item_title == NULL) {
    return 0;
  }

  NSString *menuTitle = [NSString stringWithUTF8String:menu_title];
  NSString *itemTitle = [NSString stringWithUTF8String:item_title];
  if (menuTitle == nil || itemTitle == nil) {
    return 0;
  }

  __block int32_t configured = 0;
  void (^configure)(void) = ^{
    configured = iima_mark_menu_item_alternate_on_main(
        menuTitle, itemTitle, require_option_accelerator != 0);
  };
  if (NSThread.isMainThread) {
    configure();
  } else {
    dispatch_sync(dispatch_get_main_queue(), configure);
  }
  return configured;
}

static NSMenuItem *iima_find_menu_item_on_main(NSString *menuTitle,
                                                NSString *submenuTitle,
                                                NSString *itemTitle) {
  NSMenu *menu = [NSApp.mainMenu itemWithTitle:menuTitle].submenu;
  if (menu == nil) {
    return nil;
  }
  if (submenuTitle.length > 0) {
    menu = [menu itemWithTitle:submenuTitle].submenu;
    if (menu == nil) {
      return nil;
    }
  }
  return [menu itemWithTitle:itemTitle];
}

static int32_t iima_set_menu_item_responder_action_on_main(
    NSString *menuTitle, NSString *submenuTitle, NSString *itemTitle,
    NSString *selectorName, NSString *keyEquivalent) {
  NSMenuItem *item = iima_find_menu_item_on_main(menuTitle, submenuTitle,
                                                  itemTitle);
  if (item == nil || selectorName.length == 0) {
    return 0;
  }
  SEL selector = NSSelectorFromString(selectorName);
  if (selector == NULL) {
    return 0;
  }
  item.target = nil;
  item.action = selector;
  item.enabled = YES;
  if (keyEquivalent.length > 0) {
    item.keyEquivalent = keyEquivalent;
    item.keyEquivalentModifierMask = 0;
  }
  return 1;
}

int32_t iima_native_set_menu_item_responder_action(
    const char *menu_title, const char *submenu_title, const char *item_title,
    const char *selector, const char *key_equivalent) {
  if (menu_title == NULL || submenu_title == NULL || item_title == NULL ||
      selector == NULL || key_equivalent == NULL) {
    return 0;
  }
  NSString *menuTitle = [NSString stringWithUTF8String:menu_title];
  NSString *submenuTitle = [NSString stringWithUTF8String:submenu_title];
  NSString *itemTitle = [NSString stringWithUTF8String:item_title];
  NSString *selectorName = [NSString stringWithUTF8String:selector];
  NSString *keyEquivalent = [NSString stringWithUTF8String:key_equivalent];
  if (menuTitle == nil || submenuTitle == nil || itemTitle == nil ||
      selectorName == nil || keyEquivalent == nil) {
    return 0;
  }

  __block int32_t configured = 0;
  void (^configure)(void) = ^{
    configured = iima_set_menu_item_responder_action_on_main(
        menuTitle, submenuTitle, itemTitle, selectorName, keyEquivalent);
  };
  if (NSThread.isMainThread) {
    configure();
  } else {
    dispatch_sync(dispatch_get_main_queue(), configure);
  }
  return configured;
}

static int32_t iima_set_menu_item_hidden_on_main(NSString *menuTitle,
                                                  NSString *itemTitle,
                                                  BOOL hidden) {
  NSMenu *menu = [NSApp.mainMenu itemWithTitle:menuTitle].submenu;
  NSMenuItem *item = [menu itemWithTitle:itemTitle];
  if (item == nil) {
    return 0;
  }
  item.hidden = hidden;
  return 1;
}

int32_t iima_native_set_menu_item_hidden(const char *menu_title,
                                          const char *item_title,
                                          int32_t hidden) {
  if (menu_title == NULL || item_title == NULL) {
    return 0;
  }
  NSString *menuTitle = [NSString stringWithUTF8String:menu_title];
  NSString *itemTitle = [NSString stringWithUTF8String:item_title];
  if (menuTitle == nil || itemTitle == nil) {
    return 0;
  }

  __block int32_t configured = 0;
  void (^configure)(void) = ^{
    configured = iima_set_menu_item_hidden_on_main(menuTitle, itemTitle,
                                                    hidden != 0);
  };
  if (NSThread.isMainThread) {
    configure();
  } else {
    dispatch_sync(dispatch_get_main_queue(), configure);
  }
  return configured;
}
