mod digest_auth_util;
mod fixtures;
mod utils;

use digest_auth_util::{digest_authorization, send_with_digest_auth};
use fixtures::{server, Error, TestServer};
use indexmap::IndexSet;
use rstest::rstest;

#[rstest]
fn no_auth(#[with(&["--auth", "user:pass@/:rw", "-A"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_eq!(resp.status(), 401);
    let values: Vec<&str> = resp
        .headers()
        .get_all("www-authenticate")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert!(values[0].starts_with("Digest"));
    assert!(values[1].starts_with("Basic"));

    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 401);
    Ok(())
}

#[rstest]
#[case(server(&["--auth", "user:pass@/:rw", "-A"]), "user", "pass")]
#[case(server(&["--auth", "user:pa:ss@1@/:rw", "-A"]), "user", "pa:ss@1")]
fn auth(#[case] server: TestServer, #[case] user: &str, #[case] pass: &str) -> Result<(), Error> {
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 401);
    let resp = send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), user, pass)?;
    assert_eq!(resp.status(), 201);
    Ok(())
}

#[rstest]
fn auth_digest_replay(
    #[with(&["--auth", "user:pass@/:rw", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}file1", server.url());
    let challenge = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(challenge.status(), 401);
    let authorization = digest_authorization(
        fetch!(b"PUT", &url).body(b"abc".to_vec()),
        &challenge,
        "user",
        "pass",
    )?;

    let resp = fetch!(b"PUT", &url)
        .body(b"abc".to_vec())
        .header("Authorization", &authorization)
        .send()?;
    assert_eq!(resp.status(), 201);

    // Replaying the header on another path must not authenticate.
    let other = format!("{}file2", server.url());
    let resp = fetch!(b"PUT", &other)
        .body(b"evil".to_vec())
        .header("Authorization", &authorization)
        .send()?;
    assert_eq!(resp.status(), 401);

    // Replaying it on the original path must be rejected as well.
    let resp = fetch!(b"PUT", &url)
        .body(b"abc".to_vec())
        .header("Authorization", &authorization)
        .send()?;
    assert_eq!(resp.status(), 401);
    Ok(())
}

#[rstest]
fn invalid_auth(
    #[with(&["-a", "user:pass@/:rw", "-a", "@/", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let resp = fetch!(b"GET", server.url())
        .basic_auth("user", Some("-"))
        .send()?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"GET", server.url())
        .basic_auth("-", Some("pass"))
        .send()?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"GET", server.url())
        .header("Authorization", "Basic Og==")
        .send()?;
    assert_eq!(resp.status(), 401);
    Ok(())
}

#[rstest]
#[case(server(&["--auth", "user:$6$gQxZwKyWn/ZmWEA2$4uV7KKMnSUnET2BtWTj/9T5.Jq3h/MdkOlnIl5hdlTxDZ4MZKmJ.kl6C.NL9xnNPqC4lVHC1vuI0E5cLpTJX81@/:rw", "-A"]), "user", "pass")]
#[case(server(&["--auth", "user:$6$YV1J6OHZAAgbzCbS$V55ZEgvJ6JFdz1nLO4AD696PRHAJYhfQf.Gy2HafrCz5itnbgNTtTgfUSqZrt4BJ7FcpRfSt/QZzAan68pido0@/:rw", "-A"]), "user", "pa:ss@1")]
fn auth_hashed_password(
    #[case] server: TestServer,
    #[case] user: &str,
    #[case] pass: &str,
) -> Result<(), Error> {
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 401);
    if let Err(err) = send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), user, pass)
    {
        assert_eq!(
            err.to_string(),
            r#"Missing "realm" in header: Basic realm="DUFS""#
        );
    }
    let resp = fetch!(b"PUT", &url)
        .body(b"abc".to_vec())
        .basic_auth(user, Some(pass))
        .send()?;
    assert_eq!(resp.status(), 201);
    Ok(())
}

#[rstest]
fn auth_and_public(
    #[with(&["-a", "user:pass@/:rw", "-a", "@/", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 401);
    let resp = send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), "user", "pass")?;
    assert_eq!(resp.status(), 201);
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text()?, "abc");
    Ok(())
}

