use base64::Engine;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub artwork: Option<String>,
}

pub fn current() -> Option<NowPlaying> {
    let mut track = platform::current().filter(|track| !track.title.trim().is_empty())?;
    track.artwork = track.artwork.filter(|artwork| !artwork.trim().is_empty());
    if track.artwork.is_none() {
        track.artwork = artwork_for(&track);
    }
    Some(track)
}

const MUSICBRAINZ_MIN_INTERVAL: Duration = Duration::from_millis(1_100);
const ARTWORK_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const ARTWORK_MISS_TTL: Duration = Duration::from_secs(30 * 60);
const MAX_ARTWORK_BYTES: u64 = 5 * 1024 * 1024;

struct CachedArtwork {
    value: Option<String>,
    expires_at: Instant,
}

#[derive(Default)]
struct ArtworkState {
    cache: HashMap<String, CachedArtwork>,
    last_musicbrainz_request: Option<Instant>,
}

#[derive(Deserialize)]
struct MusicBrainzSearch {
    #[serde(default)]
    recordings: Vec<MusicBrainzRecording>,
}

#[derive(Deserialize)]
struct MusicBrainzRecording {
    #[serde(default)]
    score: u16,
    title: String,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<MusicBrainzArtistCredit>,
    #[serde(default)]
    releases: Vec<MusicBrainzRelease>,
}

#[derive(Deserialize)]
struct MusicBrainzArtistCredit {
    artist: MusicBrainzArtist,
}

#[derive(Deserialize)]
struct MusicBrainzArtist {
    name: String,
}

#[derive(Deserialize)]
struct MusicBrainzRelease {
    id: String,
    title: String,
    status: Option<String>,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<MusicBrainzArtistCredit>,
    #[serde(rename = "release-group")]
    release_group: Option<MusicBrainzReleaseGroup>,
}

#[derive(Deserialize)]
struct MusicBrainzReleaseGroup {
    id: String,
    #[serde(rename = "secondary-types", default)]
    secondary_types: Vec<String>,
}

fn artwork_for(track: &NowPlaying) -> Option<String> {
    let artist = artist_for_lookup(track.artist.as_deref()?);
    if artist.is_empty() {
        return None;
    }

    let title = title_for_lookup(&track.title, artist);
    let key = format!(
        "{}|{}|{}",
        normalize(artist),
        normalize(&title),
        normalize(track.album.as_deref().unwrap_or_default())
    );
    let state = artwork_state();

    {
        let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
        state
            .cache
            .retain(|_, entry| entry.expires_at > Instant::now());
        if let Some(entry) = state.cache.get(&key) {
            return entry.value.clone();
        }
        if let Some(last_request) = state.last_musicbrainz_request {
            let elapsed = last_request.elapsed();
            if elapsed < MUSICBRAINZ_MIN_INTERVAL {
                std::thread::sleep(MUSICBRAINZ_MIN_INTERVAL - elapsed);
            }
        }
        state.last_musicbrainz_request = Some(Instant::now());
        // Prevent overlapping three-second UI polls from resolving the same track twice.
        state.cache.insert(
            key.clone(),
            CachedArtwork {
                value: None,
                expires_at: Instant::now() + Duration::from_secs(30),
            },
        );
    }

    let value = lookup_musicbrainz_artwork(&title, artist, track.album.as_deref());
    let ttl = if value.is_some() {
        ARTWORK_CACHE_TTL
    } else {
        ARTWORK_MISS_TTL
    };
    state
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .cache
        .insert(
            key,
            CachedArtwork {
                value: value.clone(),
                expires_at: Instant::now() + ttl,
            },
        );
    value
}

fn artwork_state() -> &'static Mutex<ArtworkState> {
    static STATE: OnceLock<Mutex<ArtworkState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(ArtworkState::default()))
}

fn artwork_client() -> Option<&'static reqwest::blocking::Client> {
    static CLIENT: OnceLock<Option<reqwest::blocking::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
            match reqwest::blocking::Client::builder()
                .user_agent(concat!(
                    "Sonora/",
                    env!("CARGO_PKG_VERSION"),
                    " (https://github.com/l3vith/sonara)"
                ))
                .timeout(Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::limited(5))
                .build()
            {
                Ok(client) => Some(client),
                Err(error) => {
                    tracing::warn!(%error, "failed to initialize artwork HTTP client");
                    None
                }
            }
        })
        .as_ref()
}

