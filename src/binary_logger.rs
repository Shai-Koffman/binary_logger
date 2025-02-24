#![allow(dead_code)]

use crate::efficient_clock::TimestampConverter;
use crate::loggable::Loggable;
use crate::log_format_registry::FormatInfo;
use std::sync::mpsc::{self, Sender, Receiver};
use std::sync::Arc;
use std::thread;
use std::sync::Mutex;

/// Conversion factor: how many CPU ticks per relative timestamp unit.
const TICKS_PER_UNIT: u64 = 30_000;
/// Maximum value that can be stored in 16 bits.
const REL_MAX: u64 = u16::MAX as u64;

/// The dump callback receives a reference to the flushed fixed‐buffer slice.
pub type DumpCallback = Arc<dyn Fn(&[u8]) + Send + Sync>;

/// Message sent to the background thread - just the buffer and its length
enum FlushMessage {
    Data(Box<[u8]>),
    Shutdown,
}

/// The Logger is implemented with fixed-size buffers (no dynamic allocation at log time).
/// The log record format (for normal records) is:
/// [1 byte type (0) | 2 bytes relative timestamp | 2 bytes log id | 2 bytes payload length | payload bytes]
///
/// A full timestamp record (record type 1) is written at the start of a dump (or if needed):
/// [1 byte type (1) | 8 bytes full timestamp]
pub struct Logger<const CAP: usize> {
    buffer_a: Box<[u8; CAP]>,
    pos_a: usize,
    buffer_b: Box<[u8; CAP]>,
    pos_b: usize,
    active_buffer: bool,
    dump_callback: Option<DumpCallback>,
    timestamp_converter: TimestampConverter,
    flush_sender: Sender<FlushMessage>,
}

impl<const CAP: usize> Logger<CAP>
where
    [u8; CAP]:,
{
    pub fn new(dump_callback: Option<DumpCallback>) -> Self {
        let (tx, rx) = mpsc::channel();
        
        if let Some(cb) = dump_callback.clone() {
            thread::spawn(move || {
                Self::flush_thread(rx, cb);
            });
        }

        Self {
            buffer_a: Box::new([0; CAP]),
            pos_a: 0,
            buffer_b: Box::new([0; CAP]),
            pos_b: 0,
            active_buffer: true,
            dump_callback,
            timestamp_converter: TimestampConverter::new(),
            flush_sender: tx,
        }
    }

    fn flush_thread(rx: Receiver<FlushMessage>, callback: DumpCallback) {
        while let Ok(msg) = rx.recv() {
            match msg {
                FlushMessage::Data(data) => {
                    callback(&data);
                }
                FlushMessage::Shutdown => break,
            }
        }
    }

    #[inline(always)]
    pub fn log_record(&mut self, log_id: u16, payload: &[u8]) {
        let (delta_u16, needs_timestamp) = self.timestamp_converter.get_relative_timestamp();
        
        if needs_timestamp {
            self.write_full_timestamp(self.timestamp_converter.get_current_timestamp());
        }

        let payload_len = payload.len();
        let header_len = 1 + 2 + 2 + 2;
        let total_len = header_len + payload_len;
        
        if total_len > CAP {
            panic!("Log record size exceeds buffer capacity");
        }

        // Get the current buffer and position
        let (buf, pos) = if self.active_buffer {
            if self.pos_a + total_len > CAP {
                self.flush();
                (&mut self.buffer_b[..], &mut self.pos_b)
            } else {
                (&mut self.buffer_a[..], &mut self.pos_a)
            }
        } else {
            if self.pos_b + total_len > CAP {
                self.flush();
                (&mut self.buffer_a[..], &mut self.pos_a)
            } else {
                (&mut self.buffer_b[..], &mut self.pos_b)
            }
        };

        // Write directly to the buffer
        let start = *pos;
        buf[start] = 0; // Record type
        buf[start + 1..start + 3].copy_from_slice(&delta_u16.to_le_bytes());
        buf[start + 3..start + 5].copy_from_slice(&log_id.to_le_bytes());
        buf[start + 5..start + 7].copy_from_slice(&(payload_len as u16).to_le_bytes());
        buf[start + 7..start + 7 + payload_len].copy_from_slice(payload);
        *pos += total_len;
    }

    #[inline(always)]
    fn write_full_timestamp(&mut self, ts: u64) {
        let rec_len = 1 + 8;
        
        // Get the current buffer and position
        let (buf, pos) = if self.active_buffer {
            if self.pos_a + rec_len > CAP {
                self.flush();
                (&mut self.buffer_b[..], &mut self.pos_b)
            } else {
                (&mut self.buffer_a[..], &mut self.pos_a)
            }
        } else {
            if self.pos_b + rec_len > CAP {
                self.flush();
                (&mut self.buffer_a[..], &mut self.pos_a)
            } else {
                (&mut self.buffer_b[..], &mut self.pos_b)
            }
        };

        // Write directly to the buffer
        let start = *pos;
        buf[start] = 1; // Record type
        buf[start + 1..start + 9].copy_from_slice(&ts.to_le_bytes());
        *pos += rec_len;
    }

    /// Flushes the active buffer and switches to the other buffer
    pub fn flush(&mut self) {
        let (buf, len) = if self.active_buffer {
            (&self.buffer_a[..self.pos_a], self.pos_a)
        } else {
            (&self.buffer_b[..self.pos_b], self.pos_b)
        };

        if len > 0 {
            // Send the full buffer to background thread
            let data = buf.to_vec().into_boxed_slice();
            let _ = self.flush_sender.send(FlushMessage::Data(data));
        }

        // Reset buffer and switch - this is atomic
        if self.active_buffer {
            self.pos_a = 0;
        } else {
            self.pos_b = 0;
        }
        self.active_buffer = !self.active_buffer;
        self.timestamp_converter.reset();
    }
}

