mod core;
mod managers;
mod ui;

use clap::Parser;
use std::sync::mpsc;
use std::thread;

/// Display information about your package manager
#[derive(Parser)]
#[command(name = "upkg")]
#[command(version)]
#[command(about = "Display information about your package manager", long_about = None)]

struct Cli {
    /// text mode
    #[arg(short, long)]
    text: bool,

    /// include speed test
    #[arg(short, long)]
    speed: bool,

    /// sync package databases requires root
    #[arg(short = 'y', long)]
    sync: bool,

    /// upgrade system packages (runs -Syu) requires root
    #[arg(short = 'U', long)]
    upgrade: bool,

    /// show debug timing information
    #[arg(short, long)]
    debug: bool,
}

fn main() {
    let cli = Cli::parse();
    let text_mode = cli.text;
    let speed_test = cli.speed;

    // Handle database sync if requested
    if cli.sync {
        println!("Syncing package databases...");
        if let Err(e) = core::sync_databases() {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }

    // Handle system upgrade if requested
    if cli.upgrade {
        if let Err(e) = core::upgrade_system(text_mode, speed_test) {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    println!();

    // get fast stats
    let stats = core::get_manager_stats(cli.debug);

    if text_mode {
        if speed_test {
            // Text mode with speed test
            let mirror = core::test_mirror_health();
            ui::display_stats(&stats);
            ui::display_mirror_health(&mirror, &stats);
        } else {
            // Text mode without speed test
            ui::display_stats(&stats);

            // Build MirrorHealth from stats data (no speed test)
            let mirror = if let Some(ref mirror_url) = stats.mirror_url {
                Some(managers::MirrorHealth {
                    url: mirror_url.clone(),
                    speed_mbps: None,
                    sync_age_hours: stats.mirror_sync_age_hours,
                })
            } else {
                None
            };
            ui::display_mirror_health(&mirror, &stats);
        }
    } else {
        if speed_test {
            // Graphics mode with speed test
            if let Some(ref mirror_url) = stats.mirror_url {
                let mirror_url = mirror_url.clone();
                let (progress_tx, progress_rx) = mpsc::channel();
                let (speed_tx, speed_rx) = mpsc::channel();

                thread::spawn(move || {
                    let speed = core::test_mirror_speed_with_progress(&mirror_url, |progress| {
                        let _ = progress_tx.send(progress);
                    });
                    let _ = speed_tx.send(speed);
                });

                if let Err(e) = ui::display_stats_with_graphics(&stats, progress_rx, speed_rx) {
                    eprintln!("Error running TUI: {}", e);
                }
            } else {
                ui::display_stats(&stats);
            }
        } else {
            // Graphics mode without speed test (default)
            if let Err(e) = ui::display_stats_with_graphics_no_speed(&stats) {
                eprintln!("Error running graphics display: {}", e);
            }
        }
    }
}
