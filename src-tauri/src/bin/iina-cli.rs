use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::process::CommandExt;
use std::path::Component;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

const USAGE: &str = r#"Usage: iina-cli [arguments] [files] [-- mpv_option [...]]

Arguments:
    --mpv-*:
            All mpv options are supported here, except those starting with "--no-".
            Example: --mpv-volume=20 --mpv-resume-playback=no
    --separate-windows | -w:
            Open all files in separate windows.
    --stdin, --no-stdin:
            You may also pipe to stdin directly. Sometimes iina-cli can detect whether
            stdin has file, but sometimes not. Therefore it's recommended to always
            supply --stdin when piping to iina, and --no-stdin when you are not intend
            to use stdin.
    --keep-running:
            Normally iina-cli launches IINA and quits immediately. Supply this option
            if you would like to keep it running until the main application exits.
    --music-mode:
            Enter music mode after opening the media.
    --pip:
            Enter Picture-in-Picture after opening the media. Music mode does not
            support Picture-in-Picture.
    --help | -h:
            Print this message.

mpv Option:
    Raw mpv options without --mpv- prefix. All mpv options are supported here.
    Example: --volume=20 --no-resume-playback
"#;

#[derive(Debug, PartialEq, Eq)]
struct CliRequest {
    targets: Vec<String>,
    mpv_options: Vec<(String, String)>,
    forwarded_arguments: Vec<String>,
    separate_windows: bool,
    keep_running: bool,
    music_mode: bool,
    pip: bool,
    stdin: bool,
}

enum ParseResult {
    Request(CliRequest),
    Help,
    Error(String),
}

fn main() -> ExitCode {
    match parse_args(env::args().skip(1).collect()) {
        ParseResult::Help => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        ParseResult::Error(message) => {
            println!("{message}");
            ExitCode::from(64)
        }
        ParseResult::Request(request) => launch(request),
    }
}

fn parse_args(args: Vec<String>) -> ParseResult {
    parse_args_with_detected_stdin(args, stdin_is_piped())
}

fn parse_args_with_detected_stdin(mut args: Vec<String>, detected_stdin: bool) -> ParseResult {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        return ParseResult::Help;
    }
    if args.iter().any(|arg| arg == "--music-mode") && args.iter().any(|arg| arg == "--pip") {
        return ParseResult::Error("Cannot specify both --music-mode and --pip".to_string());
    }

    let mut request = CliRequest {
        targets: Vec::new(),
        mpv_options: Vec::new(),
        forwarded_arguments: Vec::new(),
        separate_windows: false,
        keep_running: false,
        music_mode: false,
        pip: false,
        stdin: detected_stdin,
    };

    // Match the companion in IINA 1.3.5: only explicit stdin switches before the raw-mpv
    // delimiter override auto-detection. The switches themselves are still forwarded.
    let mut user_specified_stdin = false;
    for argument in &args {
        match argument.as_str() {
            "--" => break,
            "--stdin" => {
                request.stdin = true;
                user_specified_stdin = true;
            }
            "--no-stdin" => {
                request.stdin = false;
                user_specified_stdin = true;
            }
            _ => {}
        }
    }

    // IINA removes the delimiter and rewrites every following long option for its main
    // executable. Non-option values are left alone and continue through normal path handling.
    if let Some(raw_index) = args.iter().position(|argument| argument == "--") {
        args.remove(raw_index);
        for argument in &mut args[raw_index..] {
            if let Some(name) = argument.strip_prefix("--no-") {
                *argument = format!("--mpv-{name}=no");
            } else if let Some(name) = argument.strip_prefix("--") {
                *argument = format!("--mpv-{name}");
            }
        }
    }

    for argument in args {
        let forwarded = if argument == "-w" {
            "--separate-windows".to_string()
        } else if !argument.starts_with('-') {
            normalize_target(&argument)
        } else {
            argument
        };

        match forwarded.as_str() {
            "--separate-windows" => request.separate_windows = true,
            "--keep-running" => request.keep_running = true,
            "--music-mode" => request.music_mode = true,
            "--pip" => request.pip = true,
            _ => {}
        }
        if let Some(option) = forwarded.strip_prefix("--mpv-").and_then(parsed_mpv_option) {
            request.mpv_options.push(option);
        }
        if !forwarded.starts_with('-') {
            request.targets.push(forwarded.clone());
        }
        request.forwarded_arguments.push(forwarded);
    }

    if request.stdin && !user_specified_stdin {
        request.forwarded_arguments.insert(0, "--stdin".to_string());
    }
    ParseResult::Request(request)
}

