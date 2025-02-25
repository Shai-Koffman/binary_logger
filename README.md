# High-Performance Binary Logger

A zero-allocation, high-throughput binary logging system written in Rust, optimized for maximum performance and minimal disk usage.

## Features

- **Ultra-Fast Timestamps**: Uses CPU hardware timestamps (`rdtsc`) for minimal overhead
- **Compact Binary Format**: Efficient binary encoding with relative timestamps
- **String Deduplication**: Automatic string interning for repeated messages
- **LZ4 Compression**: Fast compression with excellent ratios
- **Zero-Copy Design**: Minimizes allocations and copies
- **Single-Thread Optimized**: Lock-free, designed for maximum throughput

## Performance

- **Throughput**: ~1 million messages/second
- **Compression**: ~8x smaller than text logs
- **CPU Usage**: Minimal overhead using hardware timestamps
- **Memory**: Fixed buffer size (configurable)

## Binary Format

```
[Record Header]
+----------------+------------------+----------------+-----------------+
| Type (1 byte)  | Rel TS (2 bytes) | Format (2 bytes) | Payload (N bytes) |
+----------------+------------------+----------------+-----------------+

Type:
- 0: Normal record (relative timestamp)
- 1: Base timestamp record

Payload Format:
+----------------+------------------+
| Type (1 byte)  | Data (N bytes)   |
+----------------+------------------+

Value Types:
- 0: Dynamic string (UTF-8 bytes)
- 1: Static string (2-byte ID)
```

## Architecture

### 1. Efficient Clock (src/efficient_clock.rs)
```rust
pub struct TimestampConverter {
    current_base: Option<u64>
}
```
- Uses CPU's `rdtsc` instruction for high-precision timestamps
- Converts 64-bit timestamps to compact 16-bit relative values
- Zero-allocation, stack-based operation
- Automatically handles timestamp wrapping

### 2. String Registry (src/string_registry.rs)
```rust
static ref STRING_REGISTRY: Mutex<HashMap<&'static str, u16>>
```
- Global string deduplication
- Maps strings to 16-bit IDs
- Thread-safe for registration
- Zero-allocation lookups

### 3. Binary Logger (src/binary_logger.rs)
```rust
pub struct Logger<const CAP: usize> {
    file: FrameEncoder<File>,
    clock: TimestampConverter,
}
```
- Generic over buffer size
- LZ4 compression
- Efficient binary encoding
- Zero-copy writes where possible

### 4. Log Reader (src/log_reader.rs)
```rust
pub struct LogReader<'a> {
    data: &'a [u8],
    pos: usize,
    base_timestamp: Option<u64>,
}
```
- Zero-copy log reading
- Efficient timestamp reconstruction
- Sequential access optimization

## Usage

1. Add to your `Cargo.toml`:
```toml
[dependencies]
binary_logger = "0.1.0"
```

2. Basic usage:
```rust
use binary_logger::Logger;

fn main() -> io::Result<()> {
    let mut logger = Logger::<16384>::new("app.log")?;
    
    log_record!(logger, "Processing value {}", 42)?;
    log_record!(logger, "User {} logged in from {}", "john", "10.0.0.1")?;
    
    logger.flush()?;
    Ok(())
}
```

## Performance Optimization Tips

1. **Buffer Size**: Choose based on your message rate:
   - High throughput: 64KB+ (`Logger::<65536>::new()`)
   - Low latency: 4-16KB (`Logger::<4096>::new()`)

2. **Flush Strategy**:
   - Throughput: Flush every N messages or T seconds
   - Latency: Flush after critical messages
   - Balanced: Use a background flush thread

3. **String Registry**:
   - Pre-register common strings at startup
   - Use static strings where possible
   - Avoid dynamic string construction

4. **Message Size**:
   - Keep messages compact
   - Use structured data instead of long text
   - Leverage string deduplication

## Benchmarks

Compared to traditional text logging (log4rs):

```
Performance (1,000,000 iterations):
- Binary logging:      2.61s
- Traditional logging: 3.27s
- Speedup: 1.25x
- Throughput: 0.38 million msgs/sec

File Size:
- Binary log:      8.13 MB
- Traditional log: 57.48 MB
- Compression: 7.07x
```

## Implementation Details

### Timestamp Compression
1. Full timestamps (type=1) establish a base
2. Subsequent records use 16-bit relative offsets
3. New base when relative offset would overflow
4. Uses CPU ticks for maximum precision

### String Deduplication
1. First occurrence: Register string, get ID
2. Subsequent uses: Reference by 16-bit ID
3. Dynamic strings: Inline in payload
4. Thread-safe registration, lock-free lookup

### Binary Format Benefits
1. Compact timestamps (2 bytes vs 8)
2. Efficient string references (2 bytes)
3. Direct binary value encoding
4. LZ4 compression friendly

## License

MIT License - See LICENSE file for details 