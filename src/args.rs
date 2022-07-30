use clap::{value_parser, AppSettings, Arg, ArgMatches, Command};
use clap_complete::{generate, Generator, Shell};
#[cfg(feature = "tls")]
use rustls::{Certificate, PrivateKey};
use std::env;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use crate::auth::AccessControl;
use crate::auth::AuthMethod;
#[cfg(feature = "tls")]
use crate::tls::{load_certs, load_private_key};
use crate::utils::encode_uri;
use crate::BoxResult;

pub fn build_cli() -> Command<'static> {
    let app = Command::new(env!("CARGO_CRATE_NAME"))
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
                .help("Specific path to serve"),
        )
        .arg(
            Arg::new("path-prefix")
                .long("path-prefix")
                .value_name("path")
                .help("Specify a path prefix"),
        )
        .arg(
            Arg::new("hidden")
                .long("hidden")
                .help("Hide directories from directory listings, separated by `,`")
                .value_name("value"),
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
            Arg::new("auth-method")
                .long("auth-method")
                .help("Select auth method")
                .possible_values(["basic", "digest"])
                .default_value("digest")
                .value_name("value"),
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
            Arg::new("allow-search")
                .long("allow-search")
                .help("Allow search files/folders"),
        )
        .arg(
            Arg::new("allow-symlink")
                .long("allow-symlink")
                .help("Allow symlink to files/folders outside root directory"),
        )
        .arg(
            Arg::new("enable-cors")
                .long("enable-cors")
                .help("Enable CORS, sets `Access-Control-Allow-Origin: *`"),
        )
        .arg(
            Arg::new("render-index")
                .long("render-index")
                .help("Serve index.html when requesting a directory, returns 404 if not found index.html"),
        )
        .arg(
            Arg::new("render-try-index")
                .long("render-try-index")
                .help("Serve index.html when requesting a directory, returns directory listing if not found index.html"),
        )
        .arg(
            Arg::new("render-spa")
                .long("render-spa")
                .help("Serve SPA(Single Page Application)"),
        );

    #[cfg(feature = "tls")]
    let app = app
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
        );

    app.arg(
        Arg::new("no-log")
            .long("no-log")
            .help("Don't log http information"),
    )
    .arg(
        Arg::new("log-format")
            .long("log-format")
            .value_name("format")
            .help("Customize http log format"),
    )
    .arg(
        Arg::new("completions")
            .long("completions")
            .value_name("shell")
            .value_parser(value_parser!(Shell))
            .help("Print shell completion script for <shell>"),
    )
}

pub fn print_completions<G: Generator>(gen: G, cmd: &mut Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut std::io::stdout());
}

#[derive(Debug)]
pub struct Args {
    pub addrs: Vec<IpAddr>,
    pub port: u16,
    pub path: PathBuf,
    pub path_is_file: bool,
    pub path_prefix: String,
    pub uri_prefix: String,
    pub hidden: Vec<String>,
    pub auth_method: AuthMethod,
    pub auth: AccessControl,
    pub allow_upload: bool,
    pub allow_delete: bool,
    pub allow_search: bool,
    pub allow_symlink: bool,
    pub render_index: bool,
    pub render_spa: bool,
    pub render_try_index: bool,
    pub enable_cors: bool,
    #[cfg(feature = "tls")]
    pub tls: Option<(Vec<Certificate>, PrivateKey)>,
    #[cfg(not(feature = "tls"))]
    pub tls: Option<()>,
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
        let path_is_file = path.metadata()?.is_file();
        let path_prefix = matches
            .value_of("path-prefix")
            .map(|v| v.trim_matches('/').to_owned())
            .unwrap_or_default();
        let uri_prefix = if path_prefix.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}/", &encode_uri(&path_prefix))
        };
        let hidden: Vec<String> = matches
            .value_of("hidden")
            .map(|v| v.split(',').map(|x| x.to_string()).collect())
            .unwrap_or_default();
        let enable_cors = matches.is_present("enable-cors");
        let auth: Vec<&str> = matches
            .values_of("auth")
            .map(|v| v.collect())
            .unwrap_or_default();
        let auth_method = match matches.value_of("auth-method").unwrap() {
            "basic" => AuthMethod::Basic,
            _ => AuthMethod::Digest,
        };
        let auth = AccessControl::new(&auth, &uri_prefix)?;
        let allow_upload = matches.is_present("allow-all") || matches.is_present("allow-upload");
        let allow_delete = matches.is_present("allow-all") || matches.is_present("allow-delete");
        let allow_search = matches.is_present("allow-all") || matches.is_present("allow-search");
        let allow_symlink = matches.is_present("allow-all") || matches.is_present("allow-symlink");
        let render_index = matches.is_present("render-index");
        let render_try_index = matches.is_present("render-try-index");
        let render_spa = matches.is_present("render-spa");
        #[cfg(feature = "tls")]
        let tls = match (matches.value_of("tls-cert"), matches.value_of("tls-key")) {
            (Some(certs_file), Some(key_file)) => {
                let certs = load_certs(certs_file)?;
                let key = load_private_key(key_file)?;
                Some((certs, key))
            }
            _ => None,
        };
        #[cfg(not(feature = "tls"))]
        let tls = None;

        Ok(Args {
            addrs,
            port,
            path,
            path_is_file,
            path_prefix,
            uri_prefix,
            hidden,
            auth_method,
            auth,
            enable_cors,
            allow_delete,
            allow_upload,
            allow_search,
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
