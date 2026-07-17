#import <Foundation/Foundation.h>
#import <Security/Security.h>

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static NSString *const IIMAHTTPAuthService = @"IINA Saved HTTP Password";
static NSString *const IIMAOpenSubtitlesService = @"IINA OpenSubtitles Account";

static char *iima_copy_utf8(NSString *value) {
  if (value == nil) {
    return NULL;
  }
  const char *utf8 = value.UTF8String;
  if (utf8 == NULL) {
    return NULL;
  }
  size_t length = strlen(utf8);
  char *copy = malloc(length + 1);
  if (copy != NULL) {
    memcpy(copy, utf8, length + 1);
  }
  return copy;
}

static void iima_set_keychain_error(OSStatus status, char **error_out) {
  if (error_out == NULL) {
    return;
  }
  CFStringRef message = SecCopyErrorMessageString(status, NULL);
  NSString *description = CFBridgingRelease(message);
  *error_out = iima_copy_utf8(description ?: [NSString stringWithFormat:@"Keychain error %d", status]);
}

static NSMutableDictionary *iima_http_auth_query(NSString *server, int32_t port) {
  NSMutableDictionary *query = [@{
    (__bridge id)kSecClass: (__bridge id)kSecClassInternetPassword,
    (__bridge id)kSecAttrService: IIMAHTTPAuthService,
    (__bridge id)kSecAttrServer: server,
  } mutableCopy];
  if (port > 0) {
    query[(__bridge id)kSecAttrPort] = @(port);
  }
  return query;
}

static NSMutableDictionary *iima_opensubtitles_query(NSString *username) {
  return [@{
    (__bridge id)kSecClass: (__bridge id)kSecClassGenericPassword,
    (__bridge id)kSecAttrService: IIMAOpenSubtitlesService,
    (__bridge id)kSecAttrAccount: username,
  } mutableCopy];
}

static NSMutableDictionary *iima_generic_password_query(NSString *service, NSString *account) {
  return [@{
    (__bridge id)kSecClass: (__bridge id)kSecClassGenericPassword,
    (__bridge id)kSecAttrService: service,
    (__bridge id)kSecAttrAccount: account,
  } mutableCopy];
}

int32_t iima_keychain_read_generic(const char *service_utf8,
                                   const char *account_utf8,
                                   char **password_out,
                                   char **error_out) {
  if (password_out != NULL) *password_out = NULL;
  if (error_out != NULL) *error_out = NULL;
  if (service_utf8 == NULL || account_utf8 == NULL || password_out == NULL) {
    return -1;
  }

  @autoreleasepool {
    NSString *service = [NSString stringWithUTF8String:service_utf8];
    NSString *account = [NSString stringWithUTF8String:account_utf8];
    if (service.length == 0 || account == nil) {
      return -1;
    }
    NSMutableDictionary *query = iima_generic_password_query(service, account);
    query[(__bridge id)kSecMatchLimit] = (__bridge id)kSecMatchLimitOne;
    query[(__bridge id)kSecReturnData] = @YES;

    CFTypeRef item_ref = NULL;
    OSStatus status = SecItemCopyMatching((__bridge CFDictionaryRef)query, &item_ref);
    if (status == errSecItemNotFound) {
      return 0;
    }
    if (status != errSecSuccess) {
      iima_set_keychain_error(status, error_out);
      return -1;
    }

    NSData *password_data = CFBridgingRelease(item_ref);
    NSString *password = [[NSString alloc] initWithData:password_data encoding:NSUTF8StringEncoding];
    if (password == nil) {
      if (error_out != NULL) {
        *error_out = iima_copy_utf8(@"Keychain returned unexpected generic password data");
      }
      return -1;
    }
    *password_out = iima_copy_utf8(password);
    if (*password_out == NULL) {
      if (error_out != NULL) {
        *error_out = iima_copy_utf8(@"Unable to allocate generic password result");
      }
      return -1;
    }
    return 1;
  }
}

