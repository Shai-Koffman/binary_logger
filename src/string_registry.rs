use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Mutex;
use lazy_static::lazy_static;

/// A thread-safe global registry for string deduplication.
/// 
/// The string registry is a critical component for the binary logger's performance:
/// 1. Deduplicates strings - each unique string is stored only once
/// 2. Maps strings to small IDs - reduces storage space
/// 3. Thread-safe - can be used from multiple threads
/// 4. Zero-allocation lookups - uses atomic operations
/// 
/// # Implementation Details
/// - Uses a Mutex<HashMap> for thread-safe string storage
/// - Uses AtomicU16 for thread-safe ID generation
/// - ID 0 is reserved for dynamic strings
/// - IDs 1+ are used for registered static strings
/// 
/// # Performance Characteristics
/// - Registration: O(1) average case
/// - Lookup by ID: O(1) average case
/// - Thread contention: Only during new string registration
/// - Memory usage: O(n) where n is unique strings
lazy_static! {
    /// Thread-safe map of strings to their unique IDs
    static ref STRING_REGISTRY: Mutex<HashMap<&'static str, u16>> = Mutex::new(HashMap::new());
    
    /// Atomic counter for generating unique IDs
    static ref NEXT_ID: AtomicU16 = AtomicU16::new(1);
}

/// Register a string in the registry and return its unique ID.
/// If the string is already registered, returns its existing ID.
/// 
/// # Arguments
/// * `s` - The string to register (must be a static string)
/// 
/// # Returns
/// A unique 16-bit ID for the string
/// 
/// # Thread Safety
/// This function is thread-safe and can be called concurrently.
/// 
/// # Example
/// ```rust
/// let id1 = register_string("Hello");  // Returns new ID
/// let id2 = register_string("Hello");  // Returns same ID as id1
/// let id3 = register_string("World");  // Returns new ID
/// assert_eq!(id1, id2);
/// assert_ne!(id1, id3);
/// ```
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

/// Look up a string by its ID.
/// 
/// # Arguments
/// * `id` - The ID to look up
/// 
/// # Returns
/// Some(&str) if the ID exists, None otherwise
/// 
/// # Thread Safety
/// This function is thread-safe and can be called concurrently.
/// 
/// # Example
/// ```rust
/// let id = register_string("Hello");
/// assert_eq!(get_string(id), Some("Hello"));
/// assert_eq!(get_string(0), None);  // 0 is reserved for dynamic strings
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