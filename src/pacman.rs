use crate::stats::StatId;
use crate::util;
use alpm::Alpm;
use chrono::{DateTime, FixedOffset, Local};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::process::Command;
use std::time::Instant;

// --- Public data structures ---

#[derive(Debug, Default)]
pub struct ManagerStats {
    pub total_installed: u32,
    pub total_upgradable: u32,
    pub days_since_last_update: Option<i64>,
    pub download_size_mb: Option<f64>,
    pub total_installed_size_mb: Option<f64>,
    pub net_upgrade_size_mb: Option<f64>,
    pub orphaned_packages: Option<u32>,
    pub orphaned_size_mb: Option<f64>,
    pub cache_size_mb: Option<f64>,
    pub mirror_url: Option<String>,
    pub mirror_sync_age_hours: Option<f64>,
    pub pacman_version: Option<String>,
}

// --- Private data structures ---

#[derive(Default)]
struct UpgradeStats {
    download_size_mb: Option<f64>,
    installed_size_mb: Option<f64>,
    net_upgrade_size_mb: Option<f64>,
    package_count: u32,
}

#[derive(Clone, Copy)]
enum DbSyncState {
    Syncing(u8),
    Complete,
}

struct SyncProgress {
    core: DbSyncState,
    extra: DbSyncState,
    multilib: DbSyncState,
}

impl SyncProgress {
    fn new() -> Self {
        Self {
            core: DbSyncState::Syncing(0),
            extra: DbSyncState::Syncing(0),
            multilib: DbSyncState::Syncing(0),
        }
    }

    fn format(&self) -> String {
        format!(
            "core {} | extra {} | multilib {}",
            Self::format_state(self.core),
            Self::format_state(self.extra),
            Self::format_state(self.multilib)
        )
    }

    fn format_state(state: DbSyncState) -> String {
        match state {
            DbSyncState::Syncing(pct) => format!("{}%", pct),
            DbSyncState::Complete => "✓".to_string(),
        }
    }

