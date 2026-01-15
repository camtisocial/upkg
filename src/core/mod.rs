use crate::managers::{ManagerStats, MirrorHealth, PackageManager, pacman::FetchPacmanStats};
use indicatif::{ProgressBar, ProgressStyle};

pub fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

pub fn sync_databases() -> Result<(), String> {
    let backend = FetchPacmanStats;
    backend.sync_databases()
}

pub fn upgrade_system(text_mode: bool, speed_test: bool, sync_first: bool) -> Result<(), String> {
    let backend = FetchPacmanStats;
    backend.upgrade_system(text_mode, speed_test, sync_first)
}

// local queries + fast network (mirror URL, sync age)
pub fn get_manager_stats(debug: bool) -> ManagerStats {
    let backend = FetchPacmanStats;
    backend.get_stats(debug)
}

// slow network - speed test with progress reporting (0-100%)
pub fn test_mirror_speed_with_progress<F>(mirror_url: &str, progress_callback: F) -> Option<f64>
where
    F: Fn(u64),
{
    let backend = FetchPacmanStats;
    backend.test_mirror_speed_with_progress(mirror_url, progress_callback)
}

// convenience method for backward compatibility (plain mode)
pub fn test_mirror_health() -> Option<MirrorHealth> {
    let backend = FetchPacmanStats;
    backend.test_mirror_health()
}

/// Convert seconds since last update to a human-readable string
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
