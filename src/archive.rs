use anyhow::{Context, Result, bail};
use async_zip::{
    ZipDateTime,
    base::read::{mem::ZipFileReader as ZipMemoryReader, seek::ZipFileReader as ZipPathReader},
};
use chrono::LocalResult;
use flate2::read::MultiGzDecoder;
use sevenz_rust2::{ArchiveReader as SevenZipReader, Password};
use std::{
    fs::File as StdFile,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tar::Archive as TarArchive;
use tokio::{
    fs::File as TokioFile,
    io::{AsyncReadExt, BufReader as TokioBufReader},
};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

const TAR_HEADER_SIZE: usize = 512;
const TAR_MAGIC_OFFSET: usize = 257;
const TAR_MAGIC_END: usize = TAR_MAGIC_OFFSET + 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    SevenZip,
    Tar,
    TarGz,
}

impl ArchiveFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::SevenZip => "7z",
            Self::Tar => "tar",
            Self::TarGz => "tar.gz",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArchiveBrowsePath {
    pub archive_relative_path: String,
    pub inner_path: String,
    pub inner_path_has_trailing_slash: bool,
}

#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    pub index: usize,
    pub raw_name: String,
    pub normalized_name: String,
    pub is_dir: bool,
    pub mtime: u64,
    pub size: u64,
}

pub enum ArchiveHandle {
    ZipPath {
        format: ArchiveFormat,
        label: String,
        reader: ZipPathReader<tokio_util::compat::Compat<TokioBufReader<TokioFile>>>,
        entries: Vec<ArchiveEntry>,
    },
    ZipMemory {
        format: ArchiveFormat,
        label: String,
        reader: ZipMemoryReader,
        entries: Vec<ArchiveEntry>,
    },
    SevenZipPath {
        format: ArchiveFormat,
        label: String,
        reader: SevenZipReader<std::fs::File>,
        entries: Vec<ArchiveEntry>,
    },
    SevenZipMemory {
        format: ArchiveFormat,
        label: String,
        reader: SevenZipReader<Cursor<Vec<u8>>>,
        entries: Vec<ArchiveEntry>,
    },
    TarPath {
        format: ArchiveFormat,
        label: String,
        path: PathBuf,
        entries: Vec<ArchiveEntry>,
    },
    TarMemory {
        format: ArchiveFormat,
        label: String,
        bytes: Vec<u8>,
        entries: Vec<ArchiveEntry>,
    },
}

impl ArchiveHandle {
    pub async fn open_path(path: &Path, label: String) -> Result<Self> {
        let format = detect_archive_format_path(path)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Unsupported archive format"))?;
        match format {
            ArchiveFormat::Zip => {
                let file = TokioFile::open(path).await?;
                let reader = TokioBufReader::new(file).compat();
                let reader = ZipPathReader::new(reader).await?;
                let entries = collect_zip_entries(reader.file().entries())?;
                Ok(Self::ZipPath {
                    format,
                    label,
                    reader,
                    entries,
                })
            }
            ArchiveFormat::SevenZip => {
                let reader = SevenZipReader::open(path, Password::empty())?;
                let entries = collect_seven_zip_entries(reader.archive());
                Ok(Self::SevenZipPath {
                    format,
                    label,
                    reader,
                    entries,
                })
            }
            ArchiveFormat::Tar | ArchiveFormat::TarGz => {
                let entries = collect_tar_entries_path(path.to_path_buf(), format).await?;
                Ok(Self::TarPath {
                    format,
                    label,
                    path: path.to_path_buf(),
                    entries,
                })
            }
        }
    }

    pub async fn open_bytes(bytes: Vec<u8>, label: String) -> Result<Self> {
        let format = detect_archive_format_bytes(&bytes)
            .ok_or_else(|| anyhow::anyhow!("Unsupported archive format"))?;
        match format {
            ArchiveFormat::Zip => {
                let reader = ZipMemoryReader::new(bytes).await?;
                let entries = collect_zip_entries(reader.file().entries())?;
                Ok(Self::ZipMemory {
                    format,
                    label,
                    reader,
                    entries,
                })
            }
            ArchiveFormat::SevenZip => {
                let reader = SevenZipReader::new(Cursor::new(bytes), Password::empty())?;
                let entries = collect_seven_zip_entries(reader.archive());
                Ok(Self::SevenZipMemory {
                    format,
                    label,
                    reader,
                    entries,
                })
            }
            ArchiveFormat::Tar | ArchiveFormat::TarGz => {
                let entries = collect_tar_entries_bytes(bytes.clone(), format).await?;
                Ok(Self::TarMemory {
                    format,
                    label,
                    bytes,
                    entries,
                })
            }
        }
    }

