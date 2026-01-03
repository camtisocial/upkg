mod ascii;

use crate::core;
use crate::managers::ManagerStats;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{self, Write};
use std::sync::mpsc::Receiver;
use std::time::Duration;
use termimad::crossterm::style::{Color::*, Stylize};
use termimad::crossterm::{
    cursor::{MoveToColumn, MoveUp, MoveDown},
    terminal::{Clear, ClearType},
    ExecutableCommand,
};

pub fn display_stats(stats: &ManagerStats) {

    if let Some(version) = &stats.pacman_version {
        let dashes = "-".repeat(version.len());
        println!("{}", version);
        println!("{}", dashes);
    } else {
        println!("----- upkg -----");
    }

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

// For plain mode - uses MirrorHealth from backward compat test_mirror_health()
pub fn display_mirror_health(
    mirror: &Option<crate::managers::MirrorHealth>,
    stats: &ManagerStats,
) {
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
    stats: &ManagerStats,
    progress_rx: Receiver<u64>,
    speed_rx: Receiver<Option<f64>>,
) -> io::Result<()> {

    let ascii_art = &ascii::PACMAN_ART;

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

    // display mirror info
    let mirror_url = stats
        .mirror_url
        .as_ref()
        .map(|s| s.as_str())
        .unwrap_or("Unknown");

    let sync_age = if let Some(age) = stats.mirror_sync_age_hours {
        format!("{:.1} hours ago", age)
    } else {
        "-".to_string()
    };

    // format stat lines that will be combined with ascii art as its printed
    let mut stats_lines = vec![];

    // pacman version title
    if let Some(version) = &stats.pacman_version {
        let dashes = "-".repeat(version.len());
        stats_lines.push(format!("{}", version.as_str().bold().with(Yellow)));
        stats_lines.push(dashes);
    }

    stats_lines.extend(vec![
        format!("{}: {}", "Installed".bold().with(Yellow), stats.total_installed),
        format!("{}: {}", "Upgradable".bold().with(Yellow), stats.total_upgradable),
        format!("{}: {}", "Last System Update".bold().with(Yellow), last_update),
        format!("{}: {}", "Download Size".bold().with(Yellow), download_size),
        format!("{}: {}", "Installed Size".bold().with(Yellow), installed_size),
        format!("{}: {}", "Net Upgrade Size".bold().with(Yellow), net_upgrade),
        format!("{}: {}", "Orphaned Packages".bold().with(Yellow), orphaned),
        format!("{}: {}", "Package Cache".bold().with(Yellow), cache),
        format!("{}: {}", "Mirror URL".bold().with(Yellow), mirror_url),
        format!("{}: {}", "Mirror Last Sync".bold().with(Yellow), sync_age),
        format!("{}: {}", "Mirror Speed".bold().with(Yellow), "-"),
        format!("{}: {}", "Download ETA".bold().with(Yellow), "-"),
    ]);

    // Print all ASCII art and stats side by side
    println!();
    let max_lines = ascii_art.len().max(stats_lines.len());
    for i in 0..max_lines {
        let art_line = ascii_art.get(i).copied().unwrap_or("                       ");
        let stat_line = stats_lines.get(i).map(|s| s.as_str()).unwrap_or("");
        println!("{} {}", art_line.cyan(), stat_line);
    }

    // Move cursor up to build progress bar (at line 14, which has no stats)
    let mut stdout = io::stdout();
    stdout.execute(MoveUp(2))?;
    stdout.execute(MoveToColumn(0))?;
    stdout.flush()?;

    // Create progress bar (uses ASCII art line 14, which has no stats)
    let art_line_14 = ascii_art.get(14).copied().unwrap_or("                       ");
    let pb = ProgressBar::new(100);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(&format!("{} {{spinner:.cyan}} {{msg}} {{bar:20.cyan/blue}} {{pos}}%", art_line_14.cyan()))
            .expect("Failed to create progress bar template")
            .progress_chars("━━╸")
            .tick_strings(&["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"]),
    );
    pb.set_message("Testing speed");

    // Update progress bar based on real progress from background thread
    loop {
        match progress_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(progress) => {
                pb.set_position(progress);

                if progress >= 100 {
                    break;
                }
            }
            Err(_) => {
                pb.tick();
            }
        }
    }

    // reprint progrsess bar when finished
    pb.set_style(
        ProgressStyle::default_bar()
            .template(&format!("{} {{bar:20.cyan/blue}} {{pos}}%", art_line_14.cyan()))
            .expect("Failed to create final template")
            .progress_chars("━━━━━━━━━━━━━━━━━━━━"),
    );
    pb.finish();

    // Progress bar finishes on line 14, need to move down 2 lines to get past all content
    let mut stdout = io::stdout();
    stdout.execute(MoveDown(2))?;
    stdout.flush()?;

    // Update Mirror Speed and Download ETA using crossterm
    if let Ok(Some(speed)) = speed_rx.recv() {
        // Calculate download time estimate
        let eta_display = if let Some(size) = stats.download_size_mb {
            if size > 0.0 {
                let eta_seconds = size / speed;
                if eta_seconds < 60.0 {
                    format!("{:.0}s", eta_seconds)
                } else if eta_seconds < 3600.0 {
                    format!("{:.0}m {:.0}s", eta_seconds / 60.0, eta_seconds % 60.0)
                } else {
                    format!(
                        "{:.0}h {:.0}m",
                        eta_seconds / 3600.0,
                        (eta_seconds % 3600.0) / 60.0
                    )
                }
            } else {
                "-".to_string()
            }
        } else {
            "-".to_string()
        };

        // Current position is line 16, move up 4 to get to 12
        stdout.execute(MoveUp(4))?;
        stdout.execute(MoveToColumn(0))?;

        // Reprint Mirror Speed line with new value
        let speed_art_line = ascii_art.get(12).copied().unwrap_or("                       ");
        let speed_value = format!("{}: {:.1} MB/s", "Mirror Speed".bold().with(Yellow), speed);
        print!("{} {}", speed_art_line.cyan(), speed_value);
        stdout.execute(Clear(ClearType::UntilNewLine))?;

        // Move down to Download ETA line (13)
        stdout.execute(MoveDown(1))?;
        stdout.execute(MoveToColumn(0))?;

        // Reprint Download ETA line with new value
        let eta_art_line = ascii_art.get(13).copied().unwrap_or("                       ");
        let eta_value = format!("{}: {}", "Download ETA".bold().with(Yellow), eta_display);
        print!("{} {}", eta_art_line.cyan(), eta_value);
        stdout.execute(Clear(ClearType::UntilNewLine))?;

        // Move cursor back down past all content (16)
        stdout.execute(MoveDown(5))?;
        stdout.flush()?;
    }

    println!();
    Ok(())
}

