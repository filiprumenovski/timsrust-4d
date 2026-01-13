//! Example: Reading a complete Bruker TDF dataset with metadata
//!
//! This example demonstrates how to open a Bruker TimsTOF data file (.d directory)
//! and access frames, peak data, and metadata including MALDI imaging information.
//!
//! Run with: cargo run --example read_tdf -- <path-to-data.d>

use std::env;
use std::path::Path;

use timsrust::readers::FrameReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <path-to-data.d>", args[0]);
        eprintln!("\nExample: {} data/sample.d", args[0]);
        std::process::exit(1);
    }

    let data_path = &args[1];
    if !Path::new(data_path).exists() {
        eprintln!("Error: Path does not exist: {}", data_path);
        std::process::exit(1);
    }

    println!("Opening TDF dataset: {}", data_path);

    // Open the frame reader
    let reader = FrameReader::new(data_path)?;

    // Print basic statistics
    println!("\n=== Dataset Information ===");
    println!("Total frames: {}", reader.len());
    println!("Acquisition type: {:?}", reader.get_acquisition());
    println!("Is MALDI imaging: {}", reader.is_maldi());

    // Get MS1 and MS2 frames
    let ms1_frames = reader.get_all_ms1();
    let ms2_frames = reader.get_all_ms2();
    println!("MS1 frames: {}", ms1_frames.len());
    println!("MS2 frames: {}", ms2_frames.len());

    // Analyze first few frames
    println!("\n=== First 5 Frames ===");
    for i in 0..std::cmp::min(5, reader.len()) {
        match reader.get(i) {
            Ok(frame) => {
                println!(
                    "\nFrame {}: rt={:.2}s, {} peaks",
                    i,
                    frame.rt_in_seconds,
                    frame.intensities.len()
                );

                // Show MALDI info if present
                if let Some(maldi) = &frame.maldi_info {
                    println!(
                        "  MALDI: pixel ({}, {}), spot: {}",
                        maldi.pixel_x, maldi.pixel_y, maldi.spot_name
                    );

                    if let Some(pos_x) = maldi.position_x_um {
                        println!("  Position: ({:.2} µm, {:.2} µm)", pos_x, maldi.position_y_um.unwrap_or(0.0));
                    }

                    if let Some(power) = maldi.laser_power {
                        println!("  Laser power: {:.1}%", power);
                    }
                }

                // Show peak statistics
                if !frame.intensities.is_empty() {
                    let max_intensity = frame
                        .intensities
                        .iter()
                        .max_by_key(|&&v| v)
                        .unwrap_or(&0);
                    println!("  Max intensity: {}", max_intensity);
                }
            }
            Err(e) => println!("  Error reading frame {}: {}", i, e),
        }
    }

    // Get DIA windows if available
    if let Some(windows) = reader.get_dia_windows() {
        println!("\n=== DIA Windows ===");
        println!("Window configurations: {}", windows.len());
        for (i, window) in windows.iter().take(3).enumerate() {
            println!("  Window {}: {:?}", i, window);
        }
    }

    println!("\n✓ Successfully read TDF dataset");
    Ok(())
}
