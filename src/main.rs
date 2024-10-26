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
use std::time::Duration;
use tokio::time::timeout;
use tokio::{net::TcpListener, task::JoinHandle};
#[cfg(feature = "tls")]
use tokio_rustls::{rustls::ServerConfig, TlsAcceptor};

#[tokio::main]
async fn main() -> Result<()> {
    let cmd = build_cli();
    let matches = cmd.get_matches();
    if let Some(generator) = matches.get_one::<Shell>("completions") {
        let mut cmd = build_cli();
        print_completions(*generator, &mut cmd);
        return Ok(());
    }
    let mut args = Args::parse(matches)?;
    logger::init(args.log_file.clone()).map_err(|e| anyhow!("Failed to init logger, {e}"))?;
    let (new_addrs, print_addrs) = check_addrs(&args)?;
    args.addrs = new_addrs;
    let running = Arc::new(AtomicBool::new(true));
    let listening = print_listening(&args, &print_addrs)?;
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
            BindAddr::IpAddr(ip) => {
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
                        let handshake_timeout = Duration::from_secs(10);

                        let handle = tokio::spawn(async move {
                            loop {
                                let Ok((stream, addr)) = listener.accept().await else {
                                    continue;
                                };
                                let Some(stream) =
                                    timeout(handshake_timeout, tls_accepter.accept(stream))
                                        .await
                                        .ok()
                                        .and_then(|v| v.ok())
                                else {
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
                                let Ok((stream, addr)) = listener.accept().await else {
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
                    _ => {
                        unreachable!()
                    }
                };
            }
            #[cfg(unix)]
            BindAddr::SocketPath(path) => {
                let socket_path = if path.starts_with("@")
                    && cfg!(any(target_os = "linux", target_os = "android"))
                {
                    let mut path_buf = path.as_bytes().to_vec();
                    path_buf[0] = b'\0';
                    unsafe { std::ffi::OsStr::from_encoded_bytes_unchecked(&path_buf) }
                        .to_os_string()
                } else {
                    let _ = std::fs::remove_file(path);
                    path.into()
                };
                let listener = tokio::net::UnixListener::bind(socket_path)
                    .with_context(|| format!("Failed to bind `{}`", path))?;
                let handle = tokio::spawn(async move {
                    loop {
                        let Ok((stream, _addr)) = listener.accept().await else {
                            continue;
                        };
                        let stream = TokioIo::new(stream);
                        tokio::spawn(handle_stream(server_handle.clone(), stream, None));
                    }
                });

                handles.push(handle);
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

    match Builder::new(TokioExecutor::new())
        .serve_connection_with_upgrades(stream, hyper_service)
        .await
    {
        Ok(()) => {}
        Err(_err) => {
            // This error only appears when the client doesn't send a request and terminate the connection.
            //
            // If client sends one request then terminate connection whenever, it doesn't appear.
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

fn check_addrs(args: &Args) -> Result<(Vec<BindAddr>, Vec<BindAddr>)> {
    let mut new_addrs = vec![];
    let mut print_addrs = vec![];
    let (ipv4_addrs, ipv6_addrs) = interface_addrs()?;
    for bind_addr in args.addrs.iter() {
        match bind_addr {
            BindAddr::IpAddr(ip) => match &ip {
                IpAddr::V4(_) => {
                    if !ipv4_addrs.is_empty() {
                        new_addrs.push(bind_addr.clone());
                        if ip.is_unspecified() {
                            print_addrs.extend(ipv4_addrs.clone());
                        } else {
                            print_addrs.push(bind_addr.clone());
                        }
                    }
                }
                IpAddr::V6(_) => {
                    if !ipv6_addrs.is_empty() {
                        new_addrs.push(bind_addr.clone());
                        if ip.is_unspecified() {
                            print_addrs.extend(ipv6_addrs.clone());
                        } else {
                            print_addrs.push(bind_addr.clone())
                        }
                    }
                }
            },
            #[cfg(unix)]
            _ => {
                new_addrs.push(bind_addr.clone());
                print_addrs.push(bind_addr.clone())
            }
        }
    }
    print_addrs.sort_unstable();
    Ok((new_addrs, print_addrs))
}

fn interface_addrs() -> Result<(Vec<BindAddr>, Vec<BindAddr>)> {
    let (mut ipv4_addrs, mut ipv6_addrs) = (vec![], vec![]);
    let ifaces =
        if_addrs::get_if_addrs().with_context(|| "Failed to get local interface addresses")?;
    for iface in ifaces.into_iter() {
        let ip = iface.ip();
        if ip.is_ipv4() {
            ipv4_addrs.push(BindAddr::IpAddr(ip))
        }
        if ip.is_ipv6() {
            ipv6_addrs.push(BindAddr::IpAddr(ip))
        }
    }
    Ok((ipv4_addrs, ipv6_addrs))
}

fn print_listening(args: &Args, print_addrs: &[BindAddr]) -> Result<String> {
    let mut output = String::new();
    let urls = print_addrs
        .iter()
        .map(|bind_addr| match bind_addr {
            BindAddr::IpAddr(addr) => {
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
            #[cfg(unix)]
            BindAddr::SocketPath(path) => path.to_string(),
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
