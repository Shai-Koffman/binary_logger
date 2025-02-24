use std::fmt;

/// A trait for types that can be serialized into the binary log format.
/// This is automatically implemented for all types that implement Display.
pub trait Loggable {
    /// Serializes self into the given buffer, returns number of bytes written.
    fn serialize(&self, buf: &mut [u8]) -> usize;
}

// Generic implementation for Display types
impl<T> Loggable for T where T: fmt::Display {
    fn serialize(&self, buf: &mut [u8]) -> usize {
        let s = self.to_string();
        let len = s.len() as u16;
        buf[0..2].copy_from_slice(&len.to_le_bytes());
        buf[2..2+s.len()].copy_from_slice(s.as_bytes());
        2 + s.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numeric_serialization() {
        let mut buf = [0u8; 32];
        
        // Test i32
        let value = 12345i32;
        let len = value.serialize(&mut buf);
        assert_eq!(len, 7); // 2 bytes length + 5 bytes for "12345"
        assert_eq!(&buf[2..7], b"12345");

        // Test f64
        let value = 3.14159f64;
        let len = value.serialize(&mut buf);
        assert_eq!(&buf[2..9], b"3.14159");
    }

    #[test]
    fn test_string_serialization() {
        let mut buf = [0u8; 32];
        let value = "Hello";
        let len = value.serialize(&mut buf);
        assert_eq!(len, 7); // 2 bytes length + 5 bytes for "Hello"
        assert_eq!(&buf[2..7], b"Hello");
    }

    #[test]
    fn test_bool_serialization() {
        let mut buf = [0u8; 32];
        let value = true;
        let len = value.serialize(&mut buf);
        assert_eq!(len, 6); // 2 bytes length + 4 bytes for "true"
        assert_eq!(&buf[2..6], b"true");
    }
} 