#[rstest]
fn auth_skip(#[with(&["--auth", "@/"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn auth_skip_on_options_method(
    #[with(&["--auth", "user:pass@/:rw"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"OPTIONS", &url).send()?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn auth_skip_if_no_auth_user(server: TestServer) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"GET", &url)
        .basic_auth("user", Some("pass"))
        .send()?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn auth_no_skip_if_anonymous(
    #[with(&["--auth", "@/:ro"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"GET", &url)
        .basic_auth("user", Some("pass"))
        .send()?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 200);
    let resp = fetch!(b"DELETE", &url)
        .basic_auth("user", Some("pass"))
        .send()?;
    assert_eq!(resp.status(), 401);
    Ok(())
}

#[rstest]
fn auth_check(
    #[with(&["--auth", "user:pass@/:rw", "--auth", "user2:pass2@/", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}", server.url());
    let resp = fetch!(b"CHECKAUTH", &url).send()?;
    assert_eq!(resp.status(), 401);
    let resp = send_with_digest_auth(fetch!(b"CHECKAUTH", &url), "user", "pass")?;
    assert_eq!(resp.status(), 200);
    let resp = send_with_digest_auth(fetch!(b"CHECKAUTH", &url), "user2", "pass2")?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn auth_check2(
    #[with(&["--auth", "user:pass@/:rw|user2:pass2@/", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}", server.url());
    let resp = fetch!(b"CHECKAUTH", &url).send()?;
    assert_eq!(resp.status(), 401);
    let resp = send_with_digest_auth(fetch!(b"CHECKAUTH", &url), "user", "pass")?;
    assert_eq!(resp.status(), 200);
    let resp = send_with_digest_auth(fetch!(b"CHECKAUTH", &url), "user2", "pass2")?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn auth_check3(
    #[with(&["--auth", "user:pass@/:rw", "--auth", "@/dir1:rw", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}dir1/", server.url());
    let resp = fetch!(b"CHECKAUTH", &url).send()?;
    assert_eq!(resp.status(), 200);
    let resp = fetch!(b"CHECKAUTH", format!("{url}?login")).send()?;
    assert_eq!(resp.status(), 401);
    Ok(())
}

#[rstest]
fn auth_logout(
    #[with(&["--auth", "user:pass@/:rw", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"LOGOUT", &url).send()?;
    assert_eq!(resp.status(), 401);
    let resp = send_with_digest_auth(fetch!(b"LOGOUT", &url), "user", "pass")?;
    assert_eq!(resp.status(), 401);
    Ok(())
}

#[rstest]
fn auth_readonly(
    #[with(&["--auth", "user:pass@/:rw", "--auth", "user2:pass2@/", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 401);
    let resp = send_with_digest_auth(fetch!(b"GET", &url), "user2", "pass2")?;
    assert_eq!(resp.status(), 200);
    let url = format!("{}file1", server.url());
    let resp = send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), "user2", "pass2")?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

#[rstest]
fn auth_nest(
    #[with(&["--auth", "user:pass@/:rw", "--auth", "user2:pass2@/", "--auth", "user3:pass3@/dir1:rw", "-A"])]
    server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}dir1/file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 401);
    let resp = send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), "user3", "pass3")?;
    assert_eq!(resp.status(), 201);
    let resp = send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), "user", "pass")?;
    assert_eq!(resp.status(), 201);
    Ok(())
}

#[rstest]
fn auth_nest_share(
    #[with(&["--auth", "@/", "--auth", "user:pass@/:rw", "--auth", "user3:pass3@/dir1:rw", "-A"])]
    server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
#[case(server(&["--auth", "user:pass@/:rw", "-A"]), "user", "pass")]
#[case(server(&["--auth", "u1:p1@/:rw", "-A"]), "u1", "p1")]
fn auth_basic(
    #[case] server: TestServer,
    #[case] user: &str,
    #[case] pass: &str,
) -> Result<(), Error> {
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 401);
    let resp = fetch!(b"PUT", &url)
        .body(b"abc".to_vec())
        .basic_auth(user, Some(pass))
        .send()?;
    assert_eq!(resp.status(), 201);
    Ok(())
}

