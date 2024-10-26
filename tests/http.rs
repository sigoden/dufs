mod fixtures;
mod utils;

use fixtures::{server, Error, TestServer, BIN_FILE};
use rstest::rstest;
use serde_json::Value;
use utils::retrieve_edit_file;

#[rstest]
fn get_dir(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(server.url())?;
    assert_resp_paths!(resp);
    Ok(())
}

#[rstest]
fn head_dir(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", server.url()).send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    assert_eq!(resp.text()?, "");
    Ok(())
}

#[rstest]
fn get_dir_404(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}404/", server.url()))?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn head_dir_404(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}404/", server.url())).send()?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
#[case(server(&["--allow-archive"] as &[&str]))]
#[case(server(&["--allow-archive", "--compress", "none"]))]
#[case(server(&["--allow-archive", "--compress", "low"]))]
#[case(server(&["--allow-archive", "--compress", "medium"]))]
#[case(server(&["--allow-archive", "--compress", "high"]))]
fn get_dir_zip(#[case] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?zip", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/zip"
    );
    assert!(resp.headers().contains_key("content-disposition"));
    Ok(())
}

#[rstest]
fn get_dir_json(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?json", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );
    let json: Value = serde_json::from_str(&resp.text().unwrap()).unwrap();
    assert!(json["paths"].as_array().is_some());
    Ok(())
}

#[rstest]
fn get_dir_simple(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?simple", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    let text = resp.text().unwrap();
    assert!(text.split('\n').any(|v| v == "index.html"));
    Ok(())
}

#[rstest]
fn head_dir_zip(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}?zip", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/zip"
    );
    assert!(resp.headers().contains_key("content-disposition"));
    assert_eq!(resp.text()?, "");
    Ok(())
}

#[rstest]
fn get_dir_search(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?q={}", server.url(), "test.html"))?;
    assert_eq!(resp.status(), 200);
    let paths = utils::retrieve_index_paths(&resp.text()?);
    assert!(!paths.is_empty());
    for p in paths {
        assert!(p.contains("test.html"));
    }
    Ok(())
}

#[rstest]
fn get_dir_search2(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?q={BIN_FILE}", server.url()))?;
    assert_eq!(resp.status(), 200);
    let paths = utils::retrieve_index_paths(&resp.text()?);
    assert!(!paths.is_empty());
    for p in paths {
        assert!(p.contains(BIN_FILE));
    }
    Ok(())
}

#[rstest]
fn get_dir_search3(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?q={}&simple", server.url(), "test.html"))?;
    assert_eq!(resp.status(), 200);
    let text = resp.text().unwrap();
    assert!(text.split('\n').any(|v| v == "test.html"));
    Ok(())
}

#[rstest]
fn get_dir_search4(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}dir1?q=dir1&simple", server.url()))?;
    assert_eq!(resp.status(), 200);
    let text = resp.text().unwrap();
    assert!(text.is_empty());
    Ok(())
}

#[rstest]
fn head_dir_search(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}?q={}", server.url(), "test.html")).send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    assert_eq!(resp.text()?, "");
    Ok(())
}

#[rstest]
fn empty_search(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}?q=", server.url()))?;
    assert_resp_paths!(resp);
    Ok(())
}

#[rstest]
fn get_file(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}index.html", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/html; charset=UTF-8"
    );
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert!(resp.headers().contains_key("etag"));
    assert!(resp.headers().contains_key("last-modified"));
    assert!(resp.headers().contains_key("content-length"));
    assert_eq!(resp.text()?, "This is index.html");
    Ok(())
}

#[rstest]
fn head_file(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}index.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/html; charset=UTF-8"
    );
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert!(resp.headers().contains_key("content-disposition"));
    assert!(resp.headers().contains_key("etag"));
    assert!(resp.headers().contains_key("last-modified"));
    assert!(resp.headers().contains_key("content-length"));
    assert_eq!(resp.text()?, "");
    Ok(())
}

#[rstest]
fn hash_file(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}index.html?hash", server.url()))?;
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.text()?,
        "c8dd395e3202674b9512f7b7f956e0d96a8ba8f572e785b0d5413ab83766dbc4"
    );
    Ok(())
}

#[rstest]
fn get_file_404(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}404", server.url()))?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn get_file_emoji_path(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}{BIN_FILE}", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-disposition").unwrap(),
        "inline; filename=\"😀.bin\"; filename*=UTF-8''%F0%9F%98%80.bin"
    );
    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[rstest]
fn get_file_newline_path(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}file%0A1.txt", server.url()))?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-disposition").unwrap(),
        "inline; filename=\"file 1.txt\""
    );
    Ok(())
}

#[rstest]
fn get_file_edit(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}index.html?edit", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let editable = retrieve_edit_file(&resp.text().unwrap()).unwrap();
    assert!(editable);
    Ok(())
}

#[rstest]
fn get_file_edit_bin(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"GET", format!("{}{BIN_FILE}?edit", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    let editable = retrieve_edit_file(&resp.text().unwrap()).unwrap();
    assert!(!editable);
    Ok(())
}

#[rstest]
fn head_file_404(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"HEAD", format!("{}404", server.url())).send()?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn options_dir(server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"OPTIONS", format!("{}index.html", server.url())).send()?;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("allow").unwrap(),
        "GET,HEAD,PUT,OPTIONS,DELETE,PATCH,PROPFIND,COPY,MOVE,CHECKAUTH,LOGOUT"
    );
    assert_eq!(resp.headers().get("dav").unwrap(), "1, 2, 3");
    Ok(())
}

#[rstest]
fn put_file(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 201);
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn put_file_create_dir(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}xyz/file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 201);
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn put_file_conflict_dir(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}dir1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 403);
    Ok(())
}

#[rstest]
fn delete_file(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}test.html", server.url());
    let resp = fetch!(b"DELETE", &url).send()?;
    assert_eq!(resp.status(), 204);
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn delete_file_404(#[with(&["-A"])] server: TestServer) -> Result<(), Error> {
    let resp = fetch!(b"DELETE", format!("{}file1", server.url())).send()?;
    assert_eq!(resp.status(), 404);
    Ok(())
}

#[rstest]
fn get_file_content_type(server: TestServer) -> Result<(), Error> {
    let resp = reqwest::blocking::get(format!("{}content-types/bin.tar", server.url()))?;
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/x-tar"
    );
    let resp = reqwest::blocking::get(format!("{}content-types/bin", server.url()))?;
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/octet-stream"
    );
    let resp = reqwest::blocking::get(format!("{}content-types/file-utf8.txt", server.url()))?;
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/plain; charset=UTF-8"
    );
    let resp = reqwest::blocking::get(format!("{}content-types/file-gbk.txt", server.url()))?;
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/plain; charset=GBK"
    );
    let resp = reqwest::blocking::get(format!("{}content-types/file", server.url()))?;
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/plain; charset=UTF-8"
    );
    Ok(())
}

#[rstest]
fn resumable_upload(#[with(&["--allow-upload"])] server: TestServer) -> Result<(), Error> {
    let url = format!("{}file1", server.url());
    let resp = fetch!(b"PUT", &url).body(b"abc".to_vec()).send()?;
    assert_eq!(resp.status(), 201);
    let resp = fetch!(b"PATCH", &url)
        .header("X-Update-Range", "append")
        .body(b"123".to_vec())
        .send()?;
    assert_eq!(resp.status(), 204);
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().unwrap(), "abc123");
    Ok(())
}
