mod args;
mod auth;
mod http_logger;
mod http_utils;
mod logger;
mod server;
mod utils;

#[macro_use]
extern crate log;

use crate::args::{build_cli, print_completions, Args};
use crate::server::Server;
#[cfg(feature = "tls")]
use crate::utils::{load_certs, load_private_key};

use anyhow::{anyhow, Context, Result};
use args::BindAddr;
use clap_complete::Shell;
use futures_util::future::join_all;

use hyper::{body::Incoming, service::service_fn, Request};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
};
use std::net::{IpAddr, SocketAddr, TcpListener as StdTcpListener};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::{net::TcpListener, task::JoinHandle};
#[cfg(feature = "tls")]
use tokio_rustls::{rustls::ServerConfig, TlsAcceptor};

#[tokio::main]
async fn main() -> Result<()> {
    logger::init().map_err(|e| anyhow!("Failed to init logger, {e}"))?;
    let cmd = build_cli();
    let matches = cmd.get_matches();
    if let Some(generator) = matches.get_one::<Shell>("completions") {
        let mut cmd = build_cli();
        print_completions(*generator, &mut cmd);
        return Ok(());
    }
    let args = Args::parse(matches)?;
    let running = Arc::new(AtomicBool::new(true));
    let listening = print_listening(&args)?;
    let handles = serve(args, running.clone())?;
    println!("{listening}");

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

fn serve(args: Args, running: Arc<AtomicBool>) -> Result<Vec<JoinHandle<()>>> {
    let addrs = args.addrs.clone();
    let port = args.port;
    let tls_config = (args.tls_cert.clone(), args.tls_key.clone());
    let server_handle = Arc::new(Server::init(args, running)?);
    let mut handles = vec![];
    for bind_addr in addrs.iter() {
        let server_handle = server_handle.clone();
        match bind_addr {
            BindAddr::Address(ip) => {
                let listener = create_listener(SocketAddr::new(*ip, port))
                    .with_context(|| format!("Failed to bind `{ip}:{port}`"))?;

                match &tls_config {
                    #[cfg(feature = "tls")]
                    (Some(cert_file), Some(key_file)) => {
                        let certs = load_certs(cert_file)?;
                        let key = load_private_key(key_file)?;
                        let mut config = ServerConfig::builder()
                            .with_no_client_auth()
                            .with_single_cert(certs, key)?;
                        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
                        let config = Arc::new(config);
                        let tls_accepter = TlsAcceptor::from(config);

                        let handle = tokio::spawn(async move {
                            loop {
                                let (cnx, addr) = listener.accept().await.unwrap();
                                let Ok(stream) = tls_accepter.accept(cnx).await else {
                                    eprintln!(
                                        "Warning during tls handshake connection from {}",
                                        addr
                                    );
                                    continue;
                                };
                                let stream = TokioIo::new(stream);
                                tokio::spawn(handle_stream(
                                    server_handle.clone(),
                                    stream,
                                    Some(addr),
                                ));
                            }
                        });

                        handles.push(handle);
                    }
                    (None, None) => {
                        let handle = tokio::spawn(async move {
                            loop {
                                let (cnx, addr) = listener.accept().await.unwrap();
                                let stream = TokioIo::new(cnx);
                                tokio::spawn(handle_stream(
                                    server_handle.clone(),
                                    stream,
                                    Some(addr),
                                ));
                            }
                        });
                        handles.push(handle);
                    }
                    _ => {
                        unreachable!()
                    }
                };
            }
            BindAddr::Path(path) => {
                if path.exists() {
                    std::fs::remove_file(path)?;
                }
                #[cfg(unix)]
                {
                    let listener = tokio::net::UnixListener::bind(path)
                        .with_context(|| format!("Failed to bind `{}`", path.display()))?;
                    let handle = tokio::spawn(async move {
                        loop {
                            let (cnx, _) = listener.accept().await.unwrap();
                            let stream = TokioIo::new(cnx);
                            tokio::spawn(handle_stream(server_handle.clone(), stream, None));
                        }
                    });

                    handles.push(handle);
                }
            }
        }
    }
    Ok(handles)
}

async fn handle_stream<T>(handle: Arc<Server>, stream: TokioIo<T>, addr: Option<SocketAddr>)
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let hyper_service =
        service_fn(move |request: Request<Incoming>| handle.clone().call(request, addr));

    let ret = Builder::new(TokioExecutor::new())
        .serve_connection_with_upgrades(stream, hyper_service)
        .await;

    if let Err(err) = ret {
        let scope = match addr {
            Some(addr) => format!(" from {}", addr),
            None => String::new(),
        };
        match err.downcast_ref::<std::io::Error>() {
            Some(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {}
            _ => eprintln!("Warning serving connection{}: {}", scope, err),
        }
    }
}

fn create_listener(addr: SocketAddr) -> Result<TcpListener> {
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
    let listener = TcpListener::from_std(std_listener)?;
    Ok(listener)
}

fn print_listening(args: &Args) -> Result<String> {
    let mut output = String::new();
    let mut bind_addrs = vec![];
    let (mut ipv4, mut ipv6) = (false, false);
    for bind_addr in args.addrs.iter() {
        match bind_addr {
            BindAddr::Address(ip) => {
                if ip.is_unspecified() {
                    if ip.is_ipv6() {
                        ipv6 = true;
                    } else {
                        ipv4 = true;
                    }
                } else {
                    bind_addrs.push(bind_addr.clone());
                }
            }
            _ => bind_addrs.push(bind_addr.clone()),
        }
    }
    if ipv4 || ipv6 {
        let ifaces =
            if_addrs::get_if_addrs().with_context(|| "Failed to get local interface addresses")?;
        for iface in ifaces.into_iter() {
            let local_ip = iface.ip();
            if ipv4 && local_ip.is_ipv4() {
                bind_addrs.push(BindAddr::Address(local_ip))
            }
            if ipv6 && local_ip.is_ipv6() {
                bind_addrs.push(BindAddr::Address(local_ip))
            }
        }
    }
    bind_addrs.sort_unstable();
    let urls = bind_addrs
        .into_iter()
        .map(|bind_addr| match bind_addr {
            BindAddr::Address(addr) => {
                let addr = match addr {
                    IpAddr::V4(_) => format!("{}:{}", addr, args.port),
                    IpAddr::V6(_) => format!("[{}]:{}", addr, args.port),
                };
                let protocol = if args.tls_cert.is_some() {
                    "https"
                } else {
                    "http"
                };
                format!("{}://{}{}", protocol, addr, args.uri_prefix)
            }
            BindAddr::Path(path) => path.display().to_string(),
        })
        .collect::<Vec<_>>();

    if urls.len() == 1 {
        output.push_str(&format!("Listening on {}", urls[0]))
    } else {
        let info = urls
            .iter()
            .map(|v| format!("  {v}"))
            .collect::<Vec<String>>()
            .join("\n");
        output.push_str(&format!("Listening on:\n{info}\n"))
    }

    Ok(output)
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler")
}
