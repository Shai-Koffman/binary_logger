#![allow(unused)]
use binary_logger::{Logger, log_record, BufferHandler};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::sync::{Arc, Mutex, Once};
use std::time::Instant;
use log::{info, LevelFilter};
use log4rs::{
    append::rolling_file::{RollingFileAppender, policy::compound::CompoundPolicy,
        policy::compound::trigger::size::SizeTrigger,
        policy::compound::roll::fixed_window::FixedWindowRoller},
    config::{Appender, Config, Root},
    encode::pattern::PatternEncoder,
};
use std::sync::mpsc::{channel, Sender};
use std::thread;
use lz4::EncoderBuilder;

const BUFFER_SIZE: usize = 4 * 1024 * 1024; // 4MB buffer
const NUM_BUFFER_FILLS: usize = 4; // Fill buffer 4 times
// Calculate iterations to fill buffer 4 times (approximate based on typical record size)
const RECORD_SIZE_ESTIMATE: usize = 256; // Estimated bytes per record
const ITERATIONS: usize = (BUFFER_SIZE * NUM_BUFFER_FILLS) / RECORD_SIZE_ESTIMATE;

static LOGGER_INIT: Once = Once::new();
static mut LOG4RS_HANDLE: Option<log4rs::Handle> = None;

#[derive(Debug)]
struct TestEvent {
    id: i32,
    active: bool,
    data: [u8; 16],
    large_number: u64,
    description: String,
}

impl std::fmt::Display for TestEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Event[id={}, active={}, data={:?}, large_number={}, desc={}]",
            self.id, self.active, self.data, self.large_number, self.description)
    }
}

struct FileBufferHandler {
    sender: Sender<Vec<u8>>,
}

impl FileBufferHandler {
    fn new(output_file: &str) -> Self {
        let (sender, receiver) = channel::<Vec<u8>>();
        let file_path = output_file.to_string();
        
        thread::spawn(move || {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&file_path)
                .unwrap();
                
            // Create LZ4 encoder with high compression level
            let mut encoder = EncoderBuilder::new()
                .level(4)
                .build(file)
                .unwrap();
            
            while let Ok(buffer) = receiver.recv() {
                let _ = encoder.write_all(&buffer);
                let _ = encoder.flush();
            }
            
            // Finish the encoder when the channel is closed
            let _ = encoder.finish().1;
        });

        FileBufferHandler { sender }
    }
}

impl BufferHandler for FileBufferHandler {
    fn handle_switched_out_buffer(&self, buffer: *const u8, size: usize) {
        // Safely copy the buffer to a new Vec
        let buffer_copy = unsafe {
            let slice = std::slice::from_raw_parts(buffer, size);
            slice.to_vec()
        };
        let _ = self.sender.send(buffer_copy);
    }
}

fn cleanup_files() {
    // Clean up ALL log files before starting
    for entry in fs::read_dir(".").unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let path_str = path.to_string_lossy();
        if path_str.contains("traditional.") || path_str.contains("log.bin") {
            let _ = fs::remove_file(path);
        }
    }
}

fn setup_log4rs() -> log4rs::Config {
    // Set up the rolling policy (4MB per file, keep 5 files)
    let trigger = Box::new(SizeTrigger::new(4 * 1024 * 1024));
    let roller = Box::new(
        FixedWindowRoller::builder()
            .build("traditional.{}.log", 5)
            .unwrap()
    );
    let policy = Box::new(CompoundPolicy::new(trigger, roller));

    // Create rolling file appender
    let file_appender = RollingFileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{m}{n}")))
        .build("traditional.log", policy)
        .unwrap();

    // Build and return the config
    Config::builder()
        .appender(Appender::builder().build("file", Box::new(file_appender)))
        .build(Root::builder()
            .appender("file")
            .build(LevelFilter::Info))
        .unwrap()
}

fn calculate_statistics(times: &[f64]) -> (f64, f64, f64, f64) {
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let variance = times.iter()
        .map(|x| (x - mean).powi(2))
        .sum::<f64>() / times.len() as f64;
    let std_dev = variance.sqrt();
    let min = times.iter().fold(f64::INFINITY, |a, &b| a.min(b));
    let max = times.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
    (mean, std_dev, min, max)
}

