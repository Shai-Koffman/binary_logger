use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::fmt;

#[derive(Debug)]
pub struct LogValue(String);

impl fmt::Display for LogValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug)]
pub struct LogEntry {
    pub timestamp: SystemTime,
    pub format_id: u16,
    pub values: Vec<LogValue>,
}

impl LogEntry {
    pub fn format(&self) -> String {
        // Just output the raw values since we don't have the format string at runtime
        let mut result = String::new();
        for (i, value) in self.values.iter().enumerate() {
            if i > 0 {
                result.push_str(", ");
            }
            result.push_str(&value.to_string());
        }
        result
    }
}

pub struct LogReader<'a> {
    data: &'a [u8],
    pos: usize,
    base_timestamp: Option<u64>,
    last_relative: u16,
}

impl<'a> LogReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            base_timestamp: None,
            last_relative: 0,
        }
    }

    fn read_u16(&mut self) -> Option<u16> {
        if self.pos + 2 <= self.data.len() {
            let value = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
            self.pos += 2;
            Some(value)
        } else {
            None
        }
    }

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

    fn read_bytes(&mut self, len: usize) -> Option<&'a [u8]> {
        if self.pos + len <= self.data.len() {
            let slice = &self.data[self.pos..self.pos + len];
            self.pos += len;
            Some(slice)
        } else {
            None
        }
    }

    fn read_value(&mut self) -> Option<LogValue> {
        let len = self.read_u16()? as usize;
        let bytes = self.read_bytes(len)?;
        String::from_utf8(bytes.to_vec())
            .ok()
            .map(LogValue)
    }

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
                
                let payload_start = self.pos;
                let mut values = Vec::new();
                
                // Keep reading values until we reach the expected payload length
                while self.pos < payload_start + payload_len {
                    if let Some(value) = self.read_value() {
                        values.push(value);
                    } else {
                        return None;
                    }
                }

                if self.pos != payload_start + payload_len {
                    // Payload length mismatch
                    return None;
                }

                let timestamp = if let Some(base) = self.base_timestamp {
                    UNIX_EPOCH + Duration::from_micros(base + relative_ts as u64)
                } else {
                    UNIX_EPOCH // Fallback if no base timestamp
                };

                Some(LogEntry {
                    timestamp,
                    format_id,
                    values,
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
    use crate::const_format;

    #[test]
    fn test_log_reading() {
        // Example binary log data (you would normally get this from the logger)
        let log_data = [
            // Full timestamp record
            0x01, 0xAF, 0x4D, 0xCF, 0x77, 0x54, 0x3D, 0x03, 0x00,
            // First log record
            0x00, 0x61, 0x05, 0x14, 0x00, 0x0E, 0x00,
            // "User logged in" string
            0x0C, 0x00, b'U', b's', b'e', b'r', b' ', b'l', b'o', b'g', b'g', b'e', b'd', b' ', b'i', b'n',
            // 12345 (i32)
            0x39, 0x30, 0x00, 0x00,
            // Second log record
            0x00, 0xBA, 0x05, 0x0C, 0x00, 0x0C, 0x00,
            // 42 (u32)
            0x2A, 0x00, 0x00, 0x00,
            // "Answer" string
            0x06, 0x00, b'A', b'n', b's', b'w', b'e', b'r'
        ];

        let mut reader = LogReader::new(&log_data);
        
        // Read and verify entries
        while let Some(entry) = reader.read_entry() {
            println!("Log entry at {:?}:", entry.timestamp);
            println!("  {}", entry.format());
        }
    }
} 