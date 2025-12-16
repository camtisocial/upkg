mod core;
mod managers;
mod ui;

fn main() {
    //checking for flags
    let args: Vec<String> = std::env::args().collect();
    let local_mode = args.contains(&"--local".to_string()) || args.contains(&"-l".to_string());

    if local_mode {
        println!("[LOCAL]");
    } else {
        println!("[SYNC]");
    }

    println!();

    // local stat gathering and display
    println!("[1/2] Gathering local stats...");
    let stats = core::get_manager_stats();
    ui::display_stats(&stats);

    // SLOW operations - network requests
    // In the future, this will run in a background thread
    // and update the UI with a progress bar
    println!("\n[2/2] Testing mirror health...");
    let mirror = core::test_mirror_health();
    ui::display_mirror_health(&mirror, stats.download_size_mb);
}