fn lookup_musicbrainz_artwork(title: &str, artist: &str, album: Option<&str>) -> Option<String> {
    let client = artwork_client()?;
    let query = format!(
        "recording:\"{}\" AND artist:\"{}\"",
        musicbrainz_escape(title),
        musicbrainz_escape(artist)
    );
    let search = client
        .get("https://musicbrainz.org/ws/2/recording/")
        .query(&[("query", query.as_str()), ("fmt", "json"), ("limit", "10")])
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json::<MusicBrainzSearch>()
        .ok()?;

    let expected_title = normalize(title);
    let expected_artist = normalize(artist);
    let expected_album = album.map(normalize).filter(|value| !value.is_empty());
    let mut release_candidates = Vec::new();

    for recording in search.recordings {
        if recording.score < 90 || normalize(&recording.title) != expected_title {
            continue;
        }
        if !recording
            .artist_credit
            .iter()
            .any(|credit| normalize(&credit.artist.name) == expected_artist)
        {
            continue;
        }

        let mut releases = recording.releases;
        if let Some(expected_album) = expected_album.as_deref() {
            releases.sort_by_key(|release| normalize(&release.title) != expected_album);
        }
        for release in releases {
            if !release_is_trustworthy(&release, &expected_artist) {
                continue;
            }
            let priority = if expected_album
                .as_deref()
                .is_some_and(|album| normalize(&release.title) == album)
            {
                0
            } else if release.status.as_deref() == Some("Official")
                && !looks_like_live_release(&release.title)
            {
                1
            } else if looks_like_live_release(&release.title) {
                3
            } else {
                2
            };
            if let Some(group) = release.release_group {
                release_candidates.push((
                    priority,
                    format!(
                        "https://coverartarchive.org/release-group/{}/front-500",
                        group.id
                    ),
                ));
            }
            release_candidates.push((
                priority,
                format!(
                    "https://coverartarchive.org/release/{}/front-500",
                    release.id
                ),
            ));
        }
    }

    release_candidates.sort_by_key(|(priority, _)| *priority);
    let mut seen = HashSet::new();
    release_candidates
        .into_iter()
        .map(|(_, url)| url)
        .filter(|url| seen.insert(url.clone()))
        .take(8)
        .find_map(|url| download_artwork(client, &url))
}

fn release_is_trustworthy(release: &MusicBrainzRelease, expected_artist: &str) -> bool {
    if release.status.as_deref().is_some_and(|status| {
        matches!(
            status,
            "Expunged" | "Cancelled" | "Withdrawn" | "Bootleg" | "Pseudo-Release"
        )
    }) {
        return false;
    }
    if release.release_group.as_ref().is_some_and(|group| {
        group
            .secondary_types
            .iter()
            .any(|kind| kind.eq_ignore_ascii_case("Compilation"))
    }) {
        return false;
    }
    release.artist_credit.is_empty()
        || release
            .artist_credit
            .iter()
            .any(|credit| normalize(&credit.artist.name) == expected_artist)
}

fn looks_like_live_release(title: &str) -> bool {
    let normalized = normalize(title);
    normalized.contains("live")
        || normalized.contains("bootleg")
        || normalized.contains("concert")
        || normalized.contains("session")
        || (title.contains('/') && title.chars().any(|character| character.is_ascii_digit()))
}

fn download_artwork(client: &reqwest::blocking::Client, url: &str) -> Option<String> {
    let response = client.get(url).send().ok()?.error_for_status().ok()?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_ARTWORK_BYTES)
    {
        return None;
    }
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .filter(|value| value.starts_with("image/"))?
        .to_owned();
    let bytes = response.bytes().ok()?;
    if bytes.len() as u64 > MAX_ARTWORK_BYTES {
        return None;
    }
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn title_without_artist_prefix<'a>(title: &'a str, artist: &str) -> &'a str {
    for separator in [" - ", " – ", " — "] {
        if let Some((prefix, rest)) = title.split_once(separator) {
            if normalize(prefix) == normalize(artist) && !rest.trim().is_empty() {
                return rest.trim();
            }
        }
    }
    title.trim()
}

fn title_for_lookup(title: &str, artist: &str) -> String {
    let mut title = title_without_artist_prefix(title, artist).trim().to_owned();
    loop {
        let Some(cleaned) = strip_presentation_suffix(&title) else {
            break;
        };
        title = cleaned.trim().to_owned();
    }
    strip_wrapping_quotes(title.trim()).trim().to_owned()
}

