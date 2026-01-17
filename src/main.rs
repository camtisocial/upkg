mod config;
mod pacman;
mod stats;
mod ui;
mod util;

use clap::{CommandFactory, Parser};
use config::Config;
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
  -Su           Upgrade system (using local database)
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

    // Load config
    let config = Config::load();

    let invalid_flag = (cli.sync_op && !cli.sync_db && !cli.upgrade)
        || ((cli.sync_db || cli.upgrade) && !cli.sync_op);
    if invalid_flag {
        print_error_and_help("unrecognized flag combination");
    }

    // Handle system upgrade (-Su or -Syu)
    if cli.sync_op && cli.upgrade {
        let sync_first = cli.sync_db;
        if let Err(e) = pacman::upgrade_system(text_mode, speed_test, sync_first) {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    // Get stats
    let stats = if cli.sync_op && cli.sync_db {
        if let Err(e) = pacman::sync_databases() {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        let spinner = util::create_spinner("Gathering stats");
        let stats = pacman::get_stats(&config.display.stats, cli.debug);
        spinner.finish_and_clear();
        stats
    } else if text_mode || cli.debug {
        println!();
        pacman::get_stats(&config.display.stats, cli.debug)
    } else {
        let spinner = util::create_spinner("Gathering stats");
        let stats = pacman::get_stats(&config.display.stats, cli.debug);
        spinner.finish_and_clear();
        stats
    };

    if text_mode {
        if speed_test {
            // Text mode with speed test
            let mirror = pacman::test_mirror_health();
            ui::display_stats(&stats, &config);
            ui::display_mirror_health(&mirror, &stats);
        } else {
            // Text mode without speed test
            ui::display_stats(&stats, &config);

            // Build MirrorHealth from stats data (no speed test)
            let mirror = if let Some(ref mirror_url) = stats.mirror_url {
                Some(pacman::MirrorHealth {
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
                    let speed = pacman::test_mirror_speed_with_progress(&mirror_url, |progress| {
                        let _ = progress_tx.send(progress);
                    });
                    let _ = speed_tx.send(speed);
                });

                if let Err(e) = ui::display_stats_with_graphics(&stats, &config, progress_rx, speed_rx) {
                    eprintln!("Error running TUI: {}", e);
                }
            } else {
                ui::display_stats(&stats, &config);
            }
        } else {
            // Graphics mode without speed test (default)
            if let Err(e) = ui::display_stats_with_graphics_no_speed(&stats, &config) {
                eprintln!("Error running graphics display: {}", e);
            }
        }
    }
}