#[rstest]
fn auth_webdav_move(
    #[with(&["--auth", "user:pass@/:rw", "--auth", "user3:pass3@/dir1:rw", "-A"])]
    server: TestServer,
) -> Result<(), Error> {
    let origin_url = format!("{}dir1/test.html", server.url());
    let new_url = format!("{}test2.html", server.url());
    let resp = send_with_digest_auth(
        fetch!(b"MOVE", &origin_url).header("Destination", &new_url),
        "user3",
        "pass3",
    )?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

#[rstest]
fn auth_webdav_copy(
    #[with(&["--auth", "user:pass@/:rw", "--auth", "user3:pass3@/dir1:rw", "-A"])]
    server: TestServer,
) -> Result<(), Error> {
    let origin_url = format!("{}dir1/test.html", server.url());
    let new_url = format!("{}test2.html", server.url());
    let resp = send_with_digest_auth(
        fetch!(b"COPY", &origin_url).header("Destination", &new_url),
        "user3",
        "pass3",
    )?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

#[rstest]
fn auth_path_prefix(
    #[with(&["--auth", "user:pass@/:rw", "--path-prefix", "xyz", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}xyz/index.html", server.url());
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 401);
    let resp = send_with_digest_auth(fetch!(b"GET", &url), "user", "pass")?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn auth_partial_index(
    #[with(&["--auth", "user:pass@/dir1:rw,/dir2:rw", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let resp = send_with_digest_auth(fetch!(b"GET", server.url()), "user", "pass")?;
    assert_eq!(resp.status(), 200);
    let paths = utils::retrieve_index_paths(&resp.text()?);
    assert_eq!(paths, IndexSet::from(["dir1/".into(), "dir2/".into()]));
    let resp = send_with_digest_auth(
        fetch!(b"GET", format!("{}?q={}", server.url(), "test.html")),
        "user",
        "pass",
    )?;
    assert_eq!(resp.status(), 200);
    let paths = utils::retrieve_index_paths(&resp.text()?);
    assert_eq!(
        paths,
        IndexSet::from(["dir1/test.html".into(), "dir2/test.html".into()])
    );
    Ok(())
}

#[rstest]
fn no_auth_propfind_dir(
    #[with(&["--auth", "admin:admin@/:rw", "--auth", "@/dir-assets", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let resp = fetch!(b"PROPFIND", server.url()).send()?;
    assert_eq!(resp.status(), 207);
    let body = resp.text()?;
    assert!(body.contains("<D:href>/dir-assets/</D:href>"));
    assert!(body.contains("<D:href>/dir1/</D:href>"));
    Ok(())
}

#[rstest]
fn auth_propfind_dir(
    #[with(&["--auth", "admin:admin@/:rw", "--auth", "user:pass@/dir-assets", "-A"])]
    server: TestServer,
) -> Result<(), Error> {
    let resp = send_with_digest_auth(fetch!(b"PROPFIND", server.url()), "user", "pass")?;
    assert_eq!(resp.status(), 207);
    let body = resp.text()?;
    assert!(body.contains("<D:href>/dir-assets/</D:href>"));
    assert!(!body.contains("<D:href>/dir1/</D:href>"));
    Ok(())
}

#[rstest]
fn auth_data(
    #[with(&["-a", "user:pass@/:rw", "-a", "@/", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    let content = resp.text()?;
    let json = utils::retrieve_json(&content).unwrap();
    assert_eq!(json["allow_delete"], serde_json::Value::Bool(false));
    assert_eq!(json["allow_upload"], serde_json::Value::Bool(false));
    let resp = fetch!(b"GET", server.url())
        .basic_auth("user", Some("pass"))
        .send()?;
    let content = resp.text()?;
    let json = utils::retrieve_json(&content).unwrap();
    assert_eq!(json["allow_delete"], serde_json::Value::Bool(true));
    assert_eq!(json["allow_upload"], serde_json::Value::Bool(true));
    Ok(())
}

#[rstest]
fn auth_precedence(
    #[with(&["--auth", "user:pass@/dir1:rw,/dir1/test.txt", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}dir1/test.txt", server.url());
    let resp = send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), "user", "pass")?;
    assert_eq!(resp.status(), 403);

    Ok(())
}

