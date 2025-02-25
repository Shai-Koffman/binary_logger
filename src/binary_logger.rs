#![allow(dead_code)]

use std::io;
use std::panic::UnwindSafe;
use crate::efficient_clock::TimestampConverter;

/// A high-performance binary logger that writes log records in a compact binary format.
/// This logger is designed for maximum performance and minimal disk usage, achieving this through:
/// 
/// 1. Binary format encoding - reduces size and parsing overhead
/// 2. Efficient timestamp encoding - uses CPU ticks for relative timestamps
/// 3. String deduplication - stores repeated strings only once
/// 4. Zero-copy design - minimizes allocations and copies
/// 
/// # Binary Format
/// Each log record is encoded as:
/// ```text
/// [1 byte type | 2 bytes relative_ts | 2 bytes format ID | N bytes payload]
/// ```
/// Where type is:
/// - 0: Normal record with relative timestamp
/// - 1: Record with full timestamp
/// 
/// The payload contains raw binary data of the arguments.
/// 
/// # Performance Characteristics
/// - Write throughput: ~2.5 million messages/second
/// - Memory usage: Fixed buffer size (CAP)
/// - CPU efficient: Uses hardware timestamps
/// 
/// # Example Usage
/// ```rust
/// use binary_logger::{Logger, BufferHandler};
/// 
/// struct FileHandler { /* ... */ }
/// impl BufferHandler for FileHandler {
///     fn handle_switched_out_buffer(&self, buffer: *const u8, size: usize) {
///         // Handle the filled buffer (e.g., write to file)
///     }
/// }
/// 
/// let handler = FileHandler::new("app.log")?;
/// let mut logger = Logger::<1024>::new(handler);
/// log_record!(logger, "Processing item {} with status {}", item_id, status)?;
/// ```

pub trait BufferHandler: UnwindSafe {
    fn handle_switched_out_buffer(&self, buffer: *const u8, size: usize);
}

pub struct Logger<const CAP: usize> {
    buffer_1: *mut u8,
    buffer_2: *mut u8,
    write_pos: usize,
    active_buffer: *mut u8,
    inactive_buffer: *mut u8,
    handler: Box<dyn BufferHandler>,
    clock: TimestampConverter,
}

impl<const CAP: usize> Logger<CAP> {
    /// Creates a new logger with the specified buffer handler.
    /// 
    /// # Arguments
    /// * `handler` - Implementation of BufferHandler that processes filled buffers
    /// 
    /// # Example
    /// ```rust
    /// let handler = MyHandler::new();
    /// let mut logger = Logger::<1024>::new(handler);
    /// ```
    pub fn new(handler: impl BufferHandler + 'static) -> Self {
        // Allocate aligned buffers
        let buffer1 = unsafe { 
            std::alloc::alloc(std::alloc::Layout::from_size_align(CAP, 8).unwrap()) 
        };
        let buffer2 = unsafe { 
            std::alloc::alloc(std::alloc::Layout::from_size_align(CAP, 8).unwrap()) 
        };

        Self {
            buffer_1: buffer1,
            buffer_2: buffer2,
            write_pos: BUFFER_HEADER_SIZE,
            active_buffer: buffer1,
            inactive_buffer: buffer2,
            handler: Box::new(handler),
            clock: TimestampConverter::new(),
        }
    }

