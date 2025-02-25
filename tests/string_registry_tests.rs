use binary_logger::{register_string, get_string};
use std::thread;

static TEST_STR: &str = "Test string";
static DUPLICATE_STR: &str = "Duplicate string";
static CONCURRENT_STR: &str = "Concurrent string";
static UNICODE_STR: &str = "Hello, 世界! 🌍";

#[test]
fn test_string_registration() {
    let id = register_string(TEST_STR);
    assert_eq!(get_string(id).unwrap(), TEST_STR);
}

#[test]
fn test_duplicate_registration() {
    let id1 = register_string(DUPLICATE_STR);
    let id2 = register_string(DUPLICATE_STR);
    assert_eq!(id1, id2, "Same string should get same ID");
}

#[test]
fn test_multiple_strings() {
    static STRINGS: [&str; 3] = ["First", "Second", "Third"];
    let ids: Vec<_> = STRINGS.iter().map(|s| register_string(s)).collect();
    
    // Verify all IDs are different
    for i in 0..ids.len() {
        for j in i+1..ids.len() {
            assert_ne!(ids[i], ids[j], "Different strings should get different IDs");
        }
    }
    
    // Verify all strings can be retrieved
    for (s, id) in STRINGS.iter().zip(ids.iter()) {
        assert_eq!(get_string(*id).unwrap(), *s);
    }
}

#[test]
fn test_invalid_id() {
    assert!(get_string(u16::MAX).is_none(), "Invalid ID should return None");
}

#[test]
fn test_concurrent_registration() {
    let handle = thread::spawn(|| {
        register_string(CONCURRENT_STR)
    });
    
    let id1 = register_string(CONCURRENT_STR);
    let id2 = handle.join().unwrap();
    
    assert_eq!(id1, id2, "Same string registered concurrently should get same ID");
    assert_eq!(get_string(id1).unwrap(), CONCURRENT_STR);
}

#[test]
fn test_long_string() {
    let long_str = Box::leak(vec!["a"; 1000].join("").into_boxed_str());
    let id = register_string(long_str);
    assert_eq!(get_string(id).unwrap(), long_str);
}

#[test]
fn test_empty_string() {
    let id = register_string("");
    assert_eq!(get_string(id).unwrap(), "");
}

#[test]
fn test_unicode_string() {
    let id = register_string(UNICODE_STR);
    assert_eq!(get_string(id).unwrap(), UNICODE_STR);
}

#[test]
fn test_many_registrations() {
    // Create a static array of strings for testing
    let strings: &'static [String] = Box::leak(
        (0..1000)
            .map(|i| format!("String {}", i))
            .collect::<Vec<_>>()
            .into_boxed_slice()
    );
    
    let mut ids = Vec::with_capacity(strings.len());
    
    for s in strings {
        let id = register_string(s);
        ids.push((s, id));
    }
    
    for (s, id) in ids {
        assert_eq!(get_string(id).unwrap(), s);
    }
} 