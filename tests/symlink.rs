mod fixtures;
mod utils;

use assert_fs::fixture::TempDir;
use fixtures::{server, tmpdir, Error, TestServer};
use rstest::rstest;

#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_dir;
#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_file;
#[cfg(windows)]
use std::os::windows::fs::symlink_dir;

#[rstest]
fn default_not_allow_symlink(server: TestServer, tmpdir: TempDir) -> Result<(), Error> {
    // Create symlink directory "foo" to point outside the root
    let dir = "foo";
    symlink_dir(tmpdir.path(), server.path().join(dir)).expect("Couldn't create symlink");
    let resp = reqwest::blocking::get(format!("{}{}", server.url(), dir))?;
    assert_eq!(resp.status(), 404);
    let resp = reqwest::blocking::get(format!("{}{}/index.html", server.url(), dir))?;
    assert_eq!(resp.status(), 404);
    let resp = reqwest::blocking::get(server.url())?;
    let paths = utils::retrieve_index_paths(&resp.text()?);
    assert!(!paths.is_empty());
    assert!(!paths.contains(&format!("{dir}/")));
    Ok(())
}

#[rstest]
fn allow_symlink(
    #[with(&["--allow-symlink"])] server: TestServer,
    tmpdir: TempDir,
) -> Result<(), Error> {
    // Create symlink directory "foo" to point outside the root
    let dir = "foo";
    symlink_dir(tmpdir.path(), server.path().join(dir)).expect("Couldn't create symlink");
    let resp = reqwest::blocking::get(format!("{}{}", server.url(), dir))?;
    assert_eq!(resp.status(), 200);
    let resp = reqwest::blocking::get(format!("{}{}/index.html", server.url(), dir))?;
    assert_eq!(resp.status(), 200);
    let resp = reqwest::blocking::get(server.url())?;
    let paths = utils::retrieve_index_paths(&resp.text()?);
    assert!(!paths.is_empty());
    assert!(paths.contains(&format!("{dir}/")));
    Ok(())
}

/// A link must not grant more permission than its canonical target. This covers
/// links to a restricted directory, to a file inside it, and to a subdirectory.
#[cfg(unix)]
#[rstest]
fn symlink_target_permissions(
    #[with(&["--auth", "alice:alice@/public:rw,/public/locked:ro", "-A"])] server: TestServer,
) -> Result<(), Error> {
    std::fs::create_dir_all(server.path().join("public/locked/sub"))?;
    std::fs::write(server.path().join("public/locked/secret.txt"), "PROTECTED")?;
    std::fs::write(server.path().join("public/locked/sub/file.txt"), "SUBFILE")?;
    symlink_file("locked", server.path().join("public/linkdir"))?;
    symlink_file("locked/secret.txt", server.path().join("public/linkfile"))?;
    symlink_file("locked/sub", server.path().join("public/linksub"))?;

    let resp = fetch!(b"GET", format!("{}public/linkdir/secret.txt", server.url()))
        .basic_auth("alice", Some("alice"))
        .send()?;
    assert_eq!(resp.status(), 200);

    for path in [
        "public/linkdir/w1.txt",
        "public/linkfile",
        "public/linksub/w2.txt",
    ] {
        let resp = fetch!(b"PUT", format!("{}{}", server.url(), path))
            .basic_auth("alice", Some("alice"))
            .body("hacked")
            .send()?;
        assert_eq!(resp.status(), 403, "{path}");
    }
    assert_eq!(
        std::fs::read_to_string(server.path().join("public/locked/secret.txt"))?,
        "PROTECTED"
    );
    assert!(!server.path().join("public/locked/w1.txt").exists());
    assert!(!server.path().join("public/locked/sub/w2.txt").exists());
    Ok(())
}

