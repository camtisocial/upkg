use crate::core;
use crate::managers::{ManagerStats, MirrorHealth};
use std::{io, thread, time::Duration};
use termimad::crossterm::style::Color::*;
use termimad::{Alignment, MadSkin, rgb};

fn get_spinner(frame: usize) -> char {
    let spinners = ['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];
    spinners[frame % spinners.len()]
}
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


pub fn display_stats_with_graphics(
    _stats: &ManagerStats,
    _mirror: &Option<MirrorHealth>,) -> io::Result<()> {
    // styling
    let mut skin = MadSkin::default();
    skin.set_headers_fg(rgb(255, 187, 0));
    skin.bold.set_fg(Yellow);
    skin.italic.set_fg(Cyan);
    skin.table.align = Alignment::Center;

    let mut frame = 0;
    let start_time = std::time::Instant::now();

    loop {
        // Clear screen
        print!("\x1B[2J\x1B[1;1H");

        let p1 = std::cmp::min((start_time.elapsed().as_secs() * 20) as u64, 100);

        let status1 = if p1 >= 100 {
            "✓ Done"
        } else {
            &format!("{} Downloading", get_spinner(frame))
        };

        let table = format!(
            r#"
# upkg

|-:|:-:|
|*Installed*|*test*|
|-:|:-:|
| *{}* | {}%
|-

"#,
            status1,
            p1
        );

        println!("{}", skin.term_text(&table));

        if p1 >= 100 {
            println!("\n✓ finished\n");
            break;
        }

        frame += 1;
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
