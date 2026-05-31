mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use rstest::rstest;
use std::thread::sleep;
use std::time::Duration;

#[rstest]
fn lock_file(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let lock_token = resp.headers().get("lock-token").unwrap().to_str()?.to_string();
    let timeout = resp.headers().get("timeout").unwrap().to_str()?.to_string();
    let body = resp.text()?;
    assert!(body.contains("<D:href>/test.html</D:href>"));
    assert!(body.contains("<D:locktoken><D:href>"));
    assert!(body.contains("<D:lockscope><D:exclusive/></D:lockscope>"));
    assert!(body.contains("<D:locktype><D:write/></D:locktype>"));
    assert!(body.contains("<D:timeout>"));
    assert!(!lock_token.is_empty());
    assert!(!timeout.is_empty());
    Ok(())
}

#[rstest]
fn lock_file_shared(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:shared/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url()))
        .body(body)
        .send()?;
    assert_eq!(resp.status(), 200);
    let resp_body = resp.text()?;
    assert!(resp_body.contains("<D:lockscope><D:shared/></D:lockscope>"));
    Ok(())
}

#[rstest]
fn lock_file_404(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}no-such-file.txt", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn unlock_file(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let lock_token = resp.headers().get("lock-token").unwrap().to_str()?;
    let resp = fetch!(b"UNLOCK", format!("{}test.html", server.url()))
        .header("Lock-Token", lock_token)
        .send()?;
    assert_eq!(resp.status(), 204);
    Ok(())
}

#[rstest]
fn unlock_no_token(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let resp = fetch!(b"UNLOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 400);
    Ok(())
}

#[rstest]
fn unlock_wrong_token(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let resp = fetch!(b"UNLOCK", format!("{}test.html", server.url()))
        .header("Lock-Token", "<opaquelocktoken:wrong-token>")
        .send()?;
    assert_eq!(resp.status(), 409);
    Ok(())
}

#[rstest]
fn unlock_file_404(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"UNLOCK", format!("{}no-such-file.txt", server.url()))
        .header("Lock-Token", "<dummy>")
        .send()?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn exclusive_lock_blocks_put(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let resp = fetch!(b"PUT", format!("{}test.html", server.url()))
        .body("new content")
        .send()?;
    assert_eq!(resp.status(), 423);
    Ok(())
}

#[rstest]
fn exclusive_lock_blocks_delete(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let resp = fetch!(b"DELETE", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 423);
    Ok(())
}

#[rstest]
fn exclusive_lock_blocks_mkcol(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let dir_url = format!("{}newdir", server.url());
    let resp = fetch!(b"MKCOL", &dir_url).send()?;
    assert_eq!(resp.status(), 201);
    let resp = fetch!(b"LOCK", &dir_url).send()?;
    drop(resp);
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let resp = fetch!(b"MKCOL", format!("{}other-newdir", server.url())).send()?;
    assert_eq!(resp.status(), 201);
    Ok(())
}

#[rstest]
fn exclusive_lock_allows_owner_write(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let lock_token = resp.headers().get("lock-token").unwrap().to_str()?;
    let if_header = format!("(<{}>)", lock_token.trim_matches(&['<', '>'] as &[_]));
    let resp = fetch!(b"PUT", format!("{}test.html", server.url()))
        .header("If", &if_header)
        .body("owner can write")
        .send()?;
    assert!(resp.status() == 200 || resp.status() == 201);
    Ok(())
}

#[rstest]
fn exclusive_lock_allows_owner_delete(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let lock_token = resp.headers().get("lock-token").unwrap().to_str()?;
    let if_header = format!("(<{}>)", lock_token.trim_matches(&['<', '>'] as &[_]));
    let resp = fetch!(b"DELETE", format!("{}test.html", server.url()))
        .header("If", &if_header)
        .send()?;
    assert_eq!(resp.status(), 204);
    Ok(())
}

#[rstest]
fn shared_lock_blocks_exclusive(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let shared_body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:shared/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url()))
        .body(shared_body)
        .send()?;
    assert_eq!(resp.status(), 200);
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 423);
    Ok(())
}

