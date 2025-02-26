# Binary Logger

A zero-allocation, high-performance binary logging system written in Rust, optimized for extreme throughput and minimal storage requirements.

## Key Features

- **Ultra-Fast Logging**: 30-50x faster than traditional text-based loggers
- **Compact Storage**: 80-100x smaller log files compared to text logs
- **Zero-Allocation Path**: Critical logging path has no memory allocations
- **Per-Thread Design**: One logger per thread for maximum performance
- **Separation of Concerns**: Logger handles memory operations, handler deals with I/O
- **String Deduplication**: Automatic format string interning for improved efficiency
- **Efficient Timestamps**: CPU hardware counters for minimal overhead

## Architectural Advantages

### 1. Thread-Optimized Design
- **One Logger Per Thread**: Each thread uses its own logger instance, eliminating mutex contention
- **No Thread Safety Overhead**: Loggers are deliberately not thread-safe for maximum performance
- **Shared String Registry**: Global string registry is thread-safe and shared across loggers

### 2. Double-Buffering Strategy
- **Zero Waiting**: When one buffer fills, it's swapped with a standby buffer
- **Asynchronous Processing**: Filled buffers are processed in background
- **No Blocking**: Logging never blocks on I/O operations

### 3. Minimal Memory Footprint
- **Binary Format**: Compact encoding significantly reduces space requirements
- **Relative Timestamps**: 16-bit relative timestamps instead of 64-bit absolute values
- **String Deduplication**: Format strings are stored once and referenced by ID

### 4. Flexible I/O Handling
- **Pluggable Handlers**: Implements the BufferHandler trait for custom I/O strategies
- **Separation of Concerns**: Logger focuses on memory operations, handler manages I/O
- **Compression Efficient**: Testing shows LZ4 to be very efficient in compressing the log buffers (To be sent over network or saved to files)

## How It Works

### Binary Format
```
[Record]
+----------------+------------------+----------------+----------------+
| Type (1 byte)  | Rel TS (2 bytes) | Format ID (2B) | Payload (N bytes) |
+----------------+------------------+----------------+----------------+

Type:
- 0: Normal record (relative timestamp)
- 1: Base timestamp record
```

### Logging Flow
1. **Message Preparation**:
   - Format string is registered in string registry (once per string)
   - Parameters are serialized to binary format

2. **Buffer Writing**:
   - Record is written to active buffer with zero allocations
   - When buffer fills, it's swapped with inactive buffer

3. **Asynchronous Processing**:
   - BufferHandler processes filled buffer (typically writes to disk)
   - New records continue writing to the now-active buffer

4. **Reading and Decoding**:
   - LogReader decodes binary format back to structured entries
   - String registry maps IDs back to original format strings

## Usage

### Basic Example

```rust
use binary_logger::{Logger, BufferHandler, log_record};
use std::fs::File;
use std::io::Write;
use std::cell::RefCell;

// Define a custom handler for log buffers (handles file I/O)
struct FileHandler(RefCell<File>);

impl BufferHandler for FileHandler {
    fn handle_switched_out_buffer(&self, buffer: *const u8, size: usize) {
        let data = unsafe { std::slice::from_raw_parts(buffer, size) };
        self.0.borrow_mut().write_all(data).unwrap();
    }
}

// Create a logger with 1MB buffer
let file = File::create("log.bin").unwrap();
let mut logger = Logger::<1_000_000>::new(FileHandler(RefCell::new(file)));

// Log some records
log_record!(logger, "Hello, world!", );
log_record!(logger, "Temperature: {} C", 25.5);
log_record!(logger, "Status: {}, Count: {}", true, 42);

// Ensure logs are flushed before exit
logger.flush();
```

### Reading Logs

```rust
use binary_logger::{LogReader, LogEntry};
use std::fs::File;
use std::io::Read;

// Read a binary log file
let mut file = File::open("log.bin")?;
let mut data = Vec::new();
file.read_to_end(&mut data)?;

// Create a log reader
let mut reader = LogReader::new(&data);

// Read and format log entries
while let Some(entry) = reader.read_entry() {
    println!("[{}] {}", 
        entry.timestamp.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        entry.format());
}
```

### Multi-Threaded Usage

For multi-threaded applications, create one logger per thread:

```rust
use binary_logger::{Logger, BufferHandler, log_record};
use std::thread;

// Create thread-local loggers
thread::scope(|scope| {
    for i in 0..4 {
        scope.spawn(move |_| {
            // Each thread creates its own logger with a unique file
            let file = File::create(format!("thread_{}.log", i)).unwrap();
            let mut logger = Logger::<1_000_000>::new(FileHandler(RefCell::new(file)));
            
            // Thread-local logging
            log_record!(logger, "Thread {} started", i);
            
            // Work...
            
            logger.flush();
        });
    }
});
```

## Core Components

### 1. Binary Logger (`src/binary_logger.rs`)
- `Logger<const CAP: usize>`: Main logging engine with configurable buffer size
- `BufferHandler`: Trait for processing filled buffers (typically I/O operations)
- `log_record!` macro: Primary interface for logging with format strings

### 2. String Registry (`src/string_registry.rs`)
- Thread-safe global registry for string deduplication
- Maps static string literals to compact numeric IDs
- Ensures each unique string is stored only once

### 3. Efficient Clock (`src/efficient_clock.rs`)
- `TimestampConverter`: Manages high-precision timestamping with minimal overhead
- Uses CPU hardware counters (`rdtsc` on x86_64)
- Converts 64-bit timestamps to efficient 16-bit relative values

### 4. Log Reader (`src/log_reader.rs`)
- Decodes binary log files back to structured entries
- Handles reconstructing timestamps and format strings
- Extracts typed parameter values from raw binary data

## Performance Comparison

Our benchmarks (running approximately 40,000 log operations) show:

| Metric | Binary Logger | Traditional Logger (tracing) | Improvement |
|--------|--------------|--------------------------|-------------|
| Speed | ~2.8ms | ~93ms | **33x faster** |
| File Size | 0.31 MB | 31.9 MB | **100x smaller** |

### Performance Scaling
- **Linear scaling** with number of messages
- **Consistent performance** across message sizes
- **Minimal CPU impact** due to hardware timestamp usage

## Best Practices

1. **Buffer Sizing**:
   - Larger buffers (1MB+) for highest throughput
   - Smaller buffers (64KB-256KB) for lower latency

2. **Per-Thread Loggers**:
   - Create one logger per thread
   - Never share loggers between threads

3. **Handler Implementation**:
   - Implement efficient I/O in BufferHandler
   - Consider background thread for I/O operations
   - Add compression in handler if needed

4. **Flush Strategy**:
   - Regular intervals for throughput-focused applications
   - After critical operations for latency-sensitive code

## License

MIT License - See LICENSE file for details 
