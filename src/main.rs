use std::{fs, path::PathBuf, io::{BufReader, BufRead, Read, Write, self}};

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use flate2::read::ZlibDecoder;

const BUF_SIZE: usize = 4096;

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

fn cat_file<W: Write>(object: String, mut output: W) -> Result<()> {
    let object = object.to_lowercase();

    if !valid_partial_sha1_name(&object) {
        bail!("Not a valid object name {}", object);
    }
    let (prefix, rest) = &object.split_at(2);
    let mut dir = PathBuf::from(".git/objects");
    dir.push(prefix);
    let mut candidates = fs::read_dir(dir)?
        .into_iter()
        .filter(|e| e.as_ref().is_ok_and(|e| {
            e.path().file_name().unwrap().to_string_lossy().starts_with(rest)
        }))
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        bail!("Not a valid object name {}", object);
    }

    let path = candidates.pop().unwrap()?.path();
    let file = fs::OpenOptions::new()
        .read(true)
        .open(path)?;
    let decoder = ZlibDecoder::new(file);
    let mut reader = BufReader::new(decoder);
    let mut header = vec![];
    reader.read_until(0, &mut header)?;
    let header = String::from_utf8(header)?;
    if header.starts_with("blob ") {
        let (_, raw_length) = header.split_at(5);
        let mut left = raw_length[..(raw_length.len() - 1)].parse::<usize>()?;
        let mut buf = vec![0; BUF_SIZE];
        while left > 0 {
            let to_read = std::cmp::min(BUF_SIZE, left);
            reader.read_exact(&mut buf[..to_read])?;
            left -= to_read;
            output.write(&buf[..to_read])?;
        } 
        output.flush()?;
    }

    Ok(())
}

fn do_command(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init => {
            initialize_git_directory()?;
        },
        Commands::CatFile { object, .. } => {
            cat_file(object, io::stdout())?;
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
