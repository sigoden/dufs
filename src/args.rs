use anyhow::{bail, Context, Result};
use async_zip::Compression;
use clap::builder::{PossibleValue, PossibleValuesParser};
use clap::{value_parser, Arg, ArgAction, ArgMatches, Command, ValueEnum};
use clap_complete::{generate, Generator, Shell};
use serde::{Deserialize, Deserializer};
use smart_default::SmartDefault;
use std::env;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use crate::auth::AccessControl;
use crate::http_logger::HttpLogger;
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
            Arg::new("serve-path")
                .env("DUFS_SERVE_PATH")
				.hide_env(true)
                .value_parser(value_parser!(PathBuf))
                .help("Specific path to serve [default: .]"),
        )
        .arg(
            Arg::new("config")
                .env("DUFS_CONFIG")
				.hide_env(true)
                .short('c')
                .long("config")
                .value_parser(value_parser!(PathBuf))
                .help("Specify configuration file")
                .value_name("file"),
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
                .value_parser(value_parser!(u16))
                .help("Specify port to listen on [default: 5000]")
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
                .action(ArgAction::Append)
                .value_delimiter(',')
                .help("Hide paths from directory listings, e.g. tmp,*.log,*.lock")
                .value_name("value"),
        )
        .arg(
            Arg::new("auth")
                .env("DUFS_AUTH")
				.hide_env(true)
                .short('a')
                .long("auth")
                .help("Add auth roles, e.g. user:pass@/dir1:rw,/dir2")
                .action(ArgAction::Append)
                .value_name("rules"),
        )
        .arg(
            Arg::new("auth-method")
                .hide(true)
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
                .help("Allow download folders as archive file"),
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
                .help("Set the path to the assets directory for overriding the built-in assets")
                .value_parser(value_parser!(PathBuf))
                .value_name("path")
        )
        .arg(
            Arg::new("log-format")
                .env("DUFS_LOG_FORMAT")
                .hide_env(true)
                .long("log-format")
                .value_name("format")
                .help("Customize http log format"),
        )
        .arg(
            Arg::new("log-file")
                .env("DUFS_LOG_FILE")
                .hide_env(true)
                .long("log-file")
                .value_name("file")
                .value_parser(value_parser!(PathBuf))
                .help("Specify the file to save logs to, other than stdout/stderr"),
        )
        .arg(
            Arg::new("compress")
                .env("DUFS_COMPRESS")
                .hide_env(true)
                .value_parser(clap::builder::EnumValueParser::<Compress>::new())
                .long("compress")
                .value_name("level")
                .help("Set zip compress level [default: low]")
        )
        .arg(
            Arg::new("completions")
                .long("completions")
                .value_name("shell")
                .value_parser(value_parser!(Shell))
                .help("Print shell completion script for <shell>"),
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

    app
}

pub fn print_completions<G: Generator>(gen: G, cmd: &mut Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut std::io::stdout());
}

#[derive(Debug, Deserialize, SmartDefault, PartialEq)]
#[serde(default)]
#[serde(rename_all = "kebab-case")]
pub struct Args {
    #[serde(default = "default_serve_path")]
    #[default(default_serve_path())]
    pub serve_path: PathBuf,
    #[serde(deserialize_with = "deserialize_bind_addrs")]
    #[serde(rename = "bind")]
    #[serde(default = "default_addrs")]
    #[default(default_addrs())]
    pub addrs: Vec<BindAddr>,
    #[serde(default = "default_port")]
    #[default(default_port())]
    pub port: u16,
    #[serde(skip)]
    pub path_is_file: bool,
    pub path_prefix: String,
    #[serde(skip)]
    pub uri_prefix: String,
    #[serde(deserialize_with = "deserialize_string_or_vec")]
    pub hidden: Vec<String>,
    #[serde(deserialize_with = "deserialize_access_control")]
    pub auth: AccessControl,
    pub allow_all: bool,
    pub allow_upload: bool,
    pub allow_delete: bool,
    pub allow_search: bool,
    pub allow_symlink: bool,
    pub allow_archive: bool,
    pub render_index: bool,
    pub render_spa: bool,
    pub render_try_index: bool,
    pub enable_cors: bool,
    pub assets: Option<PathBuf>,
    #[serde(deserialize_with = "deserialize_log_http")]
    #[serde(rename = "log-format")]
    pub http_logger: HttpLogger,
    pub log_file: Option<PathBuf>,
    pub compress: Compress,
    pub tls_cert: Option<PathBuf>,
    pub tls_key: Option<PathBuf>,
}

