use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Mutex;
use lazy_static::lazy_static;

/// String deduplication registry for efficient binary logging.
///
/// This module provides functionality to deduplicate strings in the binary
/// logging system, mapping them to compact numeric IDs that require much less
/// storage space. Unlike the Logger itself, the string registry is thread-safe
/// and can be safely accessed from multiple threads simultaneously.
///
/// # Thread Safety
///
/// While each thread should have its own Logger instance, all threads share the
/// same string registry. The registry uses a mutex and atomic operations to ensure
/// thread-safety.

lazy_static! {
    /// A thread-safe global registry for string deduplication.
    /// 
    /// Maps static string literals to unique 16-bit IDs for efficient storage.
    /// The registry ensures each unique string is stored only once, regardless
    /// of how many times it appears in logs.
    static ref STRING_REGISTRY: Mutex<HashMap<&'static str, u16>> = Mutex::new(HashMap::new());
    
    /// Atomic counter for generating unique string IDs.
    /// 
    /// Starts at 1 because ID 0 is reserved for special cases.
    static ref NEXT_ID: AtomicU16 = AtomicU16::new(1);
}

/// Registers a string in the registry and returns its unique ID.
/// 
/// This function is the core of the string deduplication system. When a format
/// string is first used in logging, it's registered here to get a compact ID.
/// Subsequent usages of the same string reuse this ID, saving space in the log.
/// 
/// # How It Works
/// 
/// 1. First, checks if the string is already registered (fast path)
/// 2. If not, atomically generates a new ID and stores the mapping
/// 3. Returns the ID (either existing or newly generated)
/// 
/// # Arguments
/// 
/// * `s` - A static string literal to register (must be `&'static str`)
/// 
/// # Returns
/// 
/// A unique 16-bit ID for the string
/// 
/// # Thread Safety
/// 
/// This function is thread-safe and can be called concurrently from multiple
/// threads without additional synchronization.
/// 
/// # Examples
/// 
/// ```
/// # use binary_logger::string_registry::register_string;
/// // First registration returns a new ID
/// let id1 = register_string("Hello, world!");
/// 
/// // Registering the same string again returns the same ID
/// let id2 = register_string("Hello, world!");
/// assert_eq!(id1, id2);
/// 
/// // Different strings get different IDs
/// let id3 = register_string("Different message");
/// assert_ne!(id1, id3);
/// ```
#[allow(dead_code)]
pub fn register_string(s: &'static str) -> u16 {
    // Fast path: check if string is already registered
    let mut registry = STRING_REGISTRY.lock().unwrap();
    if let Some(&id) = registry.get(s) {
        return id;
    }
    
    // Slow path: register new string
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    registry.insert(s, id);
    id
}

/// Looks up a string by its ID.
/// 
/// This function is used primarily by the log reader to retrieve the format
/// string associated with an ID found in a log record.
/// 
/// # Arguments
/// 
/// * `id` - The 16-bit string ID to look up
/// 
/// # Returns
/// 
/// * `Some(&'static str)` - The string associated with the ID
/// * `None` - If no string with that ID exists, or if ID is 0 (reserved)
/// 
/// # Thread Safety
/// 
/// This function is thread-safe and can be called concurrently from multiple
/// threads without additional synchronization.
/// 
/// # Examples
/// 
/// ```
/// # use binary_logger::string_registry::{register_string, get_string};
/// // Register a string and get its ID
/// let message = "Temperature alert";
/// let id = register_string(message);
/// 
/// // Later, look up the string by ID
/// let retrieved = get_string(id);
/// assert_eq!(retrieved, Some(message));
/// 
/// // Looking up an unregistered ID returns None
/// let not_found = get_string(65535);
/// assert_eq!(not_found, None);
/// ```
pub fn get_string(id: u16) -> Option<&'static str> {
    if id == 0 {
        return None; // Reserved for dynamic strings
    }
    
    let registry = STRING_REGISTRY.lock().unwrap();
    registry.iter()
        .find(|(_, &stored_id)| stored_id == id)
        .map(|(&s, _)| s)
} 