fn strip_presentation_suffix(title: &str) -> Option<&str> {
    let title = title.trim();
    for (open, close) in [('(', ')'), ('[', ']')] {
        if title.ends_with(close) {
            if let Some(start) = title.rfind(open) {
                let label = &title[start + open.len_utf8()..title.len() - close.len_utf8()];
                if is_presentation_label(label) {
                    return Some(title[..start].trim_end());
                }
            }
        }
    }

    for separator in [" | ", " - ", " – ", " — "] {
        if let Some((name, label)) = title.rsplit_once(separator) {
            if is_presentation_label(label) {
                return Some(name.trim_end());
            }
        }
    }
    None
}

fn is_presentation_label(label: &str) -> bool {
    let label = normalize(label);
    if [
        "live", "remix", "acoustic", "demo", "remaster", "edit", "version",
    ]
    .iter()
    .any(|meaningful| label.contains(meaningful))
    {
        return false;
    }
    matches!(
        label.as_str(),
        "audio"
            | "officialaudio"
            | "video"
            | "officialvideo"
            | "musicvideo"
            | "officialmusicvideo"
            | "lyric"
            | "lyrics"
            | "lyricvideo"
            | "officiallyricvideo"
            | "visualizer"
            | "officialvisualizer"
            | "hd"
            | "4k"
    ) || (label.starts_with("official")
        && ["audio", "video", "lyric", "visualizer"]
            .iter()
            .any(|kind| label.contains(kind)))
}

fn strip_wrapping_quotes(value: &str) -> &str {
    for (open, close) in [('\'', '\''), ('"', '"'), ('‘', '’'), ('“', '”')] {
        if let Some(inner) = value
            .strip_prefix(open)
            .and_then(|inner| inner.strip_suffix(close))
        {
            if !inner.trim().is_empty() {
                return inner;
            }
        }
    }
    value
}

fn artist_for_lookup(artist: &str) -> &str {
    let artist = artist.trim();
    for separator in [" - ", " – ", " — "] {
        if let Some((name, suffix)) = artist.rsplit_once(separator) {
            if suffix.eq_ignore_ascii_case("topic") && !name.trim().is_empty() {
                return name.trim();
            }
        }
    }
    artist
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn musicbrainz_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(target_os = "macos")]
mod platform {
    use super::NowPlaying;
    use std::ffi::CStr;
    use std::process::Command;

    const NOW_PLAYING_SCRIPT: &str = r#"
function run() {
  ObjC.import('Foundation');
  const framework = $.NSBundle.bundleWithPath('/System/Library/PrivateFrameworks/MediaRemote.framework/');
  framework.load;
  const Request = $.NSClassFromString('MRNowPlayingRequest');
  if (!Request) return 'null';

  const item = Request.localNowPlayingItem;
  const info = item ? item.nowPlayingInfo : null;
  const value = key => {
    if (!info) return null;
    const raw = info.valueForKey(key);
    return raw ? String(ObjC.unwrap(raw)) : null;
  };
  const path = Request.localNowPlayingPlayerPath;
  const app = path && path.client && path.client.displayName
    ? String(ObjC.unwrap(path.client.displayName))
    : null;
  const result = {
    title: value('kMRMediaRemoteNowPlayingInfoTitle'),
    artist: value('kMRMediaRemoteNowPlayingInfoArtist'),
    album: value('kMRMediaRemoteNowPlayingInfoAlbum')
  };

  if (app === 'Spotify') {
    try {
      const spotify = Application('Spotify');
      const track = spotify.currentTrack();
      const spotifyTitle = track.name();
      if (!result.title) result.title = spotifyTitle;
      if (!result.artist) result.artist = track.artist();
      if (!result.album) result.album = track.album();
      result.artwork = track.artworkUrl();
    } catch (_) {}
  }

  return result.title ? JSON.stringify(result) : 'null';
}
"#;

    unsafe extern "C" {
        fn sonora_media_remote_now_playing() -> *const std::ffi::c_char;
        fn sonora_media_remote_free(value: *const std::ffi::c_char);
    }

    pub fn current() -> Option<NowPlaying> {
        let Some(mut current) = system_current() else {
            return legacy_current();
        };
        if current.artwork.is_none() {
            if let Some(fallback) = legacy_current() {
                if super::normalize(&fallback.title) == super::normalize(&current.title) {
                    current.artwork = fallback.artwork;
                    if current.artist.is_none() {
                        current.artist = fallback.artist;
                    }
                    if current.album.is_none() {
                        current.album = fallback.album;
                    }
                }
            }
        }
        Some(current)
    }

