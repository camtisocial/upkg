use crate::managers::{ManagerStats, PackageManager};
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

    /// Get days since last update by parsing /var/log/pacman.log
    fn get_days_since_update(&self) -> u32 {
        // let log_path = "/var/log/pacman.log";
        0
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
            days_since_last_update: self.get_days_since_update(),
            mirror_health: self.get_mirror_health(),
            download_size_mb: download_size,
            net_upgrade_size_mb: net_upgrade_size,
        }
    }
}
