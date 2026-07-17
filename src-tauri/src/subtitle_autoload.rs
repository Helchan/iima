use std::cmp::Reverse;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::playlist_actions;
use crate::preferences::PreferenceStore;

const SUBTITLE_EXTENSIONS: &[&str] = &[
    "utf", "utf8", "utf-8", "idx", "sub", "srt", "smi", "rt", "ssa", "aqt", "jss", "js", "ass",
    "mks", "vtt", "sup", "scc",
];

const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "m4v", "mov", "3gp", "ts", "mts", "m2ts", "wmv", "flv", "f4v", "asf",
    "webm", "rm", "rmvb", "qt", "dv", "mpg", "mpeg", "mxf", "vob", "gif", "ogv", "ogm",
];

const MAX_SEARCH_DIRECTORIES: usize = 256;
const MAX_DIRECTORY_ENTRIES: usize = 8_192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoLoadMode {
    Disabled,
    FilenameContains,
    Intelligent,
}

impl AutoLoadMode {
    fn from_preferences(preferences: &PreferenceStore) -> Self {
        match preferences
            .values
            .get("subAutoLoadIINA")
            .and_then(Value::as_i64)
            .unwrap_or(2)
        {
            0 => Self::Disabled,
            1 => Self::FilenameContains,
            _ => Self::Intelligent,
        }
    }
}

#[derive(Debug, Clone)]
struct FileInfo {
    path: PathBuf,
    stem: String,
    characters: Vec<char>,
    prefix: String,
    suffix: String,
    name_in_series: Option<String>,
}

impl FileInfo {
    fn new(path: PathBuf) -> Option<Self> {
        let stem = path.file_stem()?.to_str()?.to_string();
        Some(Self {
            path,
            characters: stem.chars().collect(),
            suffix: stem.clone(),
            stem,
            prefix: String::new(),
            name_in_series: None,
        })
    }

    fn set_prefix(&mut self, prefix: &str) {
        let prefix_count = prefix.chars().count();
        if prefix_count >= self.characters.len() {
            self.prefix.clear();
            self.suffix = self.stem.clone();
            self.name_in_series = None;
            return;
        }
        self.prefix = prefix.to_string();
        self.suffix = self.characters[prefix_count..].iter().collect();
        let mut saw_digit = false;
        let name = self
            .suffix
            .chars()
            .take_while(|character| {
                if character.is_ascii_digit() {
                    saw_digit = true;
                    true
                } else {
                    !saw_digit
                }
            })
            .collect::<String>();
        self.name_in_series = (!name.is_empty()).then_some(name);
    }
}

fn string_preference<'a>(
    preferences: &'a PreferenceStore,
    key: &str,
    fallback: &'a str,
) -> &'a str {
    preferences
        .values
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.contains('\0'))
        .unwrap_or(fallback)
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

fn visible_file(path: &Path) -> bool {
    path.is_file()
        && !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with('.'))
}

fn read_visible_files(directory: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .take(MAX_DIRECTORY_ENTRIES)
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| visible_file(path) && extension_is_in(path, extensions))
        .collect()
}

fn expand_tilde(path: &str, home: Option<&Path>) -> Option<PathBuf> {
    if path == "~" {
        return home.map(Path::to_path_buf);
    }
    if let Some(suffix) = path.strip_prefix("~/") {
        return home.map(|home| home.join(suffix));
    }
    Some(PathBuf::from(path))
}

fn search_directories(parent: &Path, raw_search_path: &str, home: Option<&Path>) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    for raw in raw_search_path
        .split(':')
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        let wildcard = raw.ends_with("/*");
        let raw_base = if wildcard {
            raw.strip_suffix("/*").unwrap_or(raw)
        } else {
            raw.trim_end_matches('/')
        };
        let Some(mut base) = expand_tilde(raw_base, home) else {
            continue;
        };
        if !base.is_absolute() {
            base = parent.join(base);
        }
        if wildcard {
            let Ok(entries) = fs::read_dir(&base) else {
                continue;
            };
            directories.extend(
                entries
                    .take(MAX_DIRECTORY_ENTRIES)
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.is_dir()
                            && !path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .is_some_and(|name| name.starts_with('.'))
                    }),
            );
        } else {
            directories.push(base);
        }
        if directories.len() >= MAX_SEARCH_DIRECTORIES {
            directories.truncate(MAX_SEARCH_DIRECTORIES);
            break;
        }
    }
    directories
}

