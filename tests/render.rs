mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer, BIN_FILE, DIR_NO_FOUND, DIR_NO_INDEX, FILES};
use rstest::rstest;

#[rstest]
fn render_index(#[with(&["--render-index"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    let text = resp.text()?;
    assert_eq!(text, "This is index.html");
    Ok(())
}

#[rstest]
fn render_index2(#[with(&["--render-index"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{}", server.url(), DIR_NO_INDEX))?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn render_try_index(#[with(&["--render-try-index"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    let text = resp.text()?;
    assert_eq!(text, "This is index.html");
    Ok(())
}

#[rstest]
fn render_try_index2(#[with(&["--render-try-index"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{}", server.url(), DIR_NO_INDEX))?;
    let files: Vec<&str> = FILES
        .iter()
        .filter(|v| **v != "index.html")
        .cloned()
        .collect();
    assert_resp_paths!(resp, files);
    Ok(())
}

#[rstest]
fn render_try_index3(
    #[with(&["--render-try-index", "--allow-archive"])] server: TestServer,
) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{}?zip", server.url(), DIR_NO_INDEX))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/zip"
    );
    Ok(())
}

#[rstest]
#[case(server(&["--render-try-index"] as &[&str]), false)]
#[case(server(&["--render-try-index", "--allow-search"] as &[&str]), true)]
fn render_try_index4(#[case] server: TestServer, #[case] searched: bool) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{}?q={}", server.url(), DIR_NO_INDEX, BIN_FILE))?;
    assert_eq!(resp.status(), 200);
    let paths = utils::retrieve_index_paths(&resp.text()?);
    assert_eq!(paths.iter().all(|v| v.contains(BIN_FILE)), searched);
    Ok(())
}

#[rstest]
fn render_spa(#[with(&["--render-spa"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    let text = resp.text()?;
    assert_eq!(text, "This is index.html");
    Ok(())
}

#[rstest]
fn render_spa2(#[with(&["--render-spa"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{}", server.url(), DIR_NO_FOUND))?;
    let text = resp.text()?;
    assert_eq!(text, "This is index.html");
    Ok(())
}

#[rstest]
#[case(&["--render-index"], 404)]
#[case(&["--render-try-index"], 200)]
#[case(&["--render-spa"], 404)]
fn render_index_scope(#[case] render_args: &[&str], #[case] root_status: u16) -> Result<(), Error> {
    let mut args = vec!["--auth", "guest:pass@/dir1"];
    args.extend_from_slice(render_args);
    let server = server(&args);
    let client = reqwest::blocking::Client::new();

    // `dir1` is allowed, but the root index.html is not.
    let resp = client
        .get(server.url())
        .basic_auth("guest", Some("pass"))
        .send()?;
    assert_eq!(resp.status(), root_status);
    let text = resp.text()?;
    assert!(!text.contains("This is index.html"));
    if root_status == 200 {
        assert!(utils::retrieve_index_paths(&text).contains("dir1/"));
    }

    // The index inside the allowed directory is still rendered.
    let resp = client
        .get(format!("{}dir1/", server.url()))
        .basic_auth("guest", Some("pass"))
        .send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "This is dir1/index.html");
    Ok(())
}

#[rstest]
#[case(&["--render-index"], 404)]
#[case(&["--render-try-index"], 200)]
#[case(&["--render-spa"], 404)]
fn render_index_scope_anonymous(
    #[case] render_args: &[&str],
    #[case] root_status: u16,
) -> Result<(), Error> {
    let mut args = vec!["--auth", "@/dir1"];
    args.extend_from_slice(render_args);
    let server = server(&args);

    let resp = reqwest::blocking::get(server.url())?;
    assert_eq!(resp.status(), root_status);
    let text = resp.text()?;
    assert!(!text.contains("This is index.html"));
    if root_status == 200 {
        assert!(utils::retrieve_index_paths(&text).contains("dir1/"));
    }

    let resp = reqwest::blocking::get(format!("{}dir1/", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "This is dir1/index.html");
    Ok(())
}

#[rstest]
#[case("guest:pass@/dir1", false, 404)]
#[case("guest:pass@/", false, 200)]
#[case("guest:pass@/dir1,index.html", false, 200)]
#[case("@/dir1", true, 404)]
fn render_spa_scope(
    #[case] auth: &str,
    #[case] anonymous: bool,
    #[case] expected_status: u16,
) -> Result<(), Error> {
    let server = server(&["--auth", auth, "--render-spa"]);
    let url = format!("{}dir1/route", server.url());
    let request = reqwest::blocking::Client::new().get(&url);
    let resp = if anonymous {
        request.send()?
    } else {
        request.basic_auth("guest", Some("pass")).send()?
    };
    assert_eq!(resp.status(), expected_status);
    if expected_status == 200 {
        assert_eq!(resp.text()?, "This is index.html");
    } else {
        assert!(!resp.text()?.contains("This is index.html"));
    }
    Ok(())
}

#[rstest]
fn render_index_scope_nested(
    #[with(&["--auth", "guest:pass@/dir1/sub", "--render-index"])] server: TestServer,
) -> Result<(), Error> {
    let sub = server.path().join("dir1").join("sub");
    std::fs::create_dir_all(&sub)?;
    std::fs::write(sub.join("index.html"), "This is sub/index.html")?;
    let client = reqwest::blocking::Client::new();

    // Ancestor directories are traversable, but their index files are not.
    let resp = client
        .get(format!("{}dir1/", server.url()))
        .basic_auth("guest", Some("pass"))
        .send()?;
    assert_eq!(resp.status(), 404);
    assert!(!resp.text()?.contains("This is dir1/index.html"));

    let resp = client
        .get(format!("{}dir1/sub/", server.url()))
        .basic_auth("guest", Some("pass"))
        .send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "This is sub/index.html");
    Ok(())
}

#[rstest]
fn render_index_explicit_grant(
    #[with(&["--auth", "guest:pass@/dir1,index.html", "--render-index"])] server: TestServer,
) -> Result<(), Error> {
    let resp = reqwest::blocking::Client::new()
        .get(server.url())
        .basic_auth("guest", Some("pass"))
        .send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "This is index.html");
    Ok(())
}
