use std::env;
use std::fs;

use anyhow::{bail, Result};

fn initialize_git_directory() -> Result<()> {
    fs::create_dir(".git")?;
    fs::create_dir(".git/objects")?;
    fs::create_dir(".git/refs")?;
    fs::write(".git/HEAD", "ref: refs/heads/master\n")?;

    Ok(())
}

fn do_command() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args[1] == "init" {
        initialize_git_directory()?;
        println!("Initialized git directory")
    } else {
        bail!("unknown command: {}", args[1]);
    }

    Ok(())
}

fn main() {
    if let Err(error) = do_command() {
        eprintln!("{error}")
    }
}