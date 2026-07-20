//! Archive extraction utilities for tar.gz and zip files.
//!
//! Pure Rust implementation - no C dependencies.

use crate::error::{Result, SchalentierError};
use anyhow::Context;
use flate2::read::GzDecoder;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use tar::Archive;
use tracing::{debug, info};

/// Supported archive formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    TarGz,
    Zip,
}

impl ArchiveFormat {
    /// Detect archive format from file extension
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?.to_lowercase();

        if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            Some(ArchiveFormat::TarGz)
        } else if name.ends_with(".zip") {
            Some(ArchiveFormat::Zip)
        } else {
            None
        }
    }
}

/// Extract an archive to a destination directory
///
/// Automatically detects the archive format from the file extension.
/// Returns the list of extracted file paths.
pub fn extract(archive_path: &Path, dest_dir: &Path) -> Result<Vec<PathBuf>> {
    let format =
        ArchiveFormat::from_path(archive_path).ok_or_else(|| SchalentierError::InstallFailed {
            package: archive_path.display().to_string(),
            reason: "Unsupported archive format. Only .tar.gz and .zip are supported.".to_string(),
        })?;

    info!(
        "Extracting {:?} archive: {} -> {}",
        format,
        archive_path.display(),
        dest_dir.display()
    );

    // Ensure destination exists
    std::fs::create_dir_all(dest_dir)
        .with_context(|| format!("Failed to create directory: {}", dest_dir.display()))?;

    match format {
        ArchiveFormat::TarGz => extract_tar_gz(archive_path, dest_dir),
        ArchiveFormat::Zip => extract_zip(archive_path, dest_dir),
    }
}

/// Extract a .tar.gz archive
fn extract_tar_gz(archive_path: &Path, dest_dir: &Path) -> Result<Vec<PathBuf>> {
    let file = File::open(archive_path)
        .with_context(|| format!("Failed to open archive: {}", archive_path.display()))?;

    let decoder = GzDecoder::new(BufReader::new(file));
    let mut archive = Archive::new(decoder);

    extract_tar_archive(&mut archive, dest_dir)
}

/// Helper to extract tar archive entries
fn extract_tar_archive<R: Read>(archive: &mut Archive<R>, dest_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut extracted_files = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        // Security: prevent path traversal attacks
        let dest_path = dest_dir.join(&path);
        if !dest_path.starts_with(dest_dir) {
            debug!("Skipping path with traversal attempt: {}", path.display());
            continue;
        }

        debug!("Extracting: {}", path.display());

        // Create parent directories
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        entry.unpack(&dest_path)?;

        // Set executable permission for files that had it
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mode) = entry.header().mode() {
                if mode & 0o111 != 0 {
                    // Has execute bit
                    let current = std::fs::metadata(&dest_path)?.permissions();
                    let new_mode = current.mode() | 0o111;
                    std::fs::set_permissions(
                        &dest_path,
                        std::fs::Permissions::from_mode(new_mode),
                    )?;
                }
            }
        }

        extracted_files.push(dest_path);
    }

    info!("Extracted {} files", extracted_files.len());
    Ok(extracted_files)
}

/// Extract a .zip archive
fn extract_zip(archive_path: &Path, dest_dir: &Path) -> Result<Vec<PathBuf>> {
    let file = File::open(archive_path)
        .with_context(|| format!("Failed to open archive: {}", archive_path.display()))?;

    let mut archive =
        zip::ZipArchive::new(BufReader::new(file)).with_context(|| "Failed to read zip archive")?;

    let mut extracted_files = Vec::new();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => dest_dir.join(path),
            None => continue,
        };

        // Security: prevent path traversal
        if !outpath.starts_with(dest_dir) {
            debug!(
                "Skipping path with traversal attempt: {}",
                outpath.display()
            );
            continue;
        }

        debug!("Extracting: {}", outpath.display());

        if file.name().ends_with('/') {
            // Directory
            std::fs::create_dir_all(&outpath)?;
        } else {
            // File
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let mut outfile = File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;

            // Set executable permission
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    std::fs::set_permissions(&outpath, std::fs::Permissions::from_mode(mode))?;
                }
            }

            extracted_files.push(outpath);
        }
    }

    info!("Extracted {} files", extracted_files.len());
    Ok(extracted_files)
}