    pub fn format(&self) -> ArchiveFormat {
        match self {
            Self::ZipPath { format, .. }
            | Self::ZipMemory { format, .. }
            | Self::SevenZipPath { format, .. }
            | Self::SevenZipMemory { format, .. }
            | Self::TarPath { format, .. }
            | Self::TarMemory { format, .. } => *format,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::ZipPath { label, .. }
            | Self::ZipMemory { label, .. }
            | Self::SevenZipPath { label, .. }
            | Self::SevenZipMemory { label, .. }
            | Self::TarPath { label, .. }
            | Self::TarMemory { label, .. } => label,
        }
    }

    pub fn entries(&self) -> &[ArchiveEntry] {
        match self {
            Self::ZipPath { entries, .. }
            | Self::ZipMemory { entries, .. }
            | Self::SevenZipPath { entries, .. }
            | Self::SevenZipMemory { entries, .. }
            | Self::TarPath { entries, .. }
            | Self::TarMemory { entries, .. } => entries,
        }
    }

    pub fn find_entry(&self, normalized_name: &str) -> Option<&ArchiveEntry> {
        self.entries()
            .iter()
            .find(|entry| entry.normalized_name == normalized_name)
    }

    pub async fn read_file(&mut self, normalized_name: &str) -> Result<Vec<u8>> {
        let entry = self
            .find_entry(normalized_name)
            .ok_or_else(|| anyhow::anyhow!("Archive entry not found"))?
            .clone();
        if entry.is_dir {
            bail!("Archive entry is a directory");
        }
        match self {
            Self::ZipPath { reader, .. } => {
                let mut entry_reader = reader.reader_without_entry(entry.index).await?.compat();
                let mut data = Vec::with_capacity(
                    usize::try_from(entry.size).context("Archive entry too large")?,
                );
                entry_reader.read_to_end(&mut data).await?;
                Ok(data)
            }
            Self::ZipMemory { reader, .. } => {
                let mut entry_reader = reader.reader_without_entry(entry.index).await?.compat();
                let mut data = Vec::with_capacity(
                    usize::try_from(entry.size).context("Archive entry too large")?,
                );
                entry_reader.read_to_end(&mut data).await?;
                Ok(data)
            }
            Self::SevenZipPath { reader, .. } => Ok(reader.read_file(&entry.raw_name)?),
            Self::SevenZipMemory { reader, .. } => Ok(reader.read_file(&entry.raw_name)?),
            Self::TarPath { format, path, .. } => {
                read_tar_file_from_path(path.clone(), *format, entry.normalized_name).await
            }
            Self::TarMemory { format, bytes, .. } => {
                read_tar_file_from_bytes(bytes.clone(), *format, entry.normalized_name).await
            }
        }
    }
}

pub async fn detect_archive_format_path(path: &Path) -> Result<Option<ArchiveFormat>> {
    let mut file = TokioFile::open(path).await?;
    let mut buffer = [0u8; TAR_HEADER_SIZE];
    let size = file.read(&mut buffer).await?;
    let bytes = &buffer[..size];

    if let Some(format) = detect_archive_format_header(bytes) {
        return Ok(Some(format));
    }

    if bytes.starts_with(&[0x1F, 0x8B]) {
        let path = path.to_path_buf();
        return tokio::task::spawn_blocking(move || detect_gzip_tar_format_path(&path))
            .await
            .context("Failed to detect gzip archive format")?;
    }

    Ok(None)
}

pub fn detect_archive_format_bytes(bytes: &[u8]) -> Option<ArchiveFormat> {
    if let Some(format) = detect_archive_format_header(bytes) {
        return Some(format);
    }

    if bytes.starts_with(&[0x1F, 0x8B])
        && let Ok(true) = gzip_stream_contains_tar(bytes)
    {
        return Some(ArchiveFormat::TarGz);
    }

    None
}

