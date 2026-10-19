use super::utils::{self, Config};
use std::process::Command;

pub fn check_network(config: &Config) -> bool {
    utils::log("Checking network connectivity...", config);

    // Try the primary target first, then fall back to a second DNS resolver so a
    // single blocked ICMP destination does not abort the run.
    // macOS `ping` uses `-t` for the timeout in seconds (`-W` is milliseconds
    // on some versions and seconds on others, making it version-dependent).
    let targets = ["8.8.8.8", "1.1.1.1"];
    for target in targets {
        let reachable = Command::new("ping")
            .args(["-c", "1", "-t", "2", target])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if reachable {
            utils::log(
                &format!("✓ Network is available (reached {})", target),
                config,
            );
            return true;
        }
    }

    utils::log_error("No network connectivity detected", config);
    utils::log(
        "  → Check your internet connection or set SKIP_NETWORK_CHECK=1 to bypass",
        config,
    );
    false
}

pub fn check_disk_space(config: &Config) -> bool {
    utils::log("Checking disk space...", config);

    let output = Command::new("df").args(["-g", "/"]).output();

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        // Parse the second line, 4th column (Available)
        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                if let Ok(available) = parts[3].parse::<i64>() {
                    // Read the env var once; warn if it was set to a non-integer.
                    let min_gb_raw =
                        std::env::var("MIN_DISK_SPACE_GB").unwrap_or_else(|_| "5".to_owned());
                    let min_gb = match min_gb_raw.parse::<i64>() {
                        Ok(v) => v,
                        Err(_) => {
                            // Only warn when the env var was explicitly set.
                            if std::env::var("MIN_DISK_SPACE_GB").is_ok() {
                                utils::log(
                                    &format!(
                                        "⚠ MIN_DISK_SPACE_GB='{}' is not a valid integer; defaulting to 5GB",
                                        min_gb_raw
                                    ),
                                    config,
                                );
                            }
                            5
                        }
                    };

                    if available < min_gb {
                        utils::log_error(
                            &format!(
                                "Insufficient disk space: {}GB available (minimum {}GB required)",
                                available, min_gb
                            ),
                            config,
                        );
                        return false;
                    } else {
                        utils::log(
                            &format!("✓ Sufficient disk space: {}GB available", available),
                            config,
                        );
                        return true;
                    }
                }
            }
        }
    }

    utils::log_error("Failed to check disk space", config);
    false
}