pub fn display_stats_with_graphics_no_speed(stats: &ManagerStats) -> io::Result<()> {
    let ascii_art = &ascii::PACMAN_ART;

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

    // display mirror info
    let mirror_url = stats
        .mirror_url
        .as_ref()
        .map(|s| s.as_str())
        .unwrap_or("Unknown");

    let sync_age = if let Some(age) = stats.mirror_sync_age_hours {
        format!("{:.1} hours ago", age)
    } else {
        "-".to_string()
    };

    let mut stats_lines = vec![];

    if let Some(version) = &stats.pacman_version {
        let dashes = "-".repeat(version.len());
        stats_lines.push(format!("{}", version.as_str().bold().with(Yellow)));
        stats_lines.push(dashes);
    }

    stats_lines.extend(vec![
        format!("{}: {}", "Installed".bold().with(Yellow), stats.total_installed),
        format!("{}: {}", "Upgradable".bold().with(Yellow), stats.total_upgradable),
        format!("{}: {}", "Last System Update".bold().with(Yellow), last_update),
        format!("{}: {}", "Download Size".bold().with(Yellow), download_size),
        format!("{}: {}", "Installed Size".bold().with(Yellow), installed_size),
        format!("{}: {}", "Net Upgrade Size".bold().with(Yellow), net_upgrade),
        format!("{}: {}", "Orphaned Packages".bold().with(Yellow), orphaned),
        format!("{}: {}", "Package Cache".bold().with(Yellow), cache),
        format!("{}: {}", "Mirror URL".bold().with(Yellow), mirror_url),
        format!("{}: {}", "Mirror Last Sync".bold().with(Yellow), sync_age),
    ]);

    stats_lines.push(String::new());

    // color palette rows 
    let colors = [Black, DarkRed, DarkGreen, DarkYellow, DarkBlue, DarkMagenta, DarkCyan, Grey];
    let bright_colors = [DarkGrey, Red, Green, Yellow, Blue, Magenta, Cyan, White];

    let mut color_row_1 = String::new();
    for color in &colors {
        color_row_1.push_str(&format!("{}", "   ".on(*color)));
    }

    let mut color_row_2 = String::new();
    for color in &bright_colors {
        color_row_2.push_str(&format!("{}", "   ".on(*color)));
    }

    stats_lines.push(color_row_1);
    stats_lines.push(color_row_2);

    println!();
    let max_lines = ascii_art.len().max(stats_lines.len());
    for i in 0..max_lines {
        let art_line = ascii_art.get(i).copied().unwrap_or("                       ");
        let stat_line = stats_lines.get(i).map(|s| s.as_str()).unwrap_or("");
        println!("{} {}", art_line.cyan(), stat_line);
    }

    println!();
    Ok(())
}
