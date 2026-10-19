use chrono::Local;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Set when SIGTERM/SIGINT arrives, so long-running loops can stop at a safe
/// boundary instead of dying mid-installation.
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The flag `signal_hook` flips on SIGTERM/SIGINT.
fn signal_flag() -> &'static Arc<AtomicBool> {
    static FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    FLAG.get_or_init(|| Arc::new(AtomicBool::new(false)))
}

/// Ask whether a shutdown signal has been received.
pub fn shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::Relaxed) || signal_flag().load(Ordering::Relaxed)
}

/// Install SIGTERM/SIGINT handlers.
///
/// `launchctl bootout`/`unload` (which `mac-setup install` itself runs at
/// `main.rs`) and a reboot both send SIGTERM. Without a handler the daemon dies
/// immediately, possibly partway through a cask installer that has already
/// replaced the old `.app`. Registering the flag lets the caller finish the
/// in-flight cask and exit cleanly.
pub fn install_signal_handlers() {
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        if let Err(e) = signal_hook::flag::register(sig, Arc::clone(signal_flag())) {
            eprintln!("[mac-setup] could not register handler for signal {sig}: {e}");
        }
    }
}

/// Return `PathBuf::from($HOME).join(suffix)`.
///
/// Centralises the four independent `format!("{}/{}", home, …)` calls scattered
/// across provision.rs, ops.rs, and cleanup.rs — each of which was a potential
/// typo (wrong separator, wrong subpath).
pub fn home_path(suffix: &str) -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(suffix)
}

/// Whether a command can be found on PATH or in `~/.local/bin`.
///
/// Moved from `provision.rs` so it is available to all modules without
/// duplication. `cleanup.rs`'s old `command_exists` only called `which`;
/// this version also probes `~/.local/bin` (where stout and mise install).
pub fn command_available(cmd: &str) -> bool {
    if Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return true;
    }
    // Also check ~/.local/bin (stout and mise install here)
    home_path(&format!(".local/bin/{}", cmd)).exists()
}

/// Resolve the stout binary, preferring `~/.local/bin/stout` (where provision
/// installs it on Apple Silicon) over whatever is on PATH.
///
/// The result is cached for the lifetime of the process so repeated calls
/// (e.g. 200+ package checks during provision) do not re-stat the path every
/// time.
pub fn stout_cmd() -> &'static str {
    static STOUT: OnceLock<String> = OnceLock::new();
    STOUT.get_or_init(|| {
        let local = home_path(".local/bin/stout");
        if local.exists() {
            local.to_string_lossy().into_owned()
        } else {
            "stout".to_string()
        }
    })
}

/// Resolve the mise binary, preferring `~/.local/bin/mise` over PATH.
///
/// Cached for the lifetime of the process.
pub fn mise_bin() -> &'static str {
    static MISE: OnceLock<String> = OnceLock::new();
    MISE.get_or_init(|| {
        let local = home_path(".local/bin/mise");
        if local.exists() {
            local.to_string_lossy().into_owned()
        } else {
            "mise".to_string()
        }
    })
}

pub struct Config {
    pub log_file: PathBuf,
    pub error_log: PathBuf,
    pub lock_file: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        // Deliberately not `.expect()`: this runs for every subcommand, so a missing
        // HOME (e.g. a malformed launchd EnvironmentVariables block) would panic the
        // daemon before it could write a single log line. Downstream guards already
        // cope with an empty HOME.
        let home = std::env::var("HOME").unwrap_or_default();
        let log_dir = PathBuf::from(&home).join("Library/Logs");
        fs::create_dir_all(&log_dir).unwrap_or(());

        Config {
            log_file: log_dir.join("mac-setup.log"),
            error_log: log_dir.join("mac-setup-error.log"),
            lock_file: PathBuf::from("/tmp/mac-setup.lock"),
        }
    }
}

#[derive(Clone, Copy)]
enum Sink {
    Primary,
    Errors,
}

/// Process-wide append handles for the log files.
///
/// Re-opening per line meant one `open`/`write`/`close` syscall triple for every log
/// line. A single update run emits thousands of lines (one per quarantined binary, per
/// cask, per cleanup target), so the handles are cached and buffered. If a log file
/// cannot be opened we fall back to /dev/null rather than losing every subsequent line.
fn log_sink(path: &Path, which: Sink) -> &'static Mutex<BufWriter<fs::File>> {
    static PRIMARY: OnceLock<Mutex<BufWriter<fs::File>>> = OnceLock::new();
    static ERRORS: OnceLock<Mutex<BufWriter<fs::File>>> = OnceLock::new();

    let cell = match which {
        Sink::Primary => &PRIMARY,
        Sink::Errors => &ERRORS,
    };

    cell.get_or_init(|| {
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .or_else(|_| fs::OpenOptions::new().write(true).open("/dev/null"))
            .expect("log sink must be openable");
        Mutex::new(BufWriter::new(file))
    })
}

