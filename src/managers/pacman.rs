use crate::managers::{ManagerStats, MirrorHealth, PackageManager};
use alpm::Alpm;
use chrono::{DateTime, FixedOffset, Local};
use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Instant;

pub struct FetchPacmanStats;

impl FetchPacmanStats {
    /// Get the count of installed packages using pacman -Q
    fn get_installed_count(&self) -> u32 {
        let output = Command::new("pacman").arg("-Q").output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().count() as u32
    }

    /// Get the count of upgradable packages using checkupdates
    fn get_upgradable_count(&self) -> u32 {
        let output = Command::new("pacman").arg("-Qu").output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().count() as u32
    }

    /// get time since last update from /var/log/pacman.log
    /// returns seconds
    fn get_seconds_since_update(&self) -> Option<i64> {
        /*
        This is checking the log for the last time the user ran pacman -Syu, and packages
        actually installed thereafter, that way we are actually returning last time updated
        rathern than last time -Syu was run, ideally. kind of janky
        */

        let contents =
            fs::read_to_string("/var/log/pacman.log").expect("Failed to read pacman.log");

        let mut saw_syu = false;
        let mut saw_sync = false;
        let mut saw_upgrade_start = false;
        let mut saw_alpm = false;
        let mut block_timestamp: Option<String> = None;
        let mut last_valid_timestamp: Option<String> = None;

        for line in contents.lines() {
            let trimmed = line.trim();

            // Extract timestamp of any line
            // Format: [2025-12-05T15:43:51-0800] ...
            let timestamp = trimmed
                .split(']')
                .next()
                .map(|x| x.trim_start_matches('['))
                .unwrap_or("");

            // look for start of syu block, then alpm lines to make sure the update actually
            // started
            if trimmed.contains("Running 'pacman -Syu'") {
                // reset tracking
                saw_syu = true;
                saw_sync = false;
                saw_upgrade_start = false;
                saw_alpm = false;
                block_timestamp = Some(timestamp.to_string());
                continue;
            }

            if saw_syu && trimmed.contains("synchronizing package lists") {
                saw_sync = true;
                continue;
            }

            if saw_sync && trimmed.contains("starting full system upgrade") {
                saw_upgrade_start = true;
                continue;
            }

            if saw_upgrade_start && trimmed.contains("[ALPM]") {
                saw_alpm = true;
            }

            // If we start another pacman run before ALPM, the previous wasn't real
            if trimmed.contains("[PACMAN] Running") && !trimmed.contains("pacman -Syu") {
                // restart
                saw_syu = false;
                saw_sync = false;
                saw_upgrade_start = false;
                saw_alpm = false;
                block_timestamp = None;
                continue;
            }

            // if the block is complete, update last_valid_timestamp
            if saw_syu && saw_sync && saw_upgrade_start && saw_alpm {
                last_valid_timestamp = block_timestamp.clone();
            }
        }

        if let Some(ts) = last_valid_timestamp {
            // confert to RFC3339
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

    fn get_upgrade_sizes(&self) -> (Option<f64>, Option<f64>, Option<f64>) {

        // ########## Creating alpm connection, getting sync db and local db #########

        let mut alpm = match Alpm::new("/", "/var/lib/pacman") {
            Ok(a) => a,
            Err(_) => return (None, None, None),
        };

        // Register sync databases
        let _ = alpm.register_syncdb_mut("core", alpm::SigLevel::NONE);
        let _ = alpm.register_syncdb_mut("extra", alpm::SigLevel::NONE);
        let _ = alpm.register_syncdb_mut("multilib", alpm::SigLevel::NONE);

        // Set NO_LOCK, avoids needing root
        if alpm.trans_init(alpm::TransFlag::NO_LOCK).is_err() {
            return (None, None, None);
        }

        // Add sysupgrade to transaction
        if alpm.sync_sysupgrade(false).is_err() {
            let _ = alpm.trans_release();
            return (None, None, None);
        }

        // Prepare the transaction
        if alpm.trans_prepare().is_err() {
            let _ = alpm.trans_release();
            return (None, None, None);
        }

        // Get local database for comparing old vs new sizes
        let localdb = alpm.localdb();

        let mut total_download_size: i64 = 0;
        let mut total_installed_size: i64 = 0;
        let mut net_upgrade_size: i64 = 0;


        // ########## comparing values/accumulating totals #########

        // Get packages to be upgraded and calculate sizes
        for pkg in alpm.trans_add().into_iter() {
            total_download_size += pkg.download_size();
            let new_size = pkg.isize();
            total_installed_size += new_size;

            // Check if this is an upgrade
            if let Ok(oldpkg) = localdb.pkg(pkg.name()) {
                // upgrade
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

        (Some(download_mib), Some(installed_mib), Some(net_mib))
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

    /// test download speed with test file (extra.files)
    fn test_mirror_speed(&self, mirror_url: &str) -> Option<f64> {
        self.test_mirror_speed_with_progress_impl(mirror_url, |_| {})
    }

    /// test download speed with progress callback (private implementation)
    fn test_mirror_speed_with_progress_impl<F>(&self, mirror_url: &str, progress_callback: F) -> Option<f64>
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

        // Get content length for progress calculation
        let total_size = response.content_length().unwrap_or(0);

        // Stream the download in chunks and track progress
        let mut downloaded: u64 = 0;
        let mut buffer = vec![0; 8192];

        loop {
            match response.read(&mut buffer) {
                Ok(0) => break, // EOF
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

    /// Check the lastsync
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

        // Convert Unix timestamp to hours ago
        let now = Local::now().timestamp();
        let age_seconds = now - timestamp;
        let age_hours = age_seconds as f64 / 3600.0;

        Some(age_hours.max(0.0))
    }

    /// Filter pacman output to remove information already shown by upkg
    fn filter_pacman_line(line: &str) -> bool {
        let trimmed = line.trim();

        // Always keep empty lines (they provide spacing)
        if trimmed.is_empty() {
            return true;
        }

        // Skip size summaries (already shown by upkg)
        if trimmed.starts_with("Total Download Size:") ||
           trimmed.starts_with("Total Installed Size:") ||
           trimmed.starts_with("Net Upgrade Size:") {
            return false;
        }

        // Skip sync/resolution messages
        if trimmed == "resolving dependencies..." ||
           trimmed == "looking for conflicting packages..." {
            return false;
        }

        // Skip database sync messages
        // "core downloading...", "extra downloading...", "multilib downloading..."
        if trimmed.starts_with("core downloading") ||
           trimmed.starts_with("extra downloading") ||
           trimmed.starts_with("multilib downloading") {
            return false;
        }

        // Skip database sync progress (core, extra, multilib)
        // These look like: " core     123.4 KiB  100 KiB/s 00:01 [----] 100%"
        // if (trimmed.starts_with("core ") ||
        //     trimmed.starts_with("extra ") ||
        //     trimmed.starts_with("multilib ")) &&
        //    trimmed.contains("KiB") &&
        //    trimmed.contains("%") {
        //     return false;
        // }

        // Skip other :: messages EXCEPT the proceed prompt
        // if trimmed.starts_with(":: ") &&
        //    !trimmed.contains("Proceed with installation") {
        //     return false;
        // }

        // Keep everything else
        true
    }

    /// Read from a stream and print filtered lines
    fn filter_and_print_stream<R: std::io::Read>(reader: R) {
        let buf_reader = BufReader::new(reader);
        for line in buf_reader.lines() {
            if let Ok(line) = line {
                if Self::filter_pacman_line(&line) {
                    println!("{}", line);
                }
            }
        }
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

}

impl PackageManager for FetchPacmanStats {
    fn sync_databases(&self) -> Result<(), String> {
        // requires root
        let output = Command::new("pacman")
            .arg("-Sy")
            .output()
            .map_err(|e| format!("Failed to execute pacman: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("pacman -Sy failed: {}", stderr.trim()));
        }

        Ok(())
    }

    fn upgrade_system(&self, text_mode: bool, speed_test: bool) -> Result<(), String> {
        use std::sync::mpsc;

        // Check for root
        if !Self::is_root() {
            return Err("Sys upgrade requires root access, rerun with sudo".to_string());
        }

        // Get and display stats
        let stats = self.get_stats();
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
                        let speed = backend.test_mirror_speed_with_progress(&mirror_url, |progress| {
                            let _ = progress_tx.send(progress);
                        });
                        let _ = speed_tx.send(speed);
                    });

                    if let Err(e) = crate::ui::display_stats_with_graphics(&stats, progress_rx, speed_rx) {
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

        // Capture stdout/stderr, keep stdin for y/n prompt
        let mut cmd = Command::new("pacman")
            .arg("-Syu")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::inherit())  // Keep stdin for interactive prompt
            .spawn()
            .map_err(|e| format!("Failed to execute pacman: {}", e))?;

        // Get stdout/stderr handles
        let stdout = cmd.stdout.take()
            .ok_or("Failed to capture stdout")?;
        let stderr = cmd.stderr.take()
            .ok_or("Failed to capture stderr")?;

        // Spawn threads to filter and print output
        let stdout_thread = thread::spawn(move || {
            Self::filter_and_print_stream(stdout);
        });

        let stderr_thread = thread::spawn(move || {
            Self::filter_and_print_stream(stderr);
        });

        // Wait for pacman to finis
        let status = cmd.wait()
            .map_err(|e| format!("Failed to wait for pacman: {}", e))?;

        // Wait for output threads to finish
        stdout_thread.join().unwrap();
        stderr_thread.join().unwrap();

        if status.success() {
            Ok(())
        } else {
            Err(format!("pacman -Syu exited with status: {}", status))
        }
    }

    fn get_stats(&self) -> ManagerStats {
        let (download_size, total_installed_size, net_upgrade_size) = self.get_upgrade_sizes();
        let (orphaned_count, orphaned_size) = self.get_orphaned_packages();

        // Get mirror info
        let mirror_url = self.get_mirror_url();
        let mirror_sync_age = mirror_url
            .as_ref()
            .and_then(|url| self.check_mirror_sync(url));

        ManagerStats {
            total_installed: self.get_installed_count(),
            total_upgradable: self.get_upgradable_count(),
            days_since_last_update: self.get_seconds_since_update(),
            download_size_mb: download_size,
            total_installed_size_mb: total_installed_size,
            net_upgrade_size_mb: net_upgrade_size,
            orphaned_packages: orphaned_count,
            orphaned_size_mb: orphaned_size,
            cache_size_mb: self.get_cache_size(),
            mirror_url,
            mirror_sync_age_hours: mirror_sync_age,
            pacman_version: self.get_pacman_version(),
        }
    }

    fn test_mirror_speed_with_progress<F>(&self, mirror_url: &str, progress_callback: F) -> Option<f64>
    where
        F: Fn(u64),
    {
        self.test_mirror_speed_with_progress_impl(mirror_url, progress_callback)
    }

    fn test_mirror_health(&self) -> Option<MirrorHealth> {
        let mirror_url = self.get_mirror_url()?;
        let speed = self.test_mirror_speed(&mirror_url);
        let sync_age = self.check_mirror_sync(&mirror_url);

        Some(MirrorHealth {
            url: mirror_url,
            speed_mbps: speed,
            sync_age_hours: sync_age,
        })
    }
}
