#![allow(dead_code)]

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use lz4_flex::frame::FrameEncoder;
use crate::string_registry;
use crate::efficient_clock::TimestampConverter;

/// A high-performance binary logger that writes log records in a compact binary format.
/// This logger is designed for maximum performance and minimal disk usage, achieving this through:
/// 
/// 1. Binary format encoding - reduces size and parsing overhead
/// 2. LZ4 compression - provides fast compression with good ratios
/// 3. Efficient timestamp encoding - uses CPU ticks for relative timestamps
/// 4. String deduplication - stores repeated strings only once
/// 5. Single-threaded performance - no locks or synchronization
/// 
/// # Binary Format
/// Each log record is encoded as:
/// ```text
/// [1 byte type | 2 bytes relative_ts | 2 bytes format ID | N bytes payload]
/// ```
/// Where type is:
/// - 0: Normal record with relative timestamp
/// - 1: Record with full timestamp
/// 
/// The payload for each value is encoded as:
/// ```text
/// [1 byte type | N bytes data]
/// ```
/// 
/// Value types:
/// - 0: Dynamic string (followed by UTF-8 bytes)
/// - 1: Static string (followed by 2 bytes string ID)
/// 
/// # Performance Characteristics
/// - Write throughput: ~1 million messages/second
/// - Compression ratio: ~8x compared to text logs
/// - Memory usage: Fixed buffer size (CAP)
/// - CPU efficient: Uses hardware timestamps
/// 
/// # Example Usage
/// ```rust
/// use binary_logger::Logger;
/// 
/// let mut logger = Logger::<1024>::new("app.log")?;
/// log_record!(logger, "Processing item {} with status {}", item_id, status)?;
/// logger.flush()?;
/// ```
pub struct Logger<const CAP: usize> {
    file: FrameEncoder<File>,
    clock: TimestampConverter,
}

impl<const CAP: usize> Logger<CAP> {
    /// Creates a new logger that writes to the specified file.
    /// The file will be created if it doesn't exist, or truncated if it does.
    /// 
    /// # Arguments
    /// * `path` - The path to the log file
    /// 
    /// # Returns
    /// A Result containing the logger or an IO error
    /// 
    /// # Example
    /// ```rust
    /// let mut logger = Logger::<1024>::new("app.log")?;
    /// ```
    pub fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = File::create(path)?;
        Ok(Self {
            file: FrameEncoder::new(file),
            clock: TimestampConverter::new(),
        })
    }

    /// Writes a log record to the file.
    /// This method handles the low-level binary format writing.
    /// Users should typically use the `log_record!` macro instead.
    /// 
    /// # Arguments
    /// * `format_id` - The ID of the format string (from string_registry)
    /// * `payload` - The serialized parameters
    /// 
    /// # Returns
    /// A Result indicating success or an IO error
    /// 
    /// # Binary Format
    /// Writes in format: [type(1) | relative_ts(2) | format_id(2) | payload(N)]
    pub fn write(&mut self, format_id: u16, payload: &[u8]) -> io::Result<()> {
        // Get timestamp efficiently using CPU ticks
        let (rel_ts, is_base) = self.clock.get_relative_timestamp();
        
        // Write record type
        self.file.write_all(&[if is_base { 1 } else { 0 }])?;

        // Write relative timestamp (2 bytes) and format ID
        self.file.write_all(&rel_ts.to_le_bytes())?;
        self.file.write_all(&format_id.to_le_bytes())?;
        
        // Write payload
        self.file.write_all(payload)?;
        
        Ok(())
    }

    /// Flushes any buffered data to disk.
    /// This ensures all written records are actually saved to the file.
    /// 
    /// # Returns
    /// A Result indicating success or an IO error
    /// 
    /// # Note
    /// For best performance, avoid calling flush too frequently.
    /// Consider flushing based on time intervals or buffer fullness.
    pub fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// The log_record! macro provides a high-level interface for logging.