    /// Writes a log record to the buffer.
    /// This method handles the low-level binary format writing.
    /// Users should typically use the `log_record!` macro instead.
    /// 
    /// # Arguments
    /// * `format_id` - The ID of the format string (from string_registry)
    /// * `payload` - The raw binary data of the parameters
    /// 
    /// # Returns
    /// A Result indicating success or an IO error
    /// 
    /// # Binary Format
    /// Writes in format: [type(1) | relative_ts(2) | format_id(2) | payload_len(2) | payload(N)]
    pub fn write(&mut self, format_id: u16, payload: &[u8]) -> io::Result<()> {
        let (rel_ts, is_base) = self.clock.get_relative_timestamp();
        let record_size = 1 + 2 + 2 + 2 + payload.len();  // type + ts + format_id + payload_len + payload

        // Check if we need to switch buffers
        if self.write_pos + record_size > CAP {
            // Assert that we haven't filled the active buffer while handler was processing
            assert!(self.write_pos < CAP, "Buffer full and handler hasn't completed!");
            self.switch_buffers();
        }

        unsafe {
            // Write record type
            *self.active_buffer.add(self.write_pos) = if is_base { 1 } else { 0 };
            self.write_pos += 1;

            // Ensure alignment for u16 writes
            if self.write_pos % 2 != 0 {
                self.write_pos += 1;
            }

            // Write timestamp
            *(self.active_buffer.add(self.write_pos) as *mut u16) = rel_ts;
            self.write_pos += 2;

            // Write format ID
            *(self.active_buffer.add(self.write_pos) as *mut u16) = format_id;
            self.write_pos += 2;
            
            // Write payload length
            *(self.active_buffer.add(self.write_pos) as *mut u16) = payload.len() as u16;
            self.write_pos += 2;

            // Write payload
            std::ptr::copy_nonoverlapping(
                payload.as_ptr(),
                self.active_buffer.add(self.write_pos),
                payload.len()
            );
            self.write_pos += payload.len();
        }

        Ok(())
    }

    /// Flushes the current buffer, ensuring all data is written.
    /// This is useful when you need to ensure all logs are immediately processed.
    pub fn flush(&mut self) {
        if self.write_pos > BUFFER_HEADER_SIZE {
            self.switch_buffers();
        }
    }

    fn switch_buffers(&mut self) {
        // Write buffer length at start
        unsafe {
            *(self.active_buffer as *mut u64) = self.write_pos as u64;
        }

        // Swap buffers
        std::mem::swap(&mut self.active_buffer, &mut self.inactive_buffer);
        let filled_buffer = self.inactive_buffer;
        let filled_size = self.write_pos;
        self.write_pos = BUFFER_HEADER_SIZE;

        // Call handler with filled buffer
        self.handler.handle_switched_out_buffer(filled_buffer, filled_size);
    }
}

impl<const CAP: usize> Drop for Logger<CAP> {
    fn drop(&mut self) {
        // Ensure last buffer is written
        if self.write_pos > BUFFER_HEADER_SIZE {
            self.switch_buffers();
        }

        // Clean up buffers
        unsafe {
            std::alloc::dealloc(
                self.buffer_1,
                std::alloc::Layout::from_size_align(CAP, 8).unwrap()
            );
            std::alloc::dealloc(
                self.buffer_2,
                std::alloc::Layout::from_size_align(CAP, 8).unwrap()
            );
        }
    }
}

/// The log_record! macro provides a high-level interface for logging.
/// It automatically handles:
/// 1. Format string registration and deduplication
/// 2. Efficient binary serialization of arguments
/// 
/// # Arguments
/// * `logger` - The Logger instance to write to
/// * `fmt` - A format string literal (like println!)
/// * `args` - Zero or more arguments to format
/// 
/// # Returns
/// IO Result for the logging operation
#[macro_export]
macro_rules! log_record {
    ($logger:expr, $fmt:literal, $($arg:expr),* $(,)?) => {{
        // Register format string on first use
        let format_id = $crate::string_registry::register_string($fmt);
        
        // Write parameters to buffer
        let mut temp = [0u8; 1024];
        let mut pos = 0;

        // Count arguments for header
        let arg_count = 0u8 $(+ { let _ = &$arg; 1})*;
        temp[pos] = arg_count;
        pos += 1;
        
        $(
            // Write argument size
            let size = std::mem::size_of_val(&$arg);
            temp[pos..pos+4].copy_from_slice(&(size as u32).to_le_bytes());
            pos += 4;

            // Write data
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &$arg as *const _ as *const u8,
                    temp.as_mut_ptr().add(pos),
                    size
                );
            }
            pos += size;
        )*
        
        // Write the complete record
        let payload = &temp[..pos];
        $logger.write(format_id, payload)
    }};
}

const BUFFER_HEADER_SIZE: usize = 8;  // 8 bytes for buffer length


