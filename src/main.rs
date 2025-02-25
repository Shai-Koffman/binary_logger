#![feature(generic_const_exprs)]

use std::io;

mod binary_logger;
mod string_registry;
mod log_reader;
mod efficient_clock;

use crate::binary_logger::Logger;

/// Buffer size for the binary logger (16KB)
const BUFFER_SIZE: usize = 16 * 1024;

/// Example application demonstrating the binary logger usage.
/// Creates a logger, writes various types of log records, and ensures proper cleanup.
fn main() -> io::Result<()> {
    // Create a new logger with 16KB buffer
    let mut logger = Logger::<BUFFER_SIZE>::new("app.log")?;

    // Example 1: Simple numeric value
    log_record!(logger, "Processing value {}", 42)?;

    // Example 2: Multiple parameters of different types
    log_record!(logger, "User {} logged in from {} with role {}", 
        "john_doe", 
        "192.168.1.1",
        "admin"
    )?;

    // Example 3: Complex data structure
    let metrics = format!(
        "CPU: {}%, Memory: {}GB, Network: {}Mbps",
        95, 32, 1000
    );
    log_record!(logger, "System metrics: {}", metrics)?;

    // Ensure all records are written to disk
    logger.flush()?;
    
    Ok(())
}