fn stdin_is_piped() -> bool {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return false;
    }
    if let Ok(metadata) = fs::metadata("/dev/stdin") {
        let file_type = metadata.file_type();
        if file_type.is_char_device() {
            return false;
        }
        if file_type.is_file() {
            return metadata.len() > 0;
        }
    }

    #[repr(C)]
    struct PollFd {
        fd: i32,
        events: i16,
        revents: i16,
    }
    unsafe extern "C" {
        fn poll(fds: *mut PollFd, count: u32, timeout_milliseconds: i32) -> i32;
    }
    const POLLIN: i16 = 0x0001;
    let mut descriptor = PollFd {
        fd: stdin.as_raw_fd(),
        events: POLLIN,
        revents: 0,
    };
    let ready = unsafe { poll(&mut descriptor, 1, 0) };
    ready > 0 && descriptor.revents & POLLIN != 0
}

fn parsed_mpv_option(value: &str) -> Option<(String, String)> {
    let (name, value) = value.split_once('=').unwrap_or((value, "yes"));
    (!name.is_empty()).then(|| (name.to_string(), value.to_string()))
}

fn normalize_target(target: &str) -> String {
    normalize_target_from_directory(target, &env::current_dir().unwrap_or_default())
}

fn normalize_target_from_directory(target: &str, current_directory: &Path) -> String {
    if reference_url_matches(target) {
        return target.to_string();
    }
    let path = Path::new(target);
    let absolute_path = if path.is_absolute() {
        PathBuf::from(path)
    } else {
        current_directory.join(path)
    };
    if absolute_path.exists() {
        // URL(fileURLWithPath:) in the reference makes a relative media path absolute but does
        // not resolve its symlink. Keeping the link spelling is observable in mpv, recent files,
        // watch-later identity, and title/path APIs.
        return lexically_normalize_absolute_path(&absolute_path)
            .to_string_lossy()
            .into_owned();
    }
    target.to_string()
}

fn reference_url_matches(value: &str) -> bool {
    let Some(colon) = value.find(':') else {
        return false;
    };
    colon > 0
        && !value[..colon]
            .chars()
            .any(|character| matches!(character, ':' | '/' | '?' | '#'))
}

fn lexically_normalize_absolute_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(value) => normalized.push(value),
        }
    }
    normalized
}

fn launch(request: CliRequest) -> ExitCode {
    let executable = match app_executable_path() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("iina-cli: {error}");
            return ExitCode::from(1);
        }
    };
    let mut command = Command::new(executable);
    command.args(application_arguments(&request));
    if request.stdin {
        command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
    } else {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    }
    if request.stdin || request.keep_running {
        let error = command.exec();
        eprintln!("iina-cli: failed to start IINA: {error}");
        return ExitCode::from(1);
    }
    match command.spawn() {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("iina-cli: failed to start IINA: {error}");
            ExitCode::from(1)
        }
    }
}

fn app_executable_path() -> Result<PathBuf, String> {
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let macos = executable
        .parent()
        .ok_or_else(|| "cannot determine the IINA.app bundle path".to_string())?;
    let app = macos.join("iima");
    if app.is_file() {
        Ok(app)
    } else {
        Err("this command line tool only works inside IINA.app".to_string())
    }
}

