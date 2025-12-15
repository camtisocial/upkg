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
    ui::draw_ui();
}
