use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use tar::Builder;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const BIN_NAME: &str = "lyrics_follow_player";

pub fn default_artifact_name(target: &str) -> &'static str {
    match target {
        "x86_64-unknown-linux-gnu" => "hapi-player-linux-x86_64",
        "aarch64-apple-darwin" => "hapi-player-macos-aarch64",
        "x86_64-apple-darwin" => "hapi-player-macos-x86_64",
        "x86_64-pc-windows-msvc" => "hapi-player-windows-x86_64",
        _ => "hapi-player",
    }
}

pub fn package(root: &Path, target: &str, app_name: &str, artifact_name: &str) -> Result<PathBuf> {
    let binary = resolve_binary(root, target)?;
    let dist_dir = root.join("dist");
    let staging_dir = dist_dir.join(artifact_name);

    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir).with_context(|| {
            format!(
                "failed to remove staging directory {}",
                staging_dir.display()
            )
        })?;
    }
    fs::create_dir_all(&dist_dir)?;

    let archive_path = if target.contains("pc-windows") {
        package_windows(&binary, &staging_dir, &dist_dir, app_name, artifact_name)?
    } else if target.contains("apple-darwin") {
        package_macos(
            root,
            &binary,
            &staging_dir,
            &dist_dir,
            app_name,
            artifact_name,
        )?
    } else {
        package_linux(
            root,
            &binary,
            &staging_dir,
            &dist_dir,
            app_name,
            artifact_name,
        )?
    };

    Ok(archive_path)
}

fn resolve_binary(root: &Path, target: &str) -> Result<PathBuf> {
    let file_name = if target.contains("pc-windows") {
        format!("{BIN_NAME}.exe")
    } else {
        BIN_NAME.to_string()
    };

    let cross_path = root
        .join("target")
        .join(target)
        .join("release")
        .join(&file_name);
    if cross_path.is_file() {
        return Ok(cross_path);
    }

    let host = host_target();
    let host_path = root.join("target").join("release").join(&file_name);
    if target == host && host_path.is_file() {
        return Ok(host_path);
    }

    bail!("release binary not found: {}", cross_path.display())
}

fn package_macos(
    root: &Path,
    binary: &Path,
    staging_dir: &Path,
    dist_dir: &Path,
    app_name: &str,
    artifact_name: &str,
) -> Result<PathBuf> {
    let app_bundle = staging_dir.join(format!("{app_name}.app"));
    let macos_dir = app_bundle.join("Contents/MacOS");
    let resources_dir = app_bundle.join("Contents/Resources");

    fs::create_dir_all(&macos_dir)?;
    fs::create_dir_all(&resources_dir)?;

    let dest_binary = macos_dir.join(app_name);
    copy_executable(binary, &dest_binary)?;

    fs::copy(
        root.join("packaging/macos/Info.plist"),
        app_bundle.join("Contents/Info.plist"),
    )?;

    let archive_path = dist_dir.join(format!("{artifact_name}.tar.gz"));
    let file = File::create(&archive_path)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = Builder::new(encoder);
    tar.append_dir_all(format!("{app_name}.app"), &app_bundle)?;
    let encoder = tar.into_inner()?;
    encoder.finish()?;

    Ok(archive_path)
}

fn package_windows(
    binary: &Path,
    staging_dir: &Path,
    dist_dir: &Path,
    app_name: &str,
    artifact_name: &str,
) -> Result<PathBuf> {
    fs::create_dir_all(staging_dir)?;
    let dest_binary = staging_dir.join(format!("{app_name}.exe"));
    fs::copy(binary, &dest_binary)?;

    let archive_path = dist_dir.join(format!("{artifact_name}.zip"));
    let file = File::create(&archive_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    add_dir_to_zip(&mut zip, staging_dir, staging_dir, options)?;
    zip.finish()?;

    Ok(archive_path)
}

fn package_linux(
    root: &Path,
    binary: &Path,
    staging_dir: &Path,
    dist_dir: &Path,
    app_name: &str,
    artifact_name: &str,
) -> Result<PathBuf> {
    let bin_dir = staging_dir.join("bin");
    fs::create_dir_all(&bin_dir)?;

    let dest_binary = bin_dir.join(app_name);
    copy_executable(binary, &dest_binary)?;

    fs::copy(
        root.join("packaging/linux/hapi-player.desktop"),
        staging_dir.join(format!("{app_name}.desktop")),
    )?;

    let archive_path = dist_dir.join(format!("{artifact_name}.tar.gz"));
    let file = File::create(&archive_path)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = Builder::new(encoder);
    tar.append_dir_all(".", staging_dir)?;
    let encoder = tar.into_inner()?;
    encoder.finish()?;

    Ok(archive_path)
}

fn copy_executable(from: &Path, to: &Path) -> Result<()> {
    fs::copy(from, to)
        .with_context(|| format!("failed to copy {} to {}", from.display(), to.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(to)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(to, perms)?;
    }

    Ok(())
}

fn add_dir_to_zip(
    zip: &mut ZipWriter<File>,
    base: &Path,
    path: &Path,
    options: SimpleFileOptions,
) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let name = entry_path
            .strip_prefix(base)
            .context("failed to strip archive base path")?
            .to_string_lossy()
            .replace('\\', "/");

        if entry_path.is_dir() {
            zip.add_directory(format!("{name}/"), options)?;
            add_dir_to_zip(zip, base, &entry_path, options)?;
        } else {
            zip.start_file(name, options)?;
            let mut file = File::open(entry_path)?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)?;
        }
    }

    Ok(())
}

fn host_target() -> String {
    let output = Command::new("rustc")
        .args(["-vV"])
        .output()
        .expect("failed to run rustc");

    String::from_utf8(output.stdout)
        .expect("invalid rustc output")
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("host triple not found in rustc output")
        .to_string()
}
