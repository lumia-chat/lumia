//! Writes the spikes' store: `seed [--channels 10] [--messages 10000] [--dir PATH]`.
//! The defaults give the 100k local messages of architecture 17's cold-start row.

use std::path::PathBuf;
use std::time::Instant;

use lumia_spike_data as data;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |key: &str| args.iter().position(|a| a == key).and_then(|i| args.get(i + 1));
    let channels = value("--channels").and_then(|v| v.parse().ok()).unwrap_or(10);
    let messages = value("--messages").and_then(|v| v.parse().ok()).unwrap_or(10_000);
    let dir = value("--dir").map(PathBuf::from).unwrap_or_else(data::dir);
    let started = Instant::now();
    match data::seed(&dir, channels, messages) {
        Ok(manifest) => println!(
            "seeded {} channels of {messages} messages into {} in {:.1} s",
            manifest.channels.len(),
            dir.display(),
            started.elapsed().as_secs_f64(),
        ),
        Err(error) => {
            eprintln!("seed failed: {error}");
            std::process::exit(1);
        }
    }
}