pub fn log(msg: &str, config: &Config) {
    let line = format!("[{}] {}\n", Local::now().format("%Y-%m-%d %H:%M:%S"), msg);

    print!("{}", line);

    if let Ok(mut writer) = log_sink(&config.log_file, Sink::Primary).lock() {
        let _ = writer.write_all(line.as_bytes());
        let _ = writer.flush();
    }
}

pub fn log_error(msg: &str, config: &Config) {
    let line = format!(
        "[{}] ERROR: {}\n",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        msg
    );

    eprint!("{}", line);

    for (path, sink) in [
        (&config.log_file, Sink::Primary),
        (&config.error_log, Sink::Errors),
    ] {
        if let Ok(mut writer) = log_sink(path, sink).lock() {
            let _ = writer.write_all(line.as_bytes());
            let _ = writer.flush();
        }
    }
}

/// Escape a string for embedding in an AppleScript double-quoted literal.
///
/// The message used to be interpolated raw, so any `"` or `\` in a path or package
/// name would break the script.
fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn send_notification(status: &str, message: &str) {
    let _ = Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "display notification \"{}\" with title \"mac-setup\" subtitle \"{}\"",
            applescript_escape(message),
            applescript_escape(status)
        ))
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_does_not_panic_without_home() {
        // This is the daemon's first statement of work, so it must never panic.
        let config = Config::default();
        assert!(config.log_file.is_absolute());
    }

    #[test]
    fn applescript_escape_leaves_plain_text_alone() {
        assert_eq!(
            applescript_escape("Updated! Freed 400MB"),
            "Updated! Freed 400MB"
        );
    }

    #[test]
    fn applescript_escape_neutralises_quotes() {
        assert_eq!(applescript_escape("say \"hi\""), "say \\\"hi\\\"");
    }

    #[test]
    fn applescript_escape_escapes_backslash_before_quotes() {
        assert_eq!(applescript_escape(r"C:\x"), r"C:\\x");
    }

    #[test]
    fn applescript_escape_prevents_literal_termination() {
        let escaped = applescript_escape("\"); do shell script \"evil\"");
        // No quote may survive unescaped, or the literal could be closed early.
        let mut unescaped = 0usize;
        let mut prev_escape = false;
        for c in escaped.chars() {
            if c == '"' && !prev_escape {
                unescaped += 1;
            }
            prev_escape = c == '\\' && !prev_escape;
        }
        assert_eq!(unescaped, 0, "found unescaped quotes in {escaped:?}");
    }

    /// The update daemon fans stout/mise/pipx/bun across threads, and each of them
    /// calls `log()` concurrently on the same shared handle. Without the mutex, the
    /// buffered writer can interleave partial lines and corrupt the log.
    #[test]
    fn concurrent_logging_never_interleaves_lines() {
        let config = Config::default();

        // Only inspect bytes written by this test: the sink is process-wide and
        // every earlier test already appended to the same file.
        let start_offset = fs::metadata(&config.log_file).map(|m| m.len()).unwrap_or(0);

        let threads = 24;
        let per_thread = 40;
        let cfg = &config;

        std::thread::scope(|scope| {
            for t in 0..threads {
                scope.spawn(move || {
                    for i in 0..per_thread {
                        log(&format!("concurrency-marker t{t}-i{i:04}"), cfg);
                    }
                });
            }
        });

        let bytes = fs::read(&config.log_file).expect("log file should be readable");
        let fresh = &bytes[start_offset as usize..];
        let text = String::from_utf8_lossy(fresh);

        let expected = threads * per_thread;
        let mut found = 0usize;
        let mut malformed = Vec::new();

        for line in text.lines() {
            if line.is_empty() {
                continue;
            }
            // Every line must open with the timestamp prefix. A line that does not
            // means two writers interleaved mid-line.
            // "[YYYY-MM-DD HH:MM:SS] " is 22 bytes, so ']' sits at index 20.
            let is_well_formed = line.starts_with('[')
                && line.len() > 21
                && line.as_bytes()[20] == b']'
                && line[21..].starts_with(' ');
            if !is_well_formed {
                malformed.push(line.to_string());
                continue;
            }
            let body = &line[22..];
            if body.starts_with("concurrency-marker t") {
                found += 1;
            }
        }

        assert!(
            malformed.is_empty(),
            "{} line(s) were corrupted by interleaved writes, e.g. {:?}",
            malformed.len(),
            malformed.first()
        );
        assert_eq!(
            found, expected,
            "expected {expected} marker lines, found {found}"
        );
    }
}
