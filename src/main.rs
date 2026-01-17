mod config;
mod pacman;
mod stats;
mod ui;
mod util;

use clap::{CommandFactory, Parser};
use config::Config;

/// Display information about your package manager
#[derive(Parser)]
#[command(name = "pacfetch")]
#[command(version)]
#[command(about = "A neofetch style wrapper for pacman's Syu/Sy/Su commands")]
#[command(after_help = "\
Commands:
  -Sy           Sync package databases
  -Su           Upgrade system 
  -Syu          Sync databases and upgrade system

Options:
  -d, --debug   Debug mode
  -h, --help    Print help
  -V, --version Print version")]
#[command(disable_help_flag = true)]
#[command(disable_version_flag = true)]
struct Cli {
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

    #[arg(short = 'V', short_alias = 'v', long = "version", hide = true)]
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
        println!("pacfetch {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

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
        if let Err(e) = pacman::upgrade_system(cli.debug, sync_first) {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    // Get stats
    let stats = if cli.sync_op && cli.sync_db {
        if let Err(e) = pacman::sync_databases() {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
        let spinner = util::create_spinner("Gathering stats");
        let stats = pacman::get_stats(&config.display.stats, cli.debug, Some(&spinner));
        spinner.finish_and_clear();
        stats
    } else if cli.debug {
        println!();
        pacman::get_stats(&config.display.stats, cli.debug, None)
    } else {
        let spinner = util::create_spinner("Gathering stats");
        let stats = pacman::get_stats(&config.display.stats, cli.debug, Some(&spinner));
        spinner.finish_and_clear();
        stats
    };

    if cli.debug {
        ui::display_stats(&stats, &config);
        println!();
    } else {
        if let Err(e) = ui::display_stats_with_graphics(&stats, &config) {
            eprintln!("error: {}", e);
        }
    }
}
