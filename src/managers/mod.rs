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
}

pub trait PackageManager {
    // local, fast
    fn get_stats(&self) -> ManagerStats;

    // network, slow
    fn test_mirror_health(&self) -> Option<MirrorHealth>;
}

#[derive(Debug)]
pub struct MirrorHealth {
    pub url: String,
    pub speed_mbps: Option<f64>,
    pub sync_age_hours: Option<f64>,
}
