mod dist;

use std::process::Command as ProcessCommand;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask", about = "Build and packaging tasks for Hapi Player")]
struct Cli {
    #[command(subcommand)]
    command: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Build the application in release mode
    Build {
        /// Rust target triple (defaults to host)
        #[arg(long)]
        target: Option<String>,
    },
    /// Build and package a release artifact for the given target
    Dist {
        /// Rust target triple (defaults to host)
        #[arg(long)]
        target: Option<String>,
        /// Display name used inside the packaged app
        #[arg(long, default_value = "Hapi_Player")]
        app_name: String,
        /// Output artifact basename (defaults from target triple)
        #[arg(long)]
        artifact_name: Option<String>,
        /// Skip `cargo build --release`
        #[arg(long)]
        no_build: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = workspace_root()?;

    match cli.command {
        Task::Build { target } => {
            cargo_build(&root, target.as_deref())?;
        }
        Task::Dist {
            target,
            app_name,
            artifact_name,
            no_build,
        } => {
            let target = target.unwrap_or_else(host_target);
            if !no_build {
                cargo_build(&root, Some(&target))?;
            }

            let artifact_name =
                artifact_name.unwrap_or_else(|| dist::default_artifact_name(&target).to_string());
            let archive = dist::package(&root, &target, &app_name, &artifact_name)?;
            println!("Created {}", archive.display());
        }
    }

    Ok(())
}

fn workspace_root() -> Result<std::path::PathBuf> {
    let output = ProcessCommand::new(env!("CARGO"))
        .args(["locate-project", "--workspace", "--message-format=plain"])
        .output()
        .context("failed to locate workspace root")?;

    if !output.status.success() {
        bail!("cargo locate-project failed");
    }

    let manifest = String::from_utf8(output.stdout).context("invalid cargo output")?;
    let manifest = manifest.trim();
    Ok(std::path::Path::new(manifest)
        .parent()
        .context("workspace root has no parent")?
        .to_path_buf())
}

fn host_target() -> String {
    let output = ProcessCommand::new("rustc")
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

fn cargo_build(root: &std::path::Path, target: Option<&str>) -> Result<()> {
    let mut cmd = ProcessCommand::new("cargo");
    cmd.current_dir(root);
    cmd.args(["build", "--release", "--bin", "lyrics_follow_player"]);

    if let Some(target) = target {
        cmd.args(["--target", target]);
    }

    let status = cmd.status().context("failed to run cargo build")?;
    if !status.success() {
        bail!("cargo build failed");
    }

    Ok(())
}
