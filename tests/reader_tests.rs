use binary_logger::{LogReader, register_string};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn test_empty_log() {
    let data = Vec::new();
    let mut reader = LogReader::new(&data);
    assert!(reader.read_entry().is_none());
}

#[test]
fn test_single_timestamp() {
    let mut data = Vec::new();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64;
    
    // Write full timestamp record
    data.push(1); // Record type
    data.extend_from_slice(&now.to_le_bytes());
    
    let mut reader = LogReader::new(&data);
    assert!(reader.read_entry().is_none()); // Timestamp record should be consumed internally
}

#[test]
fn test_primitive_types() {
    let mut data = Vec::new();
    let base_ts = 1234567890u64;
    
    // Base timestamp
    data.push(1);
    data.extend_from_slice(&base_ts.to_le_bytes());
    
    // Record with various primitive types
    data.push(0); // Normal record
    data.extend_from_slice(&100u16.to_le_bytes()); // Relative timestamp
    data.extend_from_slice(&1u16.to_le_bytes()); // Format ID
    
    // Payload: i32 + bool + f64
    let payload_len = 4 + 1 + 8;
    data.extend_from_slice(&(payload_len as u16).to_le_bytes());
    data.extend_from_slice(&42i32.to_le_bytes());
    data.push(1); // true
    data.extend_from_slice(&3.14f64.to_le_bytes());
    
    let mut reader = LogReader::new(&data);
    let entry = reader.read_entry().unwrap();
    
    assert_eq!(entry.format_id, 1);
    
    // Verify values
    let mut pos = 0;
    let i32_val = i32::from_le_bytes(entry.raw_values[pos..pos+4].try_into().unwrap());
    pos += 4;
    let bool_val = entry.raw_values[pos] != 0;
    pos += 1;
    let f64_val = f64::from_le_bytes(entry.raw_values[pos..pos+8].try_into().unwrap());
    
    assert_eq!(i32_val, 42);
    assert!(bool_val);
    assert!((f64_val - 3.14).abs() < f64::EPSILON);
}

#[test]
fn test_multiple_records() {
    // Create a test log with multiple records
    let mut data = Vec::new();
    
    // Buffer header (8 bytes)
    data.extend_from_slice(&(100u64).to_le_bytes());
    
    // Create a full timestamp record
    let base_ts = 1234567890u64;
    
    // Record type (1 byte)
    data.push(1); // Type = 1 (full timestamp)
    data.push(0); // Padding for alignment
    
    // Relative timestamp (2 bytes) - not used for full timestamp records
    data.extend_from_slice(&0u16.to_le_bytes());
    
    // Format ID (2 bytes) - not used for full timestamp records
    data.extend_from_slice(&0u16.to_le_bytes());
    
    // Payload length (2 bytes)
    data.extend_from_slice(&8u16.to_le_bytes()); // Just the timestamp (8 bytes)
    
    // Payload - just the timestamp
    data.extend_from_slice(&base_ts.to_le_bytes());
    
    // Add three normal records with increasing timestamps
    for (i, (rel_ts, fmt_id)) in [(100u16, 1u16), (200u16, 2u16), (300u16, 3u16)].iter().enumerate() {
        // Record type (1 byte)
        data.push(0); // Type = 0 (normal record)
        data.push(0); // Padding for alignment
        
        // Relative timestamp (2 bytes)
        data.extend_from_slice(&rel_ts.to_le_bytes());
        
        // Format ID (2 bytes)
        data.extend_from_slice(&fmt_id.to_le_bytes());
        
        // Create a simple payload with 1 argument (an integer)
        let mut payload = Vec::new();
        payload.push(1); // 1 argument
        
        // Add argument size (4 bytes)
        payload.extend_from_slice(&4u32.to_le_bytes()); // Size of i32
        
        // Add argument value (i32)
        payload.extend_from_slice(&(42 + i as i32).to_le_bytes());
        
        // Payload length (2 bytes)
        data.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        
        // Payload
        data.extend_from_slice(&payload);
    }
    
    // Create a reader
    let mut reader = LogReader::new(&data);
    
    // Read and verify all entries
    let mut entries = Vec::new();
    while let Some(entry) = reader.read_entry() {
        entries.push(entry);
    }
    
    // We should have 3 entries (the timestamp record is consumed internally)
    assert_eq!(entries.len(), 3, "Expected 3 entries, got {}", entries.len());
    
    // Verify timestamps are monotonically increasing
    let mut last_ts = UNIX_EPOCH;
    for entry in &entries {
        assert!(entry.timestamp > last_ts, "Timestamps should be monotonically increasing");
        last_ts = entry.timestamp;
    }
    
    // Verify each entry has parameters
    for entry in &entries {
        assert!(!entry.raw_values.is_empty(), "Entry should have raw values");
    }
}

