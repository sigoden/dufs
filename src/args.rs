use clap::crate_description;
use clap::{Arg, ArgMatches};
use rustls::{Certificate, PrivateKey};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

use crate::BoxResult;

const ABOUT: &str = concat!("\n", crate_description!()); // Add extra newline.

fn app() -> clap::Command<'static> {
    clap::command!()
        .about(ABOUT)
        .arg(
            Arg::new("address")
                .short('b')
                .long("bind")
                .default_value("127.0.0.1")
                .help("Specify bind address")
                .value_name("address"),
        )
        .arg(
            Arg::new("port")
                .short('p')
                .long("port")
                .default_value("5000")
                .help("Specify port to listen on")
                .value_name("port"),
        )
        .arg(
            Arg::new("path")
                .default_value(".")
                .allow_invalid_utf8(true)
                .help("Path to a root directory for serving files"),
        )
        .arg(
            Arg::new("path-prefix")
                .long("path-prefix")
                .value_name("path")
                .help("Specify an url path prefix"),
        )
        .arg(
            Arg::new("allow-all")
                .short('A')
                .long("allow-all")
                .help("Allow all operations"),
        )
        .arg(
            Arg::new("allow-upload")
                .long("allow-upload")
                .help("Allow upload operation"),
        )
        .arg(
            Arg::new("allow-delete")
                .long("allow-delete")
                .help("Allow delete operation"),
        )
        .arg(
            Arg::new("allow-symlink")
                .long("allow-symlink")
                .help("Allow symlink to directories/files outside root directory"),
        )
        .arg(
            Arg::new("render-index")
                .long("render-index")
                .help("Render existing index.html when requesting a directory"),
        )
        .arg(
            Arg::new("render-spa")
                .long("render-spa")
                .help("Render spa, rewrite all not-found requests to `index.html"),
        )
        .arg(
            Arg::new("auth")
                .short('a')
                .long("auth")
                .help("Use HTTP authentication for all operations")
                .value_name("user:pass"),
        )
        .arg(
            Arg::new("no-auth-read")
                .long("no-auth-read")
                .help("Do not authenticate read operations like static serving"),
        )
        .arg(
            Arg::new("cors")
                .long("cors")
                .help("Enable CORS, sets `Access-Control-Allow-Origin: *`"),
        )
        .arg(
            Arg::new("tls-cert")
                .long("tls-cert")
                .value_name("path")
                .help("Path to an SSL/TLS certificate to serve with HTTPS"),
        )
        .arg(
            Arg::new("tls-key")
                .long("tls-key")
                .value_name("path")
                .help("Path to the SSL/TLS certificate's private key"),
        )
}

pub fn matches() -> ArgMatches {
    app().get_matches()
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Args {
    pub address: String,
    pub port: u16,
    pub path: PathBuf,
    pub path_prefix: Option<String>,
    pub auth: Option<String>,
    pub no_auth_read: bool,
    pub allow_upload: bool,
    pub allow_delete: bool,
    pub allow_symlink: bool,
    pub render_index: bool,
    pub render_spa: bool,
    pub cors: bool,
    pub tls: Option<(Vec<Certificate>, PrivateKey)>,
}

impl Args {
    /// Parse command-line arguments.
    ///
    /// If a parsing error ocurred, exit the process and print out informative
    /// error message to user.
    pub fn parse(matches: ArgMatches) -> BoxResult<Args> {
        let address = matches.value_of("address").unwrap_or_default().to_owned();
        let port = matches.value_of_t::<u16>("port")?;
        let path = Args::parse_path(matches.value_of_os("path").unwrap_or_default())?;
        let path_prefix = matches.value_of("path-prefix").map(|v| v.to_owned());
        let cors = matches.is_present("cors");
        let auth = matches.value_of("auth").map(|v| v.to_owned());
        let no_auth_read = matches.is_present("no-auth-read");
        let allow_upload = matches.is_present("allow-all") || matches.is_present("allow-upload");
        let allow_delete = matches.is_present("allow-all") || matches.is_present("allow-delete");
        let allow_symlink = matches.is_present("allow-all") || matches.is_present("allow-symlink");
        let render_index = matches.is_present("render-index");
        let render_spa = matches.is_present("render-spa");
        let tls = match (matches.value_of("tls-cert"), matches.value_of("tls-key")) {
            (Some(certs_file), Some(key_file)) => {
                let certs = load_certs(certs_file)?;
                let key = load_private_key(key_file)?;
                Some((certs, key))
            }
            _ => None,
        };

        Ok(Args {
            address,
            port,
            path,
            path_prefix,
            auth,
            no_auth_read,
            cors,
            allow_delete,
            allow_upload,
            allow_symlink,
            render_index,
            render_spa,
            tls,
        })
    }

    /// Parse path.
    fn parse_path<P: AsRef<Path>>(path: P) -> BoxResult<PathBuf> {
        let path = path.as_ref();
        if !path.exists() {
            bail!("error: path \"{}\" doesn't exist", path.display());
        }

        env::current_dir()
            .and_then(|mut p| {
                p.push(path); // If path is absolute, it replaces the current path.
                std::fs::canonicalize(p)
            })
            .or_else(|err| {
                bail!(
                    "error: failed to access path \"{}\": {}",
                    path.display(),
                    err,
                )
            })
    }

    /// Construct socket address from arguments.
    pub fn address(&self) -> BoxResult<SocketAddr> {
        format!("{}:{}", self.address, self.port)
            .parse()
            .or_else(|err| {
                bail!(
                    "error: invalid address {}:{} : {}",
                    self.address,
                    self.port,
                    err,
                )
            })
    }
}

// Load public certificate from file.
pub fn load_certs(filename: &str) -> BoxResult<Vec<Certificate>> {
    // Open certificate file.
    let certfile =
        fs::File::open(&filename).map_err(|e| format!("Failed to open {}: {}", &filename, e))?;
    let mut reader = io::BufReader::new(certfile);

    // Load and return certificate.
    let certs = rustls_pemfile::certs(&mut reader).map_err(|_| "Failed to load certificate")?;
    if certs.is_empty() {
        return Err("Expected at least one certificate".into());
    }
    Ok(certs.into_iter().map(Certificate).collect())
}

// Load private key from file.
pub fn load_private_key(filename: &str) -> BoxResult<PrivateKey> {
    // Open keyfile.
    let keyfile =
        fs::File::open(&filename).map_err(|e| format!("Failed to open {}: {}", &filename, e))?;
    let mut reader = io::BufReader::new(keyfile);

    // Load and return a single private key.
    let keys = rustls_pemfile::rsa_private_keys(&mut reader)
        .map_err(|e| format!("There was a problem with reading private key: {:?}", e))?;

    if keys.len() != 1 {
        return Err("Expected a single private key".into());
    }
    Ok(PrivateKey(keys[0].to_owned()))
}
