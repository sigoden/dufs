macro_rules! bail {
    ($($tt:tt)*) => {
        return Err(From::from(format!($($tt)*)))
    }
}

#[macro_use]
extern crate log;

mod args;
mod server;

pub type BoxResult<T> = Result<T, Box<dyn std::error::Error>>;

use log::LevelFilter;

use crate::args::{matches, Args};
use crate::server::serve;

#[tokio::main]
async fn main() {
    run().await.unwrap_or_else(handle_err)
}

async fn run() -> BoxResult<()> {
    let args = Args::parse(matches())?;

    if std::env::var("RUST_LOG").is_ok() {
        simple_logger::init()?;
    } else {
        simple_logger::SimpleLogger::default()
            .with_level(LevelFilter::Info)
            .init()?;
    }
    serve(args).await
}

fn handle_err<T>(err: Box<dyn std::error::Error>) -> T {
    eprintln!("Server error: {}", err);
    std::process::exit(1);
}
