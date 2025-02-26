#![allow(dead_code)]

use std::io;
use std::panic::UnwindSafe;
use crate::efficient_clock::TimestampConverter;

/// Core implementation of the binary logging system.
/// 
/// This module provides the Logger struct and BufferHandler trait for writing
/// extremely high-performance binary logs with minimal overhead.

/// Handler for processing filled logging buffers.
/// 
/// Implementations of this trait determine what happens with log data after
/// a buffer is filled, such as writing to disk, network, or compression.
/// The BufferHandler is responsible for all I/O operations, allowing the Logger
/// to focus exclusively on efficient in-memory logging.
/// 
/// # Usage
/// 
/// ```
/// # use binary_logger::BufferHandler;
/// # use std::fs::File;
/// # use std::io::Write;
/// # use std::cell::RefCell;
/// // Simple file writer handler
/// struct FileHandler(RefCell<File>);
/// 
/// impl BufferHandler for FileHandler {
///     fn handle_switched_out_buffer(&self, buffer: *const u8, size: usize) {
///         let data = unsafe { std::slice::from_raw_parts(buffer, size) };
///         self.0.borrow_mut().write_all(data).unwrap();
///     }
/// }
/// ```
pub trait BufferHandler: UnwindSafe {
    /// Process a filled buffer that has been switched out from the active logger.
    /// 
    /// # Safety
    /// 
    /// The buffer pointer is valid for reading `size` bytes. The handler should
    /// process this data before returning, as the buffer may be reused afterward.
    /// 
    /// # Arguments
    /// 
    /// * `buffer` - Pointer to the start of the buffer data
    /// * `size` - Size of the valid data in the buffer
    fn handle_switched_out_buffer(&self, buffer: *const u8, size: usize);
}

/// A high-performance binary logger that writes log records in a compact binary format.
/// 
/// The Logger uses a double-buffering strategy to achieve maximum throughput:
/// 
/// 1. Logs are written to an active buffer in memory (zero copying)
/// 2. When the active buffer fills up, it's swapped with an inactive buffer
/// 3. The filled buffer is processed by the BufferHandler asynchronously
/// 4. New logs continue writing to the now-active buffer without waiting
/// 
/// # Thread Safety
/// 
/// **Important**: Logger is NOT thread-safe and is designed to be used by a single thread.
/// For multi-threaded applications, create one Logger instance per thread for optimal performance.
/// This design eliminates mutex contention in the logging path for maximum throughput.
/// 
/// # File Handling
/// 
/// The Logger itself does not handle file I/O - this responsibility is delegated to the
/// BufferHandler implementation provided by the user. This separation of concerns allows
/// flexibility in how log data is processed (written to disk, sent over network, compressed, etc.)
/// 
/// # Type Parameters
/// 
/// * `CAP` - The capacity of each buffer in bytes
/// 
/// # Examples
/// 
/// ```
/// # use binary_logger::{Logger, BufferHandler, log_record};
/// # use std::fs::File;
/// # use std::io::Write;
/// # use std::cell::RefCell;
/// # struct FileHandler(RefCell<File>);
/// # impl BufferHandler for FileHandler {
/// #     fn handle_switched_out_buffer(&self, buffer: *const u8, size: usize) {
/// #         let data = unsafe { std::slice::from_raw_parts(buffer, size) };
/// #         self.0.borrow_mut().write_all(data).unwrap();
/// #     }
/// # }
/// // Create a logger with 1MB buffer
/// let file = File::create("log.bin").unwrap();
/// let mut logger = Logger::<1_000_000>::new(FileHandler(RefCell::new(file)));
/// 
/// // Log records using the macro
/// log_record!(logger, "Hello, world!", );
/// log_record!(logger, "Temperature is {} degrees", 25.5);
/// 
/// // Ensure logs are flushed
/// logger.flush();
/// ```
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
    /// Creates a new binary logger with the specified buffer handler.
    /// 
    /// This initializes two buffers of size `CAP` and sets up the logger
    /// to use the provided handler for processing filled buffers.
    /// 
    /// # Arguments
    /// 
    /// * `handler` - Implementation of BufferHandler that processes filled buffers
    /// 
    /// # Examples
    /// 
    /// ```
    /// # use binary_logger::{Logger, BufferHandler};
    /// # use std::fs::File;
    /// # use std::io::Write;
    /// # use std::cell::RefCell;
    /// # struct FileHandler(RefCell<File>);
    /// # impl BufferHandler for FileHandler {
    /// #     fn handle_switched_out_buffer(&self, buffer: *const u8, size: usize) {
    /// #         let data = unsafe { std::slice::from_raw_parts(buffer, size) };
    /// #         self.0.borrow_mut().write_all(data).unwrap();
    /// #     }
    /// # }
    /// let file = File::create("log.bin").unwrap();
    /// let logger = Logger::<1_000_000>::new(FileHandler(RefCell::new(file)));
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

    /// Writes a raw log record to the buffer.
    /// 
    /// This is a low-level method that handles the binary format writing.
    /// In most cases, you should use the `log_record!` macro instead, which
    /// handles format string registration and parameter serialization.
    /// 
    /// # Arguments
    /// 
    /// * `format_id` - The ID of the format string from the string registry
    /// * `payload` - The raw binary payload of the log record
    /// 
    /// # Returns
    /// 
    /// A Result indicating success or an IO error
    /// 
    /// # Binary Format
    /// 
    /// Format: `[type(1) | relative_ts(2) | format_id(2) | payload_len(2) | payload(N)]`
    /// 
    /// Where type:
    /// - 0: Record with relative timestamp
    /// - 1: Record with base timestamp reset
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

    /// Flushes the current buffer, ensuring all data is processed.
    /// 
    /// This method forces the current buffer to be switched and processed
    /// by the handler, even if it's not full. This is useful when you need
    /// to ensure all logs are immediately visible.
    /// 
    /// # Examples
    /// 
    /// ```
    /// # use binary_logger::{Logger, BufferHandler, log_record};
    /// # use std::fs::File;
    /// # use std::io::Write;
    /// # use std::cell::RefCell;
    /// # struct FileHandler(RefCell<File>);
    /// # impl BufferHandler for FileHandler {
    /// #     fn handle_switched_out_buffer(&self, buffer: *const u8, size: usize) {
    /// #         let data = unsafe { std::slice::from_raw_parts(buffer, size) };
    /// #         self.0.borrow_mut().write_all(data).unwrap();
    /// #     }
    /// # }
    /// # let file = File::create("log.bin").unwrap();
    /// # let mut logger = Logger::<1_000_000>::new(FileHandler(RefCell::new(file)));
    /// log_record!(logger, "Critical operation starting", );
    /// // Ensure log is written immediately
    /// logger.flush();
    /// ```
    pub fn flush(&mut self) {
        if self.write_pos > BUFFER_HEADER_SIZE {
            self.switch_buffers();
        }
    }

    /// Switches the active and inactive buffers, and processes the filled buffer.
    /// 
    /// This internal method handles the double-buffering mechanism. When the active
    /// buffer is full or explicitly flushed, this method:
    /// 1. Writes the buffer size header to the filled buffer
    /// 2. Swaps the active and inactive buffers
    /// 3. Calls the handler to process the filled buffer
    /// 4. Resets the write position for the new active buffer
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

