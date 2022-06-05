mod args;
mod auth;
mod server;

pub type BoxResult<T> = Result<T, Box<dyn std::error::Error>>;

use crate::args::{encode_uri, matches, Args};
use crate::server::serve;

#[tokio::main]
async fn main() {
    run().await.unwrap_or_else(handle_err)
}

async fn run() -> BoxResult<()> {
    let args = Args::parse(matches())?;
    tokio::select! {
        ret = serve(args) => {
            ret
        },
        _ = shutdown_signal() => {
            Ok(())
        },
    }
}

fn handle_err<T>(err: Box<dyn std::error::Error>) -> T {
    eprintln!("error: {}", err);
    std::process::exit(1);
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler")
}
