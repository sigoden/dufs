mod fixtures;
mod utils;

use assert_fs::fixture::TempDir;
use fixtures::{port, server, tmpdir, wait_for_port, Error, TestServer, DIR_ASSETS};
use rstest::rstest;
use std::process::{Command, Stdio};

#[rstest]
fn assets(server: TestServer) -> Result<(), Error> {
    let ver = env!("CARGO_PKG_VERSION");
    let resp = reqwest::blocking::get(server.url())?;
    let index_js = format!("/__dufs_v{ver}__/index.js");
    let index_css = format!("/__dufs_v{ver}__/index.css");
    let favicon_ico = format!("/__dufs_v{ver}__/favicon.ico");
    let text = resp.text()?;
    println!("{text}");
    assert!(text.contains(&format!(r#"href="{index_css}""#)));
    assert!(text.contains(&format!(r#"href="{favicon_ico}""#)));
    assert!(text.contains(&format!(r#"src="{index_js}""#)));
    Ok(())
}

#[rstest]
fn asset_js(server: TestServer) -> Result<(), Error> {
    let url = format!(
        "{}__dufs_v{}__/index.js",
        server.url(),
        env!("CARGO_PKG_VERSION")
    );
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/javascript; charset=UTF-8"
    );
    Ok(())
}

#[rstest]
fn asset_css(server: TestServer) -> Result<(), Error> {
    let url = format!(
        "{}__dufs_v{}__/index.css",
        server.url(),
        env!("CARGO_PKG_VERSION")
    );
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/css; charset=UTF-8"
    );
    Ok(())
}

#[rstest]
fn asset_ico(server: TestServer) -> Result<(), Error> {
    let url = format!(
        "{}__dufs_v{}__/favicon.ico",
        server.url(),
        env!("CARGO_PKG_VERSION")
    );
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/x-icon");
    Ok(())
}

#[rstest]
fn assets_with_prefix(#[with(&["--path-prefix", "xyz"])] server: TestServer) -> Result<(), Error> {
    let ver = env!("CARGO_PKG_VERSION");
    let resp = reqwest::blocking::get(format!("{}xyz/", server.url()))?;
    let index_js = format!("/xyz/__dufs_v{ver}__/index.js");
    let index_css = format!("/xyz/__dufs_v{ver}__/index.css");
    let favicon_ico = format!("/xyz/__dufs_v{ver}__/favicon.ico");
    let text = resp.text()?;
    assert!(text.contains(&format!(r#"href="{index_css}""#)));
    assert!(text.contains(&format!(r#"href="{favicon_ico}""#)));
    assert!(text.contains(&format!(r#"src="{index_js}""#)));
    Ok(())
}

#[rstest]
fn asset_js_with_prefix(
    #[with(&["--path-prefix", "xyz"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!(
        "{}xyz/__dufs_v{}__/index.js",
        server.url(),
        env!("CARGO_PKG_VERSION")
    );
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/javascript; charset=UTF-8"
    );
    Ok(())
}

#[rstest]
fn assets_override(tmpdir: TempDir, port: u16) -> Result<(), Error> {
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .arg("--assets")
        .arg(tmpdir.join(DIR_ASSETS))
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(port);

    let url = format!("http://localhost:{port}");
    let resp = reqwest::blocking::get(&url)?;
    assert!(resp.text()?.starts_with(&format!(
        "/__dufs_v{}__/index.js;<template id=\"index-data\">",
        env!("CARGO_PKG_VERSION")
    )));
    let resp = reqwest::blocking::get(&url)?;
    assert_resp_paths!(resp);

    child.kill()?;
    Ok(())
}

#[rstest]
#[case("", true)]
#[case("drive/nested", true)]
#[case("", false)]
fn assets_override_not_found_page(
    tmpdir: TempDir,
    port: u16,
    #[case] prefix: &str,
    #[case] has_placeholder: bool,
) -> Result<(), Error> {
    let template = "<html><head><link href=\"__ASSETS_PREFIX__favicon.ico\"></head><body>世界 <a href=\"__ASSETS_PREFIX__index.js\">asset</a></body></html>";
    let not_found_html = if has_placeholder {
        template
    } else {
        "<html><body>custom 404 page</body></html>"
    };
    std::fs::write(
        tmpdir.join(format!("{}404.html", DIR_ASSETS)),
        not_found_html,
    )?;

    std::fs::write(tmpdir.join(format!("{}favicon.ico", DIR_ASSETS)), b"icon")?;

    let child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .arg("--assets")
        .arg(tmpdir.join(DIR_ASSETS))
        .arg("--path-prefix")
        .arg(prefix)
        .stdout(Stdio::null())
        .spawn()?;
    let server = TestServer::new(port, tmpdir, child, false);
    wait_for_port(port);

    let uri_prefix = if prefix.is_empty() {
        "/".to_string()
    } else {
        format!("/{prefix}/")
    };
    let assets_prefix = format!("{uri_prefix}__dufs_v{}__/", env!("CARGO_PKG_VERSION"));
    let expected = not_found_html.replace("__ASSETS_PREFIX__", &assets_prefix);
    let url = format!("http://localhost:{port}{uri_prefix}missing-path");
    let client = reqwest::blocking::Client::new();
    let resp = client.get(&url).send()?;
    assert_eq!(resp.status(), 404);
    assert_eq!(resp.headers()["content-length"], expected.len().to_string());
    assert_eq!(
        resp.headers()["content-type"]
            .to_str()?
            .to_ascii_lowercase(),
        "text/html; charset=utf-8"
    );
    assert_eq!(resp.text()?, expected);

    let resp = client.head(&url).send()?;
    assert_eq!(resp.status(), 404);
    assert_eq!(resp.headers()["content-length"], expected.len().to_string());
    assert!(resp.bytes()?.is_empty());

    let resp = client.get(format!("{url}?noscript")).send()?;
    assert_eq!(resp.status(), 404);
    assert_eq!(resp.text()?, "Not Found");

    // The same prefix must resolve to a real asset, not just look plausible in HTML.
    let resp = client
        .get(format!("http://localhost:{port}{assets_prefix}favicon.ico"))
        .send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.bytes()?.as_ref(), b"icon");
    drop(server);
    Ok(())
}