/// Logs a record with the given format string and arguments.
/// 
/// This macro is the primary interface for logging. It:
/// 1. Automatically registers and deduplicates format strings
/// 2. Efficiently serializes arguments to binary format
/// 3. Writes the serialized record to the logger
/// 
/// # Arguments
/// 
/// * `logger` - The Logger instance to write to
/// * `fmt` - A format string literal, using `{}` placeholders like in `println!`
/// * `args...` - Zero or more arguments corresponding to placeholders
/// 
/// # Returns
/// 
/// IO Result for the logging operation
/// 
/// # Examples
/// 
/// ```
/// # use binary_logger::{Logger, BufferHandler, log_record};
/// # use std::fs::File;
/// # use std::io::Write;
/// # use std::cell::RefCell;
/// # struct FileHandler(RefCell<File>);
/// # impl BufferHandler for FileHandler {
/// #     fn handle_switched_out_buffer(&self, buffer: *const u8, size: usize) {
/// #         let data = unsafe { std::slice::from_raw_parts(buffer, size) };
/// #         self.0.borrow_mut().write_all(data).unwrap();
/// #     }
/// # }
/// # let file = File::create("log.bin").unwrap();
/// # let mut logger = Logger::<1_000_000>::new(FileHandler(RefCell::new(file)));
/// // Basic usage
/// log_record!(logger, "Hello, world!", );
/// 
/// // With parameters
/// log_record!(logger, "Temperature: {} C", 25.5);
/// log_record!(logger, "Status: {}, Count: {}", true, 42);
/// 
/// // With complex types
/// let values = vec![1, 2, 3];
/// log_record!(logger, "Length: {}", values.len());
/// ```
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

/// Size of the buffer header in bytes
/// 
/// The first 8 bytes of each buffer are used to store the total size
/// of valid data in the buffer. This value is always 8.
const BUFFER_HEADER_SIZE: usize = 8;  // 8 bytes for buffer length