impl<const CAP: usize> Drop for Logger<CAP> {
    fn drop(&mut self) {
        self.flush();
        let _ = self.flush_sender.send(FlushMessage::Shutdown);
    }
}

/// A simple compile-time hash function to compute a log ID from the fixed format string.
#[doc(hidden)]
pub const fn simple_hash(s: &str) -> u16 {
    let bytes = s.as_bytes();
    let mut sum = 0u16;
    let mut i = 0;
    while i < bytes.len() {
        sum = sum.wrapping_add(bytes[i] as u16);
        i += 1;
    }
    sum
}

/// The log_record! macro provides a printf-like interface for logging.
/// Format specifiers use standard Rust format syntax: {}
#[macro_export]
macro_rules! log_record {
    ($logger:expr, $fmt:expr $(, $arg:expr)*) => {{
        const FORMAT_INFO: $crate::log_format_registry::FormatInfo = $crate::const_format!($fmt);
        
        // Serialize parameters
        let mut temp = [0u8; 1024];
        let mut pos = 0;
        $(
            pos += $arg.serialize(&mut temp[pos..]);
        )*
        let payload = &temp[..pos];
        
        $logger.log_record(FORMAT_INFO.format_id, payload)
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::time::Instant;
    use crate::log_reader::LogReader;
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
    fn test_trace_correctness() {
        // Clean up any existing test file
        let _ = fs::remove_file(TEST_FILE);

        let file = Arc::new(Mutex::new(OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(TEST_FILE)
            .unwrap()));
        
        let file_clone = file.clone();
        let callback = Arc::new(move |buf: &[u8]| {
            file_clone.lock().unwrap().write_all(buf).unwrap();
        });

        let mut logger = Logger::<CAPACITY>::new(Some(callback));

        // Test Case 1: Simple i64 value
        let timestamp: i64 = 12345678;
        log_record!(logger, "Test i64: {}", timestamp);

        // Test Case 2: Custom struct with int, bool, array
        let event = TestEvent {
            id: 42,
            active: true,
            data: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            large_number: 18446744073709551615, // max u64
            description: "This is a longer description that includes some special characters !@#$%^&*() \
                        and provides more context about the event. It also contains some metrics like \
                        CPU: 95%, Memory: 2.5GB, Network: 1.2Gbps".to_string(),
        };
        log_record!(logger, "Test struct: {}", event);

        // Test Case 3: Multiple values in one log
        log_record!(logger, "Test multiple: {}, {}, {}", timestamp, event, "test message");

        // Ensure all data is written
        logger.flush();

        // Now read back and verify
        let data = fs::read(TEST_FILE).unwrap();
        let mut reader = LogReader::new(&data);
        
        let mut entries = Vec::new();
        while let Some(entry) = reader.read_entry() {
            entries.push(entry.format());
        }

        // Verify all entries were read
        assert_eq!(entries.len(), 3, "Expected 3 log entries");
        
        // Verify content of each entry
        assert!(entries[0].contains(&timestamp.to_string()), "i64 value mismatch");
        assert!(entries[1].contains("id=42") && entries[1].contains("active=true"), "Custom struct mismatch");
        assert!(entries[2].contains(&timestamp.to_string()) && entries[2].contains("test message"), "Multiple values mismatch");

        // Clean up
        let _ = fs::remove_file(TEST_FILE);
    }

    #[test]
    fn test_logging_performance() {
        // Clean up any existing test files
        let _ = fs::remove_file(TEST_FILE);
        let _ = fs::remove_dir_all("logs");
        let _ = fs::create_dir("logs");

        // Test binary logging with our efficient timestamps
        let binary_duration = {
            let file = Arc::new(Mutex::new(OpenOptions::new()
                .create(true)
                .write(true)
                .append(true)
                .open(TEST_FILE)
                .unwrap()));
            
            let file_clone = file.clone();
            let callback = Arc::new(move |buf: &[u8]| {
                file_clone.lock().unwrap().write_all(buf).unwrap();
            });

            let mut logger = Logger::<CAPACITY>::new(Some(callback));

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
                // Note: timestamp is handled automatically by our logger infrastructure
                log_record!(logger, "Test perf: iteration={}, event={}", i, event);
            }
            logger.flush();
            start.elapsed()
        };

        // Test traditional logging using log4rs with rotation
        let traditional_duration = {
            // Set up log rotation policy
            let window_roller = FixedWindowRoller::builder()
                .build(TRADITIONAL_LOG, WINDOW_SIZE)
                .unwrap();
            
            let size_trigger = SizeTrigger::new(LOG_SIZE_LIMIT);
            
            let compound_policy = CompoundPolicy::new(
                Box::new(size_trigger),
                Box::new(window_roller),
            );

            // Set up log4rs with rotation
            let logfile = RollingFileAppender::builder()
                .encoder(Box::new(PatternEncoder::new("{d} - {m}{n}")))
                .build("logs/traditional.log", Box::new(compound_policy))
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
        let binary_size = fs::metadata(TEST_FILE).unwrap().len();
        let traditional_size: u64 = fs::read_dir("logs")
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.metadata().ok())
            .map(|metadata| metadata.len())
            .sum();

        println!("\nFile size comparison:");
        println!("Binary log size:      {} MB", binary_size as f64 / (1024.0 * 1024.0));
        println!("Traditional log size: {} MB", traditional_size as f64 / (1024.0 * 1024.0));
        println!("Size ratio: {:.2}x", traditional_size as f64 / binary_size as f64);

        // Clean up
        let _ = fs::remove_file(TEST_FILE);
        let _ = fs::remove_dir_all("logs");
    }
}