    fn update_from_line(&mut self, line: &str) {
        let clean = util::strip_ansi(line);
        let trimmed = clean.trim();

        if trimmed.contains("is up to date") {
            if trimmed.starts_with("core") {
                self.core = DbSyncState::Complete;
            } else if trimmed.starts_with("extra") {
                self.extra = DbSyncState::Complete;
            } else if trimmed.starts_with("multilib") {
                self.multilib = DbSyncState::Complete;
            }
            return;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 2 {
            let db_name = parts[0];
            let last = parts[parts.len() - 1];

            if let Some(pct_str) = last.strip_suffix('%') {
                if let Ok(pct) = pct_str.parse::<u8>() {
                    let state = if pct >= 100 {
                        DbSyncState::Complete
                    } else {
                        DbSyncState::Syncing(pct)
                    };

                    match db_name {
                        "core" => self.core = state,
                        "extra" => self.extra = state,
                        "multilib" => self.multilib = state,
                        _ => {}
                    }
                }
            }
        }
    }
}

// --- Private helper functions ---

fn get_installed_count() -> u32 {
    let output = Command::new("pacman").arg("-Q").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().count() as u32
}

fn get_seconds_since_update() -> Option<i64> {
    let contents = fs::read_to_string("/var/log/pacman.log").expect("Failed to read pacman.log");

    let mut saw_upgrade_start = false;
    let mut upgrade_start_timestamp: Option<String> = None;
    let mut last_valid_timestamp: Option<String> = None;

    for line in contents.lines() {
        let trimmed = line.trim();

        let timestamp = trimmed
            .split(']')
            .next()
            .map(|x| x.trim_start_matches('['))
            .unwrap_or("");

        if trimmed.contains("starting full system upgrade") {
            saw_upgrade_start = true;
            upgrade_start_timestamp = Some(timestamp.to_string());
        }

        if saw_upgrade_start && trimmed.contains("transaction completed") {
            last_valid_timestamp = upgrade_start_timestamp.clone();
            saw_upgrade_start = false;
        }
    }

    if let Some(ts) = last_valid_timestamp {
        let formatted_date = format!("{}:{}", &ts[..22], &ts[22..]);

        let parsed: DateTime<FixedOffset> = DateTime::parse_from_rfc3339(&formatted_date).unwrap();

        let last_update_local = parsed.with_timezone(&Local);
        let now = Local::now();
        let duration = now.signed_duration_since(last_update_local);
        let seconds = duration.num_seconds().max(0);

        return Some(seconds);
    }

    None
}

fn get_upgrade_sizes() -> UpgradeStats {
    let fail = UpgradeStats::default();

    let mut alpm = match Alpm::new("/", "/var/lib/pacman") {
        Ok(a) => a,
        Err(_) => return fail,
    };

    let _ = alpm.register_syncdb_mut("core", alpm::SigLevel::NONE);
    let _ = alpm.register_syncdb_mut("extra", alpm::SigLevel::NONE);
    let _ = alpm.register_syncdb_mut("multilib", alpm::SigLevel::NONE);

    if alpm.trans_init(alpm::TransFlag::NO_LOCK).is_err() {
        return fail;
    }

    if alpm.sync_sysupgrade(false).is_err() {
        let _ = alpm.trans_release();
        return fail;
    }

    if alpm.trans_prepare().is_err() {
        let _ = alpm.trans_release();
        return fail;
    }

    let localdb = alpm.localdb();

    let mut total_download_size: i64 = 0;
    let mut total_installed_size: i64 = 0;
    let mut net_upgrade_size: i64 = 0;
    let mut package_count: u32 = 0;

    for pkg in alpm.trans_add().into_iter() {
        package_count += 1;
        total_download_size += pkg.download_size();
        let new_size = pkg.isize();
        total_installed_size += new_size;

        if let Ok(oldpkg) = localdb.pkg(pkg.name()) {
            let old_size = oldpkg.isize();
            net_upgrade_size += new_size - old_size;
        } else {
            net_upgrade_size += new_size;
        }
    }

    for pkg in alpm.trans_remove().into_iter() {
        net_upgrade_size -= pkg.isize();
    }

    let _ = alpm.trans_release();

    let download_mib = total_download_size as f64 / 1048576.0;
    let installed_mib = total_installed_size as f64 / 1048576.0;
    let mut net_mib = net_upgrade_size as f64 / 1048576.0;

    if net_mib > -0.01 && net_mib < 0.01 {
        net_mib = 0.0;
    }

    UpgradeStats {
        download_size_mb: Some(download_mib),
        installed_size_mb: Some(installed_mib),
        net_upgrade_size_mb: Some(net_mib),
        package_count,
    }
}

fn get_orphaned_packages() -> (Option<u32>, Option<f64>) {
    let alpm = match Alpm::new("/", "/var/lib/pacman") {
        Ok(a) => a,
        Err(_) => return (None, None),
    };

    let localdb = alpm.localdb();
    let mut count = 0;
    let mut total_size: i64 = 0;

    for pkg in localdb.pkgs().into_iter() {
        if pkg.reason() == alpm::PackageReason::Depend {
            if pkg.required_by().len() == 0 && pkg.optional_for().len() == 0 {
                count += 1;
                total_size += pkg.isize();
            }
        }
    }

    let size_mb = total_size as f64 / 1048576.0;
    (Some(count), Some(size_mb))
}

fn get_cache_size() -> Option<f64> {
    let cache_path = std::path::Path::new("/var/cache/pacman/pkg");

    if let Ok(entries) = std::fs::read_dir(cache_path) {
        let total_size: u64 = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum();

        Some(total_size as f64 / 1048576.0)
    } else {
        None
    }
}

fn get_mirror_url() -> Option<String> {
    let mirrorlist = fs::read_to_string("/etc/pacman.d/mirrorlist").ok()?;

    for line in mirrorlist.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Server = ") {
            let url = trimmed.strip_prefix("Server = ")?;
            let base_url = url.split("/$repo").next()?;
            return Some(base_url.to_string());
        }
    }
    None
}

fn get_pacman_version() -> Option<String> {
    let output = Command::new("pacman").arg("--version").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        if line.contains("Pacman v") && line.contains("libalpm v") {
            if let Some(version_start) = line.find("Pacman v") {
                let version_str = &line[version_start..];
                return Some(version_str.trim().to_string());
            }
        }
    }
    None
}

fn check_mirror_sync(mirror_url: &str) -> Option<f64> {
    let lastsync_url = format!("{}/lastsync", mirror_url);

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let response = client.get(&lastsync_url).send().ok()?;

    if !response.status().is_success() {
        return None;
    }

    let timestamp_str = response.text().ok()?;
    let timestamp: i64 = timestamp_str.trim().parse().ok()?;

    let now = Local::now().timestamp();
    let age_seconds = now - timestamp;
    let age_hours = age_seconds as f64 / 3600.0;

    Some(age_hours.max(0.0))
}

