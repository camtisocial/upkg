pub mod pacman;

pub struct ManagerStats {
    pub total_installed: u32,
    pub total_upgradable: u32,
    pub days_since_last_update: u32,
    pub mirror_health: Option<String>,
    pub download_size_mb: Option<f64>,
    pub net_upgrade_size_mb: Option<f64>,
}

pub trait PackageManager {
    fn get_stats(&self) -> ManagerStats;
}
