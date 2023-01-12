#![forbid(unsafe_code)]
#![warn(
    clippy::cargo,
    clippy::suspicious,
    rust_2018_idioms,
)]

pub mod tracing;
mod settings;
mod errors;
mod server;
mod model;
