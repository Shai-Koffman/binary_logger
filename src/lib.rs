#![feature(generic_const_exprs)]

pub mod binary_logger;
pub mod string_registry;
pub mod log_reader;
pub mod efficient_clock;

pub use binary_logger::{Logger, BufferHandler};
pub use string_registry::{register_string, get_string};
pub use log_reader::{LogReader, LogEntry, LogValue}; 