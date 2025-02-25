use criterion::{black_box, criterion_group, criterion_main, Criterion};
use binary_logger::{Logger, BufferHandler, log_record};
use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex};
use lz4_flex::frame::FrameEncoder;
use tempfile::tempdir;

struct NullHandler;

impl BufferHandler for NullHandler {
    fn handle_switched_out_buffer(&self, _buffer: *const u8, _size: usize) {
        // Do nothing - for pure memory performance testing
    }
}

struct FileHandler {
    file: Arc<Mutex<FrameEncoder<File>>>,
}

impl BufferHandler for FileHandler {
    fn handle_switched_out_buffer(&self, buffer: *const u8, size: usize) {
        let mut file = self.file.lock().unwrap();
        unsafe {
            file.write_all(std::slice::from_raw_parts(buffer, size)).unwrap();
        }
        file.flush().unwrap();
    }
}

fn bench_memory_logging(c: &mut Criterion) {
    let mut group = c.benchmark_group("Memory Logging");
    
    group.bench_function("simple_integers", |b| {
        let handler = NullHandler;
        let mut logger = Logger::<{ 4 * 1024 * 1024 }>::new(handler);
        
        b.iter(|| {
            for i in 0..1000 {
                log_record!(logger, "Test value: {}", black_box(i)).unwrap();
            }
        });
    });
    
    group.bench_function("mixed_types", |b| {
        let handler = NullHandler;
        let mut logger = Logger::<{ 4 * 1024 * 1024 }>::new(handler);
        
        b.iter(|| {
            for i in 0..1000 {
                log_record!(
                    logger,
                    "Complex log: int={}, bool={}, float={}, str={}",
                    black_box(i),
                    black_box(i % 2 == 0),
                    black_box(i as f64 / 3.14),
                    black_box("test string")
                ).unwrap();
            }
        });
    });
    
    group.finish();
}

fn bench_disk_logging(c: &mut Criterion) {
    let mut group = c.benchmark_group("Disk Logging");
    group.sample_size(10); // Fewer samples for disk I/O
    
    group.bench_function("compressed_file", |b| {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("bench.log");
        let file = Arc::new(Mutex::new(FrameEncoder::new(File::create(&log_path).unwrap())));
        let handler = FileHandler { file };
        let mut logger = Logger::<{ 4 * 1024 * 1024 }>::new(handler);
        
        b.iter(|| {
            for i in 0..1000 {
                log_record!(
                    logger,
                    "Benchmark log: value={}, status={}",
                    black_box(i),
                    black_box(if i % 2 == 0 { "active" } else { "inactive" })
                ).unwrap();
            }
        });
    });
    
    group.finish();
}

criterion_group!(benches, bench_memory_logging, bench_disk_logging);
criterion_main!(benches); 