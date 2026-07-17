use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const PLAYABLE_DIRECTORY_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "m4v", "mov", "3gp", "ts", "mts", "m2ts", "wmv", "flv", "f4v", "asf",
    "webm", "rm", "rmvb", "qt", "dv", "mpg", "mpeg", "mxf", "vob", "gif", "ogv", "ogm", "mp3",
    "aac", "mka", "dts", "flac", "ogg", "oga", "mogg", "m4a", "ac3", "opus", "wav", "wv", "aiff",
    "aif", "ape", "tta", "tak",
];

const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "m4v", "mov", "3gp", "ts", "mts", "m2ts", "wmv", "flv", "f4v", "asf",
    "webm", "rm", "rmvb", "qt", "dv", "mpg", "mpeg", "mxf", "vob", "gif", "ogv", "ogm",
];

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "aac", "mka", "dts", "flac", "ogg", "oga", "mogg", "m4a", "ac3", "opus", "wav", "wv",
    "aiff", "aif", "ape", "tta", "tak",
];

const SINGLE_FILE_BLACKLIST_EXTENSIONS: &[&str] = &[
    "utf", "utf8", "utf-8", "idx", "sub", "srt", "smi", "rt", "ssa", "aqt", "jss", "js", "ass",
    "mks", "vtt", "sup", "scc", "m3u", "m3u8", "pls",
];

const SUBTITLE_EXTENSIONS: &[&str] = &[
    "utf", "utf8", "utf-8", "idx", "sub", "srt", "smi", "rt", "ssa", "aqt", "jss", "js", "ass",
    "mks", "vtt", "sup", "scc",
];