/// The target rule may be several levels below the link's parent.
#[cfg(unix)]
#[rstest]
fn symlink_target_permissions_deep_rule(
    #[with(&["--auth", "alice:alice@/public:rw,/public/a/locked:ro", "-A"])] server: TestServer,
) -> Result<(), Error> {
    std::fs::create_dir_all(server.path().join("public/a/locked"))?;
    std::fs::write(
        server.path().join("public/a/locked/secret.txt"),
        "PROTECTED",
    )?;
    symlink_file("a/locked", server.path().join("public/link"))?;

    let resp = fetch!(b"GET", format!("{}public/link/secret.txt", server.url()))
        .basic_auth("alice", Some("alice"))
        .send()?;
    assert_eq!(resp.status(), 200);
    let resp = fetch!(b"PUT", format!("{}public/link/secret.txt", server.url()))
        .basic_auth("alice", Some("alice"))
        .body("hacked")
        .send()?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

/// Sharing still works: a link to a read-only target is readable but not
/// writable. Links to targets without permission are hidden from listings.
#[cfg(unix)]
#[rstest]
fn symlink_sharing_respects_target(
    #[with(&["--auth", "alice:alice@/home/alice:rw,/shared/team:ro", "-A"])] server: TestServer,
) -> Result<(), Error> {
    std::fs::create_dir_all(server.path().join("shared/team"))?;
    std::fs::write(server.path().join("shared/team/file.txt"), "SHARED")?;
    std::fs::create_dir_all(server.path().join("private"))?;
    std::fs::write(server.path().join("private/secret.txt"), "PRIVATE")?;
    std::fs::create_dir_all(server.path().join("home/alice"))?;
    symlink_file("../../shared/team", server.path().join("home/alice/team"))?;
    symlink_file(
        "../../private/secret.txt",
        server.path().join("home/alice/private-link"),
    )?;

    let resp = fetch!(b"GET", format!("{}home/alice/team/file.txt", server.url()))
        .basic_auth("alice", Some("alice"))
        .send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "SHARED");

    let resp = fetch!(b"PUT", format!("{}home/alice/team/file.txt", server.url()))
        .basic_auth("alice", Some("alice"))
        .body("hacked")
        .send()?;
    assert_eq!(resp.status(), 403);

    let resp = fetch!(b"GET", format!("{}home/alice/private-link", server.url()))
        .basic_auth("alice", Some("alice"))
        .send()?;
    assert_eq!(resp.status(), 403);

    let resp = fetch!(b"GET", format!("{}home/alice/", server.url()))
        .basic_auth("alice", Some("alice"))
        .send()?;
    let paths = utils::retrieve_index_paths(&resp.text()?);
    assert!(paths.contains(&"team/".to_string()));
    assert!(!paths.contains(&"private-link".to_string()));
    Ok(())
}

/// `MOVE` may not relocate a relative link to a different parent, because that
/// would change its target over HTTP. A rename within the same parent preserves
/// the target and is still allowed.
#[cfg(unix)]
#[rstest]
fn move_retarget_denied(
    #[with(&["--auth", "alice:alice@/public:rw", "-A"])] server: TestServer,
) -> Result<(), Error> {
    std::fs::create_dir_all(server.path().join("public/staging"))?;
    std::fs::create_dir_all(server.path().join("public/private"))?;
    std::fs::write(server.path().join("public/private/benign.txt"), "BENIGN")?;
    std::fs::create_dir_all(server.path().join("private"))?;
    std::fs::write(server.path().join("private/secret.txt"), "PRIVATE")?;
    symlink_file("../private", server.path().join("public/staging/alias"))?;

    let resp = fetch!(b"MOVE", format!("{}public/staging/alias", server.url()))
        .basic_auth("alice", Some("alice"))
        .header("Destination", format!("{}public/alias", server.url()))
        .send()?;
    assert_eq!(resp.status(), 403);
    assert!(server.path().join("public/staging/alias").is_symlink());
    assert_eq!(
        std::fs::read_to_string(server.path().join("private/secret.txt"))?,
        "PRIVATE"
    );

    let resp = fetch!(b"MOVE", format!("{}public/staging/alias", server.url()))
        .basic_auth("alice", Some("alice"))
        .header(
            "Destination",
            format!("{}public/staging/renamed", server.url()),
        )
        .send()?;
    assert_eq!(resp.status(), 204);
    let resp = fetch!(
        b"GET",
        format!("{}public/staging/renamed/benign.txt", server.url())
    )
    .basic_auth("alice", Some("alice"))
    .send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "BENIGN");
    Ok(())
}

/// `--allow-symlink` keeps its meaning with access rules: a link that resolves
/// outside the root is served under the requested path's permission.
#[cfg(unix)]
#[rstest]
fn symlink_outside_root_follows_request_path(
    #[with(&["--auth", "alice:alice@/public:ro,/rw:rw", "--allow-symlink", "-A"])]
    server: TestServer,
) -> Result<(), Error> {
    let outside = assert_fs::TempDir::new()?;
    std::fs::write(outside.path().join("secret.txt"), "OUTSIDE")?;
    std::fs::create_dir_all(server.path().join("public"))?;
    std::fs::create_dir_all(server.path().join("rw"))?;
    symlink_file(
        outside.path().join("secret.txt"),
        server.path().join("public/outside-link"),
    )?;
    symlink_file(
        outside.path().join("secret.txt"),
        server.path().join("rw/outside-link"),
    )?;

    for path in ["public/outside-link", "rw/outside-link"] {
        let resp = fetch!(b"GET", format!("{}{}", server.url(), path))
            .basic_auth("alice", Some("alice"))
            .send()?;
        assert_eq!(resp.status(), 200, "{path}");
        assert_eq!(resp.text()?, "OUTSIDE");
    }

    // The requested path's permission still governs writes through the link.
    let resp = fetch!(b"PUT", format!("{}public/outside-link", server.url()))
        .basic_auth("alice", Some("alice"))
        .body("hacked")
        .send()?;
    assert_eq!(resp.status(), 403);
    let resp = fetch!(b"PUT", format!("{}rw/outside-link", server.url()))
        .basic_auth("alice", Some("alice"))
        .body("through-link")
        .send()?;
    assert_eq!(resp.status(), 201);
    assert_eq!(
        std::fs::read_to_string(outside.path().join("secret.txt"))?,
        "through-link"
    );
    Ok(())
}

