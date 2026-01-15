pub mod pacman;

#[derive(Debug)]
pub struct ManagerStats {
    pub total_installed: u32,
    pub total_upgradable: u32,
    pub days_since_last_update: Option<i64>,
    pub download_size_mb: Option<f64>,
    pub total_installed_size_mb: Option<f64>,
    pub net_upgrade_size_mb: Option<f64>,
    pub orphaned_packages: Option<u32>,
    pub orphaned_size_mb: Option<f64>,
    pub cache_size_mb: Option<f64>,
    pub mirror_url: Option<String>,
    pub mirror_sync_age_hours: Option<f64>,
    pub pacman_version: Option<String>,
}

pub trait PackageManager {
    // requires root
    fn sync_databases(&self) -> Result<(), String>;

    // requires root - runs system upgrade (pacman -Syu)
    fn upgrade_system(&self, text_mode: bool, speed_test: bool) -> Result<(), String>;

    // local + fast network operations
    fn get_stats(&self, debug: bool) -> ManagerStats;

    // slow network operation
    fn test_mirror_speed_with_progress<F>(&self, mirror_url: &str, progress_callback: F) -> Option<f64>
    where
        F: Fn(u64);

    fn test_mirror_health(&self) -> Option<MirrorHealth>;
}

#[derive(Debug)]
pub struct MirrorHealth {
    pub url: String,
    pub speed_mbps: Option<f64>,
    pub sync_age_hours: Option<f64>,
}
