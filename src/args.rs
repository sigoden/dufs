use clap::crate_description;
use clap::{Arg, ArgMatches};
use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

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
}

pub fn matches() -> ArgMatches {
    app().get_matches()
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Args {
    pub address: String,
    pub port: u16,
    pub path: PathBuf,
    pub auth: Option<String>,
    pub no_auth_read: bool,
    pub allow_upload: bool,
    pub allow_delete: bool,
    pub allow_symlink: bool,
    pub cors: bool,
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
        let cors = matches.is_present("cors");
        let auth = matches.value_of("auth").map(|v| v.to_owned());
        let no_auth_read = matches.is_present("no-auth-read");
        let allow_upload = matches.is_present("allow-all") || matches.is_present("allow-upload");
        let allow_delete = matches.is_present("allow-all") || matches.is_present("allow-delete");
        let allow_symlink = matches.is_present("allow-all") || matches.is_present("allow-symlink");

        Ok(Args {
            address,
            port,
            path,
            auth,
            no_auth_read,
            cors,
            allow_delete,
            allow_upload,
            allow_symlink,
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