#[rstest]
fn auth_alias_case_file(
    #[with(&["--auth", "user:pass@/dir1:rw,/dir1/test.txt:ro", "-A"])] server: TestServer,
) -> Result<(), Error> {
    if !utils::fs_folds_case(server.path()) {
        return Ok(());
    }

    let url = format!("{}dir1/test.txt", server.url());
    let resp = send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), "user", "pass")?;
    assert_eq!(resp.status(), 403);

    let url = format!("{}dir1/TEST.TXT", server.url());
    let resp = send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), "user", "pass")?;
    assert_eq!(resp.status(), 403);

    let url = format!("{}dir1/test.txt", server.url());
    let resp = fetch!(b"GET", &url)
        .basic_auth("user", Some("pass"))
        .send()?;
    assert_eq!(resp.status(), 200);

    let url = format!("{}dir1/other.txt", server.url());
    let resp = send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), "user", "pass")?;
    assert_eq!(resp.status(), 201);

    Ok(())
}

#[rstest]
fn auth_alias_case_directory(
    #[with(&["--auth", "user:pass@/:rw,/Protected:ro", "-A"])] server: TestServer,
) -> Result<(), Error> {
    std::fs::create_dir_all(server.path().join("Protected"))?;
    std::fs::write(server.path().join("Protected/file.txt"), "TOP SECRET")?;

    let url = format!("{}Protected/new.txt", server.url());
    let resp = send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), "user", "pass")?;
    assert_eq!(resp.status(), 403);

    if utils::fs_folds_case(server.path()) {
        for alias in [
            "protected/new.txt",
            "PROTECTED/new.txt",
            "pRoTeCtEd/new.txt",
        ] {
            let url = format!("{}{alias}", server.url());
            let resp =
                send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), "user", "pass")?;
            assert_eq!(resp.status(), 403, "alias `{alias}` was not guarded");
        }
        let url = format!("{}PROTECTED/file.txt", server.url());
        let resp = send_with_digest_auth(fetch!(b"DELETE", &url), "user", "pass")?;
        assert_eq!(resp.status(), 403);
    } else {
        let url = format!("{}protected/new.txt", server.url());
        let resp =
            send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), "user", "pass")?;
        assert_eq!(resp.status(), 201);
    }

    Ok(())
}

#[rstest]
fn auth_alias_webdav_destination(
    #[with(&["--auth", "user:pass@/:rw,/Protected:ro", "-A"])] server: TestServer,
) -> Result<(), Error> {
    if !utils::fs_folds_case(server.path()) {
        return Ok(());
    }
    std::fs::create_dir_all(server.path().join("Protected"))?;

    let origin_url = format!("{}test.txt", server.url());
    let dest_url = format!("{}protected/moved.txt", server.url());

    let resp = send_with_digest_auth(
        fetch!(b"MOVE", &origin_url).header("Destination", &dest_url),
        "user",
        "pass",
    )?;
    assert_eq!(resp.status(), 403);

    let resp = send_with_digest_auth(
        fetch!(b"COPY", &origin_url).header("Destination", &dest_url),
        "user",
        "pass",
    )?;
    assert_eq!(resp.status(), 403);

    Ok(())
}

/// On Windows an NTFS data stream suffix (`file.txt::$DATA`) names the same
/// data as `file.txt`, and so must be authorized as it.
///
/// A trailing dot is *not* covered here: dufs canonicalizes the served path to
/// a verbatim (`\\?\`) form, under which `file.txt.` is a distinct file rather
/// than an alias of `file.txt` (matching the discovery report).
#[cfg(windows)]
#[rstest]
fn auth_alias_windows_stream(
    #[with(&["--auth", "user:pass@/dir1:rw,/dir1/test.txt:ro", "-A"])] server: TestServer,
) -> Result<(), Error> {
    for alias in ["dir1/test.txt::$DATA", "dir1/TEST.TXT::$DATA"] {
        let url = format!("{}{alias}", server.url());
        let resp =
            send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), "user", "pass")?;
        assert_eq!(resp.status(), 403, "alias `{alias}` was not guarded");
    }
    let resp = fetch!(b"GET", format!("{}dir1/test.txt", server.url()))
        .basic_auth("user", Some("pass"))
        .send()?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

