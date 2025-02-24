/// Format string information
#[derive(Debug)]
pub struct FormatInfo {
    pub format_string: &'static str,
    pub format_id: u16,
}

// Helper functions for compile-time format string analysis
#[doc(hidden)]
pub const fn validate_format(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut in_brace = false;

    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                    i += 2;
                    continue;
                }
                if in_brace {
                    return false; // Nested braces not allowed
                }
                in_brace = true;
            }
            b'}' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'}' {
                    i += 2;
                    continue;
                }
                if !in_brace {
                    return false; // Unmatched closing brace
                }
                in_brace = false;
            }
            _ => {}
        }
        i += 1;
    }
    !in_brace
}

/// Macro for compile-time format string registration
#[macro_export]
macro_rules! const_format {
    ($fmt:expr) => {{
        use $crate::log_format_registry::FormatInfo;
        const _: () = assert!($crate::log_format_registry::validate_format($fmt));
        const FORMAT_ID: u16 = $crate::binary_logger::simple_hash($fmt);
        
        FormatInfo {
            format_string: $fmt,
            format_id: FORMAT_ID,
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_time_format() {
        const INFO: FormatInfo = const_format!("Test: {} value={}");
        assert_eq!(INFO.format_string, "Test: {} value={}");
    }

    #[test]
    fn test_format_validation() {
        assert!(validate_format("Test: {} value={}"));
        assert!(!validate_format("Test: {} value={"));  // Unclosed brace
        assert!(!validate_format("Test: } value={}")); // Unopened brace
        assert!(validate_format("Test: {{escaped}} {}")); // Escaped braces
    }
} 