/// Archives and searches must not include links whose target is not readable.
#[cfg(unix)]
#[rstest]
fn zip_skips_denied_symlink(
    #[with(&["--auth", "alice:alice@/public:rw", "--allow-archive", "-A"])] server: TestServer,
) -> Result<(), Error> {
    std::fs::create_dir_all(server.path().join("public"))?;
    std::fs::write(server.path().join("public/legit.txt"), "LEGIT")?;
    std::fs::create_dir_all(server.path().join("private"))?;
    std::fs::write(server.path().join("private/secret.txt"), "PRIVATE")?;
    symlink_file(
        "../private/secret.txt",
        server.path().join("public/secret-link"),
    )?;

    let resp = fetch!(b"GET", format!("{}public/?zip", server.url()))
        .basic_auth("alice", Some("alice"))
        .send()?;
    assert_eq!(resp.status(), 200);
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(resp.bytes()?))?;
    let mut names = vec![];
    for index in 0..archive.len() {
        names.push(archive.by_index(index)?.name()?.to_string());
    }
    assert!(names.iter().any(|name| name == "legit.txt"));
    assert!(!names.iter().any(|name| name.contains("secret")));
    Ok(())
}

/// A dangling link is resolved through its link text. When that text itself
/// traverses another link, the resolved target must be authorized instead of
/// falling back to the link text.
#[cfg(unix)]
#[rstest]
fn dangling_link_through_symlink_denied(
    #[with(&["--auth", "alice:alice@/public:rw,/private:ro", "-A"])] server: TestServer,
) -> Result<(), Error> {
    std::fs::create_dir_all(server.path().join("private/x"))?;
    std::fs::create_dir_all(server.path().join("public"))?;
    symlink_file("../private", server.path().join("public/aliasdir"))?;
    symlink_file("aliasdir/new.txt", server.path().join("public/dangling"))?;

    let resp = fetch!(b"PUT", format!("{}public/dangling", server.url()))
        .basic_auth("alice", Some("alice"))
        .body("hacked")
        .send()?;
    assert_eq!(resp.status(), 403);
    assert!(!server.path().join("private/new.txt").exists());
    Ok(())
}

/// Recursive operations must apply the target subtree rules as well: a link can
/// expose a writable-looking directory whose real target contains a protected
/// child, and `DELETE`/`MOVE` act on the whole real subtree.
#[cfg(unix)]
#[rstest]
fn recursive_delete_and_move_through_symlink_parent_respect_target(
    #[with(&["--auth", "alice:alice@/public:rw,/public/locked/sub/secret.txt:ro", "-A"])]
    server: TestServer,
) -> Result<(), Error> {
    std::fs::create_dir_all(server.path().join("public/locked/sub"))?;
    std::fs::write(server.path().join("public/locked/sub/secret.txt"), "SECRET")?;
    symlink_file("locked", server.path().join("public/linkdir"))?;

    let resp = fetch!(
        b"DELETE",
        format!("{}public/linkdir/sub/secret.txt", server.url())
    )
    .basic_auth("alice", Some("alice"))
    .send()?;
    assert_eq!(resp.status(), 403);

    // Deleting the containing directory must not bypass its child rule.
    let resp = fetch!(b"DELETE", format!("{}public/linkdir/sub", server.url()))
        .basic_auth("alice", Some("alice"))
        .send()?;
    assert_eq!(resp.status(), 403);
    assert!(server.path().join("public/locked/sub/secret.txt").exists());

    // Moving it out of the protected path would make the file writable.
    let resp = fetch!(b"MOVE", format!("{}public/linkdir/sub", server.url()))
        .basic_auth("alice", Some("alice"))
        .header("Destination", format!("{}public/moved", server.url()))
        .send()?;
    assert_eq!(resp.status(), 403);
    assert!(server.path().join("public/locked/sub/secret.txt").exists());
    assert!(!server.path().join("public/moved").exists());

    // Deleting the link entry itself only unlinks it, so it stays allowed.
    let resp = fetch!(b"DELETE", format!("{}public/linkdir", server.url()))
        .basic_auth("alice", Some("alice"))
        .send()?;
    assert_eq!(resp.status(), 204);
    assert!(!server.path().join("public/linkdir").exists());
    assert!(server.path().join("public/locked/sub/secret.txt").exists());
    Ok(())
}