const LUT3D_EXTENSIONS: &[&str] = &["3dl", "cube", "dat", "m3d"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DroppedMediaPlan {
    Lut3d(String),
    Open(Vec<String>),
    Subtitles(Vec<String>),
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedPlaylistPath {
    pub index: usize,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaylistMove {
    pub from: usize,
    pub to: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistTargets {
    pub selected: Vec<IndexedPlaylistPath>,
    pub local: Vec<IndexedPlaylistPath>,
    pub network: Vec<IndexedPlaylistPath>,
}

/// The current file has to be loaded first so mpv keeps playing it. IINA then
/// inserts any naturally earlier siblings before that row with
/// `playlist-move`; these indexes describe the same post-load moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistAutoAddPlan {
    pub paths: Vec<String>,
    pub preceding_sibling_indexes: Vec<usize>,
}

impl PlaylistAutoAddPlan {
    fn unchanged(inputs: &[String]) -> Self {
        Self {
            paths: inputs.to_vec(),
            preceding_sibling_indexes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaylistInsertMove {
    pub from: usize,
    pub to: usize,
}

pub fn normalize_indexes(indexes: &[usize], playlist_len: usize) -> Vec<usize> {
    let mut indexes = indexes
        .iter()
        .copied()
        .filter(|index| *index < playlist_len)
        .collect::<Vec<_>>();
    indexes.sort_unstable();
    indexes.dedup();
    indexes
}

/// mpv removes against the playlist as it exists after each preceding remove.
/// This is the same offset rule as IINA's `playlistRemove(_ indexSet:)`.
pub fn removal_command_indexes(indexes: &[usize], playlist_len: usize) -> Vec<usize> {
    normalize_indexes(indexes, playlist_len)
        .into_iter()
        .enumerate()
        .map(|(removed_before, original_index)| original_index - removed_before)
        .collect()
}

/// IINA appends every new path first, then moves the appended rows into place.
/// Keeping the planner explicit makes Paste, Add File, Add URL, and external drops
/// share the same mpv command ordering.
pub fn insertion_moves(
    previous_playlist_len: usize,
    inserted_count: usize,
    destination: usize,
) -> Vec<PlaylistInsertMove> {
    if inserted_count == 0 || destination > previous_playlist_len {
        return Vec::new();
    }
    (0..inserted_count)
        .map(|offset| PlaylistInsertMove {
            from: previous_playlist_len + offset,
            to: destination + offset,
        })
        .collect()
}

/// Reproduces IINA 1.3.5's PlaylistViewController Play Next loop. The current
/// row remains part of the target selection, but does not emit a move command.
pub fn play_next_moves(
    indexes: &[usize],
    current_index: usize,
    playlist_len: usize,
) -> Vec<PlaylistMove> {
    if current_index >= playlist_len {
        return Vec::new();
    }

    let mut offset_before_current = 0isize;
    let mut moved_count = 1isize;
    let mut moves = Vec::new();
    for index in normalize_indexes(indexes, playlist_len) {
        if index == current_index {
            continue;
        }
        let (from, to) = if index < current_index {
            let from = index as isize + offset_before_current;
            let to = current_index as isize + moved_count + offset_before_current;
            offset_before_current -= 1;
            (from, to)
        } else {
            (
                index as isize,
                current_index as isize + moved_count + offset_before_current,
            )
        };
        if from >= 0 && to >= 0 {
            moves.push(PlaylistMove {
                from: from as usize,
                to: to as usize,
            });
        }
        moved_count += 1;
    }
    moves
}

pub fn targets(paths: &[String], indexes: &[usize]) -> PlaylistTargets {
    let selected = normalize_indexes(indexes, paths.len())
        .into_iter()
        .map(|index| IndexedPlaylistPath {
            index,
            path: paths[index].clone(),
        })
        .collect::<Vec<_>>();
    let (network, local) = selected
        .iter()
        .cloned()
        .partition(|item| is_network_resource(&item.path));
    PlaylistTargets {
        selected,
        local,
        network,
    }
}

pub fn is_network_resource(path: &str) -> bool {
    path.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && !scheme
                .chars()
                .any(|character| matches!(character, ':' | '/' | '?' | '#'))
    })
}

/// Plans IINA's startup-only `playlistAutoAdd` expansion without touching
/// player state. IINA groups video before audio and naturally sorts each group.
/// `open_media_batch` still receives the explicitly opened file first; the
/// returned move indexes put naturally earlier siblings in front afterward
/// without changing the current item.
///
/// The preference is intentionally supplied by the startup caller. Disabled
/// startup state, batches, URLs, non-media files, and unreadable directories
/// all preserve the original input verbatim.
pub fn plan_playlist_auto_add(inputs: &[String], enabled_at_startup: bool) -> PlaylistAutoAddPlan {
    plan_playlist_auto_add_with_reader(inputs, enabled_at_startup, |directory| {
        fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect()
    })
}

fn plan_playlist_auto_add_with_reader<F>(
    inputs: &[String],
    enabled_at_startup: bool,
    read_directory: F,
) -> PlaylistAutoAddPlan
where
    F: FnOnce(&Path) -> io::Result<Vec<PathBuf>>,
{
    if !enabled_at_startup || inputs.len() != 1 || is_network_resource(&inputs[0]) {
        return PlaylistAutoAddPlan::unchanged(inputs);
    }

    let current = PathBuf::from(&inputs[0]);
    if !current.is_file() || !extension_is_in(&current, PLAYABLE_DIRECTORY_EXTENSIONS) {
        return PlaylistAutoAddPlan::unchanged(inputs);
    }

    let parent = current
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let Ok(entries) = read_directory(parent) else {
        return PlaylistAutoAddPlan::unchanged(inputs);
    };
    let current_file_name = current.file_name();
    let mut videos = Vec::new();
    let mut audio = Vec::new();
    for path in entries.into_iter().filter(|path| {
        path.is_file()
            && !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'))
            && extension_is_in(path, PLAYABLE_DIRECTORY_EXTENSIONS)
    }) {
        if extension_is_in(&path, VIDEO_EXTENSIONS) {
            videos.push(path);
        } else if extension_is_in(&path, AUDIO_EXTENSIONS) {
            audio.push(path);
        }
    }
    videos.sort_by(|left, right| compare_filenames_naturally(left, right));
    audio.sort_by(|left, right| compare_filenames_naturally(left, right));
    let ordered = videos.into_iter().chain(audio).collect::<Vec<_>>();
    let Some(current_position) = ordered
        .iter()
        .position(|path| path.file_name() == current_file_name)
    else {
        return PlaylistAutoAddPlan::unchanged(inputs);
    };

    let mut planned = Vec::with_capacity(ordered.len());
    let mut seen = HashSet::<String>::new();
    planned.push(inputs[0].clone());
    seen.insert(inputs[0].clone());
    planned.extend(
        ordered
            .into_iter()
            .filter(|path| path.file_name() != current_file_name)
            .map(|path| path.to_string_lossy().into_owned())
            .filter(|path| seen.insert(path.clone())),
    );
    PlaylistAutoAddPlan {
        paths: planned,
        preceding_sibling_indexes: (1..=current_position).collect(),
    }
}

fn compare_filenames_naturally(left: &Path, right: &Path) -> Ordering {
    let left = left
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let right = right
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    natural_compare(&left, &right).then_with(|| left.cmp(&right))
}

/// A compact equivalent of Foundation's localized-standard filename ordering
/// for the numeric episode/file-name cases that matter to playlist matching.
fn natural_compare(left: &str, right: &str) -> Ordering {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let (mut left_index, mut right_index) = (0, 0);
    while left_index < left.len() && right_index < right.len() {
        if left[left_index].is_ascii_digit() && right[right_index].is_ascii_digit() {
            let left_end = left_index
                + left[left_index..]
                    .iter()
                    .take_while(|byte| byte.is_ascii_digit())
                    .count();
            let right_end = right_index
                + right[right_index..]
                    .iter()
                    .take_while(|byte| byte.is_ascii_digit())
                    .count();
            let left_number = &left[left_index..left_end];
            let right_number = &right[right_index..right_end];
            let left_significant = left_number
                .iter()
                .position(|byte| *byte != b'0')
                .map(|offset| &left_number[offset..])
                .unwrap_or(&left_number[left_number.len().saturating_sub(1)..]);
            let right_significant = right_number
                .iter()
                .position(|byte| *byte != b'0')
                .map(|offset| &right_number[offset..])
                .unwrap_or(&right_number[right_number.len().saturating_sub(1)..]);
            let numeric_order = left_significant
                .len()
                .cmp(&right_significant.len())
                .then_with(|| left_significant.cmp(right_significant));
            if numeric_order != Ordering::Equal {
                return numeric_order;
            }
            let padding_order = left_number.len().cmp(&right_number.len());
            if padding_order != Ordering::Equal {
                return padding_order;
            }
            left_index = left_end;
            right_index = right_end;
            continue;
        }
        let order = left[left_index].cmp(&right[right_index]);
        if order != Ordering::Equal {
            return order;
        }
        left_index += 1;
        right_index += 1;
    }
    left.len().cmp(&right.len())
}

/// Mirrors `PlayerCore.getPlayableFiles(in:)` from IINA 1.3.5.
///
/// Non-file URLs are accepted directly. Direct files reject subtitle and
/// multi-file-playlist extensions, while directories are recursively expanded
/// to IINA's video/audio extension set. Results are de-duplicated and sorted by
/// parent directory followed by a case-insensitive filename comparison.
pub fn resolve_playable_targets(inputs: &[String]) -> Vec<String> {
    let mut resolved = Vec::<PathBuf>::new();
    let mut urls = Vec::<String>::new();
    for raw in inputs {
        if is_network_resource(raw) {
            urls.push(raw.clone());
            continue;
        }
        let path = PathBuf::from(raw);
        if path.is_dir() {
            collect_playable_directory_files(&path, &path, &mut resolved);
        } else if !extension_is_in(&path, SINGLE_FILE_BLACKLIST_EXTENSIONS) {
            resolved.push(path);
        }
    }

    resolved.sort_by(|left, right| {
        let left_parent = left.parent().unwrap_or_else(|| Path::new(""));
        let right_parent = right.parent().unwrap_or_else(|| Path::new(""));
        left_parent
            .to_string_lossy()
            .cmp(&right_parent.to_string_lossy())
            .then_with(|| compare_playlist_filenames(left, right))
    });

    let mut seen = HashSet::<String>::new();
    resolved
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .chain(urls)
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

/// Mirrors IINA's `openFromPasteboard`: a single LUT wins, playable targets
/// win over subtitles, and subtitle-only drops attach to the current player.
pub fn plan_dropped_media(inputs: &[String]) -> DroppedMediaPlan {
    if inputs.len() == 1 {
        let path = PathBuf::from(&inputs[0]);
        if path.is_file() && extension_is_in(&path, LUT3D_EXTENSIONS) {
            return DroppedMediaPlan::Lut3d(inputs[0].clone());
        }
    }
    let playable = resolve_playable_targets(inputs);
    if !playable.is_empty() {
        return DroppedMediaPlan::Open(playable);
    }
    let subtitles = inputs
        .iter()
        .filter(|raw| {
            let path = Path::new(raw);
            path.is_file() && extension_is_in(path, SUBTITLE_EXTENSIONS)
        })
        .cloned()
        .collect::<Vec<_>>();
    if subtitles.is_empty() {
        DroppedMediaPlan::None
    } else {
        DroppedMediaPlan::Subtitles(subtitles)
    }
}

fn compare_playlist_filenames(left: &Path, right: &Path) -> Ordering {
    let left_name = left
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let right_name = right
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    natural_compare(&left_name, &right_name).then_with(|| left.as_os_str().cmp(right.as_os_str()))
}

fn collect_playable_directory_files(root: &Path, directory: &Path, output: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        if relative.components().any(|component| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(|name| name.starts_with('.'))
        }) {
            continue;
        }
        if path.is_dir() {
            collect_playable_directory_files(root, &path, output);
        } else if extension_is_in(&path, PLAYABLE_DIRECTORY_EXTENSIONS) {
            output.push(path);
        }
    }
}

fn extension_is_in(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extensions
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_remove_offsets_each_later_original_index() {
        assert_eq!(removal_command_indexes(&[1, 3], 5), vec![1, 2]);
        assert_eq!(removal_command_indexes(&[3, 1, 3, 99], 5), vec![1, 2]);
    }

    #[test]
    fn insertion_appends_then_moves_each_new_row_to_the_reference_destination() {
        assert_eq!(
            insertion_moves(3, 2, 1),
            vec![
                PlaylistInsertMove { from: 3, to: 1 },
                PlaylistInsertMove { from: 4, to: 2 },
            ]
        );
        assert!(insertion_moves(3, 2, 4).is_empty());
    }

    #[test]
    fn play_next_keeps_the_current_item_in_the_selection_contract() {
        assert_eq!(play_next_moves(&[1], 1, 5), Vec::<PlaylistMove>::new());
        assert_eq!(
            play_next_moves(&[0, 1, 3], 1, 5),
            vec![
                PlaylistMove { from: 0, to: 2 },
                PlaylistMove { from: 3, to: 2 },
            ]
        );
    }

    #[test]
    fn targets_partition_mixed_local_and_network_rows_in_playlist_order() {
        let paths = vec![
            "/tmp/one.mp4".to_string(),
            "https://example.com/watch".to_string(),
            "/tmp/two.mkv".to_string(),
            "rtsp://example.com/live".to_string(),
        ];
        let planned = targets(&paths, &[3, 0, 1, 99]);
        assert_eq!(
            planned
                .selected
                .iter()
                .map(|item| item.index)
                .collect::<Vec<_>>(),
            vec![0, 1, 3]
        );
        assert_eq!(
            planned
                .local
                .iter()
                .map(|item| item.index)
                .collect::<Vec<_>>(),
            vec![0]
        );
        assert_eq!(
            planned
                .network
                .iter()
                .map(|item| item.index)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
    }

    #[test]
    fn playable_resolver_recurses_filters_hidden_and_blacklisted_items() {
        let root = std::env::temp_dir().join(format!(
            "iima-playlist-resolver-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("Album/Sub")).unwrap();
        fs::create_dir_all(root.join(".Hidden")).unwrap();
        fs::write(root.join("Album/z.MP4"), b"").unwrap();
        fs::write(root.join("Album/a.mp3"), b"").unwrap();
        fs::write(root.join("Album/Episode 10.mp4"), b"").unwrap();
        fs::write(root.join("Album/Episode 2.mp4"), b"").unwrap();
        fs::write(root.join("Album/Sub/movie.mkv"), b"").unwrap();
        fs::write(root.join("Album/subtitle.srt"), b"").unwrap();
        fs::write(root.join(".Hidden/secret.mp4"), b"").unwrap();

        let resolved = resolve_playable_targets(&[
            root.clone().to_string_lossy().into_owned(),
            root.join("Album/subtitle.srt")
                .to_string_lossy()
                .into_owned(),
            root.join("list.m3u8").to_string_lossy().into_owned(),
            root.join("disc.cue").to_string_lossy().into_owned(),
            "https://example.com/video".into(),
        ]);
        assert_eq!(
            resolved,
            vec![
                root.join("disc.cue").to_string_lossy().into_owned(),
                root.join("Album/a.mp3").to_string_lossy().into_owned(),
                root.join("Album/Episode 2.mp4")
                    .to_string_lossy()
                    .into_owned(),
                root.join("Album/Episode 10.mp4")
                    .to_string_lossy()
                    .into_owned(),
                root.join("Album/z.MP4").to_string_lossy().into_owned(),
                root.join("Album/Sub/movie.mkv")
                    .to_string_lossy()
                    .into_owned(),
                "https://example.com/video".into(),
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dropped_media_plan_matches_lut_playable_then_subtitle_precedence() {
        let root = std::env::temp_dir().join(format!(
            "iima-drop-plan-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let movie = root.join("movie.mp4");
        let subtitle = root.join("movie.srt");
        let lut = root.join("grade.CUBE");
        fs::write(&movie, b"").unwrap();
        fs::write(&subtitle, b"").unwrap();
        fs::write(&lut, b"").unwrap();

        assert_eq!(
            plan_dropped_media(&[lut.to_string_lossy().into_owned()]),
            DroppedMediaPlan::Lut3d(lut.to_string_lossy().into_owned())
        );
        assert_eq!(
            plan_dropped_media(&[
                subtitle.to_string_lossy().into_owned(),
                movie.to_string_lossy().into_owned(),
            ]),
            DroppedMediaPlan::Open(vec![movie.to_string_lossy().into_owned()])
        );
        assert_eq!(
            plan_dropped_media(&[subtitle.to_string_lossy().into_owned()]),
            DroppedMediaPlan::Subtitles(vec![subtitle.to_string_lossy().into_owned()])
        );
        assert_eq!(
            plan_dropped_media(&[root.join("missing.srt").to_string_lossy().into_owned()]),
            DroppedMediaPlan::None
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn playlist_auto_add_keeps_current_playing_and_matches_iina_grouped_natural_order() {
        let root = std::env::temp_dir().join(format!(
            "iima-playlist-auto-add-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("Nested")).unwrap();
        let current = root.join("Episode 10.MP4");
        fs::write(&current, b"").unwrap();
        fs::write(root.join("Episode 2.mkv"), b"").unwrap();
        fs::write(root.join("Episode 12.mov"), b"").unwrap();
        fs::write(root.join("a.mp3"), b"").unwrap();
        fs::write(root.join("z.WAV"), b"").unwrap();
        fs::write(root.join(".hidden.mov"), b"").unwrap();
        fs::write(root.join("subtitle.srt"), b"").unwrap();
        fs::write(root.join("Nested/inside.mp4"), b"").unwrap();
        fs::create_dir(root.join("not-a-file.mp4")).unwrap();

        let current = current.to_string_lossy().into_owned();
        let plan = plan_playlist_auto_add(std::slice::from_ref(&current), true);
        assert_eq!(
            plan.paths,
            vec![
                current,
                root.join("Episode 2.mkv").to_string_lossy().into_owned(),
                root.join("Episode 12.mov").to_string_lossy().into_owned(),
                root.join("a.mp3").to_string_lossy().into_owned(),
                root.join("z.WAV").to_string_lossy().into_owned(),
            ]
        );
        assert_eq!(plan.preceding_sibling_indexes, vec![1]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn playlist_auto_add_preserves_inputs_outside_its_startup_contract() {
        let root = std::env::temp_dir().join(format!(
            "iima-playlist-auto-add-boundaries-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let current = root.join("current.mp4");
        let other = root.join("other.mkv");
        let non_media = root.join("notes.txt");
        fs::write(&current, b"").unwrap();
        fs::write(&other, b"").unwrap();
        fs::write(&non_media, b"").unwrap();

        let single = vec![current.to_string_lossy().into_owned()];
        assert_eq!(
            plan_playlist_auto_add(&single, false),
            PlaylistAutoAddPlan::unchanged(&single)
        );

        let multiple = vec![
            current.to_string_lossy().into_owned(),
            other.to_string_lossy().into_owned(),
        ];
        assert_eq!(
            plan_playlist_auto_add(&multiple, true),
            PlaylistAutoAddPlan::unchanged(&multiple)
        );

        let url = vec!["https://example.com/watch".to_string()];
        assert_eq!(
            plan_playlist_auto_add(&url, true),
            PlaylistAutoAddPlan::unchanged(&url)
        );

        let non_media = vec![non_media.to_string_lossy().into_owned()];
        assert_eq!(
            plan_playlist_auto_add(&non_media, true),
            PlaylistAutoAddPlan::unchanged(&non_media)
        );

        let missing = vec![root.join("missing.mp4").to_string_lossy().into_owned()];
        assert_eq!(
            plan_playlist_auto_add(&missing, true),
            PlaylistAutoAddPlan::unchanged(&missing)
        );

        let unreadable_result = plan_playlist_auto_add_with_reader(&single, true, |_| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "directory is unreadable",
            ))
        });
        assert_eq!(unreadable_result, PlaylistAutoAddPlan::unchanged(&single));

        fs::remove_dir_all(root).unwrap();
    }
}
