use crate::core;
use crate::managers::{ManagerStats, MirrorHealth};
use std::io::{self, Write};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
};

pub fn display_stats(stats: &ManagerStats) {
    println!("----- upkg -----");
    println!("Total Installed Packages: {}", stats.total_installed);
    println!("Total Upgradable Packages: {}", stats.total_upgradable);

    if let Some(seconds) = stats.days_since_last_update {
        println!("Time Since Last Update: {}", core::normalize_duration(seconds));
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
                println!("Orphaned Packages: {} ({:.2} MiB reclaimable)", orphaned, size);
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
                        format!("{:.0}h {:.0}m", eta_seconds / 3600.0, (eta_seconds % 3600.0) / 60.0)
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

pub fn display_stats_with_graphics(stats: &ManagerStats, mirror: &Option<MirrorHealth>) -> io::Result<()> {
    let title = Line::from(" upkg ".bold());
    let block = Block::bordered()
        .title(title.centered())
        .border_set(border::THICK);

    let display_text = Text::from(vec![
        Line::from(format!("Total Installed Packages: {}", stats.total_installed)),
        Line::from(format!("Total Upgradable Packages: {}", stats.total_upgradable)),
    ]);

    let paragraph = Paragraph::new(display_text).block(block);

    // define wqindow size
    let width = 80;
    let height = 6; // Adjust based on content
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);

    // Render 
    paragraph.render(area, &mut buf);

    // pipe into terminal
    let mut stdout = io::stdout();
    for y in 0..height {
        for x in 0..width {
            let cell = buf.get(x, y);
            write!(stdout, "{}", cell.symbol())?;
        }
        writeln!(stdout)?;
    }
    stdout.flush()?;

    Ok(())
}