    fn system_current() -> Option<NowPlaying> {
        let output = Command::new("/usr/bin/osascript")
            .args(["-l", "JavaScript", "-e", NOW_PLAYING_SCRIPT])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        serde_json::from_slice(&output.stdout).ok()
    }

    fn legacy_current() -> Option<NowPlaying> {
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

#[cfg(test)]
mod tests {
    use super::{
        artist_for_lookup, artwork_client, artwork_for, musicbrainz_escape, normalize,
        release_is_trustworthy, title_for_lookup, title_without_artist_prefix, MusicBrainzRelease,
        NowPlaying,
    };

    #[test]
    fn artwork_http_client_initializes() {
        assert!(artwork_client().is_some());
    }

    #[test]
    fn normalizes_track_metadata_for_matching() {
        assert_eq!(normalize("Where Am I?"), "whereami");
        assert_eq!(normalize("TITLE FIGHT"), "titlefight");
    }

    #[test]
    fn removes_matching_artist_prefix() {
        assert_eq!(
            title_without_artist_prefix("Title Fight - Where Am I?", "Title Fight"),
            "Where Am I?"
        );
        assert_eq!(
            title_without_artist_prefix("Artist — Song", "Different Artist"),
            "Artist — Song"
        );
    }

    #[test]
    fn removes_youtube_presentation_text_from_titles() {
        assert_eq!(
            title_for_lookup(
                "Black Country, New Road - 'The Place Where He Inserted the Blade' (Official Audio)",
                "Black Country, New Road"
            ),
            "The Place Where He Inserted the Blade"
        );
        assert_eq!(
            title_for_lookup("An Artist - \"A Song\" [Official Music Video]", "An Artist"),
            "A Song"
        );
    }

    #[test]
    fn preserves_meaningful_song_versions() {
        assert_eq!(
            title_for_lookup("A Song (Live at Bush Hall)", "An Artist"),
            "A Song (Live at Bush Hall)"
        );
        assert_eq!(
            title_for_lookup("A Song - Acoustic Version", "An Artist"),
            "A Song - Acoustic Version"
        );
    }

    #[test]
    fn rejects_polluted_compilation_releases() {
        let polluted: MusicBrainzRelease = serde_json::from_value(serde_json::json!({
            "id": "5511b326-4527-447f-af85-b9a2d07c112c",
            "title": "Buddy Holly Remastered",
            "status": "Expunged",
            "artist-credit": [{ "artist": { "name": "gaymichaelafton" } }],
            "release-group": {
                "id": "d78f2674-47ea-4dbb-b25b-4968d3be8489",
                "secondary-types": ["Compilation"]
            }
        }))
        .unwrap();
        let official: MusicBrainzRelease = serde_json::from_value(serde_json::json!({
            "id": "e461c1b4-4eb5-46c2-8eb1-1b29e1befdf4",
            "title": "French Exit",
            "status": "Official",
            "release-group": { "id": "a7e457e6-4124-4cb5-9f2a-8c35a30835a3" }
        }))
        .unwrap();

        assert!(!release_is_trustworthy(&polluted, "tvgirl"));
        assert!(release_is_trustworthy(&official, "tvgirl"));
    }

    #[test]
    fn removes_youtube_topic_suffix_for_musicbrainz() {
        assert_eq!(
            artist_for_lookup("Neutral Milk Hotel - Topic"),
            "Neutral Milk Hotel"
        );
        assert_eq!(artist_for_lookup("An Artist"), "An Artist");
    }

    #[test]
    fn escapes_musicbrainz_query_values() {
        assert_eq!(
            musicbrainz_escape("A \\\"quote\\\""),
            "A \\\\\\\"quote\\\\\\\""
        );
    }

    #[test]
    #[ignore = "uses the live MusicBrainz and Cover Art Archive services"]
    fn resolves_known_artwork_end_to_end() {
        let track = NowPlaying {
            title: "Hate Yourself".into(),
            artist: Some("TV Girl - Topic".into()),
            album: None,
            artwork: None,
        };
        let artwork = artwork_for(&track).expect("known track should resolve artwork");
        assert!(artwork.starts_with("data:image/"));
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::NowPlaying;
    pub fn current() -> Option<NowPlaying> {
        None
    }
}
