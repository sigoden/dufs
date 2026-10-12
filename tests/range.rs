mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use rstest::rstest;

#[rstest]
fn get_file_range(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-6"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.headers().get("content-range").unwrap(), "bytes 0-6/18");
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert_eq!(resp.headers().get("content-length").unwrap(), "7");
    assert_eq!(resp.text()?, "This is");
    Ok(())
}

#[rstest]
fn get_file_range_beyond(server: TestServer) -> Result<(), Error> {
    // last-pos beyond EOF is clamped to size - 1 (RFC 9110 §14.1.2)
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=12-20"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(
        resp.headers().get("content-range").unwrap(),
        "bytes 12-17/18"
    );
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert_eq!(resp.headers().get("content-length").unwrap(), "6");
    assert_eq!(resp.text()?, "x.html");
    Ok(())
}

#[rstest]
fn get_file_range_invalid(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=20-"))
        .send()?;
    assert_eq!(resp.status(), 416);
    assert_eq!(resp.headers().get("content-range").unwrap(), "bytes */18");
    Ok(())
}

#[rstest]
fn get_file_range_last_pos_beyond_eof(server: TestServer) -> Result<(), Error> {
    // RFC 9110 §14.1.2: last-pos >= size must be clamped to size - 1, not 416
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-19"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(
        resp.headers().get("content-range").unwrap(),
        "bytes 0-17/18"
    );
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert_eq!(resp.headers().get("content-length").unwrap(), "18");
    assert_eq!(resp.text()?, "This is index.html");
    Ok(())
}

#[rstest]
fn get_file_range_last_pos_at_eof(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-18"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(
        resp.headers().get("content-range").unwrap(),
        "bytes 0-17/18"
    );
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert_eq!(resp.headers().get("content-length").unwrap(), "18");
    assert_eq!(resp.text()?, "This is index.html");
    Ok(())
}

#[rstest]
fn get_file_range_last_pos_beyond_eof_middle(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=6-99"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(
        resp.headers().get("content-range").unwrap(),
        "bytes 6-17/18"
    );
    assert_eq!(resp.headers().get("content-length").unwrap(), "12");
    assert_eq!(resp.text()?, "s index.html");
    Ok(())
}

#[rstest]
fn head_file_range_last_pos_beyond_eof(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-19"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(
        resp.headers().get("content-range").unwrap(),
        "bytes 0-17/18"
    );
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert_eq!(resp.headers().get("content-length").unwrap(), "18");
    Ok(())
}

#[rstest]
fn get_file_multipart_range_last_pos_beyond_eof(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-19, 6-17"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");

    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()?
        .to_string();
    assert!(content_type.starts_with("multipart/byteranges; boundary="));

    let boundary = content_type.split_once('=').unwrap().1.trim_ascii_start();
    assert!(!boundary.is_empty());

    let body = resp.text()?;
    let parts = parse_multipart_body(&body, boundary);
    assert_eq!(parts.len(), 2);

    let (headers, body) = &parts[0];
    assert_eq!(headers.get("content-range").unwrap(), "bytes 0-17/18");
    assert_eq!(*body, "This is index.html");

    let (headers, body) = &parts[1];
    assert_eq!(headers.get("content-range").unwrap(), "bytes 6-17/18");
    assert_eq!(*body, "s index.html");

    Ok(())
}

#[rstest]
fn get_file_multipart_range_first_pos_beyond_eof(server: TestServer) -> Result<(), Error> {
    // Mixed satisfiable + unsatisfiable (first-pos >= size) is still 416
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-19, 20-"))
        .send()?;
    assert_eq!(resp.status(), 416);
    assert_eq!(resp.headers().get("content-range").unwrap(), "bytes */18");
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    Ok(())
}

fn parse_multipart_body<'a>(body: &'a str, boundary: &str) -> Vec<(HeaderMap, &'a str)> {
    body.split(&format!("--{boundary}"))
        .filter(|part| !part.is_empty() && *part != "--\r\n")
        .map(|part| {
            let (head, body) = part.trim_ascii().split_once("\r\n\r\n").unwrap();
            let headers = head
                .split("\r\n")
                .fold(HeaderMap::new(), |mut headers, header| {
                    let (key, value) = header.split_once(":").unwrap();
                    let key = HeaderName::from_bytes(key.as_bytes()).unwrap();
                    let value = HeaderValue::from_str(value.trim_ascii_start()).unwrap();
                    headers.insert(key, value);
                    headers
                });
            (headers, body)
        })
        .collect()
}

#[rstest]
fn get_file_multipart_range(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-11, 6-17"))
        .send()?;
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");

    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()?
        .to_string();
    assert!(content_type.starts_with("multipart/byteranges; boundary="));

    let boundary = content_type.split_once('=').unwrap().1.trim_ascii_start();
    assert!(!boundary.is_empty());

    let body = resp.text()?;
    let parts = parse_multipart_body(&body, boundary);
    assert_eq!(parts.len(), 2);

    let (headers, body) = &parts[0];
    assert_eq!(headers.get("content-range").unwrap(), "bytes 0-11/18");
    assert_eq!(*body, "This is inde");

    let (headers, body) = &parts[1];
    assert_eq!(headers.get("content-range").unwrap(), "bytes 6-17/18");
    assert_eq!(*body, "s index.html");

    Ok(())
}

#[rstest]
fn get_file_multipart_range_invalid(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=0-6, 20-30"))
        .send()?;
    assert_eq!(resp.status(), 416);
    assert_eq!(resp.headers().get("content-range").unwrap(), "bytes */18");
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert_eq!(resp.headers().get("content-length").unwrap(), "0");
    Ok(())
}

#[rstest]
fn get_file_range_reversed(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=10-1"))
        .send()?;
    assert_eq!(resp.status(), 416);
    assert_eq!(resp.headers().get("content-range").unwrap(), "bytes */18");
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    Ok(())
}

#[rstest]
fn get_file_multipart_range_reversed(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html", server.url()))
        .header("range", HeaderValue::from_static("bytes=10-1,20-2"))
        .send()?;
    assert_eq!(resp.status(), 416);
    assert_eq!(resp.headers().get("content-range").unwrap(), "bytes */18");
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    Ok(())
}
