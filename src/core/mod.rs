use crate::managers::{ManagerStats, PackageManager, pacman::FetchPacmanStats};

pub fn get_manager_stats() -> ManagerStats {
    let backend = FetchPacmanStats;
    backend.get_stats()
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
