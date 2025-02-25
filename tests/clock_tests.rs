use binary_logger::efficient_clock::{get_timestamp, TimestampConverter};
use std::thread;
use std::time::Duration;

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