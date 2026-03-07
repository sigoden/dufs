mod fixtures;
mod utils;

use assert_fs::TempDir;
use async_zip::{Compression, ZipEntryBuilder, tokio::write::ZipFileWriter};
use fixtures::{Error, TestServer, server};
use flate2::{Compression as GzCompression, write::GzEncoder};
use rstest::rstest;
use sevenz_rust2::compress_to_path;
use sha2::{Digest, Sha256};
use std::{fs, io::Cursor, path::Path};
use tar::{Builder as TarBuilder, Header as TarHeader};
use tokio::runtime::Runtime;

fn write_zip_with_compression(
    path: &Path,
    entries: Vec<(&str, &[u8])>,
    compression: Compression,
) -> Result<(), Error> {
    let rt = Runtime::new()?;
    rt.block_on(async {
        let file = tokio::fs::File::create(path).await?;
        let mut writer = ZipFileWriter::with_tokio(file);
        for (name, data) in entries {
            let builder = ZipEntryBuilder::new(name.into(), compression);
            writer.write_entry_whole(builder, data).await?;
        }
        writer.close().await?;
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}

fn write_zip(path: &Path, entries: Vec<(&str, &[u8])>) -> Result<(), Error> {
    write_zip_with_compression(path, entries, Compression::Stored)
}

fn write_7z(path: &Path, entries: Vec<(&str, &[u8])>) -> Result<(), Error> {
    let tmpdir = TempDir::new()?;
    for (name, data) in entries {
        let entry_path = tmpdir.path().join(name);
        if let Some(parent) = entry_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(entry_path, data)?;
    }
    compress_to_path(tmpdir.path(), path)?;
    Ok(())
}

fn write_tar(path: &Path, entries: Vec<(&str, &[u8])>) -> Result<(), Error> {
    let file = fs::File::create(path)?;
    let mut builder = TarBuilder::new(file);
    for (name, data) in entries {
        let mut header = TarHeader::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(1);
        header.set_cksum();
        builder.append_data(&mut header, name, Cursor::new(data))?;
    }
    builder.finish()?;
    Ok(())
}

fn write_tar_gz(path: &Path, entries: Vec<(&str, &[u8])>) -> Result<(), Error> {
    let file = fs::File::create(path)?;
    let encoder = GzEncoder::new(file, GzCompression::default());
    let mut builder = TarBuilder::new(encoder);
    for (name, data) in entries {
        let mut header = TarHeader::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(1);
        header.set_cksum();
        builder.append_data(&mut header, name, Cursor::new(data))?;
    }
    builder.finish()?;
    let encoder = builder.into_inner()?;
    encoder.finish()?;
    Ok(())
}

fn build_nested_zip_chain(depth: usize) -> Result<Vec<u8>, Error> {
    let tmpdir = TempDir::new()?;
    let mut current_path = tmpdir.path().join("level0.zip");
    write_zip(&current_path, vec![("payload.txt", b"payload")])?;
    let mut current_bytes = fs::read(&current_path)?;

    for level in 1..=depth {
        current_path = tmpdir.path().join(format!("level{level}.zip"));
        write_zip(&current_path, vec![("inner.zip", current_bytes.as_slice())])?;
        current_bytes = fs::read(&current_path)?;
    }

    Ok(current_bytes)
}

#[rstest]
fn archive_browse_lists_entries(
    #[with(&["--allow-archive-browse"])] server: TestServer,
) -> Result<(), Error> {
    let zip_path = server.path().join("archive.zip");
    write_zip(
        &zip_path,
        vec![("folder/note.txt", b"note"), ("root.txt", b"root")],
    )?;

    assert!(zip_path.exists());

    let url = format!("{}archive.zip/?json", server.url());
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    let body = resp.text()?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    let names: Vec<String> = value
        .get("paths")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .filter_map(|v| {
            v.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n.to_string())
        })
        .collect();

    assert!(names.contains(&"folder".to_string()));
    assert!(names.contains(&"root.txt".to_string()));
    Ok(())
}

#[rstest]
fn archive_view_shows_editor_ui(
    #[with(&["--allow-archive-browse"])] server: TestServer,
) -> Result<(), Error> {
    let zip_path = server.path().join("archive.zip");
    write_zip(&zip_path, vec![("folder/note.txt", b"hello")])?;

    let url = format!("{}archive.zip/folder/note.txt?view", server.url());
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    let body = resp.text()?;
    let editable = utils::retrieve_edit_file(&body).unwrap_or(false);
    let kind = utils::retrieve_kind(&body);
    assert!(editable);
    assert_eq!(kind.as_deref(), Some("View"));
    Ok(())
}

#[rstest]
fn archive_hash_works(
    #[with(&["--allow-archive-browse", "--allow-hash"])] server: TestServer,
) -> Result<(), Error> {
    let data = b"hash-me";
    let zip_path = server.path().join("archive.zip");
    write_zip(&zip_path, vec![("folder/note.txt", data)])?;

    let url = format!("{}archive.zip/folder/note.txt?hash", server.url());
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    let body = resp.text()?;

    let mut hasher = Sha256::new();
    hasher.update(data);
    let expected = format!("{:x}", hasher.finalize());
    assert_eq!(body, expected);
    Ok(())
}

#[rstest]
fn archive_extensions_allow_custom_zip_formats(
    #[with(&["--allow-archive-browse", "--archive-extensions", "tpf"])] server: TestServer,
) -> Result<(), Error> {
    let zip_path = server.path().join("archive.tpf");
    write_zip(&zip_path, vec![("folder/note.txt", b"note")])?;

    let url = format!("{}archive.tpf/?json", server.url());
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    let body = resp.text()?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    let names: Vec<String> = value
        .get("paths")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .filter_map(|v| {
            v.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n.to_string())
        })
        .collect();

    assert!(names.contains(&"folder".to_string()));
    Ok(())
}

