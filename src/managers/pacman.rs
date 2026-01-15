use crate::managers::{ManagerStats, MirrorHealth, PackageManager};
use alpm::Alpm;
use chrono::{DateTime, FixedOffset, Local};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Instant;

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

    /// Update from pacman Su output
    fn update_from_line(&mut self, line: &str) {
        let clean = FetchPacmanStats::strip_ansi(line);
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

pub struct UpgradeStats {
    pub download_size_mb: Option<f64>,
    pub installed_size_mb: Option<f64>,
    pub net_upgrade_size_mb: Option<f64>,
    pub package_count: u32,
}

pub struct FetchPacmanStats;

impl FetchPacmanStats {
    fn get_installed_count(&self) -> u32 {
        let output = Command::new("pacman").arg("-Q").output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().count() as u32
    }

    /// get time since last update from /var/log/pacman.log
    /// returns seconds
    fn get_seconds_since_update(&self) -> Option<i64> {
        // Look for "starting full system upgrade" followed by "transaction completed"

        let contents =
            fs::read_to_string("/var/log/pacman.log").expect("Failed to read pacman.log");

        let mut saw_upgrade_start = false;
        let mut upgrade_start_timestamp: Option<String> = None;
        let mut last_valid_timestamp: Option<String> = None;

        for line in contents.lines() {
            let trimmed = line.trim();

            // Extract timestamp: [2025-12-05T15:43:51-0800]
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

            let parsed: DateTime<FixedOffset> =
                DateTime::parse_from_rfc3339(&formatted_date).unwrap();

            let last_update_local = parsed.with_timezone(&Local);
            let now = Local::now();
            let duration = now.signed_duration_since(last_update_local);
            let seconds = duration.num_seconds().max(0);

            return Some(seconds);
        }

        None
    }

    fn get_upgrade_sizes(&self) -> UpgradeStats {
        let fail = UpgradeStats {
            download_size_mb: None,
            installed_size_mb: None,
            net_upgrade_size_mb: None,
            package_count: 0,
        };

        // ########## Creating alpm connection, getting sync db and local db #########

        let mut alpm = match Alpm::new("/", "/var/lib/pacman") {
            Ok(a) => a,
            Err(_) => return fail,
        };

        // Register sync databases
        let _ = alpm.register_syncdb_mut("core", alpm::SigLevel::NONE);
        let _ = alpm.register_syncdb_mut("extra", alpm::SigLevel::NONE);
        let _ = alpm.register_syncdb_mut("multilib", alpm::SigLevel::NONE);

        // Set NO_LOCK, avoids needing root
        if alpm.trans_init(alpm::TransFlag::NO_LOCK).is_err() {
            return fail;
        }

        // Add sysupgrade to transaction
        if alpm.sync_sysupgrade(false).is_err() {
            let _ = alpm.trans_release();
            return fail;
        }

        // Prepare the transaction
        if alpm.trans_prepare().is_err() {
            let _ = alpm.trans_release();
            return fail;
        }

        // Get local database for comparing old vs new sizes
        let localdb = alpm.localdb();

        let mut total_download_size: i64 = 0;
        let mut total_installed_size: i64 = 0;
        let mut net_upgrade_size: i64 = 0;
        let mut package_count: u32 = 0;

        // ########## comparing values/accumulating totals #########

        // Get packages to be upgraded/installed and calculate sizes
        for pkg in alpm.trans_add().into_iter() {
            package_count += 1;
            total_download_size += pkg.download_size();
            let new_size = pkg.isize();
            total_installed_size += new_size;

            // Check if this is an upgrade or new install
            if let Ok(oldpkg) = localdb.pkg(pkg.name()) {
                // upgrade: net size is difference
                let old_size = oldpkg.isize();
                net_upgrade_size += new_size - old_size;
            } else {
                // new install: add full size
                net_upgrade_size += new_size;
            }
        }

        // ############ Cleaning up handles/transaction and data ##########

        // Handle removals
        for pkg in alpm.trans_remove().into_iter() {
            net_upgrade_size -= pkg.isize();
        }

        // Release transaction
        let _ = alpm.trans_release();

        // Convert to MiB
        let download_mib = total_download_size as f64 / 1048576.0;
        let installed_mib = total_installed_size as f64 / 1048576.0;
        let mut net_mib = net_upgrade_size as f64 / 1048576.0;

        // Avoid -0.00 display issue
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

    fn get_orphaned_packages(&self) -> (Option<u32>, Option<f64>) {
        let alpm = match Alpm::new("/", "/var/lib/pacman") {
            Ok(a) => a,
            Err(_) => return (None, None),
        };

        let localdb = alpm.localdb();
        let mut count = 0;
        let mut total_size: i64 = 0;

        // Find packages installed as dependencies that nothing depends on
        for pkg in localdb.pkgs().into_iter() {
            // Check if installed as a dependency (not explicitly installed)
            if pkg.reason() == alpm::PackageReason::Depend {
                // Check if anything requires this package
                if pkg.required_by().len() == 0 && pkg.optional_for().len() == 0 {
                    count += 1;
                    total_size += pkg.isize();
                }
            }
        }

        let size_mb = total_size as f64 / 1048576.0;
        (Some(count), Some(size_mb))
    }

    fn get_cache_size(&self) -> Option<f64> {
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

    /// Parse /etc/pacman.d/mirrorlist to find the first active mirror
    fn get_mirror_url(&self) -> Option<String> {
        let mirrorlist = fs::read_to_string("/etc/pacman.d/mirrorlist").ok()?;

        for line in mirrorlist.lines() {
            let trimmed = line.trim();
            // Look for uncommented Server lines
            if trimmed.starts_with("Server = ") {
                let url = trimmed.strip_prefix("Server = ")?;
                let base_url = url.split("/$repo").next()?;
                return Some(base_url.to_string());
            }
        }
        None
    }

    /// Get pacman version
    fn get_pacman_version(&self) -> Option<String> {
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

    /// Test download speed with progress callback
    fn test_mirror_speed_with_progress_impl<F>(
        &self,
        mirror_url: &str,
        progress_callback: F,
    ) -> Option<f64>
    where
        F: Fn(u64),
    {
        use std::io::Read;

        let test_url = format!("{}/extra/os/x86_64/extra.files", mirror_url);

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .ok()?;

        let start = Instant::now();
        let mut response = client.get(&test_url).send().ok()?;

        if !response.status().is_success() {
            return None;
        }

        let total_size = response.content_length().unwrap_or(0);
        let mut downloaded: u64 = 0;
        let mut buffer = vec![0; 8192];

        loop {
            match response.read(&mut buffer) {
                Ok(0) => break, 
                Ok(n) => {
                    downloaded += n as u64;

                    // Calculate and report progress percentage
                    if total_size > 0 {
                        let progress = (downloaded * 100) / total_size;
                        progress_callback(progress);
                    }
                }
                Err(_) => return None,
            }
        }

        let duration = start.elapsed();
        let seconds = duration.as_secs_f64();

        if seconds > 0.0 {
            // Convert to MB/s
            Some(downloaded as f64 / seconds / 1_048_576.0)
        } else {
            None
        }
    }

    fn check_mirror_sync(&self, mirror_url: &str) -> Option<f64> {
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

    fn strip_ansi(s: &str) -> String {
        let mut result = String::new();
        let mut in_escape = false;
        for c in s.chars() {
            if c == '\x1b' {
                in_escape = true;
            } else if in_escape {
                if c == 'm' {
                    in_escape = false;
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    /// Filter for -Su output
    fn filter_upgrade_line(line: &str) -> bool {
        let clean = Self::strip_ansi(line);
        let trimmed = clean.trim();

        if trimmed.is_empty() {
            return false;
        }

        // Skip size summaries
        if trimmed.contains("Total Download Size:")
            || trimmed.contains("Total Installed Size:")
            || trimmed.contains("Net Upgrade Size:")
        {
            return false;
        }

        // Skip other messages
        if trimmed.contains("resolving dependencies")
            || trimmed.contains("looking for conflicting packages")
            || trimmed.contains(":: Starting full system upgrade...")
        {
            return false;
        }

        // Keep everything else
        true
    }

    /// Check if running as root
    fn is_root() -> bool {
        #[cfg(unix)]
        {
            unsafe { libc::geteuid() == 0 }
        }
        #[cfg(not(unix))]
        {
            false
        }
    }


    /// Filter output for run_pacman_pty
    fn should_print(line: &str, filter: bool) -> bool {
        if filter {
            Self::filter_upgrade_line(line)
        } else {
            true
        }
    }

    fn run_pacman_pty(args: &[&str], filter: bool) -> Result<(), String> {
        use std::io::Write;

        let cmd = format!("pacman {}", args.join(" "));
        let mut session =
            expectrl::spawn(&cmd).map_err(|e| format!("Failed to spawn pacman: {}", e))?;

        // Set pseudo terminal size to match actual terminal
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
                    if !line_buffer.is_empty() && Self::should_print(&line_buffer, filter) {
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
                            if Self::should_print(&line_buffer, filter) {
                                println!("{}", line_buffer);
                            }
                            line_buffer.clear();
                        } else if ch == '\r' {
                            if !line_buffer.is_empty() && Self::should_print(&line_buffer, filter) {
                                print!("\r{}", line_buffer);
                                let _ = stdout.flush();
                            }
                            line_buffer.clear();
                        } else {
                            line_buffer.push(ch);

                            // Check for prompts
                            if line_buffer.ends_with("[Y/n] ")
                                || (line_buffer.contains("::") && line_buffer.ends_with("]: "))
                            {
                                if Self::should_print(&line_buffer, filter) {
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

        if !line_buffer.is_empty() && Self::should_print(&line_buffer, filter) {
            println!("{}", line_buffer);
        }
        Ok(())
    }

    fn run_pacman_sync() -> Result<(), String> {
        if !Self::is_root() {
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
}

impl PackageManager for FetchPacmanStats {
    fn sync_databases(&self) -> Result<(), String> {
        Self::run_pacman_sync()
    }

    fn upgrade_system(&self, text_mode: bool, speed_test: bool, sync_first: bool) -> Result<(), String> {
        use std::sync::mpsc;

        // Check for root
        if !Self::is_root() {
            return Err("System upgrade requires root, rerun with sudo".to_string());
        }

        // 1: Optionally sync databases with spinner, then get stats
        if sync_first {
            Self::run_pacman_sync()?;
        }
        let spinner = crate::core::create_spinner("Gathering stats");
        let stats = self.get_stats(false);
        spinner.finish_and_clear();

        // 2: Display stats
        if text_mode {
            if speed_test {
                // Text mode with speed test
                let mirror = self.test_mirror_health();
                crate::ui::display_stats(&stats);
                crate::ui::display_mirror_health(&mirror, &stats);
            } else {
                // Text mode without speed test
                crate::ui::display_stats(&stats);
                println!();
            }
        } else {
            if speed_test {
                // Graphics mode with speed test
                if let Some(ref mirror_url) = stats.mirror_url {
                    let mirror_url = mirror_url.clone();
                    let (progress_tx, progress_rx) = mpsc::channel();
                    let (speed_tx, speed_rx) = mpsc::channel();

                    thread::spawn(move || {
                        let backend = FetchPacmanStats;
                        let speed =
                            backend.test_mirror_speed_with_progress(&mirror_url, |progress| {
                                let _ = progress_tx.send(progress);
                            });
                        let _ = speed_tx.send(speed);
                    });

                    if let Err(e) =
                        crate::ui::display_stats_with_graphics(&stats, progress_rx, speed_rx)
                    {
                        eprintln!("Error running TUI: {}", e);
                    }
                } else {
                    crate::ui::display_stats(&stats);
                    println!();
                }
            } else {
                // Graphics mode without speed test
                if let Err(e) = crate::ui::display_stats_with_graphics_no_speed(&stats) {
                    eprintln!("Error running graphics display: {}", e);
                    // Fall back to text mode
                    crate::ui::display_stats(&stats);
                    println!();
                }
            }
        }

        // 3: Run upgrade
        Self::run_pacman_pty(&["-Su"], true)
    }

    fn get_stats(&self, debug: bool) -> ManagerStats {
        let total_start = Instant::now();

        let start = Instant::now();
        let upgrade_stats = self.get_upgrade_sizes();
        if debug {
            eprintln!("Upgrade sizes + count: {:?}", start.elapsed());
        }

        let start = Instant::now();
        let (orphaned_count, orphaned_size) = self.get_orphaned_packages();
        if debug {
            eprintln!("Orphaned packages: {:?}", start.elapsed());
        }

        // Get mirror info
        let start = Instant::now();
        let mirror_url = self.get_mirror_url();
        if debug {
            eprintln!("Mirror URL: {:?}", start.elapsed());
        }

        let start = Instant::now();
        let mirror_sync_age = mirror_url
            .as_ref()
            .and_then(|url| self.check_mirror_sync(url));
        if debug {
            eprintln!("Mirror sync age: {:?}", start.elapsed());
        }

        let start = Instant::now();
        let total_installed = self.get_installed_count();
        if debug {
            eprintln!("Installed count: {:?}", start.elapsed());
        }

        let start = Instant::now();
        let days_since_last_update = self.get_seconds_since_update();
        if debug {
            eprintln!("Last update time: {:?}", start.elapsed());
        }

        let start = Instant::now();
        let cache_size_mb = self.get_cache_size();
        if debug {
            eprintln!("Cache size: {:?}", start.elapsed());
        }

        let start = Instant::now();
        let pacman_version = self.get_pacman_version();
        if debug {
            eprintln!("Pacman version: {:?}", start.elapsed());
        }

        if debug {
            eprintln!("TOTAL: {:?}", total_start.elapsed());
        }

        ManagerStats {
            total_installed,
            total_upgradable: upgrade_stats.package_count,
            days_since_last_update,
            download_size_mb: upgrade_stats.download_size_mb,
            total_installed_size_mb: upgrade_stats.installed_size_mb,
            net_upgrade_size_mb: upgrade_stats.net_upgrade_size_mb,
            orphaned_packages: orphaned_count,
            orphaned_size_mb: orphaned_size,
            cache_size_mb,
            mirror_url,
            mirror_sync_age_hours: mirror_sync_age,
            pacman_version,
        }
    }

    fn test_mirror_speed_with_progress<F>(
        &self,
        mirror_url: &str,
        progress_callback: F,
    ) -> Option<f64>
    where
        F: Fn(u64),
    {
        self.test_mirror_speed_with_progress_impl(mirror_url, progress_callback)
    }

    fn test_mirror_health(&self) -> Option<MirrorHealth> {
        let mirror_url = self.get_mirror_url()?;
        let speed = self.test_mirror_speed_with_progress_impl(&mirror_url, |_| {});
        let sync_age = self.check_mirror_sync(&mirror_url);

        Some(MirrorHealth {
            url: mirror_url,
            speed_mbps: speed,
            sync_age_hours: sync_age,
        })
    }
}