pub fn normalize_archive_inner_path(inner: &str) -> Option<String> {
    if inner.is_empty() {
        return Some(String::new());
    }
    let mut parts = Vec::new();
    for part in inner.split('/') {
        if part.is_empty() {
            continue;
        }
        if part == "." || part == ".." {
            return None;
        }
        parts.push(part);
    }
    Some(parts.join("/"))
}

pub fn normalize_archive_entry_name(name: &str) -> Option<String> {
    let name = name.replace('\\', "/");
    if name.starts_with('/') {
        return None;
    }
    let mut parts = Vec::new();
    let mut trailing_slash = false;
    let total_parts = name.split('/').count();
    for (idx, part) in name.split('/').enumerate() {
        if part.is_empty() {
            if idx == total_parts - 1 {
                trailing_slash = true;
                continue;
            }
            return None;
        }
        if part == "." || part == ".." {
            return None;
        }
        parts.push(part);
    }
    let mut normalized = parts.join("/");
    if trailing_slash {
        normalized.push('/');
    }
    Some(normalized)
}

fn collect_zip_entries(entries: &[async_zip::StoredZipEntry]) -> Result<Vec<ArchiveEntry>> {
    let mut items = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let raw_name = match entry.filename().as_str() {
            Ok(value) => value,
            Err(_) => continue,
        };
        let normalized_name = match normalize_archive_entry_name(raw_name) {
            Some(value) => value,
            None => continue,
        };
        let is_dir = entry.dir().unwrap_or(false) || normalized_name.ends_with('/');
        items.push(ArchiveEntry {
            index,
            raw_name: raw_name.to_string(),
            normalized_name,
            is_dir,
            mtime: zip_datetime_to_timestamp(entry.last_modification_date()),
            size: if is_dir { 0 } else { entry.uncompressed_size() },
        });
    }
    Ok(items)
}

fn collect_seven_zip_entries(archive: &sevenz_rust2::Archive) -> Vec<ArchiveEntry> {
    archive
        .files
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let normalized_name = normalize_archive_entry_name(entry.name())?;
            let is_dir = entry.is_directory() || normalized_name.ends_with('/');
            let mtime = if entry.has_last_modified_date {
                seven_zip_datetime_to_timestamp(entry.last_modified_date())
            } else {
                0
            };
            Some(ArchiveEntry {
                index,
                raw_name: entry.name().to_string(),
                normalized_name,
                is_dir,
                mtime,
                size: if is_dir { 0 } else { entry.size() },
            })
        })
        .collect()
}

async fn collect_tar_entries_path(
    path: PathBuf,
    format: ArchiveFormat,
) -> Result<Vec<ArchiveEntry>> {
    tokio::task::spawn_blocking(move || {
        let reader = open_tar_reader_from_path(&path, format)?;
        collect_tar_entries(reader)
    })
    .await
    .context("Failed to inspect tar archive")?
}

async fn collect_tar_entries_bytes(
    bytes: Vec<u8>,
    format: ArchiveFormat,
) -> Result<Vec<ArchiveEntry>> {
    tokio::task::spawn_blocking(move || {
        let reader = open_tar_reader_from_bytes(bytes, format)?;
        collect_tar_entries(reader)
    })
    .await
    .context("Failed to inspect tar archive")?
}

fn collect_tar_entries(reader: Box<dyn Read>) -> Result<Vec<ArchiveEntry>> {
    let mut archive = TarArchive::new(reader);
    let mut items = Vec::new();
    for (index, entry) in archive.entries()?.enumerate() {
        let entry = entry?;
        let entry_type = entry.header().entry_type();
        if !entry_type.is_dir() && !entry_type.is_file() {
            continue;
        }
        let raw_name = String::from_utf8_lossy(entry.path_bytes().as_ref()).to_string();
        let normalized_name = match normalize_archive_entry_name(&raw_name) {
            Some(value) => value,
            None => continue,
        };
        let is_dir = entry_type.is_dir() || normalized_name.ends_with('/');
        items.push(ArchiveEntry {
            index,
            raw_name,
            normalized_name,
            is_dir,
            mtime: entry.header().mtime().unwrap_or(0),
            size: if is_dir { 0 } else { entry.size() },
        });
    }
    Ok(items)
}

