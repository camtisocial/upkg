use crate::managers::{ManagerStats, PackageManager};
use chrono::{DateTime, FixedOffset, Local};
use std::fs;
use std::process::Command;

pub struct FetchPacmanStats;

impl FetchPacmanStats {
    /// Get the count of installed packages using pacman -Q
    fn get_installed_count(&self) -> u32 {
        let output = Command::new("pacman").arg("-Q").output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().count() as u32
    }

    /// Get the count of upgradable packages using checkupdates
    fn get_upgradable_count(&self) -> u32 {
        let output = Command::new("pacman").arg("-Qu").output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().count() as u32
    }

    /// get time since last update from /var/log/pacman.log
    /// returns seconds
    fn get_seconds_since_update(&self) -> Option<i64> {
        /*
        This is checking the log for the last time the user ran pacman -Syu, and packages
        actually installed thereafter, that way we are actually returning last time updated
        rathern than last time -Syu was run, ideally. kind of janky
        */

        let contents =
            fs::read_to_string("/var/log/pacman.log").expect("Failed to read pacman.log");

        let mut saw_syu = false;
        let mut saw_sync = false;
        let mut saw_upgrade_start = false;
        let mut saw_alpm = false;
        let mut block_timestamp: Option<String> = None;
        let mut last_valid_timestamp: Option<String> = None;

        for line in contents.lines() {
            let trimmed = line.trim();

            // Extract timestamp of any line
            // Format: [2025-12-05T15:43:51-0800] ...
            let timestamp = trimmed
                .split(']')
                .next()
                .map(|x| x.trim_start_matches('['))
                .unwrap_or("");

            // look for start of syu block, then alpm lines to make sure the update actually
            // started
            if trimmed.contains("Running 'pacman -Syu'") {
                // reset tracking
                saw_syu = true;
                saw_sync = false;
                saw_upgrade_start = false;
                saw_alpm = false;
                block_timestamp = Some(timestamp.to_string());
                continue;
            }

            if saw_syu && trimmed.contains("synchronizing package lists") {
                saw_sync = true;
                continue;
            }

            if saw_sync && trimmed.contains("starting full system upgrade") {
                saw_upgrade_start = true;
                continue;
            }

            if saw_upgrade_start && trimmed.contains("[ALPM]") {
                saw_alpm = true;
            }

            // If we start another pacman run before ALPM, the previous wasn't real
            if trimmed.contains("[PACMAN] Running") && !trimmed.contains("pacman -Syu") {
                // restart
                saw_syu = false;
                saw_sync = false;
                saw_upgrade_start = false;
                saw_alpm = false;
                block_timestamp = None;
                continue;
            }

            // if the block is complete, update last_valid_timestamp
            if saw_syu && saw_sync && saw_upgrade_start && saw_alpm {
                last_valid_timestamp = block_timestamp.clone();
            }
        }

        if let Some(ts) = last_valid_timestamp {
            // confert to RFC3339
            let formatted_date = format!("{}:{}", &ts[..22], &ts[22..]);

            let parsed: DateTime<FixedOffset> =
                DateTime::parse_from_rfc3339(&formatted_date).unwrap();

            let last_update_local = parsed.with_timezone(&Local);
            let now = Local::now();
            let duration = now.signed_duration_since(last_update_local);
            let seconds = duration.num_seconds().max(0);

            return Some(seconds);
        }

        None
    }

    fn get_upgrade_sizes(&self) -> (Option<f64>, Option<f64>) {
        return (Some(0.0), Some(0.0));
    }

    fn get_mirror_health(&self) -> Option<String> {
        Some("test".to_string())
    }
}

impl PackageManager for FetchPacmanStats {
    fn get_stats(&self) -> ManagerStats {
        let (download_size, net_upgrade_size) = self.get_upgrade_sizes();

        ManagerStats {
            total_installed: self.get_installed_count(),
            total_upgradable: self.get_upgradable_count(),
            days_since_last_update: self.get_seconds_since_update(),
            mirror_health: self.get_mirror_health(),
            download_size_mb: download_size,
            net_upgrade_size_mb: net_upgrade_size,
        }
    }
}
