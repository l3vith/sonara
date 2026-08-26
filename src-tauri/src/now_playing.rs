#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub artwork: Option<String>,
}

pub fn current() -> Option<NowPlaying> {
    platform::current().filter(|track| !track.title.trim().is_empty())
}

#[cfg(target_os = "macos")]
mod platform {
    use super::NowPlaying;
    use std::ffi::CStr;

    unsafe extern "C" {
        fn sonora_media_remote_now_playing() -> *const std::ffi::c_char;
        fn sonora_media_remote_free(value: *const std::ffi::c_char);
    }

    pub fn current() -> Option<NowPlaying> {
        let raw = unsafe { sonora_media_remote_now_playing() };
        if raw.is_null() {
            return None;
        }
        let result = unsafe { CStr::from_ptr(raw).to_string_lossy().into_owned() };
        unsafe { sonora_media_remote_free(raw) };
        serde_json::from_str(&result).ok()
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::NowPlaying;
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

    pub fn current() -> Option<NowPlaying> {
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .ok()?
            .get()
            .ok()?;
        let session = manager.GetCurrentSession().ok()?;
        let properties = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
        let title = properties.Title().ok()?.to_string();
        if title.trim().is_empty() {
            return None;
        }
        Some(NowPlaying {
            title,
            artist: properties
                .Artist()
                .ok()
                .map(|value| value.to_string())
                .filter(|value| !value.is_empty()),
            album: properties
                .AlbumTitle()
                .ok()
                .map(|value| value.to_string())
                .filter(|value| !value.is_empty()),
            artwork: None,
        })
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::NowPlaying;
    pub fn current() -> Option<NowPlaying> {
        None
    }
}
