mod core;
mod managers;
mod ui;

use clap::{CommandFactory, Parser};
use std::sync::mpsc;
use std::thread;

/// Display information about your package manager
#[derive(Parser)]
#[command(name = "upkg")]
#[command(version)]
#[command(about = "Display information about your package manager")]
#[command(after_help = "\
Commands:
  -Sy           Sync package databases
  -Syu          Sync databases and upgrade system

Options:
  -t, --text    Text mode (no ASCII art)
  -s, --speed   Include mirror speed test
  -d, --debug   Show debug timing information
  -h, --help    Print help
  -V, --version Print version")]
#[command(disable_help_flag = true)]
#[command(disable_version_flag = true)]
struct Cli {
    #[arg(short, long, hide = true)]
    text: bool,

    #[arg(short, long, hide = true)]
    speed: bool,

    #[arg(short = 'S', hide = true)]
    sync_op: bool,

    #[arg(short = 'y', hide = true)]
    sync_db: bool,

    #[arg(short = 'u', hide = true)]
    upgrade: bool,

    #[arg(short, long, hide = true)]
    debug: bool,

    #[arg(short = 'h', long = "help", hide = true)]
    help: bool,

    #[arg(short = 'V', long = "version", hide = true)]
    version: bool,
}

fn print_error_and_help(msg: &str) -> ! {
    eprintln!("error: {}\n", msg);
    let _ = Cli::command().print_help();
    eprintln!();
    std::process::exit(1);
}

fn main() {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(_) => print_error_and_help("unrecognized flag"),
    };

    if cli.help {
        let _ = Cli::command().print_help();
        println!();
        std::process::exit(0);
    }

    if cli.version {
        println!("upkg {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    let text_mode = cli.text;
    let speed_test = cli.speed;

    let invalid_flag = (cli.sync_op && !cli.sync_db && !cli.upgrade)
        || ((cli.sync_db || cli.upgrade) && !cli.sync_op);
    if invalid_flag {
        print_error_and_help("unrecognized flag combination");
    }

    // Handle system upgrade if requested
    if cli.sync_op && cli.upgrade {
        if let Err(e) = core::upgrade_system(text_mode, speed_test) {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    // Get stats (with spinner if syncing)
    let stats = if cli.sync_op && cli.sync_db {
        let spinner = core::create_spinner("Syncing package databases");
        if let Err(e) = core::sync_databases() {
            spinner.finish_and_clear();
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        spinner.set_message("Gathering stats");
        let stats = core::get_manager_stats(cli.debug);
        spinner.finish_and_clear();
        stats
    } else {
        println!();
        core::get_manager_stats(cli.debug)
    };

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
