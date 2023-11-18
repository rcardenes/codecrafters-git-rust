use std::{fs, io::{BufReader, BufRead, Read, Write, self}, path::PathBuf, ffi::OsStr, os::unix::ffi::OsStrExt};

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use flate2::read::ZlibDecoder;
use git_starter_rust::{GIT_DIR, ObjectManipulator, get_object_path, ensure_dir};

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
    /// Compute object ID and optionally creates a blob from a file
    HashObject {
        #[arg(short)]
        w: bool,
        object: Vec<String>,
    },
    /// List the contents of a tree object
    LsTree {
        #[arg(long, required = true)]
        name_only: bool,
        tree_ish: String,
    },
    /// Create a tree object from the current index
    WriteTree,
}

fn initialize_git_directory() -> Result<()> {
    let mut path = PathBuf::from(&GIT_DIR);
    fs::create_dir(&path)?;
    for subdir in ["objects", "refs"] {
        let _ = ensure_dir(path.clone(), subdir)?;
    }
    path.push("HEAD");
    fs::write(path, "ref: refs/heads/master\n")?;

    Ok(())
}

fn cat_file<W: Write>(object: String, mut output: W) -> Result<()> {
    let file = fs::File::open(get_object_path(&object)?)?;
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

fn hash_object<W: Write>(objects: Vec<String>, persist: bool, mut output: W) -> Result<()> {
    // TODO: This can be done in a much more efficient way, writing to both the hasher
    // and the destination file (if persisting) at the same time. Leave it for
    // later.
    for object in objects {
        let hash = if persist {
            ObjectManipulator::write_blob(&PathBuf::from(object))?
        } else {
            ObjectManipulator::hash_blob(&object)?
        };
        writeln!(output, "{}", hash)?;
    }

    Ok(())
}

const SHA1_LENGTH: usize = 20;

fn ls_tree<W: Write>(object: String, mut output: W) -> Result<()> {
    let file = fs::File::open(get_object_path(&object)?)?;
    let decoder = ZlibDecoder::new(file);
    let mut reader = BufReader::new(decoder);
    let mut header = vec![];
    reader.read_until(0, &mut header)?;
    let header = String::from_utf8(header)?;

    if header.starts_with("tree ") {
        let (_, raw_length) = header.split_at(5);
        let mut left = raw_length[..(raw_length.len() - 1)].parse::<usize>()?;

        let mut hash_buf = vec![0; SHA1_LENGTH];
        while left > 0 {
            let mut path_header_raw = vec![];
            let header_bytes = reader.read_until(0, &mut path_header_raw)?;
            reader.read_exact(&mut hash_buf)?;
            if let Some((_, vec)) = String::from_utf8(path_header_raw)?.split_once(" ") {
                left -= header_bytes + SHA1_LENGTH;
                writeln!(output, "{}", &vec[..vec.len() - 1])?;
            } else {
                bail!("corrupt stuff!")
            }
        } 
        output.flush()?;
    }

    Ok(())
}

fn write_tree<W: Write>(mut output: W) -> Result<()> {
    let this_dir = PathBuf::from(".");
    let hash = ObjectManipulator::write_tree(&this_dir, |p| {
        let git_dir = OsStr::from_bytes(GIT_DIR.as_bytes());
        p.file_name() != Some(&git_dir)
        })?;
    writeln!(output, "{}", hash)?;

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
        Commands::LsTree { tree_ish, .. } => {
            ls_tree(tree_ish, io::stdout())?;
        }
        Commands::WriteTree => {
            write_tree(io::stdout())?;
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
