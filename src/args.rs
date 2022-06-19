use clap::{AppSettings, Arg, ArgMatches, Command};
use rustls::{Certificate, PrivateKey};
use std::env;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use crate::auth::AccessControl;
use crate::tls::{load_certs, load_private_key};
use crate::BoxResult;

fn app() -> Command<'static> {
    Command::new(env!("CARGO_CRATE_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(concat!(
            env!("CARGO_PKG_DESCRIPTION"),
            " - ",
            env!("CARGO_PKG_REPOSITORY")
        ))
        .global_setting(AppSettings::DeriveDisplayOrder)
        .arg(
            Arg::new("bind")
                .short('b')
                .long("bind")
                .help("Specify bind address")
                .multiple_values(true)
                .multiple_occurrences(true)
                .value_name("addr"),
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
            Arg::new("auth")
                .short('a')
                .long("auth")
                .help("Add auth for path")
                .multiple_values(true)
                .multiple_occurrences(true)
                .value_name("rule"),
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
                .help("Allow upload files/folders"),
        )
        .arg(
            Arg::new("allow-delete")
                .long("allow-delete")
                .help("Allow delete files/folders"),
        )
        .arg(
            Arg::new("allow-symlink")
                .long("allow-symlink")
                .help("Allow symlink to files/folders outside root directory"),
        )
        .arg(
            Arg::new("render-index")
                .long("render-index")
                .help("Render index.html when requesting a directory"),
        )
        .arg(
            Arg::new("render-try-index")
                .long("render-try-index")
                .help("Render index.html if it exists when requesting a directory"),
        )
        .arg(
            Arg::new("render-spa")
                .long("render-spa")
                .help("Render for single-page application"),
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

#[derive(Debug, Clone)]
pub struct Args {
    pub addrs: Vec<IpAddr>,
    pub port: u16,
    pub path: PathBuf,
    pub path_prefix: String,
    pub uri_prefix: String,
    pub auth: AccessControl,
    pub allow_upload: bool,
    pub allow_delete: bool,
    pub allow_symlink: bool,
    pub render_index: bool,
    pub render_spa: bool,
    pub render_try_index: bool,
    pub cors: bool,
    pub tls: Option<(Vec<Certificate>, PrivateKey)>,
}

impl Args {
    /// Parse command-line arguments.
    ///
    /// If a parsing error ocurred, exit the process and print out informative
    /// error message to user.
    pub fn parse(matches: ArgMatches) -> BoxResult<Args> {
        let port = matches.value_of_t::<u16>("port")?;
        let addrs = matches
            .values_of("bind")
            .map(|v| v.collect())
            .unwrap_or_else(|| vec!["0.0.0.0", "::"]);
        let addrs: Vec<IpAddr> = Args::parse_addrs(&addrs)?;
        let path = Args::parse_path(matches.value_of_os("path").unwrap_or_default())?;
        let path_prefix = matches
            .value_of("path-prefix")
            .map(|v| v.trim_matches('/').to_owned())
            .unwrap_or_default();
        let uri_prefix = if path_prefix.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}/", &path_prefix)
        };
        let cors = matches.is_present("cors");
        let auth: Vec<&str> = matches
            .values_of("auth")
            .map(|v| v.collect())
            .unwrap_or_default();
        let auth = AccessControl::new(&auth, &uri_prefix)?;
        let allow_upload = matches.is_present("allow-all") || matches.is_present("allow-upload");
        let allow_delete = matches.is_present("allow-all") || matches.is_present("allow-delete");
        let allow_symlink = matches.is_present("allow-all") || matches.is_present("allow-symlink");
        let render_index = matches.is_present("render-index");
        let render_try_index = matches.is_present("render-try-index");
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
            addrs,
            port,
            path,
            path_prefix,
            uri_prefix,
            auth,
            cors,
            allow_delete,
            allow_upload,
            allow_symlink,
            render_index,
            render_try_index,
            render_spa,
            tls,
        })
    }

    fn parse_addrs(addrs: &[&str]) -> BoxResult<Vec<IpAddr>> {
        let mut ip_addrs = vec![];
        let mut invalid_addrs = vec![];
        for addr in addrs {
            match addr.parse::<IpAddr>() {
                Ok(v) => {
                    ip_addrs.push(v);
                }
                Err(_) => {
                    invalid_addrs.push(*addr);
                }
            }
        }
        if !invalid_addrs.is_empty() {
            return Err(format!("Invalid bind address `{}`", invalid_addrs.join(",")).into());
        }
        Ok(ip_addrs)
    }

    fn parse_path<P: AsRef<Path>>(path: P) -> BoxResult<PathBuf> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(format!("Path `{}` doesn't exist", path.display()).into());
        }

        env::current_dir()
            .and_then(|mut p| {
                p.push(path); // If path is absolute, it replaces the current path.
                std::fs::canonicalize(p)
            })
            .map_err(|err| format!("Failed to access path `{}`: {}", path.display(), err,).into())
    }
}
