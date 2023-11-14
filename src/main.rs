use std::{fs, path::PathBuf};

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create an empty Git repository or reinitialize an existing one
    Init,
    /// Provide content or type and size information for repository objects
    CatFile {
        /// pretty-print object's content
        #[arg(short, required=true)]
        p: bool,
        object: String,
    },
}

fn initialize_git_directory() -> Result<()> {
    fs::create_dir(".git")?;
    fs::create_dir(".git/objects")?;
    fs::create_dir(".git/refs")?;
    fs::write(".git/HEAD", "ref: refs/heads/master\n")?;

    Ok(())
}

fn valid_partial_sha1_name(name: &str) -> bool {
    // SHA1 hashes are 160 bits or 20 bytes. This translates to 40 hex characters.
    // For partial unique names we need a minimum of 4 characters (2 bytes)
    name.len() > 3 &&
        name.len() < 41 &&
        name.chars().all(|c| ('0'..='9').contains(&c) || ('a'..='z').contains(&c))
}

fn cat_file(object: String) -> Result<()> {
    let object = object.to_lowercase();

    if !valid_partial_sha1_name(&object) {
        bail!("Not a valid object name {}", object);
    }

    Ok(())
}

fn do_command(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init => {
            initialize_git_directory()?;
        },
        Commands::CatFile { object, .. } => {
            cat_file(object)?;
        }
    }

    Ok(())
}

fn main() {
    let cli = Cli::parse();
    if let Err(error) = do_command(cli) {
        eprintln!("{error}")
    }
}
