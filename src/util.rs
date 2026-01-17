use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

/// Convert seconds to a human-readable duration string
pub fn normalize_duration(seconds: i64) -> String {
    if seconds < 60 {
        return format!("{} second{}", seconds, if seconds != 1 { "s" } else { "" });
    }

    if seconds < 3600 {
        let minutes = seconds / 60;
        return format!("{} minute{}", minutes, if minutes != 1 { "s" } else { "" });
    }

    if seconds < 86400 {
        let hours = seconds / 3600;
        return format!("{} hour{}", hours, if hours != 1 { "s" } else { "" });
    }

    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;

    format!(
        "{} day{} {} hour{}",
        days,
        if days != 1 { "s" } else { "" },
        hours,
        if hours != 1 { "s" } else { "" }
    )
}

/// Create a spinner with the given message
pub fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

/// Strip ANSI escape codes from a string
pub fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c == 'm' {
                in_escape = false;
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Check if running as root
pub fn is_root() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}