int32_t iima_keychain_write_generic(const char *service_utf8,
                                    const char *account_utf8,
                                    const char *password_utf8,
                                    char **error_out) {
  if (error_out != NULL) *error_out = NULL;
  if (service_utf8 == NULL || account_utf8 == NULL || password_utf8 == NULL) {
    return -1;
  }

  @autoreleasepool {
    NSString *service = [NSString stringWithUTF8String:service_utf8];
    NSString *account = [NSString stringWithUTF8String:account_utf8];
    NSString *password = [NSString stringWithUTF8String:password_utf8];
    NSData *password_data = [password dataUsingEncoding:NSUTF8StringEncoding];
    if (service.length == 0 || account == nil || password == nil || password_data == nil) {
      return -1;
    }

    NSMutableDictionary *query = iima_generic_password_query(service, account);
    NSDictionary *attributes = @{ (__bridge id)kSecValueData: password_data };
    OSStatus status = SecItemUpdate((__bridge CFDictionaryRef)query,
                                    (__bridge CFDictionaryRef)attributes);
    if (status == errSecItemNotFound) {
      NSMutableDictionary *item = [query mutableCopy];
      item[(__bridge id)kSecAttrLabel] = service;
      [item addEntriesFromDictionary:attributes];
      status = SecItemAdd((__bridge CFDictionaryRef)item, NULL);
      if (status == errSecDuplicateItem) {
        status = SecItemUpdate((__bridge CFDictionaryRef)query,
                               (__bridge CFDictionaryRef)attributes);
      }
    }
    if (status != errSecSuccess) {
      iima_set_keychain_error(status, error_out);
      return -1;
    }
    return 0;
  }
}

int32_t iima_keychain_read_http_auth(const char *server_utf8,
                                     int32_t port,
                                     char **username_out,
                                     char **password_out,
                                     char **error_out) {
  if (username_out != NULL) *username_out = NULL;
  if (password_out != NULL) *password_out = NULL;
  if (error_out != NULL) *error_out = NULL;
  if (server_utf8 == NULL || username_out == NULL || password_out == NULL) {
    return -1;
  }

  @autoreleasepool {
    NSString *server = [NSString stringWithUTF8String:server_utf8];
    if (server.length == 0) {
      return -1;
    }
    NSMutableDictionary *query = iima_http_auth_query(server, port);
    query[(__bridge id)kSecMatchLimit] = (__bridge id)kSecMatchLimitOne;
    query[(__bridge id)kSecReturnAttributes] = @YES;
    query[(__bridge id)kSecReturnData] = @YES;

    CFTypeRef item_ref = NULL;
    OSStatus status = SecItemCopyMatching((__bridge CFDictionaryRef)query, &item_ref);
    if (status == errSecItemNotFound) {
      return 0;
    }
    if (status != errSecSuccess) {
      iima_set_keychain_error(status, error_out);
      return -1;
    }

    NSDictionary *item = CFBridgingRelease(item_ref);
    NSString *username = item[(__bridge id)kSecAttrAccount];
    NSData *password_data = item[(__bridge id)kSecValueData];
    NSString *password = [[NSString alloc] initWithData:password_data encoding:NSUTF8StringEncoding];
    if (![username isKindOfClass:NSString.class] || password == nil) {
      if (error_out != NULL) {
        *error_out = iima_copy_utf8(@"Keychain returned unexpected HTTP credential data");
      }
      return -1;
    }

    *username_out = iima_copy_utf8(username);
    *password_out = iima_copy_utf8(password);
    if (*username_out == NULL || *password_out == NULL) {
      free(*username_out);
      free(*password_out);
      *username_out = NULL;
      *password_out = NULL;
      if (error_out != NULL) {
        *error_out = iima_copy_utf8(@"Unable to allocate HTTP credential result");
      }
      return -1;
    }
    return 1;
  }
}

