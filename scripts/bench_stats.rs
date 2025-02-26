use std::process::Command;
use std::io::{self, Write};
use std::time::Instant;
use std::env;

fn main() -> io::Result<()> {
    let num_runs = env::args()
        .nth(1)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10);
    
    println!("Running benchmark {} times...", num_runs);
    
    let mut binary_times = Vec::with_capacity(num_runs);
    let mut traditional_times = Vec::with_capacity(num_runs);
    let mut binary_sizes = Vec::with_capacity(num_runs);
    let mut traditional_sizes = Vec::with_capacity(num_runs);
    let mut speedups = Vec::with_capacity(num_runs);
    let mut size_ratios = Vec::with_capacity(num_runs);
    
    let start_time = Instant::now();
    
    for i in 1..=num_runs {
        print!("Run {}/{}... ", i, num_runs);
        io::stdout().flush()?;
        
        let output = Command::new("cargo")
            .args(["run", "--release", "--bin", "perf_tests"])
            .env("SINGLE_ITERATION", "1")
            .output()?;
        
        if !output.status.success() {
            eprintln!("Benchmark failed on run {}", i);
            continue;
        }
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        
        // Parse binary logging time
        let binary_time = parse_value(&output_str, "Binary logging: ", "ms");
        if let Some(time) = binary_time {
            binary_times.push(time);
        }
        
        // Parse traditional logging time
        let traditional_time = parse_value(&output_str, "Traditional logging: ", "ms");
        if let Some(time) = traditional_time {
            traditional_times.push(time);
        }
        
        // Parse binary log size
        let binary_size = parse_value(&output_str, "Binary log size: ", " MB");
        if let Some(size) = binary_size {
            binary_sizes.push(size);
        }
        
        // Parse traditional log size
        let traditional_size = parse_value(&output_str, "Traditional log size: ", " MB");
        if let Some(size) = traditional_size {
            traditional_sizes.push(size);
        }
        
        // Calculate speedup and size ratio
        if let (Some(binary), Some(traditional)) = (binary_time, traditional_time) {
            let speedup = traditional / binary;
            speedups.push(speedup);
            println!("Speedup: {:.2}x", speedup);
        }
        
        if let (Some(binary), Some(traditional)) = (binary_size, traditional_size) {
            let ratio = traditional / binary;
            size_ratios.push(ratio);
        }
    }
    
    let elapsed = start_time.elapsed();
    println!("\nCompleted {} runs in {:.2?}", binary_times.len(), elapsed);
    
    // Calculate and display statistics
    println!("\n===== PERFORMANCE SUMMARY =====");
    
    if !binary_times.is_empty() {
        let stats = calculate_stats(&binary_times);
        println!("\nBinary Logging Time (ms):");
        print_stats(stats);
    }
    
    if !traditional_times.is_empty() {
        let stats = calculate_stats(&traditional_times);
        println!("\nTraditional Logging Time (ms):");
        print_stats(stats);
    }
    
    if !speedups.is_empty() {
        let stats = calculate_stats(&speedups);
        println!("\nSpeedup (Traditional/Binary):");
        print_stats(stats);
    }
    
    if !binary_sizes.is_empty() {
        let stats = calculate_stats(&binary_sizes);
        println!("\nBinary Log Size (MB):");
        print_stats(stats);
    }
    
    if !traditional_sizes.is_empty() {
        let stats = calculate_stats(&traditional_sizes);
        println!("\nTraditional Log Size (MB):");
        print_stats(stats);
    }
    
    if !size_ratios.is_empty() {
        let stats = calculate_stats(&size_ratios);
        println!("\nSize Ratio (Traditional/Binary):");
        print_stats(stats);
    }
    
    Ok(())
}

fn parse_value(text: &str, prefix: &str, suffix: &str) -> Option<f64> {
    text.lines()
        .find(|line| line.contains(prefix))
        .and_then(|line| {
            let start = line.find(prefix)? + prefix.len();
            let end = line[start..].find(suffix)?;
            line[start..start + end].parse::<f64>().ok()
        })
}

struct Stats {
    min: f64,
    max: f64,
    mean: f64,
    median: f64,
    std_dev: f64,
    std_dev_percent: f64,
}

fn calculate_stats(values: &[f64]) -> Stats {
    if values.is_empty() {
        return Stats {
            min: 0.0,
            max: 0.0,
            mean: 0.0,
            median: 0.0,
            std_dev: 0.0,
            std_dev_percent: 0.0,
        };
    }
    
    let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
    let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
    
    let sum: f64 = values.iter().sum();
    let count = values.len() as f64;
    let mean = sum / count;
    
    // Calculate median
    let mut sorted_values = values.to_vec();
    sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = if values.len() % 2 == 0 {
        let mid = values.len() / 2;
        (sorted_values[mid - 1] + sorted_values[mid]) / 2.0
    } else {
        sorted_values[values.len() / 2]
    };
    
    // Calculate standard deviation
    let variance = values.iter()
        .map(|&value| {
            let diff = mean - value;
            diff * diff
        })
        .sum::<f64>() / count;
    
    let std_dev = variance.sqrt();
    let std_dev_percent = if mean != 0.0 { (std_dev / mean) * 100.0 } else { 0.0 };
    
    Stats {
        min,
        max,
        mean,
        median,
        std_dev,
        std_dev_percent,
    }
}

fn print_stats(stats: Stats) {
    println!("  Min: {:.3}", stats.min);
    println!("  Max: {:.3}", stats.max);
    println!("  Mean: {:.3}", stats.mean);
    println!("  Median: {:.3}", stats.median);
    println!("  Std Dev: {:.3} ({:.2}% of mean)", stats.std_dev, stats.std_dev_percent);
    println!("  Range: {:.3} - {:.3}", stats.min, stats.max);
} 