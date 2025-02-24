#![allow(dead_code)]

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::_rdtsc;

/// Conversion factor: how many CPU ticks per relative timestamp unit.
/// Adjust this constant to match your CPU and desired resolution.
const TICKS_PER_UNIT: u64 = 30_000;
/// Maximum value that can be stored in 16 bits.
const REL_MAX: u64 = u16::MAX as u64;

#[derive(Copy, Clone)]  // Ensure it can be copied on the stack
pub struct TimestampConverter {
    current_base: Option<u64>
}

impl TimestampConverter {
    #[inline(always)]
    pub const fn new() -> Self {  // Make it const constructible
        Self { current_base: None }
    }

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

    pub fn get_current_timestamp(&self) -> u64 {
        get_timestamp()
    }

    pub fn reset(&mut self) {
        self.current_base = None;
    }
}

/// Returns a monotonic timestamp with the highest precision available for the architecture
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