int32_t iima_keychain_write_http_auth(const char *server_utf8,
                                      int32_t port,
                                      const char *username_utf8,
                                      const char *password_utf8,
                                      char **error_out) {
  if (error_out != NULL) *error_out = NULL;
  if (server_utf8 == NULL || username_utf8 == NULL || password_utf8 == NULL) {
    return -1;
  }

  @autoreleasepool {
    NSString *server = [NSString stringWithUTF8String:server_utf8];
    NSString *username = [NSString stringWithUTF8String:username_utf8];
    NSString *password = [NSString stringWithUTF8String:password_utf8];
    NSData *password_data = [password dataUsingEncoding:NSUTF8StringEncoding];
    if (server.length == 0 || username.length == 0 || password == nil || password_data == nil) {
      return -1;
    }

    NSMutableDictionary *query = iima_http_auth_query(server, port);
    NSDictionary *attributes = @{
      (__bridge id)kSecAttrAccount: username,
      (__bridge id)kSecValueData: password_data,
    };
    OSStatus status = SecItemUpdate((__bridge CFDictionaryRef)query,
                                    (__bridge CFDictionaryRef)attributes);
    if (status == errSecItemNotFound) {
      NSMutableDictionary *item = [query mutableCopy];
      item[(__bridge id)kSecAttrLabel] = IIMAHTTPAuthService;
      [item addEntriesFromDictionary:attributes];
      status = SecItemAdd((__bridge CFDictionaryRef)item, NULL);
      if (status == errSecDuplicateItem) {
        status = SecItemUpdate((__bridge CFDictionaryRef)query,
                               (__bridge CFDictionaryRef)attributes);
      }
    }
    if (status != errSecSuccess) {
      iima_set_keychain_error(status, error_out);
      return -1;
    }
    return 0;
  }
}

int32_t iima_keychain_read_opensubtitles(const char *username_utf8,
                                         char **password_out,
                                         char **error_out) {
  if (password_out != NULL) *password_out = NULL;
  if (error_out != NULL) *error_out = NULL;
  if (username_utf8 == NULL || password_out == NULL) {
    return -1;
  }

  @autoreleasepool {
    NSString *username = [NSString stringWithUTF8String:username_utf8];
    if (username.length == 0) {
      return -1;
    }
    NSMutableDictionary *query = iima_opensubtitles_query(username);
    query[(__bridge id)kSecMatchLimit] = (__bridge id)kSecMatchLimitOne;
    query[(__bridge id)kSecReturnData] = @YES;

    CFTypeRef item_ref = NULL;
    OSStatus status = SecItemCopyMatching((__bridge CFDictionaryRef)query, &item_ref);
    if (status == errSecItemNotFound) {
      return 0;
    }
    if (status != errSecSuccess) {
      iima_set_keychain_error(status, error_out);
      return -1;
    }

    NSData *password_data = CFBridgingRelease(item_ref);
    NSString *password = [[NSString alloc] initWithData:password_data encoding:NSUTF8StringEncoding];
    if (password == nil) {
      if (error_out != NULL) {
        *error_out = iima_copy_utf8(@"Keychain returned unexpected OpenSubtitles credential data");
      }
      return -1;
    }
    *password_out = iima_copy_utf8(password);
    if (*password_out == NULL) {
      if (error_out != NULL) {
        *error_out = iima_copy_utf8(@"Unable to allocate OpenSubtitles credential result");
      }
      return -1;
    }
    return 1;
  }
}

int32_t iima_keychain_write_opensubtitles(const char *username_utf8,
                                          const char *password_utf8,
                                          char **error_out) {
  if (error_out != NULL) *error_out = NULL;
  if (username_utf8 == NULL || password_utf8 == NULL) {
    return -1;
  }

  @autoreleasepool {
    NSString *username = [NSString stringWithUTF8String:username_utf8];
    NSString *password = [NSString stringWithUTF8String:password_utf8];
    NSData *password_data = [password dataUsingEncoding:NSUTF8StringEncoding];
    if (username.length == 0 || password == nil || password_data == nil) {
      return -1;
    }

    NSMutableDictionary *query = iima_opensubtitles_query(username);
    NSDictionary *attributes = @{
      (__bridge id)kSecValueData: password_data,
    };
    OSStatus status = SecItemUpdate((__bridge CFDictionaryRef)query,
                                    (__bridge CFDictionaryRef)attributes);
    if (status == errSecItemNotFound) {
      NSMutableDictionary *item = [query mutableCopy];
      item[(__bridge id)kSecAttrLabel] = IIMAOpenSubtitlesService;
      [item addEntriesFromDictionary:attributes];
      status = SecItemAdd((__bridge CFDictionaryRef)item, NULL);
      if (status == errSecDuplicateItem) {
        status = SecItemUpdate((__bridge CFDictionaryRef)query,
                               (__bridge CFDictionaryRef)attributes);
      }
    }
    if (status != errSecSuccess) {
      iima_set_keychain_error(status, error_out);
      return -1;
    }
    return 0;
  }
}

void iima_keychain_free_string(char *value) {
  free(value);
}