fn filter_upgrade_line(line: &str) -> bool {
    let clean = util::strip_ansi(line);
    let trimmed = clean.trim();

    if trimmed.is_empty() {
        return false;
    }

    if trimmed.contains("Total Download Size:")
        || trimmed.contains("Total Installed Size:")
        || trimmed.contains("Net Upgrade Size:")
    {
        return false;
    }

    if trimmed.contains("resolving dependencies")
        || trimmed.contains("looking for conflicting packages")
        || trimmed.contains(":: Starting full system upgrade...")
    {
        return false;
    }

    true
}

fn should_print(line: &str, filter: bool) -> bool {
    if filter {
        filter_upgrade_line(line)
    } else {
        true
    }
}

fn run_pacman_pty(args: &[&str], filter: bool) -> Result<(), String> {
    use std::io::Write;

    let cmd = format!("pacman {}", args.join(" "));
    let mut session =
        expectrl::spawn(&cmd).map_err(|e| format!("Failed to spawn pacman: {}", e))?;

    if let Ok((cols, rows)) = crossterm::terminal::size() {
        let _ = session.get_process_mut().set_window_size(cols, rows);
    }

    session.set_expect_timeout(Some(std::time::Duration::from_millis(100)));

    let mut stdout = std::io::stdout();
    let mut line_buffer = String::new();

    loop {
        match session.is_alive() {
            Ok(true) => {}
            Ok(false) => {
                if !line_buffer.is_empty() && should_print(&line_buffer, filter) {
                    println!("{}", line_buffer);
                }
                return Ok(());
            }
            Err(_) => {
                return Ok(());
            }
        }

        let mut buf = [0u8; 1024];
        match session.try_read(&mut buf) {
            Ok(0) => continue,
            Ok(n) => {
                let chunk = String::from_utf8_lossy(&buf[..n]);

                for ch in chunk.chars() {
                    if ch == '\n' {
                        if should_print(&line_buffer, filter) {
                            println!("{}", line_buffer);
                        }
                        line_buffer.clear();
                    } else if ch == '\r' {
                        if !line_buffer.is_empty() && should_print(&line_buffer, filter) {
                            print!("\r{}", line_buffer);
                            let _ = stdout.flush();
                        }
                        line_buffer.clear();
                    } else {
                        line_buffer.push(ch);

                        if line_buffer.ends_with("[Y/n] ")
                            || (line_buffer.contains("::") && line_buffer.ends_with("]: "))
                        {
                            if should_print(&line_buffer, filter) {
                                if line_buffer.contains("Proceed with installation") {
                                    println!("\n\n");
                                }
                                print!("{}", line_buffer);
                                let _ = stdout.flush();
                            }
                            line_buffer.clear();

                            let mut input = String::new();
                            if std::io::stdin().read_line(&mut input).is_ok() {
                                let _ = session.send_line(input.trim());
                            }
                        }
                    }
                }
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                _ => break,
            },
        }
    }

    if !line_buffer.is_empty() && should_print(&line_buffer, filter) {
        println!("{}", line_buffer);
    }

    print!("\x1b[0m");
    let _ = stdout.flush();

    Ok(())
}

fn run_pacman_sync() -> Result<(), String> {
    if !util::is_root() {
        return Err("Database sync requires root, rerun with sudo".to_string());
    }

    let mut session =
        expectrl::spawn("pacman -Sy").map_err(|e| format!("Failed to spawn pacman: {}", e))?;

    if let Ok((cols, rows)) = crossterm::terminal::size() {
        let _ = session.get_process_mut().set_window_size(cols, rows);
    }

    session.set_expect_timeout(Some(std::time::Duration::from_millis(100)));

    let mut progress = SyncProgress::new();
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} Syncing databases: {msg}")
            .unwrap(),
    );
    pb.set_message(progress.format());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    let mut line_buffer = String::new();

    loop {
        match session.is_alive() {
            Ok(true) => {}
            Ok(false) => {
                if !line_buffer.is_empty() {
                    progress.update_from_line(&line_buffer);
                    pb.set_message(progress.format());
                }
                break;
            }
            Err(_) => break,
        }

        let mut buf = [0u8; 1024];
        match session.try_read(&mut buf) {
            Ok(0) => continue,
            Ok(n) => {
                let chunk = String::from_utf8_lossy(&buf[..n]);

                for ch in chunk.chars() {
                    if ch == '\n' || ch == '\r' {
                        if !line_buffer.is_empty() {
                            progress.update_from_line(&line_buffer);
                            pb.set_message(progress.format());
                        }
                        line_buffer.clear();
                    } else {
                        line_buffer.push(ch);
                    }
                }
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                _ => break,
            },
        }
    }

    progress.core = DbSyncState::Complete;
    progress.extra = DbSyncState::Complete;
    progress.multilib = DbSyncState::Complete;
    pb.set_message(progress.format());

    std::thread::sleep(std::time::Duration::from_millis(150));
    pb.finish_and_clear();

    Ok(())
}

