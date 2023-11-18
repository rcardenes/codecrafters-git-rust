use anyhow::{Result, bail};
use flate2::{write::ZlibEncoder, Compression};
use tempfile::NamedTempFile;
use std::{
    fs,
    path::{PathBuf, Path},
    io::{self, Write}, os::unix::fs::PermissionsExt,
};
use sha1::{Sha1, Digest};

pub const GIT_DIR: &str = ".git";

pub struct ObjectWriter {
    hasher: Sha1,
    temp_path: Box<Path>,
    file: ZlibEncoder<NamedTempFile>,
    renamed: bool,
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
            renamed: false,
        })
    }

    pub fn finalize(mut self) -> Result<String> {
        let hash = format!("{:x}", self.hasher.finalize_reset());
        let mut path = find_git_dir()?;
        path.push("objects");
        let mut new_path = ensure_dir(path, &hash[..2])?;
        new_path.push(&hash[2..]);
        fs::rename(&self.temp_path, new_path)?;
        self.renamed = true;

        Ok(hash)
    }
}

impl Drop for ObjectWriter {
    fn drop(&mut self) {
        if !self.renamed {
            eprintln!("Removing non-renamed temporary file");
            if let Err(error) = fs::remove_file(&self.temp_path) {
                eprintln!("Trying to remove temporary file {}: {}",
                          self.temp_path.file_name().unwrap().to_string_lossy(),
                          error);
            }
        }
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
//    object_root: PathBuf,
}

struct TreeEntry {
    mode: String,
    name: String,
    hash: String,
}

impl TreeEntry {
    fn len_as_bytes(&self) -> usize {
        // The format consists on a number digits describing the mode, followed by a space,
        // then a zero-terminated name string, and 20 bytes with the SHA1 checksum of
        // the object. Hence 20 + 1 + 1 + name.len() + mode.len
        self.name.len() + self.mode.len() + 22
    }

    fn as_bytes(&self) -> Vec<u8> {
        let hash_u8 = self.hash.chars()
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|tbyte| {
                let st: String = tbyte.into_iter().collect();
                u8::from_str_radix(&st, 16).unwrap()
            })
            .collect::<Vec<_>>();
        vec![
            self.mode.as_bytes(),
            &[32],
            self.name.as_bytes(),
            &[0],
            &hash_u8,
        ].concat()
    }
}

impl ObjectManipulator {
//    pub fn new() -> Result<Self> {
//        Ok(Self::new_at(find_git_dir()?))
//    }
//
//    pub fn new_at(path: PathBuf) -> Self {
//        let mut path = path;
//        path.push("objects");
//
//        ObjectManipulator {
//            object_root: path,
//        }
//    }

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

    pub fn write_blob(object: &Path) -> Result<String> {
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

    pub fn write_tree(path: &Path, filter: fn (&Path) -> bool) -> Result<String> {
        let mut writer = ObjectWriter::new()?;

        let mut contents = vec![];
        
        let mut it = fs::read_dir(&path)?;
        while let Some(Ok(entry)) = it.next() {
            let entry_path = entry.path();
            if !filter(&entry_path) {
                continue
            }

            let hash = if entry_path.is_dir() {
                Self::write_tree(&entry_path, filter)?
            } else {
                Self::write_blob(&entry_path)?
            };
            let name = entry_path.file_name().unwrap().to_string_lossy().into();
            let meta = entry_path.metadata()?;
            let raw_mode = meta.permissions().mode();
            let mode = String::from(
                if meta.file_type().is_dir() { "40000" }
                else if meta.file_type().is_symlink() { "120000" }
                else if (raw_mode & 0o111) != 0 { "100755" }
                else { "100644" }
            );
            contents.push(TreeEntry { mode, name, hash });
        }
        contents.sort_by_key(|te| te.name.clone());
        write!(writer, "tree {}\0", contents.iter().map(|te| te.len_as_bytes()).sum::<usize>())?;
        for entry in contents {
            writer.write(&entry.as_bytes())?;
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

pub fn ensure_dir(mut path: PathBuf, subdir: &str) -> Result<PathBuf> {
    path.push(subdir);
    if !path.is_dir() {
        fs::create_dir_all(&path)?;
    }

    Ok(path)
}
