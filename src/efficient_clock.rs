#![allow(dead_code)]

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::_rdtsc;

/// High-precision timestamp utilities for efficient logging.
///
/// This module provides mechanisms for generating and managing high-resolution 
/// timestamps with minimal overhead using CPU hardware counters when available.

/// Conversion factor: how many CPU ticks per relative timestamp unit.
/// Adjust this constant to match your CPU and desired resolution.
const TICKS_PER_UNIT: u64 = 30_000;
/// Maximum value that can be stored in 16 bits.
const REL_MAX: u64 = u16::MAX as u64;

/// Converts high-precision timestamps to efficient relative values.
///
/// This struct manages timestamp conversion for binary logging, providing:
/// 
/// 1. Compression - Converts 64-bit absolute timestamps to 16-bit relative values
/// 2. Base resets - Automatically resets the base when relative values overflow
/// 3. Zero overhead - Uses CPU hardware counters for maximum performance
/// 
/// # Examples
/// 
/// ```
/// # use binary_logger::efficient_clock::TimestampConverter;
/// let mut converter = TimestampConverter::new();
/// 
/// // Get a relative timestamp and flag indicating if base was reset
/// let (rel_ts, is_base_ts) = converter.get_relative_timestamp();
/// 
/// // First timestamp will always have is_base_ts == true
/// assert!(is_base_ts);
/// assert_eq!(rel_ts, 0);
/// 
/// // Subsequent timestamps will be relative to the base
/// let (rel_ts2, is_base_ts2) = converter.get_relative_timestamp();
/// assert!(!is_base_ts2);
/// ```
#[derive(Copy, Clone)]
pub struct TimestampConverter {
    current_base: Option<u64>
}

impl TimestampConverter {
    /// Creates a new timestamp converter.
    ///
    /// The new converter has no base timestamp set. The first call to
    /// `get_relative_timestamp()` will set the base and return 0.
    #[inline(always)]
    pub const fn new() -> Self {
        Self { current_base: None }
    }

    /// Gets a relative timestamp and indicates if a new base timestamp was set.
    ///
    /// Returns a tuple containing:
    /// 1. A 16-bit relative timestamp value
    /// 2. A boolean indicating if a new base timestamp was set (true = new base)
    ///
    /// The relative timestamp is calculated as:
    /// `(current_timestamp - base_timestamp) / TICKS_PER_UNIT`
    ///
    /// If the calculated relative value would exceed 16 bits (65535), 
    /// a new base timestamp is set automatically.
    ///
    /// # Returns
    ///
    /// * `(u16, bool)` - The relative timestamp and a flag indicating base reset
    ///
    /// # Examples
    ///
    /// ```
    /// # use binary_logger::efficient_clock::TimestampConverter;
    /// let mut converter = TimestampConverter::new();
    /// let (ts1, is_base1) = converter.get_relative_timestamp();
    /// // First call always sets a new base
    /// assert!(is_base1);
    /// assert_eq!(ts1, 0);
    /// ```
    pub fn get_relative_timestamp(&mut self) -> (u16, bool) {
        let current_ts = get_timestamp();
        let needs_new_base = self.current_base.is_none();
        
        if needs_new_base {
            self.current_base = Some(current_ts);
            return (0, true);
        }

        let base = self.current_base.unwrap();
        let delta_ticks = current_ts.saturating_sub(base);
        let delta = delta_ticks / TICKS_PER_UNIT;

        if delta > REL_MAX {
            self.current_base = Some(current_ts);
            (0, true)
        } else {
            (delta as u16, false)
        }
    }

    /// Gets the current absolute timestamp using the highest precision available.
    ///
    /// This is a thin wrapper over `get_timestamp()` that can be used with
    /// a TimestampConverter instance.
    ///
    /// # Returns
    ///
    /// * `u64` - The current timestamp in CPU-specific units
    pub fn get_current_timestamp(&self) -> u64 {
        get_timestamp()
    }

    /// Resets the base timestamp.
    ///
    /// After calling this method, the next call to `get_relative_timestamp()`
    /// will set a new base and return 0.
    pub fn reset(&mut self) {
        self.current_base = None;
    }
}

/// Returns a monotonic timestamp with the highest precision available.
///
/// This function uses architecture-specific instructions when available:
/// - x86_64: RDTSC instruction (CPU time stamp counter)
/// - aarch64: CNTVCT_EL0 register (ARM virtual counter)
/// - Other platforms: System time with nanosecond precision
///
/// # Returns
///
/// * `u64` - A high-precision timestamp value
///
/// # Performance
///
/// This function is highly optimized and has minimal overhead:
/// - On x86_64: ~25 CPU cycles
/// - On aarch64: ~10-20 CPU cycles
/// - Other platforms: Varies by OS
#[inline(always)]
pub fn get_timestamp() -> u64 {
    #[cfg(target_arch = "x86_64")]
    unsafe { _rdtsc() }

    #[cfg(target_arch = "aarch64")]
    unsafe {
        let mut value: u64;
        std::arch::asm!("mrs {}, cntvct_el0", out(reg) value);
        value
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }
} 