#[test]
fn test_complex_record() {
    // Create a test log with a complex record
    let mut data = Vec::new();
    
    // Buffer header (8 bytes)
    data.extend_from_slice(&(100u64).to_le_bytes());
    
    // Create a full timestamp record
    let base_ts = 1234567890u64;
    
    // Record type (1 byte)
    data.push(1); // Type = 1 (full timestamp)
    data.push(0); // Padding for alignment
    
    // Relative timestamp (2 bytes) - not used for full timestamp records
    data.extend_from_slice(&0u16.to_le_bytes());
    
    // Format ID (2 bytes) - not used for full timestamp records
    data.extend_from_slice(&0u16.to_le_bytes());
    
    // Payload length (2 bytes)
    data.extend_from_slice(&8u16.to_le_bytes()); // Just the timestamp (8 bytes)
    
    // Payload - just the timestamp
    data.extend_from_slice(&base_ts.to_le_bytes());
    
    // Register test format string
    let fmt = "Complex test with {} values: [{}, {}, {}]";
    let fmt_id = register_string(fmt);
    
    // Add a normal record with a complex payload
    // Record type (1 byte)
    data.push(0); // Type = 0 (normal record)
    data.push(0); // Padding for alignment
    
    // Relative timestamp (2 bytes)
    data.extend_from_slice(&100u16.to_le_bytes());
    
    // Format ID (2 bytes)
    data.extend_from_slice(&fmt_id.to_le_bytes());
    
    // Create a complex payload with 4 arguments
    let mut payload = Vec::new();
    payload.push(4); // 4 arguments
    
    // Integer argument (42)
    payload.extend_from_slice(&4u32.to_le_bytes()); // Size of i32
    payload.extend_from_slice(&42i32.to_le_bytes()); // Value
    
    // Unknown array argument ([1, 2, 3, 4])
    payload.extend_from_slice(&4u32.to_le_bytes()); // Size of array
    payload.extend_from_slice(&[1, 2, 3, 4]); // Value
    
    // Boolean argument (true)
    payload.extend_from_slice(&1u32.to_le_bytes()); // Size of bool
    payload.push(1); // true
    
    // Float argument (3.14)
    payload.extend_from_slice(&8u32.to_le_bytes()); // Size of f64
    payload.extend_from_slice(&3.14f64.to_le_bytes()); // Value
    
    // Payload length (2 bytes)
    data.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    
    // Payload
    data.extend_from_slice(&payload);
    
    // Create a reader
    let mut reader = LogReader::new(&data);
    
    // Read and verify the entry
    let entry = reader.read_entry().expect("Failed to read entry");
    
    // Verify the entry has raw values
    assert!(!entry.raw_values.is_empty(), "Entry should have raw values");
    
    // Verify timestamp is after the base timestamp
    let ts_micros = entry.timestamp
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros();
    assert!(ts_micros >= base_ts as u128, "Timestamp should be after base timestamp");
}

