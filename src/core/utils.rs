use chrono::Local;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

pub struct Config {
    pub log_file: PathBuf,
    pub error_log: PathBuf,
    pub lock_file: PathBuf,
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for Config {
    fn default() -> Self {
        let home = std::env::var("HOME").expect("HOME not set");
        let log_dir = PathBuf::from(&home).join("Library/Logs");
        fs::create_dir_all(&log_dir).unwrap_or(());

        Config {
            log_file: log_dir.join("mac-setup.log"),
            error_log: log_dir.join("mac-setup-error.log"),
            lock_file: PathBuf::from("/tmp/mac-setup.lock"),
        }
    }
}

pub fn log(msg: &str, config: &Config) {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let formatted = format!("[{}] {}\n", timestamp, msg);

    // Print to stdout
    print!("{}", formatted);

    // Append to file
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.log_file)
    {
        let _ = file.write_all(formatted.as_bytes());
    }
}

pub fn log_error(msg: &str, config: &Config) {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let formatted = format!("[{}] ERROR: {}\n", timestamp, msg);

    eprint!("{}", formatted);

    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.log_file)
    {
        let _ = file.write_all(formatted.as_bytes());
    }

    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.error_log)
    {
        let _ = file.write_all(formatted.as_bytes());
    }
}

pub fn send_notification(status: &str, message: &str) {
    let _ = Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "display notification \"{}\" with title \"mac-setup Update\" subtitle \"{}\"",
            message, status
        ))
        .output();
}