async fn read_tar_file_from_path(
    path: PathBuf,
    format: ArchiveFormat,
    normalized_name: String,
) -> Result<Vec<u8>> {
    tokio::task::spawn_blocking(move || {
        let reader = open_tar_reader_from_path(&path, format)?;
        read_tar_file(reader, &normalized_name)
    })
    .await
    .context("Failed to read tar archive entry")?
}

async fn read_tar_file_from_bytes(
    bytes: Vec<u8>,
    format: ArchiveFormat,
    normalized_name: String,
) -> Result<Vec<u8>> {
    tokio::task::spawn_blocking(move || {
        let reader = open_tar_reader_from_bytes(bytes, format)?;
        read_tar_file(reader, &normalized_name)
    })
    .await
    .context("Failed to read tar archive entry")?
}

fn read_tar_file(reader: Box<dyn Read>, normalized_name: &str) -> Result<Vec<u8>> {
    let mut archive = TarArchive::new(reader);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() {
            continue;
        }
        let raw_name = String::from_utf8_lossy(entry.path_bytes().as_ref()).to_string();
        let Some(candidate_name) = normalize_archive_entry_name(&raw_name) else {
            continue;
        };
        if candidate_name != normalized_name {
            continue;
        }
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;
        return Ok(data);
    }
    bail!("Archive entry not found")
}

fn open_tar_reader_from_path(path: &Path, format: ArchiveFormat) -> Result<Box<dyn Read>> {
    let file = StdFile::open(path)?;
    open_tar_reader(file, format)
}

fn open_tar_reader_from_bytes(bytes: Vec<u8>, format: ArchiveFormat) -> Result<Box<dyn Read>> {
    open_tar_reader(Cursor::new(bytes), format)
}

fn open_tar_reader<R>(reader: R, format: ArchiveFormat) -> Result<Box<dyn Read>>
where
    R: Read + 'static,
{
    match format {
        ArchiveFormat::Tar => Ok(Box::new(reader)),
        ArchiveFormat::TarGz => Ok(Box::new(MultiGzDecoder::new(reader))),
        _ => bail!("Unsupported tar archive format"),
    }
}

fn detect_archive_format_header(bytes: &[u8]) -> Option<ArchiveFormat> {
    const ZIP_SIGNATURES: [&[u8]; 3] = [b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08"];
    const SEVEN_Z_SIGNATURE: &[u8] = &[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C];

    if ZIP_SIGNATURES
        .iter()
        .any(|signature| bytes.starts_with(signature))
    {
        return Some(ArchiveFormat::Zip);
    }
    if bytes.starts_with(SEVEN_Z_SIGNATURE) {
        return Some(ArchiveFormat::SevenZip);
    }
    if looks_like_tar_header(bytes) {
        return Some(ArchiveFormat::Tar);
    }
    None
}

fn looks_like_tar_header(bytes: &[u8]) -> bool {
    if bytes.len() < TAR_MAGIC_END {
        return false;
    }
    let magic = &bytes[TAR_MAGIC_OFFSET..TAR_MAGIC_END];
    magic == b"ustar\0" || magic == b"ustar "
}

fn detect_gzip_tar_format_path(path: &Path) -> Result<Option<ArchiveFormat>> {
    let file = StdFile::open(path)?;
    Ok(if gzip_stream_contains_tar(file)? {
        Some(ArchiveFormat::TarGz)
    } else {
        None
    })
}

fn gzip_stream_contains_tar<R>(reader: R) -> Result<bool>
where
    R: Read,
{
    let mut decoder = MultiGzDecoder::new(reader);
    let mut buffer = [0u8; TAR_HEADER_SIZE];
    let size = decoder.read(&mut buffer)?;
    Ok(looks_like_tar_header(&buffer[..size]))
}

fn zip_datetime_to_timestamp(dt: &ZipDateTime) -> u64 {
    match dt.as_chrono() {
        LocalResult::Single(value) => value.timestamp_millis().max(0) as u64,
        _ => 0,
    }
}

fn seven_zip_datetime_to_timestamp(value: sevenz_rust2::NtTime) -> u64 {
    let system_time: SystemTime = value.into();
    match system_time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as u64,
        Err(_) => 0,
    }
}