/// Listings and search results must derive the recursive-delete indicator from
/// the resolved target subtree, not only from the request path.
#[cfg(unix)]
#[rstest]
fn listing_and_search_follow_target_descendants(
    #[with(&["--auth", "alice:alice@/public:rw,/public/locked/sub/secret.txt:ro", "-A"])]
    server: TestServer,
) -> Result<(), Error> {
    std::fs::create_dir_all(server.path().join("public/locked/sub"))?;
    std::fs::write(server.path().join("public/locked/sub/secret.txt"), "SECRET")?;
    symlink_file("locked", server.path().join("public/linkdir"))?;

    let resp = fetch!(b"GET", format!("{}public/linkdir/?json", server.url()))
        .basic_auth("alice", Some("alice"))
        .send()?;
    assert_eq!(resp.status(), 200);
    let value: serde_json::Value = serde_json::from_str(&resp.text()?)?;
    let sub = value["paths"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == "sub")
        .expect("sub should be listed");
    assert_eq!(sub["allow_delete"], false);
    assert_eq!(sub["readonly_descendant"], true);

    let resp = fetch!(b"GET", format!("{}public/?q=sub&json", server.url()))
        .basic_auth("alice", Some("alice"))
        .send()?;
    assert_eq!(resp.status(), 200);
    let value: serde_json::Value = serde_json::from_str(&resp.text()?)?;
    let sub = value["paths"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == "linkdir/sub")
        .expect("linkdir/sub should be found");
    assert_eq!(sub["allow_delete"], false);
    assert_eq!(sub["readonly_descendant"], true);

    // The link entry itself can still be removed without touching the target.
    let resp = fetch!(b"GET", format!("{}public/?json", server.url()))
        .basic_auth("alice", Some("alice"))
        .send()?;
    assert_eq!(resp.status(), 200);
    let value: serde_json::Value = serde_json::from_str(&resp.text()?)?;
    let link = value["paths"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == "linkdir")
        .expect("linkdir should be listed");
    assert_eq!(link["allow_delete"], true);
    assert_eq!(link["readonly_descendant"], false);
    Ok(())
}

/// A directory reached through a link exposes only the entries that the target
/// rules allow, and does not advertise write access to a read-only target.
#[cfg(unix)]
#[rstest]
fn listing_through_symlink_hides_denied_entries(
    #[with(&["--auth", "alice:alice@/public:rw,/shared/team/secret.txt:ro", "--allow-symlink"])]
    server: TestServer,
) -> Result<(), Error> {
    std::fs::create_dir_all(server.path().join("shared/team"))?;
    std::fs::write(server.path().join("shared/team/secret.txt"), "SECRET")?;
    std::fs::write(server.path().join("shared/team/other.txt"), "OTHER")?;
    std::fs::create_dir_all(server.path().join("public"))?;
    symlink_file("../shared/team", server.path().join("public/team"))?;

    let resp = fetch!(b"GET", format!("{}public/team/?json", server.url()))
        .basic_auth("alice", Some("alice"))
        .send()?;
    assert_eq!(resp.status(), 200);
    let value: serde_json::Value = serde_json::from_str(&resp.text()?)?;
    assert_eq!(value["allow_upload"], false);
    assert_eq!(value["allow_delete"], false);
    let names: Vec<&str> = value["paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["secret.txt"]);
    assert_eq!(value["paths"][0]["allow_upload"], false);

    let resp = fetch!(b"GET", format!("{}public/team/other.txt", server.url()))
        .basic_auth("alice", Some("alice"))
        .send()?;
    assert_eq!(resp.status(), 403);

    let resp = fetch!(b"GET", format!("{}public/team/secret.txt", server.url()))
        .basic_auth("alice", Some("alice"))
        .send()?;
    assert_eq!(resp.status(), 200);

    let resp = fetch!(
        b"GET",
        format!("{}public/team/secret.txt?json", server.url())
    )
    .basic_auth("alice", Some("alice"))
    .send()?;
    assert_eq!(resp.status(), 200);
    let value: serde_json::Value = serde_json::from_str(&resp.text()?)?;
    assert_eq!(value["allow_upload"], false);
    assert_eq!(value["allow_delete"], false);
    Ok(())
}
