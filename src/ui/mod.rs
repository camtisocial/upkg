use crate::core;

pub fn draw_ui() {
    match core::get_manager_stats() {
        stats => {
            println!("----- upkg -----");
            println!("Total Installed Packages: {}", stats.total_installed);
            println!("Total Upgradable Packages: {}", stats.total_upgradable);
            println!("Days Since Last Update: {}", stats.days_since_last_update);

            if let Some(download) = stats.download_size_mb {
                println!("Total Download Size: {:.2} MiB", download);
            }

            if let Some(net_upgrade) = stats.net_upgrade_size_mb {
                println!("Net Upgrade Size: {:.2} MiB", net_upgrade);
            }

            if let Some(mirror) = stats.mirror_health {
                println!("Mirror Health: {}", mirror);
            }
        }
    }
}