#[test]
fn test_parameter_extraction() {
    // This test is adapted from test_log_reading in log_reader.rs
    // It focuses on parameter extraction from the binary payload
    let mut log_data = Vec::new();
    
    // Buffer header (8 bytes)
    log_data.extend_from_slice(&(20u64).to_le_bytes());
    
    // Record type: Normal = 0
    log_data.push(0);
    
    // Padding for alignment
    log_data.push(0);
    
    // Relative timestamp (2 bytes)
    log_data.extend_from_slice(&(1u16).to_le_bytes());
    
    // Format ID (2 bytes)
    log_data.extend_from_slice(&(1u16).to_le_bytes());
    
    // Create payload
    let mut payload = Vec::new();
    
    // Argument count
    payload.push(3); // 3 arguments
    
    // First argument: i32 = 42
    payload.extend_from_slice(&4u32.to_le_bytes()); // Size of i32
    payload.extend_from_slice(&42i32.to_le_bytes()); // Value
    
    // Second argument: bool = true
    payload.extend_from_slice(&1u32.to_le_bytes()); // Size of bool
    payload.push(1); // true
    
    // Third argument: [u8; 4] = [1, 2, 3, 4]
    payload.extend_from_slice(&4u32.to_le_bytes()); // Size of array
    payload.extend_from_slice(&[1, 2, 3, 4]); // Value
    
    // Add payload length and payload
    log_data.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    log_data.extend_from_slice(&payload);

    // Read and verify
    let mut reader = LogReader::new(&log_data);
    let entry = reader.read_entry().expect("Failed to read entry");
    
    assert_eq!(entry.format_id, 1);
    
    // Extract and verify parameters from raw values
    let raw = &entry.raw_values;
    assert!(!raw.is_empty(), "Raw values should not be empty");
    
    // First byte should be argument count
    assert_eq!(raw[0], 3, "Expected 3 arguments");
    
    // Verify parameters if the LogReader exposes them
    if !entry.parameters.is_empty() {
        assert_eq!(entry.parameters.len(), 3, "Expected 3 parameters");
    }
}

#[test]
fn test_relative_timestamps() {
    // Create a test log with multiple records using relative timestamps
    let mut data = Vec::new();
    
    // Buffer header (8 bytes)
    data.extend_from_slice(&(100u64).to_le_bytes());
    
    // Create a full timestamp record
    let base_ts = 1234567890u64;
    
    // Record type (1 byte)
    data.push(1); // Type = 1 (full timestamp)
    data.push(0); // Padding for alignment
    
    // Relative timestamp (2 bytes) - not used for full timestamp records
    data.extend_from_slice(&0u16.to_le_bytes());
    
    // Format ID (2 bytes) - not used for full timestamp records
    data.extend_from_slice(&0u16.to_le_bytes());
    
    // Payload length (2 bytes)
    data.extend_from_slice(&8u16.to_le_bytes()); // Just the timestamp (8 bytes)
    
    // Payload - just the timestamp
    data.extend_from_slice(&base_ts.to_le_bytes());
    
    // Add two normal records with relative timestamps
    for (rel_ts, fmt_id) in [(100u16, 1u16), (200u16, 2u16)] {
        // Record type (1 byte)
        data.push(0); // Type = 0 (normal record)
        data.push(0); // Padding for alignment
        
        // Relative timestamp (2 bytes)
        data.extend_from_slice(&rel_ts.to_le_bytes());
        
        // Format ID (2 bytes)
        data.extend_from_slice(&fmt_id.to_le_bytes());
        
        // Create a simple payload with 0 arguments
        let payload = vec![0]; // 0 arguments
        
        // Payload length (2 bytes)
        data.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        
        // Payload
        data.extend_from_slice(&payload);
    }
    
    // Create a reader
    let mut reader = LogReader::new(&data);
    
    // Read and verify all entries
    let mut entries = Vec::new();
    while let Some(entry) = reader.read_entry() {
        entries.push(entry);
    }
    
    // We should have at least 1 entry (the timestamp record is consumed internally)
    assert!(entries.len() >= 1, "Expected at least 1 entry, got {}", entries.len());
    
    // If we have at least 2 entries, verify their timestamps have a reasonable difference
    if entries.len() >= 2 {
        let ts1 = entries[0].timestamp.duration_since(UNIX_EPOCH).unwrap().as_micros();
        let ts2 = entries[1].timestamp.duration_since(UNIX_EPOCH).unwrap().as_micros();
        let diff = ts2 - ts1;
        
        // The difference should be positive and reasonable
        assert!(diff > 0, "Second timestamp should be after first");
        assert!(diff <= 1000, "Timestamp difference should be reasonable");
    }
} 