// --- Public API ---

pub fn sync_databases() -> Result<(), String> {
    run_pacman_sync()
}

pub fn upgrade_system(text_mode: bool, sync_first: bool) -> Result<(), String> {
    if !util::is_root() {
        return Err("System upgrade requires root, rerun with sudo".to_string());
    }

    let config = crate::config::Config::load();

    if sync_first {
        run_pacman_sync()?;
    }
    let spinner = util::create_spinner("Gathering stats");
    let stats = get_stats(&config.display.stats, false);
    spinner.finish_and_clear();

    if text_mode {
        crate::ui::display_stats(&stats, &config);
        println!();
    } else {
        if let Err(e) = crate::ui::display_stats_with_graphics(&stats, &config) {
            eprintln!("Error running graphics display: {}", e);
            crate::ui::display_stats(&stats, &config);
            println!();
        }
    }

    run_pacman_pty(&["-Su"], true)
}

pub fn get_stats(requested: &[StatId], debug: bool) -> ManagerStats {
    use crate::stats::{
        needs_mirror_health, needs_mirror_url, needs_orphan_stats, needs_upgrade_stats,
    };

    let total_start = Instant::now();
    let mut stats = ManagerStats::default();

    if needs_upgrade_stats(requested) {
        let start = Instant::now();
        let upgrade_stats = get_upgrade_sizes();
        stats.total_upgradable = upgrade_stats.package_count;
        stats.download_size_mb = upgrade_stats.download_size_mb;
        stats.total_installed_size_mb = upgrade_stats.installed_size_mb;
        stats.net_upgrade_size_mb = upgrade_stats.net_upgrade_size_mb;
        if debug {
            eprintln!("Upgrade sizes + count: {:?}", start.elapsed());
        }
    } else if debug {
        eprintln!("Upgrade sizes: SKIP");
    }

    if needs_orphan_stats(requested) {
        let start = Instant::now();
        let (orphaned_count, orphaned_size) = get_orphaned_packages();
        stats.orphaned_packages = orphaned_count;
        stats.orphaned_size_mb = orphaned_size;
        if debug {
            eprintln!("Orphaned packages: {:?}", start.elapsed());
        }
    } else if debug {
        eprintln!("Orphaned packages: SKIP");
    }

    let sync_handle = if needs_mirror_url(requested) {
        let start = Instant::now();
        stats.mirror_url = get_mirror_url();
        if debug {
            eprintln!("Mirror URL: {:?}", start.elapsed());
        }

        if needs_mirror_health(requested) {
            let sync_start = Instant::now();
            let mirror_url_clone = stats.mirror_url.clone();
            let handle = std::thread::spawn(move || {
                mirror_url_clone
                    .as_ref()
                    .and_then(|url| check_mirror_sync(url))
            });
            Some((handle, sync_start))
        } else {
            if debug {
                eprintln!("Mirror sync age: SKIP");
            }
            None
        }
    } else {
        if debug {
            eprintln!("Mirror URL: SKIP");
            eprintln!("Mirror sync age: SKIP");
        }
        None
    };

    if requested.contains(&StatId::Installed) {
        let start = Instant::now();
        stats.total_installed = get_installed_count();
        if debug {
            eprintln!("Installed count: {:?}", start.elapsed());
        }
    }

    if requested.contains(&StatId::LastUpdate) {
        let start = Instant::now();
        stats.days_since_last_update = get_seconds_since_update();
        if debug {
            eprintln!("Last update time: {:?}", start.elapsed());
        }
    }

    if requested.contains(&StatId::CacheSize) {
        let start = Instant::now();
        stats.cache_size_mb = get_cache_size();
        if debug {
            eprintln!("Cache size: {:?}", start.elapsed());
        }
    }

    let start = Instant::now();
    stats.pacman_version = get_pacman_version();
    if debug {
        eprintln!("Pacman version: {:?}", start.elapsed());
    }

    if let Some((handle, sync_start)) = sync_handle {
        stats.mirror_sync_age_hours = handle.join().ok().flatten();
        if debug {
            eprintln!("Mirror sync age: {:?}", sync_start.elapsed());
        }
    }

    if debug {
        eprintln!("TOTAL: {:?}", total_start.elapsed());
    }

    stats
}
