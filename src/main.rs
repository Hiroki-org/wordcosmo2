mod config;
mod core;
mod render;
mod spatial;
mod types;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ui::run()
}
