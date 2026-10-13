mod fixtures;
mod utils;

use assert_fs::fixture::TempDir;
use fixtures::{port, server, tmpdir, wait_for_port, Error, Port, TestServer, DIR_ASSETS};
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
fn head_asset_js(server: TestServer) -> Result<(), Error> {
    let url = format!(
        "{}__dufs_v{}__/index.js",
        server.url(),
        env!("CARGO_PKG_VERSION")
    );
    let len = reqwest::blocking::get(&url)?.text()?.len();
    let resp = fetch!(b"HEAD", &url).send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/javascript; charset=UTF-8"
    );
    assert_eq!(
        resp.headers()
            .get("content-length")
            .unwrap()
            .to_str()?
            .parse::<usize>()?,
        len
    );
    assert_eq!(resp.text()?, "");
    Ok(())
}

#[rstest]
fn head_asset_css(server: TestServer) -> Result<(), Error> {
    let url = format!(
        "{}__dufs_v{}__/index.css",
        server.url(),
        env!("CARGO_PKG_VERSION")
    );
    let len = reqwest::blocking::get(&url)?.text()?.len();
    let resp = fetch!(b"HEAD", &url).send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/css; charset=UTF-8"
    );
    assert_eq!(
        resp.headers()
            .get("content-length")
            .unwrap()
            .to_str()?
            .parse::<usize>()?,
        len
    );
    assert_eq!(resp.text()?, "");
    Ok(())
}

#[rstest]
fn head_asset_ico(server: TestServer) -> Result<(), Error> {
    let url = format!(
        "{}__dufs_v{}__/favicon.ico",
        server.url(),
        env!("CARGO_PKG_VERSION")
    );
    let len = reqwest::blocking::get(&url)?.bytes()?.len();
    let resp = fetch!(b"HEAD", &url).send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/x-icon");
    assert_eq!(
        resp.headers()
            .get("content-length")
            .unwrap()
            .to_str()?
            .parse::<usize>()?,
        len
    );
    assert_eq!(resp.text()?, "");
    Ok(())
}

#[rstest]
fn head_asset_missing(server: TestServer) -> Result<(), Error> {
    let url = format!(
        "{}__dufs_v{}__/not-found.js",
        server.url(),
        env!("CARGO_PKG_VERSION")
    );
    let resp = fetch!(b"HEAD", &url).send()?;
    assert_eq!(resp.status(), 404);
    assert_eq!(resp.text()?, "");
    Ok(())
}

#[rstest]
fn head_asset_override(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    std::fs::write(
        tmpdir.join(format!("{}index.js", DIR_ASSETS)),
        "console.log(1)",
    )?;

    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .arg("--assets")
        .arg(tmpdir.join(DIR_ASSETS))
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(&port);

    let url = format!(
        "http://localhost:{port}/__dufs_v{}__/index.js",
        env!("CARGO_PKG_VERSION")
    );
    let resp = fetch!(b"HEAD", &url).send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-length").unwrap(), "14");
    assert_eq!(resp.text()?, "");

    child.kill()?;
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
fn assets_override(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .arg("--assets")
        .arg(tmpdir.join(DIR_ASSETS))
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(&port);

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
fn assets_override_not_found_page(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let not_found_html = "<html><body>custom 404 page</body></html>";
    std::fs::write(
        tmpdir.join(format!("{}404.html", DIR_ASSETS)),
        not_found_html,
    )?;

    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .arg("--assets")
        .arg(tmpdir.join(DIR_ASSETS))
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(&port);

    let url = format!("http://localhost:{port}/missing-path");
    let resp = reqwest::blocking::get(&url)?;
    assert_eq!(resp.status(), 404);
    assert_eq!(resp.text()?, not_found_html);

    let url = format!("http://localhost:{port}/missing-path?noscript");
    let resp = reqwest::blocking::get(&url)?;
    assert_eq!(resp.status(), 404);
    assert_eq!(resp.text()?, "Not Found");

    child.kill()?;
    Ok(())
}

#[rstest]
fn assets_override_not_found_page_with_prefix(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    let ver = env!("CARGO_PKG_VERSION");
    std::fs::write(
        tmpdir.join(format!("{}404.html", DIR_ASSETS)),
        r#"<link href="__ASSETS_PREFIX__logo.svg"><a href="__PATH_PREFIX__">home</a>"#,
    )?;

    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .arg("--assets")
        .arg(tmpdir.join(DIR_ASSETS))
        .arg("--path-prefix")
        .arg("xyz")
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(&port);

    let url = format!("http://localhost:{port}/xyz/missing-path");
    let resp = reqwest::blocking::get(&url)?;
    assert_eq!(resp.status(), 404);
    assert_eq!(
        resp.text()?,
        format!(r#"<link href="/xyz/__dufs_v{ver}__/logo.svg"><a href="/xyz/">home</a>"#)
    );

    child.kill()?;
    Ok(())
}

#[cfg(windows)]
#[rstest]
fn asset_drive_relative_path_is_not_served(tmpdir: TempDir, port: Port) -> Result<(), Error> {
    use std::path::Component;

    let ver = env!("CARGO_PKG_VERSION");

    // Run the server from a directory whose drive-relative form resolves to a
    // canary that is outside both the assets directory and the served root.
    let cwd = assert_fs::TempDir::new()?;
    std::fs::write(cwd.join("canary.txt"), "canary-secret")?;

    let mut child = Command::new(assert_cmd::cargo::cargo_bin!())
        .arg(tmpdir.path())
        .arg("-p")
        .arg(port.to_string())
        .arg("--assets")
        .arg(tmpdir.join(DIR_ASSETS))
        .current_dir(cwd.path())
        .stdout(Stdio::piped())
        .spawn()?;

    wait_for_port(&port);

    let drive = match cwd.path().components().next() {
        Some(Component::Prefix(prefix)) => prefix.as_os_str().to_string_lossy().into_owned(),
        // e.g. a UNC path; the drive-relative form does not apply.
        _ => {
            child.kill()?;
            return Ok(());
        }
    };

    let url = format!("http://localhost:{port}/__dufs_v{ver}__/{drive}canary.txt");
    let resp = reqwest::blocking::get(&url)?;
    // Not found, and never the canary that lives outside the assets directory.
    assert_eq!(resp.status(), 404);
    assert_ne!(resp.text()?, "canary-secret");

    child.kill()?;
    Ok(())
}
