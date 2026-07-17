use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use serde::Serialize;

use crate::history::{mpv_watch_later_md5, playback_progress_from_watch_later};
use crate::media::{probe_media, MediaProbe};

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PlaylistCacheItem {
    pub path: String,
    pub ready: bool,
    pub duration_seconds: Option<f64>,
    pub playback_progress_seconds: Option<f64>,
    pub metadata_title: Option<String>,
    pub metadata_album: Option<String>,
    pub metadata_artist: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PlaylistCacheSnapshot {
    pub items: Vec<PlaylistCacheItem>,
    pub total_duration_seconds: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct PlaylistInfoCache {
    entries: BTreeMap<String, PlaylistCacheItem>,
    queue: VecDeque<String>,
    queued: HashSet<String>,
    worker_running: bool,
}

impl PlaylistInfoCache {
    pub fn record_runtime(
        &mut self,
        path: &str,
        duration_seconds: f64,
        playback_progress_seconds: f64,
        title: &str,
        album: &str,
        artist: &str,
    ) {
        if path.is_empty() {
            return;
        }
        let entry = self.entry_mut(path);
        if duration_seconds.is_finite() && duration_seconds > 0.0 {
            entry.duration_seconds = Some(duration_seconds);
        }
        if playback_progress_seconds.is_finite() && playback_progress_seconds >= 0.0 {
            entry.playback_progress_seconds = Some(playback_progress_seconds);
        }
        set_nonempty(&mut entry.metadata_title, title);
        set_nonempty(&mut entry.metadata_album, album);
        set_nonempty(&mut entry.metadata_artist, artist);
    }

    pub fn snapshot<'a>(&self, paths: impl IntoIterator<Item = &'a str>) -> PlaylistCacheSnapshot {
        let items = paths
            .into_iter()
            .map(|path| {
                self.entries
                    .get(path)
                    .cloned()
                    .unwrap_or_else(|| PlaylistCacheItem {
                        path: path.to_string(),
                        ..PlaylistCacheItem::default()
                    })
            })
            .collect::<Vec<_>>();
        // IINA keeps the total hidden when even one playlist item has no
        // cached duration. A completed probe is not sufficient by itself: an
        // unsupported item may still have metadata but no duration.
        let total_duration_seconds = items
            .iter()
            .all(|entry| {
                entry.ready
                    && entry
                        .duration_seconds
                        .is_some_and(|duration| duration.is_finite())
            })
            .then(|| {
                items
                    .iter()
                    .filter_map(|entry| entry.duration_seconds)
                    .filter(|duration| *duration > 0.0)
                    .sum()
            });
        PlaylistCacheSnapshot {
            items,
            total_duration_seconds,
        }
    }

    fn entry_mut(&mut self, path: &str) -> &mut PlaylistCacheItem {
        self.entries
            .entry(path.to_string())
            .or_insert_with(|| PlaylistCacheItem {
                path: path.to_string(),
                ..PlaylistCacheItem::default()
            })
    }

    fn complete(&mut self, path: &str, probe: Result<MediaProbe, String>, progress: Option<f64>) {
        let entry = self.entry_mut(path);
        entry.ready = true;
        if let Ok(probe) = probe {
            entry.duration_seconds = finite(probe.duration_seconds).or(entry.duration_seconds);
            entry.metadata_title = nonempty(probe.title).or_else(|| entry.metadata_title.clone());
            entry.metadata_album = nonempty(probe.album).or_else(|| entry.metadata_album.clone());
            entry.metadata_artist =
                nonempty(probe.artist).or_else(|| entry.metadata_artist.clone());
        }
        if progress.is_some_and(|value| value.is_finite() && value >= 0.0) {
            entry.playback_progress_seconds = progress;
        }
    }
}

pub fn schedule_playlist_cache(
    cache: Arc<Mutex<PlaylistInfoCache>>,
    paths: impl IntoIterator<Item = String>,
    watch_later_directory: Option<PathBuf>,
) -> Result<usize, String> {
    let (scheduled, should_start) = {
        let mut cache = cache.lock().map_err(|error| error.to_string())?;
        let mut scheduled = 0;
        for path in paths {
            let ready = cache.entries.get(&path).is_some_and(|entry| entry.ready);
            if !path.is_empty() && !ready && cache.queued.insert(path.clone()) {
                cache.queue.push_back(path);
                scheduled += 1;
            }
        }
        let should_start = !cache.worker_running && !cache.queue.is_empty();
        if should_start {
            cache.worker_running = true;
        }
        (scheduled, should_start)
    };

    if should_start {
        let worker_cache = Arc::clone(&cache);
        thread::Builder::new()
            .name("iima-playlist-info".to_string())
            .spawn(move || playlist_cache_worker(worker_cache, watch_later_directory))
            .map_err(|error| {
                if let Ok(mut cache) = cache.lock() {
                    cache.worker_running = false;
                }
                format!("Unable to start playlist information worker: {error}")
            })?;
    }
    Ok(scheduled)
}

fn playlist_cache_worker(
    cache: Arc<Mutex<PlaylistInfoCache>>,
    watch_later_directory: Option<PathBuf>,
) {
    loop {
        let path = {
            let Ok(mut cache) = cache.lock() else {
                return;
            };
            let Some(path) = cache.queue.pop_front() else {
                cache.worker_running = false;
                return;
            };
            path
        };

        let progress = watch_later_directory.as_ref().and_then(|directory| {
            playback_progress_from_watch_later(&directory.join(mpv_watch_later_md5(&path)))
        });
        let probe = probe_media(&path);
        let Ok(mut cache) = cache.lock() else {
            return;
        };
        cache.complete(&path, probe, progress);
        cache.queued.remove(&path);
    }
}

fn finite(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite())
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn set_nonempty(destination: &mut Option<String>, value: &str) {
    let trimmed = value.trim();
    if !trimmed.is_empty() {
        *destination = Some(trimmed.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{MediaChapterProbe, MediaStreamProbe};

    fn probe(path: &str, duration: Option<f64>) -> MediaProbe {
        MediaProbe {
            path: path.to_string(),
            title: Some("Track title".to_string()),
            album: Some("Album".to_string()),
            artist: Some("Artist".to_string()),
            duration_seconds: duration,
            format_name: None,
            format_long_name: None,
            bit_rate: None,
            streams: Vec::<MediaStreamProbe>::new(),
            chapters: Vec::<MediaChapterProbe>::new(),
        }
    }

    #[test]
    fn total_is_hidden_until_every_playlist_path_has_a_cached_duration() {
        let mut cache = PlaylistInfoCache::default();
        cache.complete("one.mp4", Ok(probe("one.mp4", Some(12.0))), Some(3.0));
        let pending = cache.snapshot(["one.mp4", "two.mp4"]);
        assert_eq!(pending.total_duration_seconds, None);

        cache.complete("two.mp4", Err("unsupported".to_string()), None);
        let missing_duration = cache.snapshot(["one.mp4", "two.mp4"]);
        assert_eq!(missing_duration.total_duration_seconds, None);
        assert!(missing_duration.items.iter().all(|entry| entry.ready));

        cache.complete("two.mp4", Ok(probe("two.mp4", Some(0.0))), None);
        let ready = cache.snapshot(["one.mp4", "two.mp4"]);
        assert_eq!(ready.total_duration_seconds, Some(12.0));
    }

    #[test]
    fn current_runtime_values_are_visible_without_marking_probe_ready() {
        let mut cache = PlaylistInfoCache::default();
        cache.record_runtime(
            "/tmp/song.m4a",
            91.5,
            12.0,
            "Live title",
            "Live album",
            "Live artist",
        );
        let snapshot = cache.snapshot(["/tmp/song.m4a"]);
        assert_eq!(snapshot.total_duration_seconds, None);
        assert_eq!(snapshot.items[0].duration_seconds, Some(91.5));
        assert_eq!(snapshot.items[0].playback_progress_seconds, Some(12.0));
        assert_eq!(
            snapshot.items[0].metadata_artist.as_deref(),
            Some("Live artist")
        );
    }

    #[test]
    fn completion_preserves_better_runtime_values_when_probe_has_no_data() {
        let mut cache = PlaylistInfoCache::default();
        cache.record_runtime("stream", 20.0, 4.0, "Runtime", "", "Artist");
        cache.complete("stream", Err("no probe".to_string()), None);
        let snapshot = cache.snapshot(["stream"]);
        assert_eq!(snapshot.total_duration_seconds, Some(20.0));
        assert_eq!(snapshot.items[0].metadata_title.as_deref(), Some("Runtime"));
    }
}
