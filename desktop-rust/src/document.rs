use std::fs::File;
use std::path::{Path, PathBuf};

use memmap2::{Mmap, MmapOptions};

pub struct BinaryDocument {
    path: PathBuf,
    file_name: String,
    mmap: Mmap,
}

impl BinaryDocument {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path).map_err(|error| format!("Failed to open file: {error}"))?;
        let mmap = unsafe { MmapOptions::new().map(&file) }
            .map_err(|error| format!("Failed to memory-map file: {error}"))?;
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("unnamed")
            .to_owned();

        Ok(Self {
            path,
            file_name,
            mmap,
        })
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len_bytes(&self) -> usize {
        self.mmap.len()
    }

    pub fn len_bits(&self) -> usize {
        self.len_bytes().saturating_mul(8)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.mmap
    }
}