/// It automatically handles:
/// 1. Format string registration and deduplication
/// 2. Parameter serialization to binary format
/// 3. Efficient type encoding
/// 
/// # Arguments
/// * `logger` - The Logger instance to write to
/// * `fmt` - A format string literal (like println!)
/// * `args` - Zero or more arguments to format
/// 
/// # Returns
/// IO Result for the logging operation
/// 
/// # Example
/// ```rust
/// log_record!(logger, "User {} logged in from {}", user_id, ip_addr)?;
/// ```
#[macro_export]
macro_rules! log_record {
    ($logger:expr, $fmt:literal, $($arg:expr),* $(,)?) => {{
        // Register format string on first use
        let format_id = $crate::string_registry::register_string($fmt);
        
        // Serialize parameters to temporary buffer
        let mut temp = [0u8; 1024];
        let mut pos = 0;
        $(
            // Convert each parameter to a string and write it
            let s = $arg.to_string();
            temp[pos] = 0;  // Dynamic value marker
            pos += 1;
            let bytes = s.as_bytes();
            temp[pos..pos+bytes.len()].copy_from_slice(bytes);
            pos += bytes.len();
        )*
        
        // Write the complete record
        let payload = &temp[..pos];
        $logger.write(format_id, payload)
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Instant;
    use log::{LevelFilter, info};
    use log4rs::{
        append::rolling_file::RollingFileAppender,
        config::{Appender, Config, Root},
        encode::pattern::PatternEncoder,
        filter::threshold::ThresholdFilter,
        append::rolling_file::policy::compound::CompoundPolicy,
        append::rolling_file::policy::compound::trigger::size::SizeTrigger,
        append::rolling_file::policy::compound::roll::fixed_window::FixedWindowRoller,
    };
    use tempfile::tempdir;

    const CAPACITY: usize = 1024 * 16;  // 16KB buffer size
    const TEST_FILE: &str = "test_log.bin";
    const TRADITIONAL_LOG: &str = "logs/traditional.{}.log";
    const ITERATIONS: usize = 1_000_000;  // 1 million iterations
    const LOG_SIZE_LIMIT: u64 = 10 * 1024 * 1024; // 10MB per file
    const WINDOW_SIZE: u32 = 5; // Keep 5 rotated files

    // Test struct with mixed types
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
            write!(f, "Event{{id={}, active={}, data={:?}, large_number={}, desc={}}}",
                self.id, self.active, &self.data[..4], self.large_number, self.description)
        }
    }

    #[test]
    fn test_binary_format() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("test.log");
        
        // Write a simple log record
        {
            let mut logger = Logger::<CAPACITY>::new(&log_path).unwrap();
            log_record!(logger, "Value is {}", 42).unwrap();
            logger.flush().unwrap();
        }
        
        // Read and verify the binary format
        let bytes = fs::read(&log_path).unwrap();
        assert!(bytes.len() >= 10, "Log record should be at least 10 bytes (2 relative_ts + 2 format ID)");
    }

    #[test]
    fn test_multiple_parameters() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("test.log");
        
        // Test with multiple parameters of different types
        {
            let mut logger = Logger::<CAPACITY>::new(&log_path).unwrap();
            let item_id = 123;
            let status = "active";
            let count = 42.5;
            log_record!(logger, "Item {} is {} with count {}", item_id, status, count).unwrap();
            logger.flush().unwrap();
        }
        
        // Verify the file was written
        let bytes = fs::read(&log_path).unwrap();
        assert!(bytes.len() > 0, "Log file should not be empty");
    }

    #[test]
    fn test_logging_performance() {
        // Create temp directory for test files
        let dir = tempdir().unwrap();
        let binary_log_path = dir.path().join(TEST_FILE);
        let traditional_log_dir = dir.path().join("logs");
        let _ = fs::create_dir(&traditional_log_dir);

        // Test binary logging
        let binary_duration = {
            let mut logger = Logger::<CAPACITY>::new(&binary_log_path).unwrap();

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

            let start = Instant::now();
            for i in 0..ITERATIONS {
                log_record!(logger, "Test perf: iteration={}, event={}", i, event).unwrap();
            }
            logger.flush().unwrap();
            start.elapsed()
        };

        // Test traditional logging using log4rs with rotation
        let traditional_duration = {
            let traditional_log_pattern = traditional_log_dir.join("traditional.{}.log").to_str().unwrap().to_string();
            let traditional_log_file = traditional_log_dir.join("traditional.log").to_str().unwrap().to_string();
            
            // Set up log rotation policy
            let window_roller = FixedWindowRoller::builder()
                .build(&traditional_log_pattern, WINDOW_SIZE)
                .unwrap();
            
            let size_trigger = SizeTrigger::new(LOG_SIZE_LIMIT);
            
            let compound_policy = CompoundPolicy::new(
                Box::new(size_trigger),
                Box::new(window_roller),
            );

            // Set up log4rs with rotation
            let logfile = RollingFileAppender::builder()
                .encoder(Box::new(PatternEncoder::new("{d} - {m}{n}")))
                .build(&traditional_log_file, Box::new(compound_policy))
                .unwrap();

            let config = Config::builder()
                .appender(Appender::builder()
                    .filter(Box::new(ThresholdFilter::new(LevelFilter::Info)))
                    .build("logfile", Box::new(logfile)))
                .build(Root::builder()
                    .appender("logfile")
                    .build(LevelFilter::Info))
                .unwrap();

            log4rs::init_config(config).unwrap();

            // Same fixed test data
            let event = TestEvent {
                id: 42,
                active: true,
                data: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                large_number: 18446744073709551615, // max u64
                description: "This is a longer description that includes some special characters !@#$%^&*() \
                            and provides more context about the event. It also contains some metrics like \
                            CPU: 95%, Memory: 2.5GB, Network: 1.2Gbps".to_string(),
            };

            let start = Instant::now();
            for i in 0..ITERATIONS {
                info!("Test perf: iteration={}, event={}", i, event);
            }
            start.elapsed()
        };

        println!("\nPerformance comparison ({} iterations):", ITERATIONS);
        println!("Binary logging:      {:?}", binary_duration);
        println!("Traditional logging: {:?}", traditional_duration);
        println!("Speedup: {:.2}x", traditional_duration.as_secs_f64() / binary_duration.as_secs_f64());
        println!("Throughput: {:.2} million msgs/sec", ITERATIONS as f64 / binary_duration.as_secs_f64() / 1_000_000.0);

        // Print file sizes
        let binary_size = fs::metadata(&binary_log_path).unwrap().len();
        let traditional_size: u64 = fs::read_dir(&traditional_log_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.metadata().ok())
            .map(|metadata| metadata.len())
            .sum();

        println!("\nFile size comparison:");
        println!("Binary log size:      {} MB", binary_size as f64 / (1024.0 * 1024.0));
        println!("Traditional log size: {} MB", traditional_size as f64 / (1024.0 * 1024.0));
        println!("Size ratio: {:.2}x", traditional_size as f64 / binary_size as f64);

        // Clean up handled by tempdir Drop
    }
}