impl Args {
    /// Parse command-line arguments.
    ///
    /// If a parsing error occurred, exit the process and print out informative
    /// error message to user.
    pub fn parse(matches: ArgMatches) -> Result<Args> {
        let mut args = Self::default();

        if let Some(config_path) = matches.get_one::<PathBuf>("config") {
            let contents = std::fs::read_to_string(config_path)
                .with_context(|| format!("Failed to read config at {}", config_path.display()))?;
            args = serde_yaml::from_str(&contents)
                .with_context(|| format!("Failed to load config at {}", config_path.display()))?;
        }

        if let Some(path) = matches.get_one::<PathBuf>("serve-path") {
            args.serve_path.clone_from(path)
        }

        args.serve_path = Self::sanitize_path(args.serve_path)?;

        if let Some(port) = matches.get_one::<u16>("port") {
            args.port = *port
        }

        if let Some(addrs) = matches.get_many::<String>("bind") {
            let addrs: Vec<_> = addrs.map(|v| v.as_str()).collect();
            args.addrs = BindAddr::parse_addrs(&addrs)?;
        }

        args.path_is_file = args.serve_path.metadata()?.is_file();
        if let Some(path_prefix) = matches.get_one::<String>("path-prefix") {
            args.path_prefix.clone_from(path_prefix)
        }
        args.path_prefix = args.path_prefix.trim_matches('/').to_string();

        args.uri_prefix = if args.path_prefix.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}/", &encode_uri(&args.path_prefix))
        };

        if let Some(hidden) = matches.get_many::<String>("hidden") {
            args.hidden = hidden.cloned().collect();
        } else {
            let mut hidden = vec![];
            std::mem::swap(&mut args.hidden, &mut hidden);
            args.hidden = hidden
                .into_iter()
                .flat_map(|v| v.split(',').map(|v| v.to_string()).collect::<Vec<String>>())
                .collect();
        }

        if !args.enable_cors {
            args.enable_cors = matches.get_flag("enable-cors");
        }

        if let Some(rules) = matches.get_many::<String>("auth") {
            let rules: Vec<_> = rules.map(|v| v.as_str()).collect();
            args.auth = AccessControl::new(&rules)?;
        }

        if !args.allow_all {
            args.allow_all = matches.get_flag("allow-all");
        }

        let allow_all = args.allow_all;

        if !args.allow_upload {
            args.allow_upload = allow_all || matches.get_flag("allow-upload");
        }
        if !args.allow_delete {
            args.allow_delete = allow_all || matches.get_flag("allow-delete");
        }
        if !args.allow_search {
            args.allow_search = allow_all || matches.get_flag("allow-search");
        }
        if !args.allow_symlink {
            args.allow_symlink = allow_all || matches.get_flag("allow-symlink");
        }
        if !args.allow_archive {
            args.allow_archive = allow_all || matches.get_flag("allow-archive");
        }
        if !args.render_index {
            args.render_index = matches.get_flag("render-index");
        }

        if !args.render_try_index {
            args.render_try_index = matches.get_flag("render-try-index");
        }

        if !args.render_spa {
            args.render_spa = matches.get_flag("render-spa");
        }

        if let Some(assets_path) = matches.get_one::<PathBuf>("assets") {
            args.assets = Some(assets_path.clone());
        }

        if let Some(assets_path) = &args.assets {
            args.assets = Some(Args::sanitize_assets_path(assets_path)?);
        }

        if let Some(log_format) = matches.get_one::<String>("log-format") {
            args.http_logger = log_format.parse()?;
        }

        if let Some(log_file) = matches.get_one::<PathBuf>("log-file") {
            args.log_file = Some(log_file.clone());
        }

        if let Some(compress) = matches.get_one::<Compress>("compress") {
            args.compress = *compress;
        }

        #[cfg(feature = "tls")]
        {
            if let Some(tls_cert) = matches.get_one::<PathBuf>("tls-cert") {
                args.tls_cert = Some(tls_cert.clone())
            }

            if let Some(tls_key) = matches.get_one::<PathBuf>("tls-key") {
                args.tls_key = Some(tls_key.clone())
            }

            match (&args.tls_cert, &args.tls_key) {
                (Some(_), Some(_)) => {}
                (Some(_), _) => bail!("No tls-key set"),
                (_, Some(_)) => bail!("No tls-cert set"),
                (None, None) => {}
            }
        }
        #[cfg(not(feature = "tls"))]
        {
            args.tls_cert = None;
            args.tls_key = None;
        }

        Ok(args)
    }

    fn sanitize_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
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

    fn sanitize_assets_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
        let path = Self::sanitize_path(path)?;
        if !path.join("index.html").exists() {
            bail!("Path `{}` doesn't contains index.html", path.display());
        }
        Ok(path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum BindAddr {
    IpAddr(IpAddr),
    #[cfg(unix)]
    SocketPath(String),
}

impl BindAddr {
    fn parse_addrs(addrs: &[&str]) -> Result<Vec<Self>> {
        let mut bind_addrs = vec![];
        #[cfg(not(unix))]
        let mut invalid_addrs = vec![];
        for addr in addrs {
            match addr.parse::<IpAddr>() {
                Ok(v) => {
                    bind_addrs.push(BindAddr::IpAddr(v));
                }
                Err(_) => {
                    #[cfg(unix)]
                    bind_addrs.push(BindAddr::SocketPath(addr.to_string()));
                    #[cfg(not(unix))]
                    invalid_addrs.push(*addr);
                }
            }
        }
        #[cfg(not(unix))]
        if !invalid_addrs.is_empty() {
            bail!("Invalid bind address `{}`", invalid_addrs.join(","));
        }
        Ok(bind_addrs)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Compress {
    None,
    Low,
    Medium,
    High,
}

impl Default for Compress {
    fn default() -> Self {
        Self::Low
    }
}

impl ValueEnum for Compress {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::None, Self::Low, Self::Medium, Self::High]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            Compress::None => PossibleValue::new("none"),
            Compress::Low => PossibleValue::new("low"),
            Compress::Medium => PossibleValue::new("medium"),
            Compress::High => PossibleValue::new("high"),
        })
    }
}

