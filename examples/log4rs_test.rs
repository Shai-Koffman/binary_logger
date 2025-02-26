use std::fs;
use log::{info, LevelFilter};
use log4rs::{
    append::rolling_file::RollingFileAppender,
    config::{Appender, Config, Root},
    encode::pattern::PatternEncoder,
};
use log4rs::append::rolling_file::policy::compound::CompoundPolicy;
use log4rs::append::rolling_file::policy::compound::trigger::size::SizeTrigger;
use log4rs::append::rolling_file::policy::compound::roll::fixed_window::FixedWindowRoller;

fn main() {
    // Clean up any existing log files at start
    for entry in fs::read_dir(".").unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let path_str = path.to_string_lossy();
        if path_str.contains("test.log") {
            let _ = fs::remove_file(path);
        }
    }

    // Set up the rolling policy
    let trigger = Box::new(SizeTrigger::new(4 * 1024 * 1024)); // 4MB
    let roller = Box::new(
        FixedWindowRoller::builder()
            .build("test.{}.log", 5)
            .unwrap()
    );
    let policy = Box::new(CompoundPolicy::new(trigger, roller));

    // Create a rolling file appender
    let file_appender = RollingFileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S)} - {l} - {m}{n}")))
        .build("test.log", policy)
        .unwrap();

    // Build the config
    let config = Config::builder()
        .appender(Appender::builder().build("file", Box::new(file_appender)))
        .build(Root::builder()
            .appender("file")
            .build(LevelFilter::Info))
        .unwrap();

    // Initialize the logger
    log4rs::init_config(config).unwrap();

    println!("Writing logs in batches to trigger rotation...");

    // Write logs in batches to better see the rotation happening
    for batch in 0..3 {
        println!("Writing batch {}...", batch + 1);
        for i in 0..20000 {
            info!("Batch {} - Test log message {} with additional text to make the line longer for testing rotation", batch, i);
        }
    }

    println!("Done writing logs. Check the files with: ls -lh test.log*");
} 