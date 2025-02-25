use binary_logger::{Logger, BufferHandler, LogReader, log_record, LogValue};
use binary_logger::efficient_clock::{get_timestamp, TimestampConverter};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

struct CountingHandler {
    buffer_count: Arc<AtomicUsize>,
    total_bytes: Arc<AtomicUsize>,
}

impl CountingHandler {
    fn new() -> Self {
        Self {
            buffer_count: Arc::new(AtomicUsize::new(0)),
            total_bytes: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl BufferHandler for CountingHandler {
    fn handle_switched_out_buffer(&self, _buffer: *const u8, size: usize) {
        self.buffer_count.fetch_add(1, Ordering::SeqCst);
        self.total_bytes.fetch_add(size, Ordering::SeqCst);
    }
}

struct CollectingHandler {
    data: Arc<Mutex<Vec<u8>>>,
}

impl CollectingHandler {
    fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl BufferHandler for CollectingHandler {
    fn handle_switched_out_buffer(&self, buffer: *const u8, size: usize) {
        let mut data = self.data.lock().unwrap();
        unsafe {
            // Get the buffer data including the header
            let buffer_slice = std::slice::from_raw_parts(buffer, size);
            data.extend_from_slice(buffer_slice);
        }
    }
}

#[test]
fn test_timestamp_monotonicity() {
    let mut prev = get_timestamp();
    for _ in 0..1000 {
        let current = get_timestamp();
        assert!(current >= prev, "Timestamps should be monotonically increasing");
        prev = current;
    }
}

#[test]
fn test_relative_timestamp_conversion() {
    let mut converter = TimestampConverter::new();
    
    // First call establishes base
    let (first, is_base1) = converter.get_relative_timestamp();
    assert_eq!(first, 0, "First relative timestamp should be 0");
    assert!(is_base1, "First call should establish base");
    
    // Subsequent calls should return increasing values
    thread::sleep(Duration::from_micros(100));
    let (second, is_base2) = converter.get_relative_timestamp();
    assert!(!is_base2, "Second call should not be base");
    assert!(second > first, "Subsequent timestamps should be greater");
}

#[test]
fn test_timestamp_overflow() {
    let mut converter = TimestampConverter::new();
    let (mut prev, _) = converter.get_relative_timestamp();
    
    // Generate many timestamps to test overflow handling
    for _ in 0..1000 {
        let (current, is_base) = converter.get_relative_timestamp();
        if !is_base {
            assert!(current >= prev, "Non-base timestamps should be monotonic");
        }
        prev = current;
    }
}

#[test]
fn test_reset() {
    let mut converter = TimestampConverter::new();
    
    // Get initial timestamp
    let (first, is_base1) = converter.get_relative_timestamp();
    assert!(is_base1, "First call should establish base");
    thread::sleep(Duration::from_micros(100));
    let (second, is_base2) = converter.get_relative_timestamp();
    assert!(!is_base2, "Second call should not be base");
    assert!(second > first, "Second timestamp should be greater than first");
    
    // Reset and verify new base is established
    converter.reset();
    let (after_reset, is_base3) = converter.get_relative_timestamp();
    assert_eq!(after_reset, 0, "Timestamp after reset should be 0");
    assert!(is_base3, "Call after reset should establish new base");
}

#[test]
fn test_current_timestamp() {
    let converter = TimestampConverter::new();
    let first = converter.get_current_timestamp();
    thread::sleep(Duration::from_micros(100));
    let second = converter.get_current_timestamp();
    assert!(second > first, "Current timestamp should increase over time");
}

#[test]
fn test_concurrent_timestamps() {
    let converter = TimestampConverter::new();
    let converter_clone = converter.clone();
    
    let handle = thread::spawn(move || {
        let mut local_converter = converter_clone;
        let mut timestamps = Vec::new();
        for _ in 0..100 {
            let (ts, _) = local_converter.get_relative_timestamp();
            timestamps.push(ts);
        }
        timestamps
    });
    
    let mut main_timestamps = Vec::new();
    let mut main_converter = converter;
    for _ in 0..100 {
        let (ts, _) = main_converter.get_relative_timestamp();
        main_timestamps.push(ts);
    }
    
    let thread_timestamps = handle.join().unwrap();
    
    // Verify both threads got monotonically increasing timestamps
    // (ignoring potential base resets)
    for window in main_timestamps.windows(2) {
        if window[1] != 0 {  // Skip if it's a base reset
            assert!(window[1] >= window[0], "Main thread timestamps should be monotonic");
        }
    }
    
    for window in thread_timestamps.windows(2) {
        if window[1] != 0 {  // Skip if it's a base reset
            assert!(window[1] >= window[0], "Spawned thread timestamps should be monotonic");
        }
    }
}

#[test]
fn test_high_frequency_timestamps() {
    let mut converter = TimestampConverter::new();
    let mut timestamps = Vec::with_capacity(10000);
    
    // Generate timestamps as fast as possible
    for _ in 0..10000 {
        let (ts, _) = converter.get_relative_timestamp();
        timestamps.push(ts);
    }
    
    // Verify monotonicity under high-frequency access
    // (ignoring potential base resets)
    for window in timestamps.windows(2) {
        if window[1] != 0 {  // Skip if it's a base reset
            assert!(window[1] >= window[0], "Timestamps should be monotonic under high frequency");
        }
    }
}

#[test]
fn test_timestamp_precision() {
    let mut converter = TimestampConverter::new();
    let (start, _) = converter.get_relative_timestamp();
    
    // Sleep for 1ms
    thread::sleep(Duration::from_millis(1));
    
    let (end, _) = converter.get_relative_timestamp();
    
    // Given TICKS_PER_UNIT is 30000, 1ms should result in a difference > 0
    // (unless there was a base reset)
    if end != 0 {  // Only check if we didn't hit a base reset
        assert!(end > start, "Timestamp should be precise enough to detect 1ms difference");
    }
}

#[test]
fn test_buffer_switching() {
    const BUFFER_SIZE: usize = 1024;
    let handler = CountingHandler::new();
    let buffer_count = handler.buffer_count.clone();
    let total_bytes = handler.total_bytes.clone();
    
    let mut logger = Logger::<BUFFER_SIZE>::new(handler);
    
    // Write enough data to force multiple buffer switches
    for i in 0..1000 {
        log_record!(logger, "Test message {}", i).unwrap();
    }
    
    assert!(buffer_count.load(Ordering::SeqCst) > 0, "Should have switched buffers");
    assert!(total_bytes.load(Ordering::SeqCst) > 0, "Should have written data");
}

#[test]
fn test_log_format() {
    const BUFFER_SIZE: usize = 1024;
    let handler = CollectingHandler::new();
    let data = handler.data.clone();
    
    {
        let mut logger = Logger::<BUFFER_SIZE>::new(handler);
        
        // Log different types of records
        log_record!(logger, "Integer: {}", 42).unwrap();
        log_record!(logger, "Boolean: {}", true).unwrap();
        log_record!(logger, "String: {}", "test").unwrap();
        log_record!(logger, "Multiple: {} and {}", 1, false).unwrap();
        
        // Force buffer flush to ensure all data is written
        logger.flush();
    }
    
    let data = data.lock().unwrap();
    println!("Data length: {}", data.len());
    
    // Print the buffer header
    if data.len() >= 8 {
        let header = u64::from_le_bytes(data[0..8].try_into().unwrap());
        println!("Buffer header (length): {}", header);
        
        // Print the first few bytes after the header for debugging
        if data.len() > 16 {
            println!("First bytes after header: {:?}", &data[8..16]);
        }
    }
    
    // Print the entire data for debugging
    println!("Full data: {:?}", &data[..]);
    
    // Print the data in a more readable format
    println!("Data in hex format:");
    for i in 0..data.len() {
        if i % 16 == 0 {
            print!("\n{:04x}: ", i);
        }
        print!("{:02x} ", data[i]);
    }
    println!();
    
    let mut reader = LogReader::new(&data);
    
    let mut count = 0;
    while let Some(entry) = reader.read_entry() {
        println!("\nEntry #{}: format_id={}", count + 1, entry.format_id);
        println!("  Format string: {:?}", entry.format_string);
        println!("  Parameters: {:?}", entry.parameters);
        println!("  Raw values length: {}", entry.raw_values.len());
        println!("  Raw values: {:?}", entry.raw_values);
        
        // Print raw values in hex format
        print!("  Raw values (hex):");
        for (i, b) in entry.raw_values.iter().enumerate() {
            if i % 8 == 0 {
                print!("\n    {:04x}: ", i);
            }
            print!("{:02x} ", b);
        }
        println!();
        
        count += 1;
        
        match count {
            1 => {
                // Integer record
                if let Some(LogValue::Integer(value)) = entry.parameters.get(0) {
                    println!("  Extracted integer value: {}", value);
                    assert_eq!(*value, 42);
                } else {
                    println!("  ERROR: Expected integer parameter, got: {:?}", entry.parameters.get(0));
                    panic!("Expected integer parameter");
                }
            }
            2 => {
                // Boolean record
                if let Some(LogValue::Boolean(value)) = entry.parameters.get(0) {
                    println!("  Extracted boolean value: {}", value);
                    assert!(*value);
                } else {
                    println!("  ERROR: Expected boolean parameter, got: {:?}", entry.parameters.get(0));
                    panic!("Expected boolean parameter");
                }
            }
            3 => {
                // String record
                if let Some(LogValue::String(value)) = entry.parameters.get(0) {
                    println!("  Extracted string value: {}", value);
                    assert_eq!(value, "test");
                } else {
                    println!("  ERROR: Expected string parameter, got: {:?}", entry.parameters.get(0));
                    panic!("Expected string parameter");
                }
            }
            4 => {
                // Multiple values
                if let (Some(LogValue::Integer(i)), Some(LogValue::Boolean(b))) = 
                   (entry.parameters.get(0), entry.parameters.get(1)) {
                    println!("  Extracted i32 value: {}", i);
                    println!("  Extracted boolean value: {}", b);
                    
                    assert_eq!(*i, 1);
                    assert!(!b);
                } else {
                    println!("  ERROR: Expected integer and boolean parameters, got: {:?}", entry.parameters);
                    panic!("Expected integer and boolean parameters");
                }
            }
            _ => panic!("Too many records"),
        }
    }
    
    println!("\nTotal entries read: {}", count);
    assert_eq!(count, 4, "Should have read all records");
}

#[test]
fn test_buffer_overflow() {
    // Use a buffer size that's too small for the header + a minimal record
    const TINY_BUFFER: usize = 8;  // Just enough for the header, but not for any records
    let handler = CountingHandler::new();
    
    // This should panic during creation because the buffer is too small
    let result = std::panic::catch_unwind(|| {
        let mut logger = Logger::<TINY_BUFFER>::new(handler);
        log_record!(logger, "Test", ).unwrap();
    });
    
    assert!(result.is_err(), "Should have panicked on buffer overflow");
}

#[test]
fn test_format_deduplication() {
    const BUFFER_SIZE: usize = 1024;
    let handler = CollectingHandler::new();
    let data = handler.data.clone();
    
    {
        let mut logger = Logger::<BUFFER_SIZE>::new(handler);
        
        // Use same format string multiple times
        for i in 0..3 {
            log_record!(logger, "Test message {}", i).unwrap();
        }
        
        // Force buffer flush to ensure all data is written
        logger.flush();
    }
    
    let data = data.lock().unwrap();
    let mut reader = LogReader::new(&data);
    
    let mut last_format_id = None;
    let mut count = 0;
    
    while let Some(entry) = reader.read_entry() {
        if let Some(id) = last_format_id {
            assert_eq!(entry.format_id, id, "Format IDs should be same for identical strings");
        }
        last_format_id = Some(entry.format_id);
        count += 1;
    }
    
    assert_eq!(count, 3, "Should have read all records");
} 