impl Compress {
    pub fn to_compression(self) -> Compression {
        match self {
            Compress::None => Compression::Stored,
            Compress::Low => Compression::Deflate,
            Compress::Medium => Compression::Bz,
            Compress::High => Compression::Xz,
        }
    }
}

fn deserialize_bind_addrs<'de, D>(deserializer: D) -> Result<Vec<BindAddr>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrVec;

    impl<'de> serde::de::Visitor<'de> for StringOrVec {
        type Value = Vec<BindAddr>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string or list of strings")
        }

        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            BindAddr::parse_addrs(&[s]).map_err(serde::de::Error::custom)
        }

        fn visit_seq<S>(self, seq: S) -> Result<Self::Value, S::Error>
        where
            S: serde::de::SeqAccess<'de>,
        {
            let addrs: Vec<&'de str> =
                Deserialize::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))?;
            BindAddr::parse_addrs(&addrs).map_err(serde::de::Error::custom)
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

fn deserialize_string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrVec;

    impl<'de> serde::de::Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string or list of strings")
        }

        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(vec![s.to_owned()])
        }

        fn visit_seq<S>(self, seq: S) -> Result<Self::Value, S::Error>
        where
            S: serde::de::SeqAccess<'de>,
        {
            Deserialize::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

fn deserialize_access_control<'de, D>(deserializer: D) -> Result<AccessControl, D::Error>
where
    D: Deserializer<'de>,
{
    let rules: Vec<&str> = Vec::deserialize(deserializer)?;
    AccessControl::new(&rules).map_err(serde::de::Error::custom)
}

fn deserialize_log_http<'de, D>(deserializer: D) -> Result<HttpLogger, D::Error>
where
    D: Deserializer<'de>,
{
    let value: String = Deserialize::deserialize(deserializer)?;
    value.parse().map_err(serde::de::Error::custom)
}

fn default_serve_path() -> PathBuf {
    PathBuf::from(".")
}

fn default_addrs() -> Vec<BindAddr> {
    BindAddr::parse_addrs(&["0.0.0.0", "::"]).unwrap()
}

fn default_port() -> u16 {
    5000
}

#[cfg(test)]
mod tests {
    use super::*;

    use assert_fs::prelude::*;

    #[test]
    fn test_default() {
        let cli = build_cli();
        let matches = cli.try_get_matches_from(vec![""]).unwrap();
        let args = Args::parse(matches).unwrap();
        let cwd = Args::sanitize_path(std::env::current_dir().unwrap()).unwrap();
        assert_eq!(args.serve_path, cwd);
        assert_eq!(args.port, default_port());
        assert_eq!(args.addrs, default_addrs());
    }

    #[test]
    fn test_args_from_cli1() {
        let tmpdir = assert_fs::TempDir::new().unwrap();
        let cli = build_cli();
        let matches = cli
            .try_get_matches_from(vec![
                "",
                "--hidden",
                "tmp,*.log,*.lock",
                &tmpdir.to_string_lossy(),
            ])
            .unwrap();
        let args = Args::parse(matches).unwrap();
        assert_eq!(args.serve_path, Args::sanitize_path(&tmpdir).unwrap());
        assert_eq!(args.hidden, ["tmp", "*.log", "*.lock"]);
    }

    #[test]
    fn test_args_from_cli2() {
        let cli = build_cli();
        let matches = cli
            .try_get_matches_from(vec![
                "", "--hidden", "tmp", "--hidden", "*.log", "--hidden", "*.lock",
            ])
            .unwrap();
        let args = Args::parse(matches).unwrap();
        assert_eq!(args.hidden, ["tmp", "*.log", "*.lock"]);
    }

    #[test]
    fn test_args_from_empty_config_file() {
        let tmpdir = assert_fs::TempDir::new().unwrap();
        let config_file = tmpdir.child("config.yaml");
        config_file.write_str("").unwrap();

        let cli = build_cli();
        let matches = cli
            .try_get_matches_from(vec!["", "-c", &config_file.to_string_lossy()])
            .unwrap();
        let args = Args::parse(matches).unwrap();
        let cwd = Args::sanitize_path(std::env::current_dir().unwrap()).unwrap();
        assert_eq!(args.serve_path, cwd);
        assert_eq!(args.port, default_port());
        assert_eq!(args.addrs, default_addrs());
    }

    #[test]
    fn test_args_from_config_file1() {
        let tmpdir = assert_fs::TempDir::new().unwrap();
        let config_file = tmpdir.child("config.yaml");
        let contents = format!(
            r#"
serve-path: {}
bind: 0.0.0.0
port: 3000
allow-upload: true
hidden: tmp,*.log,*.lock
"#,
            tmpdir.display()
        );
        config_file.write_str(&contents).unwrap();

        let cli = build_cli();
        let matches = cli
            .try_get_matches_from(vec!["", "-c", &config_file.to_string_lossy()])
            .unwrap();
        let args = Args::parse(matches).unwrap();
        assert_eq!(args.serve_path, Args::sanitize_path(&tmpdir).unwrap());
        assert_eq!(
            args.addrs,
            vec![BindAddr::IpAddr("0.0.0.0".parse().unwrap())]
        );
        assert_eq!(args.hidden, ["tmp", "*.log", "*.lock"]);
        assert_eq!(args.port, 3000);
        assert!(args.allow_upload);
    }

    #[test]
    fn test_args_from_config_file2() {
        let tmpdir = assert_fs::TempDir::new().unwrap();
        let config_file = tmpdir.child("config.yaml");
        let contents = r#"
bind:
  - 127.0.0.1
  - 192.168.8.10
hidden:
  - tmp
  - '*.log'
  - '*.lock'
"#;
        config_file.write_str(contents).unwrap();

        let cli = build_cli();
        let matches = cli
            .try_get_matches_from(vec!["", "-c", &config_file.to_string_lossy()])
            .unwrap();
        let args = Args::parse(matches).unwrap();
        assert_eq!(
            args.addrs,
            vec![
                BindAddr::IpAddr("127.0.0.1".parse().unwrap()),
                BindAddr::IpAddr("192.168.8.10".parse().unwrap())
            ]
        );
        assert_eq!(args.hidden, ["tmp", "*.log", "*.lock"]);
    }
}
