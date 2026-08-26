#import <Foundation/Foundation.h>
#import <dispatch/dispatch.h>
#import <dlfcn.h>

typedef void (*MRNowPlayingFn)(dispatch_queue_t, void (^)(CFDictionaryRef));

const char *sonora_media_remote_now_playing(void) {
  @autoreleasepool {
    void *framework = dlopen("/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote", RTLD_LAZY);
    if (framework == NULL) return NULL;
    MRNowPlayingFn get_now_playing = (MRNowPlayingFn)dlsym(framework, "MRMediaRemoteGetNowPlayingInfo");
    if (get_now_playing == NULL) return NULL;

    __block NSData *encoded = nil;
    dispatch_semaphore_t completed = dispatch_semaphore_create(0);
    get_now_playing(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^(CFDictionaryRef information) {
      NSDictionary *info = (__bridge NSDictionary *)information;
      NSString *title = info[@"kMRMediaRemoteNowPlayingInfoTitle"];
      if (title.length > 0) {
        NSMutableDictionary *result = [@{ @"title": title } mutableCopy];
        NSString *artist = info[@"kMRMediaRemoteNowPlayingInfoArtist"];
        NSString *album = info[@"kMRMediaRemoteNowPlayingInfoAlbum"];
        if (artist.length > 0) result[@"artist"] = artist;
        if (album.length > 0) result[@"album"] = album;
        NSData *artwork = info[@"kMRMediaRemoteNowPlayingInfoArtworkData"];
        if (artwork.length > 0) {
          NSString *mime = info[@"kMRMediaRemoteNowPlayingInfoArtworkMIMEType"];
          if (mime.length == 0) mime = @"image/jpeg";
          NSString *base64 = [artwork base64EncodedStringWithOptions:0];
          result[@"artwork"] = [NSString stringWithFormat:@"data:%@;base64,%@", mime, base64];
        }
        encoded = [NSJSONSerialization dataWithJSONObject:result options:0 error:nil];
      }
      dispatch_semaphore_signal(completed);
    });
    if (dispatch_semaphore_wait(completed, dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_SEC)) != 0 || encoded == nil) return NULL;
    NSString *json = [[NSString alloc] initWithData:encoded encoding:NSUTF8StringEncoding];
    return json == nil ? NULL : strdup(json.UTF8String);
  }
}

void sonora_media_remote_free(const char *value) { free((void *)value); }