/// On Windows a volume may also report an NTFS 8.3 short name for a long name;
/// requests using it must be authorized as the configured long name.
#[cfg(windows)]
#[rstest]
fn auth_alias_windows_short_name(
    #[with(&["--auth", "user:pass@/:rw,/supersecretstorage:ro", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let dir = server.path().join("supersecretstorage");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("file.txt"), "TOP SECRET")?;

    let Some(short) = utils::short_name(&dir) else {
        // 8.3 name generation is disabled for this volume
        return Ok(());
    };

    let url = format!("{}{short}/new.txt", server.url());
    let resp = send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), "user", "pass")?;
    assert_eq!(resp.status(), 403);

    Ok(())
}

#[rstest]
fn auth_anonymous_no_precedence(
    #[with(&["--auth", "user:pass@/:rw", "-a", "@/dir1", "-A"])] server: TestServer,
) -> Result<(), Error> {
    let url = format!("{}dir1/test.txt", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 401);

    let resp = send_with_digest_auth(fetch!(b"PUT", &url).body(b"abc".to_vec()), "user", "pass")?;
    assert_eq!(resp.status(), 201);

    Ok(())
}

#[rstest]
fn token_auth(#[with(&["-a", "user:pass@/"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 401);
    let url = format!("{}index.html?tokengen", server.url());
    let resp = fetch!(b"GET", &url)
        .basic_auth("user", Some("pass"))
        .send()?;
    let token = resp.text()?;
    let url = format!("{}index.html?token={token}", server.url());
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn token_auth_cannot_renew(
    #[with(&["-a", "user:pass@/"])] server: TestServer,
) -> Result<(), Error> {
    // A share token must not be able to mint another token, otherwise a
    // short-lived link could be renewed forever.
    let url = format!("{}index.html?tokengen", server.url());
    let token = fetch!(b"GET", &url)
        .basic_auth("user", Some("pass"))
        .send()?
        .text()?;

    // The token is still accepted for a normal request.
    let url = format!("{}index.html?token={token}", server.url());
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 200);

    // But it must not be able to renew itself.
    let url = format!("{}index.html?token={token}&tokengen", server.url());
    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 401);

    Ok(())
}

#[rstest]
fn auth_digest_malformed_header(
    #[with(&["--auth", "user:pass@/:rw", "-A"])] server: TestServer,
) -> Result<(), Error> {
    for authorization in [
        "Digest a=\"",
        "Digest a==,",
        "Digest a=",
        "Digest =,\"",
        "Digest a=\"b\",",
        "Digest a=\"b\" , c",
    ] {
        let resp = fetch!(b"GET", server.url())
            .header("Authorization", authorization)
            .send()?;
        assert_eq!(resp.status(), 401);
    }

    let url = format!("{}index.html", server.url());
    let resp = fetch!(b"GET", &url)
        .basic_auth("user", Some("pass"))
        .send()?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn auth_digest_malformed_header_without_auth(
    #[with(&[
        "--log-format",
        "$remote_addr $remote_user \"$request\" $status",
    ])]
    server: TestServer,
) -> Result<(), Error> {
    let resp = fetch!(b"GET", server.url())
        .header("Authorization", "Digest a=\"")
        .send()?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn token_auth_rejects(#[with(&["-a", "user:pass@/"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}index.html?tokengen", server.url());

    let resp = fetch!(b"GET", &url).send()?;
    assert_eq!(resp.status(), 401);

    let resp = fetch!(b"GET", &url)
        .basic_auth("user", Some("wrong"))
        .send()?;
    assert_eq!(resp.status(), 401);

    let resp = fetch!(b"OPTIONS", &url).send()?;
    assert_eq!(resp.status(), 401);

    let resp = fetch!(b"OPTIONS", &url)
        .basic_auth("user", Some("wrong"))
        .send()?;
    assert_eq!(resp.status(), 401);

    let resp = fetch!(b"OPTIONS", &url)
        .basic_auth("no-such-user", Some("wrong"))
        .send()?;
    assert_eq!(resp.status(), 401);

    Ok(())
}
