mod ascii;

use crate::config::Config;
use crate::pacman::ManagerStats;
use crate::stats::StatId;
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

pub fn display_stats(stats: &ManagerStats, config: &Config) {
    // Header
    if let Some(version) = &stats.pacman_version {
        let dashes = "-".repeat(version.len());
        println!("{}", version);
        println!("{}", dashes);
    } else {
        println!("----- pacfetch -----");
    }

    // stats
    for stat_id in &config.display.stats {
        if let Some(value) = stat_id.format_value(stats) {
            println!("{}: {}", stat_id.label(), value);
        }
    }
}

pub fn display_mirror_health(
    mirror: &Option<crate::pacman::MirrorHealth>,
    stats: &ManagerStats,
) {
    println!("----- Mirror -----");

    if let Some(m) = mirror {
        let status = if let Some(age) = m.sync_age_hours {
            format!("OK (last sync {:.1} hours)", age)
        } else {
            "Err - could not check sync status".to_string()
        };
        println!("Status: {}", status);
        println!("URL: {}", m.url);

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
    } else {
        println!("Status: Err - no mirror found");
    }
}

pub fn display_stats_with_graphics(
    stats: &ManagerStats,
    config: &Config,
    progress_rx: Receiver<u64>,
    speed_rx: Receiver<Option<f64>>,
) -> io::Result<()> {

    let ascii_art = &ascii::PACMAN_ART;

    // Build stat lines from config
    let mut stats_lines = vec![];

    if let Some(version) = &stats.pacman_version {
        let dashes = "-".repeat(version.len());
        stats_lines.push(format!("{}", version.as_str().bold().with(Yellow)));
        stats_lines.push(dashes);
    }

    // Add configured stats with colored labels
    for stat_id in &config.display.stats {
        let value = stat_id.format_value(stats).unwrap_or_else(|| "-".to_string());
        let formatted_value = if *stat_id == StatId::MirrorHealth {
            match (&stats.mirror_url, stats.mirror_sync_age_hours) {
                (Some(_), Some(age)) => format!("{} (last sync {:.1} hours)", "OK".green(), age),
                (Some(_), None) => format!("{} - could not check sync status", "Err".red()),
                (None, _) => format!("{} - no mirror found", "Err".red()),
            }
        } else {
            value
        };
        stats_lines.push(format!("{}: {}", stat_id.label().bold().with(Yellow), formatted_value));
    }

    stats_lines.push(format!("{}: {}", "Download Speed".bold().with(Yellow), "-"));
    stats_lines.push(format!("{}: {}", "Download ETA".bold().with(Yellow), "-"));

    // Print ascii art and stats side by side
    println!();
    let max_lines = ascii_art.len().max(stats_lines.len());
    for i in 0..max_lines {
        let art_line = ascii_art.get(i).copied().unwrap_or("                       ");
        let stat_line = stats_lines.get(i).map(|s| s.as_str()).unwrap_or("");
        println!("{} {}", art_line.cyan(), stat_line);
    }

    let mut stdout = io::stdout();
    stdout.execute(MoveUp(2))?;
    stdout.execute(MoveToColumn(0))?;
    stdout.flush()?;

    let art_line_14 = ascii_art.get(14).copied().unwrap_or("                       ");
    let pb = ProgressBar::new(100);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(&format!("{} {{spinner:.cyan}} {{msg}} {{bar:20.cyan/blue}} {{pos}}%", art_line_14.cyan()))
            .expect("Failed to create progress bar template")
            .progress_chars("━━╸"),
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

    // Update Download Speed and Download ETA using crossterm
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

        // Reprint Download Speed line with new value
        let speed_art_line = ascii_art.get(12).copied().unwrap_or("                       ");
        let speed_value = format!("{}: {:.1} MB/s", "Download Speed".bold().with(Yellow), speed);
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

pub fn display_stats_with_graphics_no_speed(stats: &ManagerStats, config: &Config) -> io::Result<()> {
    let ascii_art = &ascii::PACMAN_ART;

    // Build stat lines from config
    let mut stats_lines = vec![];

    if let Some(version) = &stats.pacman_version {
        let dashes = "-".repeat(version.len());
        stats_lines.push(format!("{}", version.as_str().bold().with(Yellow)));
        stats_lines.push(dashes);
    }

    // Add stats
    for stat_id in &config.display.stats {
        let value = stat_id.format_value(stats).unwrap_or_else(|| "-".to_string());
        let formatted_value = if *stat_id == StatId::MirrorHealth {
            match (&stats.mirror_url, stats.mirror_sync_age_hours) {
                (Some(_), Some(age)) => format!("{} (last sync {:.1} hours)", "OK".green(), age),
                (Some(_), None) => format!("{} - could not check sync status", "Err".red()),
                (None, _) => format!("{} - no mirror found", "Err".red()),
            }
        } else {
            value
        };
        stats_lines.push(format!("{}: {}", stat_id.label().bold().with(Yellow), formatted_value));
    }

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
