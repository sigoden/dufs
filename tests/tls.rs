mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use predicates::str::contains;
use reqwest::blocking::ClientBuilder;
use rstest::rstest;

use crate::fixtures::port;

/// Can start the server with TLS and receive encrypted responses.
#[rstest]
#[case(server(&[
        "--tls-cert", "tests/data/cert.pem",
        "--tls-key", "tests/data/key_pkcs8.pem",
]))]
#[case(server(&[
        "--tls-cert", "tests/data/cert.pem",
        "--tls-key", "tests/data/key_pkcs1.pem",
]))]
#[case(server(&[
        "--tls-cert", "tests/data/cert_ecdsa.pem",
        "--tls-key", "tests/data/key_ecdsa.pem",
]))]
fn tls_works(#[case] server: TestServer) -> Result<(), Error> {
    let client = ClientBuilder::new()
        .tls_danger_accept_invalid_certs(true)
        .build()?;
    let resp = client.get(server.url()).send()?.error_for_status()?;
    assert_resp_paths!(resp);
    Ok(())
}

/// Wrong path for cert throws error.
#[rstest]
fn wrong_path_cert() -> Result<(), Error> {
    let port = port().to_string();
    assert_cmd::cargo::cargo_bin_cmd!()
        .args([
            "--tls-cert",
            "wrong",
            "--tls-key",
            "tests/data/key.pem",
            "--port",
            &port,
        ])
        .assert()
        .failure()
        .stderr(contains("Failed to load cert file at `wrong`"));

    Ok(())
}

/// Wrong paths for key throws errors.
#[rstest]
fn wrong_path_key() -> Result<(), Error> {
    let port = port().to_string();
    assert_cmd::cargo::cargo_bin_cmd!()
        .args([
            "--tls-cert",
            "tests/data/cert.pem",
            "--tls-key",
            "wrong",
            "--port",
            &port,
        ])
        .assert()
        .failure()
        .stderr(contains("Failed to load key file at `wrong`"));

    Ok(())
}
