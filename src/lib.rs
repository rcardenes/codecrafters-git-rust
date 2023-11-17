use anyhow::{Result, bail};
use flate2::{write::ZlibEncoder, Compression};
use tempfile::NamedTempFile;
use std::{
    fs,
    path::{PathBuf, Path},
    io::{self, Write},
};
use sha1::{Sha1, Digest};

pub struct ObjectWriter {
    hasher: Sha1,
    temp_path: Box<Path>,
    file: ZlibEncoder<NamedTempFile>,
}

impl ObjectWriter {
    pub fn new() -> Result<Self> {
        let mut path = find_git_dir()?;
        path.push("objects");
        let temp = NamedTempFile::new_in(path)?;
        Ok(ObjectWriter {
            hasher: Sha1::new(),
            temp_path: temp.path().into(),
            file: ZlibEncoder::new(temp, Compression::new(9)),
        })
    }

    pub fn finalize(self) -> Result<String> {
        let hash = format!("{:x}", self.hasher.finalize());
        let mut path = find_git_dir()?;
        path.push("objects");
        let mut new_path = ensure_dir(path, &hash[..2])?;
        new_path.push(&hash[2..]);
        fs::rename(self.temp_path, new_path)?;

        Ok(hash)
    }
}

impl Write for ObjectWriter {
    fn flush(&mut self) -> io::Result<()> {
       self.file.flush()
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.hasher.update(buf);
        self.file.write(buf)
    }
}

pub struct ObjectManipulator {
    object_root: PathBuf,
}

impl ObjectManipulator {
    pub fn new() -> Result<Self> {
        Ok(Self::new_at(find_git_dir()?))
    }

    pub fn new_at(path: PathBuf) -> Self {
        let mut path = path;
        path.push("objects");

        ObjectManipulator {
            object_root: path,
        }
    }

    pub fn hash_blob(path: &str) -> Result<String> {
        let mut hasher = Sha1::new();
        let mut file = fs::File::open(path)?;
        let size = file.metadata()?.len();
        let header = format!("blob {}\0", size);
        hasher.update(&header);

        if size != io::copy(&mut file, &mut hasher)? {
            bail!("Disparity between the file size and the number of copied bytes");
        }
        let hash = format!("{:x}", hasher.finalize_reset());

        Ok(hash)
    }

    pub fn write_object(&self, object: &str) -> Result<String> {
        // TODO: This can be done in a much more efficient way, writing to both the hasher
        // and the destination file (if persisting) at the same time. Leave it for
        // later.
        let path = PathBuf::from(object);
        let mut file = fs::File::open(&path)?;
        let mut writer = ObjectWriter::new()?;

        let size = file.metadata()?.len();
        write!(writer, "blob {}\0", size)?;

        if size != io::copy(&mut file, &mut writer)? {
            bail!("Disparity between the file size and the number of copied bytes");
        }

        writer.finalize()
    }

}

fn find_git_dir() -> Result<PathBuf> {
    // At the moment this function just returns the .git in the current working
    // directory, if any. Not making it recursive to avoid screwing with real git
    // projects until things are stable
    let path = PathBuf::from(".git");
    let objects = PathBuf::from(".git/objects");
    let refs = PathBuf::from(".git/refs");
    if !path.is_dir() || !objects.is_dir() || !refs.is_dir() {
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

pub fn get_object_path(object: &str) -> Result<PathBuf> {
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

    Ok(candidates.pop().unwrap()?.path())
}

fn ensure_dir(mut path: PathBuf, subdir: &str) -> Result<PathBuf> {
    path.push(subdir);
    if !path.is_dir() {
        fs::create_dir_all(&path)?;
    }

    Ok(path)
}

fn compose_dir(subdir: &str) -> Result<PathBuf> {
    let mut path = find_git_dir()?;
    path.push(subdir);
    if !path.is_dir() {
        bail!("fatal: expected to find subdirectory {}", subdir)
    }
    Ok(path)
}
