#![forbid(unsafe_code)]
#![warn(clippy::cargo, clippy::suspicious, rust_2018_idioms)]

mod errors;
mod model;
mod server;
mod services;
mod settings;
pub mod tracing;

pub use settings::{CliOptions, Settings};
pub use server::Server;