fn main() {
    // Initialize logger once at the start
    let config = setup_log4rs();
    let handle = log4rs::init_config(config).unwrap();

    const NUM_RUNS: usize = 10;
    let mut binary_times = Vec::with_capacity(NUM_RUNS);
    let mut traditional_times = Vec::with_capacity(NUM_RUNS);

    println!("\nRunning {} iterations of performance comparison:", NUM_RUNS);
    println!("({} iterations per run, {} buffer fills of {} MB)\n", 
             ITERATIONS, NUM_BUFFER_FILLS, BUFFER_SIZE as f64 / (1024.0 * 1024.0));

    for run in 1..=NUM_RUNS {
        println!("Run {}:", run);
        
        // Clean up ALL files before starting
        cleanup_files();
        
        // Reconfigure logger with fresh config
        handle.set_config(setup_log4rs());
        
        // Fixed test data with more complexity
        let event = TestEvent {
            id: 42,
            active: true,
            data: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            large_number: 18446744073709551615,
            description: "This is a longer description that includes some special characters !@#$%^&*() \
                        and provides more context about the event. It also contains some metrics like \
                        CPU: 95%, Memory: 2.5GB, Network: 1.2Gbps".to_string(),
        };

        // Binary logging test with file output
        let handler = FileBufferHandler::new("log.bin");
        let mut logger = Logger::<BUFFER_SIZE>::new(handler);

        let binary_start = Instant::now();
        for i in 0..ITERATIONS {
            log_record!(logger, "Test perf: iteration={}, event={}", i, event).unwrap();
        }
        logger.flush();
        drop(logger); // Ensure logger is dropped and flushed
        let binary_duration = binary_start.elapsed();
        binary_times.push(binary_duration.as_secs_f64() * 1000.0); // Convert to ms

        let traditional_start = Instant::now();
        for i in 0..ITERATIONS {
            info!("Test perf: iteration={}, event={}", i, event);
        }
        // Force flush by reconfiguring the logger
        handle.set_config(setup_log4rs());
        let traditional_duration = traditional_start.elapsed();
        traditional_times.push(traditional_duration.as_secs_f64() * 1000.0); // Convert to ms

        // Wait longer to ensure all writes complete
        thread::sleep(std::time::Duration::from_secs(2));
        
        // Sum up all binary log files
        let mut total_binary_size = 0;
        for entry in fs::read_dir(".").unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let path_str = path.to_string_lossy();
            if path_str.contains("log.bin") {
                total_binary_size += fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            }
        }

        // Sum up all traditional log files
        let mut total_traditional_size = 0;
        for entry in fs::read_dir(".").unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let path_str = path.to_string_lossy();
            if path_str.contains("traditional.") {
                total_traditional_size += fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            }
        }

        println!("Binary logging: {:.6}ms", binary_duration.as_secs_f64() * 1000.0);
        println!("Traditional logging: {:.6}ms", traditional_duration.as_secs_f64() * 1000.0);
        println!("Binary log size: {:.2} MB", total_binary_size as f64 / (1024.0 * 1024.0));
        println!("Traditional log size: {:.2} MB", total_traditional_size as f64 / (1024.0 * 1024.0));
        println!("Size ratio: {:.2}x\n", total_traditional_size as f64 / total_binary_size as f64);
    }

    // Calculate and display statistics
    let (binary_mean, binary_std, binary_min, binary_max) = calculate_statistics(&binary_times);
    let (trad_mean, trad_std, trad_min, trad_max) = calculate_statistics(&traditional_times);

    println!("\nFinal Statistics:");
    println!("Binary logging:");
    println!("  Mean: {:.3} ms", binary_mean);
    println!("  Std Dev: {:.3} ms ({:.1}% of mean)", binary_std, (binary_std/binary_mean)*100.0);
    println!("  Min: {:.3} ms", binary_min);
    println!("  Max: {:.3} ms", binary_max);
    println!("  Range: {:.3} ms", binary_max - binary_min);
    
    println!("\nTraditional logging:");
    println!("  Mean: {:.3} ms", trad_mean);
    println!("  Std Dev: {:.3} ms ({:.1}% of mean)", trad_std, (trad_std/trad_mean)*100.0);
    println!("  Min: {:.3} ms", trad_min);
    println!("  Max: {:.3} ms", trad_max);
    println!("  Range: {:.3} ms", trad_max - trad_min);

    println!("\nAverage speedup: {:.1}x", trad_mean / binary_mean);
    println!("Speedup range: {:.1}x to {:.1}x", trad_min / binary_max, trad_max / binary_min);
}

// Remove criterion-related code
// ... existing code ... 