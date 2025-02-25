#![feature(generic_const_exprs)]
#![allow(incomplete_features)]

pub mod binary_logger;
pub mod string_registry;
pub mod log_reader;
pub mod efficient_clock;

pub use binary_logger::{Logger, BufferHandler};
pub use string_registry::{register_string, get_string};
pub use log_reader::{LogReader, LogValue, LogEntry}; 