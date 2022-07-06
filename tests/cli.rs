//! Run cli with different args, not starting a server

mod fixtures;

use assert_cmd::prelude::*;
use clap::ValueEnum;
use clap_complete::Shell;
use fixtures::Error;
use std::process::Command;

#[test]
/// Show help and exit.
fn help_shows() -> Result<(), Error> {
    Command::cargo_bin("dufs")?.arg("-h").assert().success();

    Ok(())
}

#[test]
/// Print completions and exit.
fn print_completions() -> Result<(), Error> {
    // let shell_enums = EnumValueParser::<Shell>::new();
    for shell in Shell::value_variants() {
        Command::cargo_bin("dufs")?
            .arg("--completions")
            .arg(shell.to_string())
            .assert()
            .success();
    }

    Ok(())
}
