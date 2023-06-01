use anyhow::{bail, Context, Result};
use clap::builder::PossibleValuesParser;
use clap::{value_parser, Arg, ArgAction, ArgMatches, Command};
use clap_complete::{generate, Generator, Shell};
#[cfg(feature = "tls")]
use rustls::{Certificate, PrivateKey};
use std::env;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use crate::auth::AccessControl;
use crate::auth::AuthMethod;
use crate::log_http::{LogHttp, DEFAULT_LOG_FORMAT};
#[cfg(feature = "tls")]
use crate::tls::{load_certs, load_private_key};
use crate::utils::encode_uri;

pub fn build_cli() -> Command {
    let app = Command::new(env!("CARGO_CRATE_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(concat!(
            env!("CARGO_PKG_DESCRIPTION"),
            " - ",
            env!("CARGO_PKG_REPOSITORY")
        ))
        .arg(
            Arg::new("serve_path")
                .env("DUFS_SERVE_PATH")
				.hide_env(true)
                .default_value(".")
                .value_parser(value_parser!(PathBuf))
                .help("Specific path to serve"),
        )
        .arg(
            Arg::new("bind")
                .env("DUFS_BIND")
				.hide_env(true)
                .short('b')
                .long("bind")
                .help("Specify bind address or unix socket")
                .action(ArgAction::Append)
                .value_delimiter(',')
                .value_name("addrs"),
        )
        .arg(
            Arg::new("port")
                .env("DUFS_PORT")
				.hide_env(true)
                .short('p')
                .long("port")
                .default_value("5000")
                .value_parser(value_parser!(u16))
                .help("Specify port to listen on")
                .value_name("port"),
        )
        .arg(
            Arg::new("path-prefix")
                .env("DUFS_PATH_PREFIX")
				.hide_env(true)
                .long("path-prefix")
                .value_name("path")
                .help("Specify a path prefix"),
        )
        .arg(
            Arg::new("hidden")
                .env("DUFS_HIDDEN")
				.hide_env(true)
                .long("hidden")
                .help("Hide paths from directory listings, separated by `,`")
                .value_name("value"),
        )
        .arg(
            Arg::new("auth")
                .env("DUFS_AUTH")
				.hide_env(true)
                .short('a')
                .long("auth")
                .help("Add auth role")
                .action(ArgAction::Append)
                .value_delimiter('|')
                .value_name("rules"),
        )
        .arg(
            Arg::new("auth-method")
                .env("DUFS_AUTH_METHOD")
				.hide_env(true)
                .long("auth-method")
                .help("Select auth method")
                .value_parser(PossibleValuesParser::new(["basic", "digest"]))
                .default_value("digest")
                .value_name("value"),
        )
        .arg(
            Arg::new("allow-all")
                .env("DUFS_ALLOW_ALL")
				.hide_env(true)
                .short('A')
                .long("allow-all")
                .action(ArgAction::SetTrue)
                .help("Allow all operations"),
        )
        .arg(
            Arg::new("allow-upload")
                .env("DUFS_ALLOW_UPLOAD")
				.hide_env(true)
                .long("allow-upload")
                .action(ArgAction::SetTrue)
                .help("Allow upload files/folders"),
        )
        .arg(
            Arg::new("allow-delete")
                .env("DUFS_ALLOW_DELETE")
				.hide_env(true)
                .long("allow-delete")
                .action(ArgAction::SetTrue)
                .help("Allow delete files/folders"),
        )
        .arg(
            Arg::new("allow-search")
                .env("DUFS_ALLOW_SEARCH")
				.hide_env(true)
                .long("allow-search")
                .action(ArgAction::SetTrue)
                .help("Allow search files/folders"),
        )
        .arg(
            Arg::new("allow-symlink")
                .env("DUFS_ALLOW_SYMLINK")
				.hide_env(true)
                .long("allow-symlink")
                .action(ArgAction::SetTrue)
                .help("Allow symlink to files/folders outside root directory"),
        )
        .arg(
            Arg::new("allow-archive")
                .env("DUFS_ALLOW_ARCHIVE")
				.hide_env(true)
                .long("allow-archive")
                .action(ArgAction::SetTrue)
                .help("Allow zip archive generation"),
        )
        .arg(
            Arg::new("enable-cors")
                .env("DUFS_ENABLE_CORS")
				.hide_env(true)
                .long("enable-cors")
                .action(ArgAction::SetTrue)
                .help("Enable CORS, sets `Access-Control-Allow-Origin: *`"),
        )
        .arg(
            Arg::new("render-index")
                .env("DUFS_RENDER_INDEX")
				.hide_env(true)
                .long("render-index")
                .action(ArgAction::SetTrue)
                .help("Serve index.html when requesting a directory, returns 404 if not found index.html"),
        )
        .arg(
            Arg::new("render-try-index")
                .env("DUFS_RENDER_TRY_INDEX")
				.hide_env(true)
                .long("render-try-index")
                .action(ArgAction::SetTrue)
                .help("Serve index.html when requesting a directory, returns directory listing if not found index.html"),
        )
        .arg(
            Arg::new("render-spa")
                .env("DUFS_RENDER_SPA")
				.hide_env(true)
                .long("render-spa")
                .action(ArgAction::SetTrue)
                .help("Serve SPA(Single Page Application)"),
        )
        .arg(
            Arg::new("assets")
                .env("DUFS_ASSETS")
				.hide_env(true)
                .long("assets")
                .help("Use custom assets to override builtin assets")
                .value_parser(value_parser!(PathBuf))
                .value_name("path")
        );

    #[cfg(feature = "tls")]
    let app = app
        .arg(
            Arg::new("tls-cert")
                .env("DUFS_TLS_CERT")
                .hide_env(true)
                .long("tls-cert")
                .value_name("path")
                .value_parser(value_parser!(PathBuf))
                .help("Path to an SSL/TLS certificate to serve with HTTPS"),
        )
        .arg(
            Arg::new("tls-key")
                .env("DUFS_TLS_KEY")
                .hide_env(true)
                .long("tls-key")
                .value_name("path")
                .value_parser(value_parser!(PathBuf))
                .help("Path to the SSL/TLS certificate's private key"),
        );

    app.arg(
        Arg::new("log-format")
            .env("DUFS_LOG_FORMAT")
            .hide_env(true)
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
    pub addrs: Vec<BindAddr>,
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
    pub allow_archive: bool,
    pub render_index: bool,
    pub render_spa: bool,
    pub render_try_index: bool,
    pub enable_cors: bool,
    pub assets_path: Option<PathBuf>,
    pub log_http: LogHttp,
    #[cfg(feature = "tls")]
    pub tls: Option<(Vec<Certificate>, PrivateKey)>,
    #[cfg(not(feature = "tls"))]
    pub tls: Option<()>,
}

impl Args {
    /// Parse command-line arguments.
    ///
    /// If a parsing error occurred, exit the process and print out informative
    /// error message to user.
    pub fn parse(matches: ArgMatches) -> Result<Args> {
        let port = *matches.get_one::<u16>("port").unwrap();
        let addrs = matches
            .get_many::<String>("bind")
            .map(|bind| bind.map(|v| v.as_str()).collect())
            .unwrap_or_else(|| vec!["0.0.0.0", "::"]);
        let addrs: Vec<BindAddr> = Args::parse_addrs(&addrs)?;
        let path = Args::parse_path(matches.get_one::<PathBuf>("serve_path").unwrap())?;
        let path_is_file = path.metadata()?.is_file();
        let path_prefix = matches
            .get_one::<String>("path-prefix")
            .map(|v| v.trim_matches('/').to_owned())
            .unwrap_or_default();
        let uri_prefix = if path_prefix.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}/", &encode_uri(&path_prefix))
        };
        let hidden: Vec<String> = matches
            .get_one::<String>("hidden")
            .map(|v| v.split(',').map(|x| x.to_string()).collect())
            .unwrap_or_default();
        let enable_cors = matches.get_flag("enable-cors");
        let auth: Vec<&str> = matches
            .get_many::<String>("auth")
            .map(|auth| auth.map(|v| v.as_str()).collect())
            .unwrap_or_default();
        let auth_method = match matches.get_one::<String>("auth-method").unwrap().as_str() {
            "basic" => AuthMethod::Basic,
            _ => AuthMethod::Digest,
        };
        let auth = AccessControl::new(&auth)?;
        let allow_upload = matches.get_flag("allow-all") || matches.get_flag("allow-upload");
        let allow_delete = matches.get_flag("allow-all") || matches.get_flag("allow-delete");
        let allow_search = matches.get_flag("allow-all") || matches.get_flag("allow-search");
        let allow_symlink = matches.get_flag("allow-all") || matches.get_flag("allow-symlink");
        let allow_archive = matches.get_flag("allow-all") || matches.get_flag("allow-archive");
        let render_index = matches.get_flag("render-index");
        let render_try_index = matches.get_flag("render-try-index");
        let render_spa = matches.get_flag("render-spa");
        #[cfg(feature = "tls")]
        let tls = match (
            matches.get_one::<PathBuf>("tls-cert"),
            matches.get_one::<PathBuf>("tls-key"),
        ) {
            (Some(certs_file), Some(key_file)) => {
                let certs = load_certs(certs_file)?;
                let key = load_private_key(key_file)?;
                Some((certs, key))
            }
            _ => None,
        };
        #[cfg(not(feature = "tls"))]
        let tls = None;
        let log_http: LogHttp = matches
            .get_one::<String>("log-format")
            .map(|v| v.as_str())
            .unwrap_or(DEFAULT_LOG_FORMAT)
            .parse()?;
        let assets_path = match matches.get_one::<PathBuf>("assets") {
            Some(v) => Some(Args::parse_assets_path(v)?),
            None => None,
        };

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
            allow_archive,
            render_index,
            render_try_index,
            render_spa,
            tls,
            log_http,
            assets_path,
        })
    }

    fn parse_addrs(addrs: &[&str]) -> Result<Vec<BindAddr>> {
        let mut bind_addrs = vec![];
        let mut invalid_addrs = vec![];
        for addr in addrs {
            match addr.parse::<IpAddr>() {
                Ok(v) => {
                    bind_addrs.push(BindAddr::Address(v));
                }
                Err(_) => {
                    if cfg!(unix) {
                        bind_addrs.push(BindAddr::Path(PathBuf::from(addr)));
                    } else {
                        invalid_addrs.push(*addr);
                    }
                }
            }
        }
        if !invalid_addrs.is_empty() {
            bail!("Invalid bind address `{}`", invalid_addrs.join(","));
        }
        Ok(bind_addrs)
    }

    fn parse_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
        let path = path.as_ref();
        if !path.exists() {
            bail!("Path `{}` doesn't exist", path.display());
        }

        env::current_dir()
            .and_then(|mut p| {
                p.push(path); // If path is absolute, it replaces the current path.
                std::fs::canonicalize(p)
            })
            .with_context(|| format!("Failed to access path `{}`", path.display()))
    }

    fn parse_assets_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
        let path = Self::parse_path(path)?;
        if !path.join("index.html").exists() {
            bail!("Path `{}` doesn't contains index.html", path.display());
        }
        Ok(path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum BindAddr {
    Address(IpAddr),
    Path(PathBuf),
}
