use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::fmt;

/// A value read from a log record.
/// Currently supports string values, which may be either static (from registry) or dynamic.
#[derive(Debug)]
pub struct LogValue(String);

impl fmt::Display for LogValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A single log entry read from the binary log file.
/// Contains the timestamp, format string ID, and parameter values.
#[derive(Debug)]
pub struct LogEntry {
    /// When the log entry was written (UNIX timestamp)
    pub timestamp: SystemTime,
    /// ID of the format string in the string registry
    pub format_id: u16,
    /// Raw bytes of the parameter values
    pub values: Vec<u8>,
}

/// Reader for the binary log format.
/// Provides efficient sequential access to log entries.
/// 
/// # Binary Format
/// The reader handles two types of records:
/// 1. Full timestamp records (type=1):
///    ```text
///    [1 byte type | 8 bytes timestamp]
///    ```
/// 
/// 2. Normal records (type=0):
///    ```text
///    [1 byte type | 2 bytes relative ts | 2 bytes format ID | 2 bytes payload len | N bytes payload]
///    ```
/// 
/// # Performance
/// - Sequential read performance: O(n) where n is file size
/// - Memory usage: O(1) - reads records one at a time
/// - Timestamp compression: Uses relative timestamps between full timestamps
pub struct LogReader<'a> {
    data: &'a [u8],
    pos: usize,
    base_timestamp: Option<u64>,
    last_relative: u16,
}

impl<'a> LogReader<'a> {
    /// Creates a new reader for the given binary log data.
    /// 
    /// # Arguments
    /// * `data` - The raw bytes of the binary log file
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            base_timestamp: None,
            last_relative: 0,
        }
    }

    /// Reads a 16-bit unsigned integer from the current position.
    /// 
    /// # Returns
    /// Some(u16) if there are enough bytes remaining, None otherwise
    fn read_u16(&mut self) -> Option<u16> {
        if self.pos + 2 <= self.data.len() {
            let value = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
            self.pos += 2;
            Some(value)
        } else {
            None
        }
    }

    /// Reads a 64-bit unsigned integer from the current position.
    /// 
    /// # Returns
    /// Some(u64) if there are enough bytes remaining, None otherwise
    fn read_u64(&mut self) -> Option<u64> {
        if self.pos + 8 <= self.data.len() {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&self.data[self.pos..self.pos + 8]);
            self.pos += 8;
            Some(u64::from_le_bytes(bytes))
        } else {
            None
        }
    }

    /// Reads a slice of bytes from the current position.
    /// 
    /// # Arguments
    /// * `len` - Number of bytes to read
    /// 
    /// # Returns
    /// Some(&[u8]) if there are enough bytes remaining, None otherwise
    fn read_bytes(&mut self, len: usize) -> Option<&'a [u8]> {
        if self.pos + len <= self.data.len() {
            let slice = &self.data[self.pos..self.pos + len];
            self.pos += len;
            Some(slice)
        } else {
            None
        }
    }

    /// Reads the next log entry from the binary data.
    /// 
    /// # Returns
    /// - Some(LogEntry) if a valid entry was read
    /// - None if end of data reached or invalid format
    /// 
    /// # Format
    /// Handles two record types:
    /// 1. Full timestamp (type=1): Updates base timestamp
    /// 2. Normal record (type=0): Contains log entry data
    pub fn read_entry(&mut self) -> Option<LogEntry> {
        if self.pos >= self.data.len() {
            return None;
        }

        let record_type = self.read_bytes(1)?[0];
        
        match record_type {
            0 => { // Normal record
                let relative_ts = self.read_u16()?;
                self.last_relative = relative_ts;
                
                let format_id = self.read_u16()?;
                let payload_len = self.read_u16()? as usize;
                
                let payload = self.read_bytes(payload_len)?.to_vec();

                let timestamp = if let Some(base) = self.base_timestamp {
                    UNIX_EPOCH + Duration::from_micros(base + relative_ts as u64)
                } else {
                    UNIX_EPOCH // Fallback if no base timestamp
                };

                Some(LogEntry {
                    timestamp,
                    format_id,
                    values: payload,
                })
            }
            1 => { // Full timestamp
                let ts = self.read_u64()?;
                self.base_timestamp = Some(ts);
                self.last_relative = 0;
                self.read_entry() // Read next record
            }
            _ => None // Unknown record type
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_reading() {
        // Create test log data
        let mut log_data = Vec::new();
        
        // Full timestamp record
        log_data.push(1); // Record type
        log_data.extend_from_slice(&1234567890u64.to_le_bytes()); // Base timestamp

        // Normal log record
        log_data.push(0); // Record type
        log_data.extend_from_slice(&100u16.to_le_bytes()); // Relative timestamp
        log_data.extend_from_slice(&1u16.to_le_bytes()); // Format ID
        
        // Test payload with mixed types
        let payload_len = 4 + 1 + 4; // i32 + bool + [u8; 4]
        log_data.extend_from_slice(&(payload_len as u16).to_le_bytes());
        log_data.extend_from_slice(&42i32.to_le_bytes());
        log_data.push(1); // true
        log_data.extend_from_slice(&[1, 2, 3, 4]);

        // Read and verify
        let mut reader = LogReader::new(&log_data);
        let entry = reader.read_entry().expect("Failed to read entry");
        
        assert_eq!(entry.format_id, 1);
        
        // Verify payload contents
        let mut pos = 0;
        let id = i32::from_le_bytes(entry.values[pos..pos+4].try_into().unwrap());
        pos += 4;
        let bool_val = entry.values[pos] != 0;
        pos += 1;
        let array = &entry.values[pos..pos+4];
        
        assert_eq!(id, 42);
        assert!(bool_val);
        assert_eq!(array, &[1, 2, 3, 4]);
    }

    #[test]
    fn test_timestamp_handling() {
        let mut log_data = Vec::new();
        
        // Full timestamp
        log_data.push(1);
        log_data.extend_from_slice(&1234567890u64.to_le_bytes());

        // Two records with relative timestamps
        for (rel_ts, fmt_id) in [(100u16, 1u16), (200u16, 2u16)] {
            log_data.push(0);
            log_data.extend_from_slice(&rel_ts.to_le_bytes());
            log_data.extend_from_slice(&fmt_id.to_le_bytes());
            log_data.extend_from_slice(&0u16.to_le_bytes()); // Empty payload
        }

        let mut reader = LogReader::new(&log_data);
        let entry1 = reader.read_entry().unwrap();
        let entry2 = reader.read_entry().unwrap();

        assert_eq!(entry1.format_id, 1);
        assert_eq!(entry2.format_id, 2);
        
        // Verify timestamps are properly calculated
        let ts1 = entry1.timestamp.duration_since(UNIX_EPOCH).unwrap();
        let ts2 = entry2.timestamp.duration_since(UNIX_EPOCH).unwrap();
        assert_eq!(ts2.as_micros() - ts1.as_micros(), 100);
    }
} 