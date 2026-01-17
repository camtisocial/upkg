use serde::Deserialize;

use crate::pacman::ManagerStats;
use crate::util;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatId {
    Installed,
    Upgradable,
    LastUpdate,
    DownloadSize,
    InstalledSize,
    NetUpgradeSize,
    OrphanedPackages,
    CacheSize,
    MirrorUrl,
    MirrorHealth,
}

impl StatId {
    pub fn label(&self) -> &'static str {
        match self {
            StatId::Installed => "Installed",
            StatId::Upgradable => "Upgradable",
            StatId::LastUpdate => "Last System Update",
            StatId::DownloadSize => "Download Size",
            StatId::InstalledSize => "Installed Size",
            StatId::NetUpgradeSize => "Net Upgrade Size",
            StatId::OrphanedPackages => "Orphaned Packages",
            StatId::CacheSize => "Package Cache",
            StatId::MirrorUrl => "Mirror URL",
            StatId::MirrorHealth => "Mirror Health",
        }
    }

    pub fn format_value(&self, stats: &ManagerStats) -> Option<String> {
        match self {
            StatId::Installed => Some(stats.total_installed.to_string()),
            StatId::Upgradable => Some(stats.total_upgradable.to_string()),
            StatId::LastUpdate => stats
                .days_since_last_update
                .map(|s| util::normalize_duration(s)),
            StatId::DownloadSize => stats.download_size_mb.map(|s| format!("{:.2} MiB", s)),
            StatId::InstalledSize => stats.total_installed_size_mb.map(|s| format!("{:.2} MiB", s)),
            StatId::NetUpgradeSize => stats.net_upgrade_size_mb.map(|s| format!("{:.2} MiB", s)),
            StatId::OrphanedPackages => {
                if let Some(count) = stats.orphaned_packages {
                    if count > 0 {
                        if let Some(size) = stats.orphaned_size_mb {
                            Some(format!("{} ({:.2} MiB)", count, size))
                        } else {
                            Some(count.to_string())
                        }
                    } else {
                        Some("0".to_string())
                    }
                } else {
                    None
                }
            }
            StatId::CacheSize => stats.cache_size_mb.map(|s| format!("{:.2} MiB", s)),
            StatId::MirrorUrl => stats.mirror_url.clone(),
            StatId::MirrorHealth => {
                match (&stats.mirror_url, stats.mirror_sync_age_hours) {
                    (Some(_), Some(age)) => Some(format!("OK (last sync {:.1} hours)", age)),
                    (Some(_), None) => Some("Err - could not check sync status".to_string()),
                    (None, _) => Some("Err - no mirror found".to_string()),
                }
            }
        }
    }
}

pub fn default_stats() -> Vec<StatId> {
    vec![
        StatId::Installed,
        StatId::Upgradable,
        StatId::LastUpdate,
        StatId::DownloadSize,
        StatId::InstalledSize,
        StatId::NetUpgradeSize,
        StatId::OrphanedPackages,
        StatId::CacheSize,
        StatId::MirrorUrl,
        StatId::MirrorHealth,
    ]
}

// --- stat fetch request helpers ---
pub fn needs_upgrade_stats(requested: &[StatId]) -> bool {
    requested.iter().any(|s| {
        matches!(
            s,
            StatId::Upgradable
                | StatId::DownloadSize
                | StatId::InstalledSize
                | StatId::NetUpgradeSize
        )
    })
}

pub fn needs_orphan_stats(requested: &[StatId]) -> bool {
    requested.contains(&StatId::OrphanedPackages)
}

pub fn needs_mirror_health(requested: &[StatId]) -> bool {
    requested.contains(&StatId::MirrorHealth)
}

pub fn needs_mirror_url(requested: &[StatId]) -> bool {
    requested.contains(&StatId::MirrorUrl) || needs_mirror_health(requested)
}