/// Find a binary in the extracted files
///
/// Searches for an executable file matching the given name (with or without extension)
pub fn find_binary(extracted_files: &[PathBuf], name: &str) -> Option<PathBuf> {
    let name_lower = name.to_lowercase();
    let name_exe = format!("{}.exe", name_lower);

    // First, look for exact match
    for path in extracted_files {
        if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
            let filename_lower = filename.to_lowercase();
            if filename_lower == name_lower || filename_lower == name_exe {
                return Some(path.clone());
            }
        }
    }

    // Then look for partial match (e.g., "ripgrep" in "ripgrep-14.1.0-x86_64...")
    for path in extracted_files {
        if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
            let filename_lower = filename.to_lowercase();

            // Match "name-version" style binaries without extension
            if filename_lower.starts_with(&format!("{}-", name_lower))
                && !filename_lower.contains('.')
            {
                return Some(path.clone());
            }

            // Match "name-version.exe" style binaries
            if filename_lower.starts_with(&format!("{}-", name_lower))
                && filename_lower.ends_with(".exe")
            {
                return Some(path.clone());
            }
        }
    }

    None
}

/// Find any executable files in the extracted files
pub fn find_executables(extracted_files: &[PathBuf]) -> Vec<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    extracted_files
        .iter()
        .filter(|path| {
            if let Ok(metadata) = std::fs::metadata(path) {
                if metadata.is_file() {
                    let mode = metadata.permissions().mode();
                    return mode & 0o111 != 0; // Has execute bit
                }
            }
            false
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_archive_format_detection() {
        assert_eq!(
            ArchiveFormat::from_path(Path::new("foo.tar.gz")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::from_path(Path::new("foo.tgz")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::from_path(Path::new("foo.zip")),
            Some(ArchiveFormat::Zip)
        );
        // bz2 and xz are no longer supported
        assert_eq!(ArchiveFormat::from_path(Path::new("foo.tar.bz2")), None);
        assert_eq!(ArchiveFormat::from_path(Path::new("foo.tar.xz")), None);
        assert_eq!(ArchiveFormat::from_path(Path::new("foo.txt")), None);
    }

    #[test]
    fn test_extract_tar_gz() {
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test.tar.gz");
        let extract_dir = temp_dir.path().join("extracted");

        // Create a simple tar.gz archive
        let file = File::create(&archive_path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);

        // Add a file to the archive
        let content = b"Hello, World!";
        let mut header = tar::Header::new_gnu();
        header.set_path("test.txt").unwrap();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, &content[..]).unwrap();

        // Properly finish the builder and encoder
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();

        // Extract
        let extracted = extract(&archive_path, &extract_dir).unwrap();
        assert!(!extracted.is_empty());

        // Verify content
        let extracted_file = extract_dir.join("test.txt");
        assert!(extracted_file.exists());
        let content = std::fs::read_to_string(&extracted_file).unwrap();
        assert_eq!(content, "Hello, World!");
    }

    #[test]
    fn test_extract_zip() {
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test.zip");
        let extract_dir = temp_dir.path().join("extracted");

        // Create a simple zip archive
        let file = File::create(&archive_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("test.txt", options).unwrap();
        zip.write_all(b"Hello from zip!").unwrap();
        zip.finish().unwrap();

        // Extract
        let extracted = extract(&archive_path, &extract_dir).unwrap();
        assert!(!extracted.is_empty());

        // Verify content
        let extracted_file = extract_dir.join("test.txt");
        assert!(extracted_file.exists());
        let content = std::fs::read_to_string(&extracted_file).unwrap();
        assert_eq!(content, "Hello from zip!");
    }

    #[test]
    fn test_find_binary() {
        let files = vec![
            PathBuf::from("/tmp/extract/ripgrep-14.1.0/rg"),
            PathBuf::from("/tmp/extract/ripgrep-14.1.0/README.md"),
            PathBuf::from("/tmp/extract/ripgrep-14.1.0/LICENSE"),
        ];

        let found = find_binary(&files, "rg");
        assert_eq!(found, Some(PathBuf::from("/tmp/extract/ripgrep-14.1.0/rg")));

        let not_found = find_binary(&files, "nonexistent");
        assert_eq!(not_found, None);
    }

    #[test]
    fn test_find_binary_with_exe_extension() {
        // Use forward slashes which work on all platforms
        let files = vec![
            PathBuf::from("/extract/tool/tool.exe"),
            PathBuf::from("/extract/tool/README.md"),
        ];

        let found = find_binary(&files, "tool");
        assert_eq!(found, Some(PathBuf::from("/extract/tool/tool.exe")));
    }
}
