#![feature(adt_const_params)]
#![feature(generic_const_exprs)]

mod efficient_clock;
mod binary_logger;
mod loggable;
mod log_format_registry;
mod log_reader;

use binary_logger::{Logger, DumpCallback};
use loggable::Loggable;
use log_reader::LogReader;
use std::sync::Mutex;
use std::time::Duration;
use lazy_static::lazy_static;
use std::sync::Arc;

lazy_static! {
    static ref LAST_DUMP: Mutex<Vec<u8>> = Mutex::new(Vec::new());
}

fn main() {
    // Example dump callback: stores the buffer for later reading
    let dump_cb = Arc::new(|buf: &[u8]| {
        let mut last_dump = LAST_DUMP.lock().unwrap();
        last_dump.extend_from_slice(buf);
        
        // Also print the raw hex dump for debugging
        println!("Dumped buffer with {} bytes", buf.len());
        println!("Raw data: {:02x?}", buf);
    });

    let mut logger = Logger::<256>::new(Some(dump_cb));

    // Log some events with standard Rust format syntax
    log_record!(logger, "Action: {} at time={}", "Started application", 100i32);
    log_record!(logger, "User {} logged in", "alice");
    log_record!(logger, "Measurement: value={} units={}", 3_141_592u64, 42u32);
    std::thread::sleep(Duration::from_millis(50));
    log_record!(logger, "Event occurred: {}", "User 'bob' logged out with a very long message that demonstrates variable-sized strings");

    logger.flush();

    // Now read back and decode the logs
    println!("\nDecoded log entries:");
    println!("-------------------");
    
    let dump = LAST_DUMP.lock().unwrap();
    let mut reader = LogReader::new(&dump);
    
    while let Some(entry) = reader.read_entry() {
        println!("At {:?}:", entry.timestamp);
        println!("  {}", entry.format());
    }
}