fn canonical_deduplicated(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter_map(|path| fs::canonicalize(path).ok())
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

fn stop_grouping(branch_characters: &[Option<char>]) -> bool {
    if branch_characters
        .iter()
        .flatten()
        .any(|character| character.is_ascii_digit())
    {
        return true;
    }
    const CHINESE_NUMBERS: &[char] = &[
        '零', '一', '二', '三', '四', '五', '六', '七', '八', '九', '十',
    ];
    branch_characters
        .iter()
        .flatten()
        .filter(|character| CHINESE_NUMBERS.contains(character))
        .count()
        >= 3
}

fn assign_group_prefixes(files: &mut [FileInfo]) {
    let indexes = (0..files.len()).collect::<Vec<_>>();
    assign_group_prefixes_recursive(files, &indexes, String::new());
}

fn assign_group_prefixes_recursive(files: &mut [FileInfo], indexes: &[usize], mut prefix: String) {
    if indexes.len() < 3 {
        for index in indexes {
            files[*index].set_prefix(&prefix);
        }
        return;
    }

    let mut position = prefix.chars().count();
    loop {
        let mut groups = BTreeMap::<String, Vec<usize>>::new();
        let mut branch_characters = Vec::<Option<char>>::new();
        let mut any_processed = false;
        let mut last_prefix = prefix.clone();
        for index in indexes {
            let character = files[*index].characters.get(position).copied();
            let group_prefix = match character {
                Some(character) => {
                    any_processed = true;
                    let mut group_prefix = prefix.clone();
                    group_prefix.push(character);
                    last_prefix = group_prefix.clone();
                    group_prefix
                }
                None => prefix.clone(),
            };
            if !groups.contains_key(&group_prefix) {
                branch_characters.push(character);
            }
            groups.entry(group_prefix).or_default().push(*index);
        }

        if !any_processed {
            for index in indexes {
                files[*index].set_prefix(&prefix);
            }
            return;
        }
        if groups.len() == 1 {
            prefix = last_prefix;
            position += 1;
            continue;
        }

        let largest_subgroup = groups.values().map(Vec::len).max().unwrap_or_default();
        if stop_grouping(&branch_characters) || largest_subgroup < 3 {
            for index in indexes {
                files[*index].set_prefix(&prefix);
            }
            return;
        }
        for (group_prefix, group_indexes) in groups {
            assign_group_prefixes_recursive(files, &group_indexes, group_prefix);
        }
        return;
    }
}

fn levenshtein(left: &str, right: &str) -> usize {
    let right = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    for (left_index, left_character) in left.chars().enumerate() {
        let mut current = vec![left_index + 1];
        for (right_index, right_character) in right.iter().enumerate() {
            current.push(
                (previous[right_index + 1] + 1)
                    .min(current[right_index] + 1)
                    .min(previous[right_index] + usize::from(left_character != *right_character)),
            );
        }
        previous = current;
    }
    previous[right.len()]
}

fn series_names_match(left: Option<&str>, right: Option<&str>) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };
    match (left.parse::<u64>(), right.parse::<u64>()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn matched_series_prefixes(
    videos: &[FileInfo],
    subtitles: &[FileInfo],
) -> BTreeMap<String, String> {
    let mut video_groups = BTreeMap::<String, Vec<usize>>::new();
    let mut subtitle_groups = BTreeMap::<String, Vec<usize>>::new();
    for (index, video) in videos.iter().enumerate() {
        video_groups
            .entry(video.prefix.clone())
            .or_default()
            .push(index);
    }
    for (index, subtitle) in subtitles.iter().enumerate() {
        subtitle_groups
            .entry(subtitle.prefix.clone())
            .or_default()
            .push(index);
    }

    let mut closest_video_for_subtitle = BTreeMap::<String, String>::new();
    for subtitle_prefix in subtitle_groups.keys() {
        let closest = video_groups
            .iter()
            .filter(|(_, videos)| videos.len() > 2)
            .min_by_key(|(video_prefix, _)| levenshtein(video_prefix, subtitle_prefix))
            .map(|(video_prefix, _)| video_prefix.clone())
            .unwrap_or_default();
        closest_video_for_subtitle.insert(subtitle_prefix.clone(), closest);
    }

    let mut matched = BTreeMap::new();
    for (video_prefix, group) in &video_groups {
        if group.len() <= 2 {
            continue;
        }
        let Some((subtitle_prefix, distance)) = subtitle_groups
            .keys()
            .map(|subtitle_prefix| (subtitle_prefix, levenshtein(video_prefix, subtitle_prefix)))
            .min_by_key(|(_, distance)| *distance)
        else {
            continue;
        };
        let threshold = ((video_prefix.chars().count() + subtitle_prefix.chars().count()) as f64
            * 0.6) as usize;
        if closest_video_for_subtitle.get(subtitle_prefix) == Some(video_prefix)
            && distance < threshold
        {
            matched.insert(video_prefix.clone(), subtitle_prefix.clone());
        }
    }
    matched
}

fn containing_matches(video: &FileInfo, subtitles: &[FileInfo]) -> Vec<usize> {
    subtitles
        .iter()
        .enumerate()
        .filter_map(|(index, subtitle)| subtitle.stem.contains(&video.stem).then_some(index))
        .collect()
}

fn intelligent_matches(videos: &[FileInfo], subtitles: &[FileInfo]) -> Vec<Vec<usize>> {
    let matched_prefixes = matched_series_prefixes(videos, subtitles);
    let mut matched = vec![Vec::<usize>::new(); videos.len()];
    let mut claimed_subtitles = HashSet::<usize>::new();

    for (video_index, video) in videos.iter().enumerate() {
        if !video.prefix.is_empty() {
            if let Some(subtitle_prefix) = matched_prefixes.get(&video.prefix) {
                for (subtitle_index, subtitle) in subtitles.iter().enumerate() {
                    if subtitle.prefix == *subtitle_prefix
                        && series_names_match(
                            video.name_in_series.as_deref(),
                            subtitle.name_in_series.as_deref(),
                        )
                    {
                        matched[video_index].push(subtitle_index);
                        claimed_subtitles.insert(subtitle_index);
                    }
                }
            }
        }
        for subtitle_index in containing_matches(video, subtitles) {
            if claimed_subtitles.insert(subtitle_index) {
                matched[video_index].push(subtitle_index);
            }
        }
    }
    matched
}

fn force_match_unmatched(videos: &[FileInfo], subtitles: &[FileInfo], matched: &mut [Vec<usize>]) {
    let unmatched_videos = matched
        .iter()
        .enumerate()
        .filter_map(|(index, subtitles)| subtitles.is_empty().then_some(index))
        .collect::<Vec<_>>();
    let claimed = matched.iter().flatten().copied().collect::<HashSet<_>>();
    let unmatched_subtitles = (0..subtitles.len())
        .filter(|index| !claimed.contains(index))
        .collect::<Vec<_>>();
    if unmatched_videos
        .len()
        .saturating_mul(unmatched_subtitles.len())
        >= 10_000
    {
        return;
    }

    let mut distances = BTreeMap::<(usize, usize), usize>::new();
    for subtitle_index in &unmatched_subtitles {
        for video_index in &unmatched_videos {
            let video = &videos[*video_index];
            let subtitle = &subtitles[*subtitle_index];
            let distance = levenshtein(&video.prefix, &subtitle.prefix)
                + levenshtein(&video.suffix, &subtitle.suffix);
            let threshold = ((video.stem.chars().count() + subtitle.stem.chars().count()) as f64
                * 0.6) as usize;
            if distance < threshold {
                distances.insert((*video_index, *subtitle_index), distance);
            }
        }
    }
    for video_index in unmatched_videos {
        let Some(video_minimum) = unmatched_subtitles
            .iter()
            .filter_map(|subtitle_index| distances.get(&(video_index, *subtitle_index)))
            .min()
            .copied()
        else {
            continue;
        };
        for subtitle_index in &unmatched_subtitles {
            if distances.get(&(video_index, *subtitle_index)) != Some(&video_minimum) {
                continue;
            }
            let subtitle_minimum = matched
                .iter()
                .enumerate()
                .filter(|(_, matches)| matches.is_empty())
                .filter_map(|(candidate_video, _)| {
                    distances.get(&(candidate_video, *subtitle_index))
                })
                .min()
                .copied();
            if subtitle_minimum == Some(video_minimum) {
                matched[video_index].push(*subtitle_index);
            }
        }
    }
}

fn occurrence_count(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    haystack.match_indices(needle).count()
}

fn prioritize(matches: &mut [usize], subtitles: &[FileInfo], priorities: &[String]) {
    let original_positions = matches
        .iter()
        .enumerate()
        .map(|(position, index)| (*index, position))
        .collect::<BTreeMap<_, _>>();
    matches.sort_by_key(|index| {
        let occurrences = priorities
            .iter()
            .map(|priority| occurrence_count(&subtitles[*index].stem, priority))
            .sum::<usize>();
        (Reverse(occurrences), original_positions[index])
    });
}

/// Plans IINA 1.3.5 local subtitle auto-loading for a single explicit media open.
///
/// The planner is intentionally independent from `PlayerState`: directory scanning and matching
/// finish before the player lock is acquired, then callers append the returned `sub-add` commands
/// through the existing external-track path. Multi-file opens and network resources retain IINA's
/// behavior of skipping the folder matcher.
pub fn plan(
    inputs: &[String],
    preferences: &PreferenceStore,
    playlist_auto_add_enabled: bool,
    home: Option<&Path>,
) -> Vec<String> {
    let mode = AutoLoadMode::from_preferences(preferences);
    if mode == AutoLoadMode::Disabled || inputs.len() != 1 {
        return Vec::new();
    }
    let raw_media_path = &inputs[0];
    if playlist_actions::is_network_resource(raw_media_path) {
        return Vec::new();
    }
    let Ok(media_path) = fs::canonicalize(raw_media_path) else {
        return Vec::new();
    };
    if !media_path.is_file() || !extension_is_in(&media_path, VIDEO_EXTENSIONS) {
        return Vec::new();
    }
    let Some(parent) = media_path.parent() else {
        return Vec::new();
    };

    let mut candidates = read_visible_files(parent, SUBTITLE_EXTENSIONS);
    let search_path = string_preference(preferences, "subAutoLoadSearchPath", "./*");
    for directory in search_directories(parent, search_path, home) {
        candidates.extend(read_visible_files(&directory, SUBTITLE_EXTENSIONS));
    }
    let mut subtitles = canonical_deduplicated(candidates)
        .into_iter()
        .filter_map(FileInfo::new)
        .collect::<Vec<_>>();
    if subtitles.is_empty() {
        return Vec::new();
    }
    subtitles.sort_by(|left, right| {
        left.stem
            .to_lowercase()
            .cmp(&right.stem.to_lowercase())
            .then_with(|| left.path.cmp(&right.path))
    });

    let mut videos = read_visible_files(parent, VIDEO_EXTENSIONS)
        .into_iter()
        .filter_map(FileInfo::new)
        .collect::<Vec<_>>();
    if videos.iter().all(|video| video.path != media_path) {
        let Some(media) = FileInfo::new(media_path.clone()) else {
            return Vec::new();
        };
        videos.push(media);
    }
    assign_group_prefixes(&mut videos);
    assign_group_prefixes(&mut subtitles);
    let Some(current_video_index) = videos.iter().position(|video| video.path == media_path) else {
        return Vec::new();
    };

    let mut matches = match mode {
        AutoLoadMode::Disabled => Vec::new(),
        AutoLoadMode::FilenameContains => {
            containing_matches(&videos[current_video_index], &subtitles)
        }
        AutoLoadMode::Intelligent => {
            let mut matches = intelligent_matches(&videos, &subtitles);
            if playlist_auto_add_enabled {
                force_match_unmatched(&videos, &subtitles, &mut matches);
            }
            std::mem::take(&mut matches[current_video_index])
        }
    };
    matches.sort_unstable();
    matches.dedup();
    let priorities = string_preference(preferences, "subAutoLoadPriorityString", "")
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    prioritize(&mut matches, &subtitles, &priorities);
    matches
        .into_iter()
        .map(|index| subtitles[index].path.to_string_lossy().into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(name: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "iima-subtitle-autoload-{}-{name}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn file(&self, relative: &str) -> String {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::File::create(&path)
                .unwrap()
                .write_all(b"fixture")
                .unwrap();
            path.to_string_lossy().into_owned()
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn preferences(mode: i64, search_path: &str, priority: &str) -> PreferenceStore {
        let mut preferences = PreferenceStore::default();
        preferences
            .values
            .insert("subAutoLoadIINA".into(), json!(mode));
        preferences
            .values
            .insert("subAutoLoadSearchPath".into(), json!(search_path));
        preferences
            .values
            .insert("subAutoLoadPriorityString".into(), json!(priority));
        preferences
    }

    #[test]
    fn disabled_network_and_multi_file_opens_do_not_scan() {
        let directory = TestDirectory::new("disabled");
        let media = directory.file("Show 01.mkv");
        directory.file("Show 01.srt");
        assert!(plan(&[media.clone()], &preferences(0, "./*", ""), true, None).is_empty());
        assert!(plan(
            &["https://example.com/show.mkv".into()],
            &preferences(2, "./*", ""),
            true,
            None,
        )
        .is_empty());
        assert!(plan(
            &[media.clone(), media],
            &preferences(2, "./*", ""),
            true,
            None,
        )
        .is_empty());
    }

    #[test]
    fn containing_mode_expands_wildcard_directories_and_applies_priority_strings() {
        let directory = TestDirectory::new("containing");
        let media = directory.file("Movie.mkv");
        let normal = directory.file("Movie.en.srt");
        let forced = directory.file("Subs/Movie.forced.en.ass");
        directory.file("Subs/Unrelated.srt");
        directory.file(".Hidden/Movie.hidden.srt");

        let result = plan(&[media], &preferences(1, "./*", "forced,en"), true, None);
        assert_eq!(
            result,
            vec![
                fs::canonicalize(forced).unwrap().to_string_lossy(),
                fs::canonicalize(normal).unwrap().to_string_lossy(),
            ]
        );
    }

    #[test]
    fn intelligent_mode_matches_mutual_series_prefixes() {
        let directory = TestDirectory::new("series");
        directory.file("[Video] Show 01.mkv");
        let current = directory.file("[Video] Show 02.mkv");
        directory.file("[Video] Show 03.mkv");
        directory.file("[Sub] Show 01.srt");
        let expected = directory.file("[Sub] Show 02.srt");
        directory.file("[Sub] Show 03.srt");

        assert!(plan(&[current.clone()], &preferences(1, "", ""), true, None,).is_empty());
        assert_eq!(
            plan(&[current], &preferences(2, "", ""), true, None),
            vec![fs::canonicalize(expected)
                .unwrap()
                .to_string_lossy()
                .into_owned()]
        );
    }

    #[test]
    fn explicit_relative_and_tilde_search_paths_follow_the_reference_contract() {
        let directory = TestDirectory::new("paths");
        let media = directory.file("Movie.mkv");
        let relative = directory.file("Captions/Movie.relative.srt");
        let home = directory.0.join("home");
        fs::create_dir_all(home.join("Subs")).unwrap();
        let home_subtitle = {
            let path = home.join("Subs/Movie.home.srt");
            fs::File::create(&path).unwrap();
            path
        };

        let result = plan(
            &[media],
            &preferences(1, "Captions:~/Subs", ""),
            true,
            Some(&home),
        );
        assert_eq!(
            result.into_iter().collect::<HashSet<_>>(),
            [
                fs::canonicalize(relative)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                fs::canonicalize(home_subtitle)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            ]
            .into_iter()
            .collect()
        );
    }
}