#[rstest]
fn builtin_archive_extensions_still_work_when_custom_extensions_are_overridden(
    #[with(&["--allow-archive-browse", "--archive-extensions", "tpf"])] server: TestServer,
) -> Result<(), Error> {
    let zip_path = server.path().join("archive.zip");
    write_zip(&zip_path, vec![("folder/note.txt", b"note")])?;

    let url = format!("{}archive.zip/?json", server.url());
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    Ok(())
}

#[rstest]
fn archive_download_returns_raw_file(
    #[with(&["--allow-archive-browse"])] server: TestServer,
) -> Result<(), Error> {
    let zip_path = server.path().join("archive.zip");
    write_zip(&zip_path, vec![("folder/note.txt", b"note")])?;

    let url = format!("{}archive.zip?download", server.url());
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    let body = resp.bytes()?;
    let expected = fs::read(&zip_path)?;
    assert_eq!(body.as_ref(), expected.as_slice());
    Ok(())
}

#[rstest]
fn archive_search_filters_entries(
    #[with(&["--allow-archive-browse", "--allow-search"])] server: TestServer,
) -> Result<(), Error> {
    let zip_path = server.path().join("archive.zip");
    write_zip(
        &zip_path,
        vec![
            ("folder/note.txt", b"note"),
            ("folder/readme.md", b"readme"),
            ("root.txt", b"root"),
            ("data.json", b"{}"),
        ],
    )?;

    let url = format!("{}archive.zip/?q=note", server.url());
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    let body = resp.text()?;
    let paths = utils::retrieve_index_paths(&body);

    // Search should find "note.txt" but not "readme.md", "root.txt", or "data.json"
    assert!(paths.iter().any(|p| p.contains("note.txt")));
    assert!(!paths.iter().any(|p| p.contains("readme.md")));
    assert!(!paths.iter().any(|p| p.contains("root.txt")));
    assert!(!paths.iter().any(|p| p.contains("data.json")));
    Ok(())
}

#[rstest]
fn archive_edit_shows_editor_ui(
    #[with(&["--allow-archive-browse"])] server: TestServer,
) -> Result<(), Error> {
    let zip_path = server.path().join("archive.zip");
    write_zip(&zip_path, vec![("folder/note.txt", b"hello world")])?;

    let url = format!("{}archive.zip/folder/note.txt?edit", server.url());
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    let body = resp.text()?;
    let editable = utils::retrieve_edit_file(&body).unwrap_or(false);
    let kind = utils::retrieve_kind(&body);
    assert!(editable);
    assert_eq!(kind.as_deref(), Some("Edit"));
    Ok(())
}

#[rstest]
fn sevenz_browse_lists_entries(
    #[with(&["--allow-archive-browse"])] server: TestServer,
) -> Result<(), Error> {
    let archive_path = server.path().join("archive.7z");
    write_7z(
        &archive_path,
        vec![("folder/note.txt", b"note"), ("root.txt", b"root")],
    )?;

    let url = format!("{}archive.7z/?json", server.url());
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    let body = resp.text()?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    let names: Vec<String> = value
        .get("paths")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .filter_map(|v| {
            v.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n.to_string())
        })
        .collect();

    assert!(names.contains(&"folder".to_string()));
    assert!(names.contains(&"root.txt".to_string()));
    Ok(())
}

