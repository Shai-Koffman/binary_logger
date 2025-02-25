#![allow(unused)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use binary_logger::{Logger, log_record, BufferHandler};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::sync::{Arc, Mutex, Once};
use std::time::Instant;
use tempfile::tempdir;
use log::{info, LevelFilter};
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Config, Root},
    encode::pattern::PatternEncoder,
};

const TEST_FILE: &str = "perf_test.log";
const BUFFER_SIZE: usize = 4 * 1024 * 1024; // 4MB buffer
const NUM_BUFFER_FILLS: usize = 4; // Fill buffer 4 times
// Calculate iterations to fill buffer 4 times (approximate based on typical record size)
const RECORD_SIZE_ESTIMATE: usize = 256; // Estimated bytes per record
const ITERATIONS: usize = (BUFFER_SIZE * NUM_BUFFER_FILLS) / RECORD_SIZE_ESTIMATE;

static LOGGER_INIT: Once = Once::new();

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

// Handler that does nothing - for measuring pure in-memory performance
struct NullHandler;

impl BufferHandler for NullHandler {
    fn handle_switched_out_buffer(&self, _buffer: *const u8, _size: usize) {
        // Do nothing - we're only measuring in-memory performance
    }
}

fn setup_log4rs(log_file: &str) {
    LOGGER_INIT.call_once(|| {
        let logfile = FileAppender::builder()
            .encoder(Box::new(PatternEncoder::new("{d} - {m}{n}")))
            .append(true)
            .build(log_file)
            .unwrap();

        let config = Config::builder()
            .appender(Appender::builder()
                .filter(Box::new(log4rs::filter::threshold::ThresholdFilter::new(LevelFilter::Info)))
                .build("logfile", Box::new(logfile)))
            .build(Root::builder()
                .appender("logfile")
                .build(LevelFilter::Info))
            .unwrap();

        log4rs::init_config(config).unwrap();
    });
}

fn bench_logging_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("Logging Comparison");
    group.sample_size(10); // Fewer samples due to I/O operations
    
    group.bench_function("binary_vs_traditional", |b| {
        b.iter(|| {
            // Clean up any existing test files
            let _ = fs::remove_file(TEST_FILE);
            let _ = fs::remove_dir_all("logs");
            let _ = fs::create_dir("logs");
            
            // Create temp directory for test files
            let dir = tempdir().unwrap();
            let traditional_log_dir = dir.path().join("logs");
            let _ = fs::create_dir(&traditional_log_dir);

            // Fixed test data with more complexity
            let event = TestEvent {
                id: 42,
                active: true,
                data: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                large_number: 18446744073709551615, // max u64
                description: "This is a longer description that includes some special characters !@#$%^&*() \
                            and provides more context about the event. It also contains some metrics like \
                            CPU: 95%, Memory: 2.5GB, Network: 1.2Gbps".to_string(),
            };

            // Binary logging test - using NullHandler to measure only in-memory performance
            let handler = NullHandler;
            let mut logger = Logger::<BUFFER_SIZE>::new(handler);

            let binary_start = Instant::now();
            for i in 0..ITERATIONS {
                log_record!(logger, "Test perf: iteration={}, event={}", i, event).unwrap();
            }
            logger.flush();
            let binary_duration = binary_start.elapsed();

            // Test traditional logging using log4rs - measuring full I/O performance
            let traditional_log_file = traditional_log_dir.join("traditional.log").to_str().unwrap().to_string();
            setup_log4rs(&traditional_log_file);

            // Traditional logging test - measuring full I/O performance
            let traditional_start = Instant::now();
            for i in 0..ITERATIONS {
                info!("Test perf: iteration={}, event={}", i, event);
            }
            let traditional_duration = traditional_start.elapsed();

            println!("\nPerformance comparison ({} iterations, {} buffer fills of {} MB):", 
                    ITERATIONS, NUM_BUFFER_FILLS, BUFFER_SIZE as f64 / (1024.0 * 1024.0));
            println!("Binary logging (in-memory): {:?}", binary_duration);
            println!("Traditional logging (with I/O): {:?}", traditional_duration);
            println!("Speedup: {:.2}x", traditional_duration.as_secs_f64() / binary_duration.as_secs_f64());
            println!("Binary throughput: {:.2} million msgs/sec", 
                    ITERATIONS as f64 / binary_duration.as_secs_f64() / 1_000_000.0);

            black_box((binary_duration, traditional_duration))
        });
    });

    group.finish();
}

criterion_group!(benches, bench_logging_comparison);
criterion_main!(benches); 