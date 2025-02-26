#![feature(generic_const_exprs)]
#![allow(incomplete_features)]

//! # Binary Logger
//! 
//! A high-performance logging library that uses a compact binary format to achieve:
//! 
//! * **Ultra-fast logging**: 30-50x faster than traditional text-based loggers
//! * **Compact storage**: 80-100x smaller log files compared to text logs
//! * **Efficient reading**: Fast random access and filtering capabilities
//! 
//! ## Key Features
//! 
//! * Zero-allocation logging path for maximum performance
//! * Automatic string deduplication to reduce storage requirements
//! * Efficient timestamp encoding using CPU hardware counters
//! * Compact binary format with minimal overhead
//! * Per-thread logging design for maximum throughput without contention
//! 
//! ## Main Components
//! 
//! * `Logger`: Core logging engine that writes records in binary format (one per thread)
//! * `LogReader`: Utility for reading and decoding binary log files
//! * `string_registry`: Registry for efficient string deduplication
//! * `efficient_clock`: High-precision, low-overhead timestamp generation
//! 
//! ## Quick Start
//! 
//! ```
//! use binary_logger::{Logger, BufferHandler, log_record};
//! use std::fs::File;
//! use std::io::Write;
//! use std::cell::RefCell;
//! 
//! // Define a handler for log buffers
//! struct FileHandler(RefCell<File>);
//! impl BufferHandler for FileHandler {
//!     fn handle_switched_out_buffer(&self, buffer: *const u8, size: usize) {
//!         let data = unsafe { std::slice::from_raw_parts(buffer, size) };
//!         self.0.borrow_mut().write_all(data).unwrap();
//!     }
//! }
//! 
//! // Create a logger with 1MB buffer
//! let file = File::create("log.bin").unwrap();
//! let mut logger = Logger::<1_000_000>::new(FileHandler(RefCell::new(file)));
//! 
//! // Log some records
//! log_record!(logger, "Hello, world!", );
//! log_record!(logger, "Temperature: {} C", 25.5);
//! log_record!(logger, "Status: {}, Count: {}", true, 42);
//! ```

pub mod binary_logger;
pub mod string_registry;
pub mod log_reader;
pub mod efficient_clock;

pub use binary_logger::{Logger, BufferHandler};
pub use string_registry::{register_string, get_string};
pub use log_reader::{LogReader, LogValue, LogEntry}; 