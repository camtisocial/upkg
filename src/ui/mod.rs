use crate::core;
use crate::managers::{ManagerStats, MirrorHealth};
use indicatif::{ProgressBar, ProgressStyle};
use std::{io, thread, time::Duration};
use termimad::crossterm::style::Color::*;
use termimad::{MadSkin, rgb};

pub fn display_stats(stats: &ManagerStats) {
    println!("----- upkg -----");
    println!("Total Installed Packages: {}", stats.total_installed);
    println!("Total Upgradable Packages: {}", stats.total_upgradable);

    if let Some(seconds) = stats.days_since_last_update {
        println!(
            "Time Since Last Update: {}",
            core::normalize_duration(seconds)
        );
    } else {
        println!("Time Since Last Update: Unknown");
    }

    if let Some(download) = stats.download_size_mb {
        println!("Total Download Size: {:.2} MiB", download);
    }

    if let Some(installed) = stats.total_installed_size_mb {
        println!("Total Installed Size: {:.2} MiB", installed);
    }

    if let Some(net_upgrade) = stats.net_upgrade_size_mb {
        println!("Net Upgrade Size: {:.2} MiB", net_upgrade);
    }

    if let Some(orphaned) = stats.orphaned_packages {
        if orphaned > 0 {
            if let Some(size) = stats.orphaned_size_mb {
                println!(
                    "Orphaned Packages: {} ({:.2} MiB reclaimable)",
                    orphaned, size
                );
            } else {
                println!("Orphaned Packages: {}", orphaned);
            }
        }
    }

    if let Some(cache_size) = stats.cache_size_mb {
        println!("Package Cache: {:.2} MiB", cache_size);
    }
}

pub fn display_mirror_health(mirror: &Option<MirrorHealth>, stats: &ManagerStats) {
    if let Some(m) = mirror {
        println!("----- Mirror Health -----");
        println!("Mirror: {}", m.url);

        if let Some(speed) = m.speed_mbps {
            println!("Speed: {:.1} MB/s", speed);

            if let Some(size) = stats.download_size_mb {
                if size > 0.0 {
                    let eta_seconds = size / speed;
                    let eta_display = if eta_seconds < 60.0 {
                        format!("{:.0}s", eta_seconds)
                    } else if eta_seconds < 3600.0 {
                        format!("{:.0}m {:.0}s", eta_seconds / 60.0, eta_seconds % 60.0)
                    } else {
                        format!(
                            "{:.0}h {:.0}m",
                            eta_seconds / 3600.0,
                            (eta_seconds % 3600.0) / 60.0
                        )
                    };
                    println!("Estimated Download Time: {}", eta_display);
                }
            }
        }

        if let Some(age) = m.sync_age_hours {
            println!("Last Sync: {:.1} hours ago", age);
        }
    }
}

pub fn display_stats_with_graphics( stats: &ManagerStats, _mirror: &Option<MirrorHealth>,) -> io::Result<()> {
    let mut skin = MadSkin::default();
    skin.set_headers_fg(rgb(255, 187, 0));
    skin.bold.set_fg(Yellow);
    skin.italic.set_fg(Cyan);

    // Format stats
    let last_update = stats
        .days_since_last_update
        .map(|s| core::normalize_duration(s))
        .unwrap_or_else(|| "Unknown".to_string());

    let download_size = stats
        .download_size_mb
        .map(|s| format!("{:.2} MiB", s))
        .unwrap_or_else(|| "-".to_string());

    let installed_size = stats
        .total_installed_size_mb
        .map(|s| format!("{:.2} MiB", s))
        .unwrap_or_else(|| "-".to_string());

    let net_upgrade = stats
        .net_upgrade_size_mb
        .map(|s| format!("{:.2} MiB", s))
        .unwrap_or_else(|| "-".to_string());

    let orphaned = if let Some(count) = stats.orphaned_packages {
        if let Some(size) = stats.orphaned_size_mb {
            format!("{} ({:.2} MiB)", count, size)
        } else {
            count.to_string()
        }
    } else {
        "-".to_string()
    };

    let cache = stats
        .cache_size_mb
        .map(|s| format!("{:.2} MiB", s))
        .unwrap_or_else(|| "-".to_string());

    // Print non network stats once
    let content = format!(
        r#"

**{:<20}** {}
**{:<20}** {}
**{:<20}** {}
**{:<20}** {}
**{:<20}** {}
**{:<20}** {}
**{:<20}** {}
**{:<20}** {}
"#,
        "Installed:",
        stats.total_installed,
        "Upgradable:",
        stats.total_upgradable,
        "Last System Update:",
        last_update,
        "Download Size:",
        download_size,
        "Installed Size:",
        installed_size,
        "Net Upgrade Size:",
        net_upgrade,
        "Orphaned Packages:",
        orphaned,
        "Package Cache:",
        cache
    );

    let width = 80;
    println!("{}", skin.text(&content, Some(width)));

    // progress bar with spinner
    let pb = ProgressBar::new(100);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan} {msg} {bar:20.cyan/blue} {pos}%")
            .expect("Failed to create progress bar template")
            .progress_chars("━━╸")
            .tick_strings(&["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"]),
    );

    // tester progress info
    let start_time = std::time::Instant::now();
    loop {
        let progress = std::cmp::min((start_time.elapsed().as_secs() * 20) as u64, 100);

        if progress >= 100 {
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{msg} {bar:20.cyan/blue} {pos}%")
                    .expect("Failed to create final template")
                    .progress_chars("━━━━━━━━━━━━━━━━━━━━"),
            );
            pb.finish_with_message("✓ Done");
            break;
        } else {
            pb.set_message("Downloading");
            pb.set_position(progress);
        }

        thread::sleep(Duration::from_millis(100));
    }

    println!();
    Ok(())
}