fn application_arguments(request: &CliRequest) -> Vec<String> {
    request.forwarded_arguments.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iina_cli_media_and_mpv_arguments() {
        let ParseResult::Request(request) = parse_args_with_detected_stdin(
            vec![
                "video.mkv".to_string(),
                "--mpv-volume=20".to_string(),
                "--".to_string(),
                "--no-resume-playback".to_string(),
            ],
            false,
        ) else {
            panic!("expected request");
        };
        assert_eq!(request.targets, vec!["video.mkv"]);
        assert_eq!(
            request.mpv_options,
            vec![
                ("volume".to_string(), "20".to_string()),
                ("resume-playback".to_string(), "no".to_string()),
            ]
        );
        assert_eq!(
            application_arguments(&request),
            vec!["video.mkv", "--mpv-volume=20", "--mpv-resume-playback=no",]
        );
    }

    #[test]
    fn rejects_incompatible_music_mode_and_pip() {
        let ParseResult::Error(message) = parse_args_with_detected_stdin(
            vec![
                "video.mkv".to_string(),
                "--music-mode".to_string(),
                "--pip".to_string(),
            ],
            false,
        ) else {
            panic!("expected usage error");
        };
        assert_eq!(message, "Cannot specify both --music-mode and --pip");
    }

    #[test]
    fn emits_main_executable_arguments_with_playback_options() {
        let request = CliRequest {
            targets: vec!["movie file.mp4".to_string()],
            mpv_options: vec![("volume".to_string(), "20".to_string())],
            forwarded_arguments: vec![
                "--music-mode".to_string(),
                "--mpv-volume=20".to_string(),
                "movie file.mp4".to_string(),
            ],
            separate_windows: false,
            keep_running: false,
            music_mode: true,
            pip: false,
            stdin: false,
        };
        assert_eq!(
            application_arguments(&request),
            vec!["--music-mode", "--mpv-volume=20", "movie file.mp4"]
        );
    }

    #[test]
    fn stdin_detection_and_explicit_overrides_match_iina_cli() {
        let ParseResult::Request(auto) = parse_args_with_detected_stdin(Vec::new(), true) else {
            panic!("expected request");
        };
        assert!(auto.stdin);

        let ParseResult::Request(disabled) =
            parse_args_with_detected_stdin(vec!["--no-stdin".to_string()], true)
        else {
            panic!("expected request");
        };
        assert!(!disabled.stdin);

        let ParseResult::Request(enabled) =
            parse_args_with_detected_stdin(vec!["--stdin".to_string()], false)
        else {
            panic!("expected request");
        };
        assert!(enabled.stdin);
        assert_eq!(application_arguments(&enabled), vec!["--stdin"]);
    }

    #[test]
    fn forwards_unknown_dash_arguments_in_reference_order() {
        let ParseResult::Request(request) = parse_args_with_detected_stdin(
            vec![
                "--future-iina-option=value".to_string(),
                "-x".to_string(),
                "short-option-value".to_string(),
                "movie.mkv".to_string(),
                "--keep-running".to_string(),
            ],
            false,
        ) else {
            panic!("expected request");
        };

        assert!(request.keep_running);
        assert_eq!(
            application_arguments(&request),
            vec![
                "--future-iina-option=value",
                "-x",
                "short-option-value",
                "movie.mkv",
                "--keep-running",
            ]
        );
    }

    #[test]
    fn existing_media_symlink_is_absolutized_without_resolving_it() {
        use std::os::unix::fs::symlink;

        let unique = format!(
            "iima-cli-symlink-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        );
        let directory = env::temp_dir().join(unique);
        fs::create_dir_all(&directory).expect("create test directory");
        let media = directory.join("media.mp4");
        fs::write(&media, b"test media marker").expect("create test media");
        let link = directory.join("media-link.mp4");
        symlink(&media, &link).expect("create media symlink");
        fs::create_dir(directory.join("child")).expect("create child directory");
        let url_shaped_file = directory.join("custom:media");
        fs::write(&url_shaped_file, b"still treated as a URL by IINA")
            .expect("create URL-shaped file");

        let normalized = normalize_target_from_directory("./child/../media-link.mp4", &directory);
        assert_eq!(normalized, link.to_string_lossy());
        assert_ne!(normalized, media.to_string_lossy());
        assert_eq!(
            normalize_target_from_directory("custom:media", &directory),
            "custom:media"
        );

        fs::remove_dir_all(&directory).expect("remove test directory");
    }
}
