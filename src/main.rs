use std::{fs, path::PathBuf, io::{BufReader, BufRead, Read, Write, self, BufWriter, Seek}};

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use flate2::{
    read::ZlibDecoder,
    write::ZlibEncoder, Compression,
};
use sha1::{Sha1, Digest};

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
    /// 
    HashObject {
        #[arg(short)]
        w: bool,
        object: Vec<String>,
    }
}

fn initialize_git_directory() -> Result<()> {
    fs::create_dir(".git")?;
    fs::create_dir(".git/objects")?;
    fs::create_dir(".git/refs")?;
    fs::write(".git/HEAD", "ref: refs/heads/master\n")?;

    Ok(())
}

fn find_git_dir() -> Result<PathBuf> {
    // At the moment this function just returns the .git in the current working
    // directory, if any. Not making it recursive to avoid screwing with real git
    // projects until things are stable
    let path = PathBuf::from(".git");
    if !path.is_dir() {
        bail!("fatal: not a git repository (or any of the parent directories): .git");
    }
    Ok(path)
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
    let mut dir = find_git_dir()?;
    dir.push("objects");
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
    let file = fs::File::open(path)?;
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

fn ensure_dir(mut path: PathBuf, subdir: &str) -> Result<PathBuf> {
    path.push(subdir);
    if !path.is_dir() {
        fs::create_dir_all(&path)?;
    }

    Ok(path)
}

fn hash_object<W: Write>(objects: Vec<String>, persist: bool, mut output: W) -> Result<()> {
    let objects_dir = ensure_dir(find_git_dir()?, "objects")?;
    for object in objects {
        let mut hasher = Sha1::new();
        let path = PathBuf::from(object);
        let mut file = fs::File::open(&path)?;
        let size = file.metadata()?.len();
        let header = format!("blob {}\0", size);
        hasher.update(&header);
        if size != io::copy(&mut file, &mut hasher)? {
            bail!("Disparity between the file size and the number of copied bytes");
        }
        let hash = format!("{:x}", hasher.finalize());
        writeln!(output, "{}", hash)?;

        if persist {
            let mut write_path = ensure_dir(objects_dir.clone(), &hash[..2])?;
            write_path.push(PathBuf::from(&hash[2..]));
            let dfile = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(write_path)?;
            let writer = BufWriter::new(dfile);
            let mut encoder = ZlibEncoder::new(writer, Compression::new(9));
            encoder.write(header.as_ref())?;
            file.rewind()?;
            io::copy(&mut file, &mut encoder)?;
        }
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
        Commands::HashObject { w, object } => {
            hash_object(object, w, io::stdout())?;
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
