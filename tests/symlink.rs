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

/// A link that resolves outside the root is denied when access rules exist,
/// even if `--allow-symlink` is enabled.
#[cfg(unix)]
#[rstest]
fn symlink_outside_root_denied_with_auth(
    #[with(&["--auth", "alice:alice@/:rw", "--allow-symlink", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let outside = assert_fs::TempDir::new()?;
    std::fs::write(outside.path().join("secret.txt"), "OUTSIDE")?;
    symlink_file(
        outside.path().join("secret.txt"),
        server.path().join("outside-link"),
    )?;

    let resp = fetch!(b"GET", format!("{}outside-link", server.url()))
        .basic_auth("alice", Some("alice"))
        .send()?;
    assert_eq!(resp.status(), 403);
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