#[rstest]
fn sevenz_view_shows_editor_ui(
    #[with(&["--allow-archive-browse"])] server: TestServer,
) -> Result<(), Error> {
    let archive_path = server.path().join("archive.7z");
    write_7z(&archive_path, vec![("folder/note.txt", b"hello")])?;

    let url = format!("{}archive.7z/folder/note.txt?view", server.url());
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    let body = resp.text()?;
    let editable = utils::retrieve_edit_file(&body).unwrap_or(false);
    let kind = utils::retrieve_kind(&body);
    assert!(editable);
    assert_eq!(kind.as_deref(), Some("View"));
    Ok(())
}

#[rstest]
fn tar_browse_lists_entries(
    #[with(&["--allow-archive-browse"])] server: TestServer,
) -> Result<(), Error> {
    let archive_path = server.path().join("archive.tar");
    write_tar(
        &archive_path,
        vec![("folder/note.txt", b"note"), ("root.txt", b"root")],
    )?;

    let url = format!("{}archive.tar/?json", server.url());
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    let body = resp.text()?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    let names: Vec<String> = value
        .get("paths")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .filter_map(|v| {
            v.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n.to_string())
        })
        .collect();

    assert!(names.contains(&"folder".to_string()));
    assert!(names.contains(&"root.txt".to_string()));
    Ok(())
}

#[rstest]
fn tar_gz_view_shows_editor_ui(
    #[with(&["--allow-archive-browse"])] server: TestServer,
) -> Result<(), Error> {
    let archive_path = server.path().join("archive.tar.gz");
    write_tar_gz(&archive_path, vec![("folder/note.txt", b"hello")])?;

    let url = format!("{}archive.tar.gz/folder/note.txt?view", server.url());
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    let body = resp.text()?;
    let editable = utils::retrieve_edit_file(&body).unwrap_or(false);
    let kind = utils::retrieve_kind(&body);
    assert!(editable);
    assert_eq!(kind.as_deref(), Some("View"));
    Ok(())
}

#[rstest]
fn nested_zip_browse_lists_entries(
    #[with(&["--allow-archive-browse"])] server: TestServer,
) -> Result<(), Error> {
    let inner_path = server.path().join("inner.zip");
    write_zip(
        &inner_path,
        vec![("folder/note.txt", b"note"), ("root.txt", b"root")],
    )?;
    let inner_bytes = fs::read(&inner_path)?;

    let outer_path = server.path().join("outer.zip");
    write_zip(&outer_path, vec![("inner.zip", inner_bytes.as_slice())])?;

    let url = format!("{}outer.zip/inner.zip/?json", server.url());
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    let body = resp.text()?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    let names: Vec<String> = value
        .get("paths")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .filter_map(|v| {
            v.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n.to_string())
        })
        .collect();

    assert!(names.contains(&"folder".to_string()));
    assert!(names.contains(&"root.txt".to_string()));
    Ok(())
}

#[rstest]
fn nested_zip_view_shows_editor_ui(
    #[with(&["--allow-archive-browse"])] server: TestServer,
) -> Result<(), Error> {
    let inner_path = server.path().join("inner.zip");
    write_zip(&inner_path, vec![("folder/note.txt", b"hello")])?;
    let inner_bytes = fs::read(&inner_path)?;

    let outer_path = server.path().join("outer.zip");
    write_zip(&outer_path, vec![("inner.zip", inner_bytes.as_slice())])?;

    let url = format!("{}outer.zip/inner.zip/folder/note.txt?view", server.url());
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), 200);
    let body = resp.text()?;
    let editable = utils::retrieve_edit_file(&body).unwrap_or(false);
    let kind = utils::retrieve_kind(&body);
    assert!(editable);
    assert_eq!(kind.as_deref(), Some("View"));
    Ok(())
}

#[rstest]
fn archive_download_rejects_large_entries(
    #[with(&["--allow-archive-browse"])] server: TestServer,
) -> Result<(), Error> {
    let zip_path = server.path().join("archive.zip");
    let data = vec![0u8; 33 * 1024 * 1024];
    write_zip_with_compression(
        &zip_path,
        vec![("folder/large.bin", data.as_slice())],
        Compression::Deflate,
    )?;

    let url = format!("{}archive.zip/folder/large.bin", server.url());
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), reqwest::StatusCode::PAYLOAD_TOO_LARGE);
    Ok(())
}

#[rstest]
fn nested_zip_browse_rejects_excessive_depth(
    #[with(&["--allow-archive-browse"])] server: TestServer,
) -> Result<(), Error> {
    let outer_path = server.path().join("outer.zip");
    fs::write(&outer_path, build_nested_zip_chain(9)?)?;

    let nested_path = "inner.zip/".repeat(9);
    let url = format!("{}outer.zip/{}?json", server.url(), nested_path);
    let resp = reqwest::blocking::get(url)?;
    assert_eq!(resp.status(), reqwest::StatusCode::PAYLOAD_TOO_LARGE);
    Ok(())
}
