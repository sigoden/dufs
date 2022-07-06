mod args;
mod auth;
mod logger;
mod server;
mod streamer;
#[cfg(feature = "tls")]
mod tls;
mod utils;

#[macro_use]
extern crate log;

use crate::args::{build_cli, print_completions, Args};
use crate::server::{Request, Server};
#[cfg(feature = "tls")]
use crate::tls::{TlsAcceptor, TlsStream};

use std::net::{IpAddr, SocketAddr, TcpListener as StdTcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap_complete::Shell;
use futures::future::join_all;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use hyper::server::conn::{AddrIncoming, AddrStream};
use hyper::service::{make_service_fn, service_fn};
#[cfg(feature = "tls")]
use rustls::ServerConfig;

pub type BoxResult<T> = Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() {
    run().await.unwrap_or_else(handle_err)
}

async fn run() -> BoxResult<()> {
    logger::init().map_err(|e| format!("Failed to init logger, {}", e))?;
    let cmd = build_cli();
    let matches = cmd.get_matches();
    if let Some(generator) = matches.get_one::<Shell>("completions") {
        let mut cmd = build_cli();
        print_completions(*generator, &mut cmd);
        return Ok(());
    }
    let args = Args::parse(matches)?;
    let args = Arc::new(args);
    let running = Arc::new(AtomicBool::new(true));
    let handles = serve(args.clone(), running.clone())?;
    print_listening(args)?;

    tokio::select! {
        ret = join_all(handles) => {
            for r in ret {
                if let Err(e) = r {
                    error!("{}", e);
                }
            }
            Ok(())
        },
        _ = shutdown_signal() => {
            running.store(false, Ordering::SeqCst);
            Ok(())
        },
    }
}

fn serve(
    args: Arc<Args>,
    running: Arc<AtomicBool>,
) -> BoxResult<Vec<JoinHandle<Result<(), hyper::Error>>>> {
    let inner = Arc::new(Server::new(args.clone(), running));
    let mut handles = vec![];
    let port = args.port;
    for ip in args.addrs.iter() {
        let inner = inner.clone();
        let incoming = create_addr_incoming(SocketAddr::new(*ip, port))
            .map_err(|e| format!("Failed to bind `{}:{}`, {}", ip, port, e))?;
        let serv_func = move |remote_addr: SocketAddr| {
            let inner = inner.clone();
            async move {
                Ok::<_, hyper::Error>(service_fn(move |req: Request| {
                    let inner = inner.clone();
                    inner.call(req, remote_addr)
                }))
            }
        };
        match args.tls.as_ref() {
            #[cfg(feature = "tls")]
            Some((certs, key)) => {
                let config = ServerConfig::builder()
                    .with_safe_defaults()
                    .with_no_client_auth()
                    .with_single_cert(certs.clone(), key.clone())?;
                let config = Arc::new(config);
                let accepter = TlsAcceptor::new(config.clone(), incoming);
                let new_service = make_service_fn(move |socket: &TlsStream| {
                    let remote_addr = socket.remote_addr();
                    serv_func(remote_addr)
                });
                let server = tokio::spawn(hyper::Server::builder(accepter).serve(new_service));
                handles.push(server);
            }
            #[cfg(not(feature = "tls"))]
            Some(_) => {
                unreachable!()
            }
            None => {
                let new_service = make_service_fn(move |socket: &AddrStream| {
                    let remote_addr = socket.remote_addr();
                    serv_func(remote_addr)
                });
                let server = tokio::spawn(hyper::Server::builder(incoming).serve(new_service));
                handles.push(server);
            }
        };
    }
    Ok(handles)
}

fn create_addr_incoming(addr: SocketAddr) -> BoxResult<AddrIncoming> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    if addr.is_ipv6() {
        socket.set_only_v6(true)?;
    }
    socket.set_reuse_address(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024 /* Default backlog */)?;
    let std_listener = StdTcpListener::from(socket);
    std_listener.set_nonblocking(true)?;
    let incoming = AddrIncoming::from_listener(TcpListener::from_std(std_listener)?)?;
    Ok(incoming)
}

fn print_listening(args: Arc<Args>) -> BoxResult<()> {
    let mut addrs = vec![];
    let (mut ipv4, mut ipv6) = (false, false);
    for ip in args.addrs.iter() {
        if ip.is_unspecified() {
            if ip.is_ipv6() {
                ipv6 = true;
            } else {
                ipv4 = true;
            }
        } else {
            addrs.push(*ip);
        }
    }
    if ipv4 || ipv6 {
        let ifaces = if_addrs::get_if_addrs()
            .map_err(|e| format!("Failed to get local interface addresses: {}", e))?;
        for iface in ifaces.into_iter() {
            let local_ip = iface.ip();
            if ipv4 && local_ip.is_ipv4() {
                addrs.push(local_ip)
            }
            if ipv6 && local_ip.is_ipv6() {
                addrs.push(local_ip)
            }
        }
    }
    addrs.sort_unstable();
    let urls = addrs
        .into_iter()
        .map(|addr| match addr {
            IpAddr::V4(_) => format!("{}:{}", addr, args.port),
            IpAddr::V6(_) => format!("[{}]:{}", addr, args.port),
        })
        .map(|addr| match &args.tls {
            Some(_) => format!("https://{}", addr),
            None => format!("http://{}", addr),
        })
        .map(|url| format!("{}{}", url, args.uri_prefix))
        .collect::<Vec<_>>();

    if urls.len() == 1 {
        println!("Listening on {}", urls[0]);
    } else {
        let info = urls
            .iter()
            .map(|v| format!("  {}", v))
            .collect::<Vec<String>>()
            .join("\n");
        println!("Listening on:\n{}\n", info);
    }

    Ok(())
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