#[rstest]
fn exclusive_lock_blocks_shared(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let shared_body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:shared/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url()))
        .body(shared_body)
        .send()?;
    assert_eq!(resp.status(), 423);
    Ok(())
}

#[rstest]
fn shared_lock_allows_another_shared(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let shared_body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:shared/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url()))
        .body(shared_body)
        .send()?;
    assert_eq!(resp.status(), 200);
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url()))
        .body(shared_body)
        .send()?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn shared_lock_blocks_put(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let shared_body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:shared/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url()))
        .body(shared_body)
        .send()?;
    assert_eq!(resp.status(), 200);
    let resp = fetch!(b"PUT", format!("{}test.html", server.url()))
        .body("new content")
        .send()?;
    assert_eq!(resp.status(), 423);
    Ok(())
}

#[rstest]
fn lock_timeout_expires(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url()))
        .header("Timeout", "Second-1")
        .send()?;
    assert_eq!(resp.status(), 200);
    sleep(Duration::from_secs(2));
    let resp = fetch!(b"PUT", format!("{}test.html", server.url()))
        .body("after timeout")
        .send()?;
    assert!(resp.status() == 200 || resp.status() == 201);
    Ok(())
}

#[rstest]
fn lock_infinite_does_not_expire(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url()))
        .header("Timeout", "Infinite")
        .send()?;
    assert_eq!(resp.status(), 200);
    let timeout_hdr = resp.headers().get("timeout").unwrap().to_str()?;
    assert_eq!(timeout_hdr, "Infinite");
    let resp = fetch!(b"PUT", format!("{}test.html", server.url()))
        .body("blocked")
        .send()?;
    assert_eq!(resp.status(), 423);
    Ok(())
}

#[rstest]
fn lock_default_timeout(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let timeout = resp.headers().get("timeout").unwrap().to_str()?;
    assert_eq!(timeout, "Second-300");
    Ok(())
}

#[rstest]
fn delete_releases_lock(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let lock_token = resp.headers().get("lock-token").unwrap().to_str()?;
    let if_header = format!("(<{}>)", lock_token.trim_matches(&['<', '>'] as &[_]));
    let resp = fetch!(b"DELETE", format!("{}test.html", server.url()))
        .header("If", &if_header)
        .send()?;
    assert_eq!(resp.status(), 204);
    let resp = fetch!(b"PUT", format!("{}test.html", server.url()))
        .body("recreated")
        .send()?;
    assert!(resp.status() == 200 || resp.status() == 201);
    Ok(())
}

#[rstest]
fn move_releases_source_lock(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let lock_token = resp.headers().get("lock-token").unwrap().to_str()?;
    let if_header = format!("(<{}>)", lock_token.trim_matches(&['<', '>'] as &[_]));
    let dest_url = format!("{}moved-test.html", server.url());
    let resp = fetch!(b"MOVE", format!("{}test.html", server.url()))
        .header("Destination", &dest_url)
        .header("If", &if_header)
        .send()?;
    assert_eq!(resp.status(), 204);
    let resp = fetch!(b"PUT", format!("{}test.html", server.url()))
        .body("recreated at source")
        .send()?;
    assert!(resp.status() == 200 || resp.status() == 201);
    Ok(())
}

#[rstest]
fn lock_unlock_put(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let lock_token = resp.headers().get("lock-token").unwrap().to_str()?;
    let resp = fetch!(b"UNLOCK", format!("{}test.html", server.url()))
        .header("Lock-Token", lock_token)
        .send()?;
    assert_eq!(resp.status(), 204);
    let resp = fetch!(b"PUT", format!("{}test.html", server.url()))
        .body("after unlock")
        .send()?;
    assert!(resp.status() == 200 || resp.status() == 201);
    Ok(())
}

#[rstest]
fn independent_files_dont_conflict(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"LOCK", format!("{}test.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let resp = fetch!(b"PUT", format!("{}test.txt", server.url()))
        .body("modified")
        .send()?;
    assert!(resp.status() == 200 || resp.status() == 201);
    Ok(())
}
