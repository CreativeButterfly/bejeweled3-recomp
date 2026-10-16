//! Reader for the PopCap-style PAK container.
//!
//! Layout (every byte of the file is XORed with 0xF7):
//!
//! ```text
//! u32  magic    = 0xBAC04AC0
//! u32  version  = 0
//! repeat:
//!     u8   flags        (0x80 = end of index)
//!     u8   name_len
//!     [u8] name         (backslash-separated path)
//!     u32  size
//!     u64  filetime     (Windows FILETIME)
//! then the file data, concatenated in index order
//! ```
//!
//! The reader only parses the index up front. Entry bytes are read from disk
//! on demand, so nothing from the game is ever copied anywhere else.

use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;

const XOR_KEY: u8 = 0xF7;
const MAGIC: u32 = 0xBAC0_4AC0;
const VERSION: u32 = 0;
const FLAG_END: u8 = 0x80;

#[derive(Debug)]
pub enum PakError {
    Io(io::Error),
    BadMagic(u32),
    BadVersion(u32),
    /// The index claims more data than the file actually contains.
    Truncated { expected_end: u64, file_len: u64 },
    NotFound(String),
    LockPoisoned,
}

impl fmt::Display for PakError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PakError::Io(e) => write!(f, "I/O error: {e}"),
            PakError::BadMagic(m) => write!(f, "not a PAK file (magic {m:#010x})"),
            PakError::BadVersion(v) => write!(f, "unsupported PAK version {v}"),
            PakError::Truncated { expected_end, file_len } => write!(
                f,
                "PAK is truncated: index implies {expected_end} bytes, file has {file_len}"
            ),
            PakError::NotFound(n) => write!(f, "entry not found: {n}"),
            PakError::LockPoisoned => write!(f, "internal lock poisoned"),
        }
    }
}

impl std::error::Error for PakError {}

impl From<io::Error> for PakError {
    fn from(e: io::Error) -> Self {
        PakError::Io(e)
    }
}

/// One file inside the PAK.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Name exactly as stored (backslash-separated).
    pub name: String,
    pub size: u32,
    /// Absolute byte offset of the entry's data within the PAK file.
    pub offset: u64,
    /// Windows FILETIME (100 ns ticks since 1601-01-01).
    pub filetime: u64,
}

/// Lowercase and convert backslashes to forward slashes, so lookups work
/// regardless of how a path is spelled.
pub fn normalize(name: &str) -> String {
    name.chars()
        .map(|c| if c == '\\' { '/' } else { c.to_ascii_lowercase() })
        .collect()
}

pub struct Pak {
    file: Mutex<File>,
    entries: Vec<Entry>,
    lookup: HashMap<String, usize>,
    data_start: u64,
    file_len: u64,
}

/// Wraps a reader and undoes the XOR as bytes pass through.
struct XorReader<R: Read> {
    inner: R,
}

impl<R: Read> Read for XorReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        for b in &mut buf[..n] {
            *b ^= XOR_KEY;
        }
        Ok(n)
    }
}

fn read_u8<R: Read>(r: &mut R) -> io::Result<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn read_u32<R: Read>(r: &mut R) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64<R: Read>(r: &mut R) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

impl Pak {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Pak, PakError> {
        let file = File::open(path)?;
        let file_len = file.metadata()?.len();
        let mut r = XorReader {
            inner: BufReader::new(file),
        };

        let magic = read_u32(&mut r)?;
        if magic != MAGIC {
            return Err(PakError::BadMagic(magic));
        }
        let version = read_u32(&mut r)?;
        if version != VERSION {
            return Err(PakError::BadVersion(version));
        }

        // Bytes consumed so far: magic + version.
        let mut pos: u64 = 8;
        let mut entries: Vec<Entry> = Vec::new();

        loop {
            let flags = read_u8(&mut r)?;
            pos += 1;
            if flags & FLAG_END != 0 {
                break;
            }
            let name_len = read_u8(&mut r)? as usize;
            pos += 1;
            let mut name_bytes = vec![0u8; name_len];
            r.read_exact(&mut name_bytes)?;
            pos += name_len as u64;
            // Names are plain ASCII in practice; map bytes 1:1 so nothing is lost.
            let name: String = name_bytes.iter().map(|&b| b as char).collect();
            let size = read_u32(&mut r)?;
            let filetime = read_u64(&mut r)?;
            pos += 12;
            entries.push(Entry {
                name,
                size,
                offset: 0, // filled in below once the data start is known
                filetime,
            });
        }

        // File data begins right after the end-of-index flag and is laid out
        // in index order.
        let data_start = pos;
        let mut offset = data_start;
        for e in &mut entries {
            e.offset = offset;
            offset += e.size as u64;
        }
        if offset > file_len {
            return Err(PakError::Truncated {
                expected_end: offset,
                file_len,
            });
        }

        let mut lookup = HashMap::with_capacity(entries.len());
        for (i, e) in entries.iter().enumerate() {
            // Keep the first entry if a name ever appears twice.
            lookup.entry(normalize(&e.name)).or_insert(i);
        }

        let file = r.inner.into_inner();
        Ok(Pak {
            file: Mutex::new(file),
            entries,
            lookup,
            data_start,
            file_len,
        })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Byte offset where the first entry's data starts.
    pub fn data_start(&self) -> u64 {
        self.data_start
    }

    pub fn file_len(&self) -> u64 {
        self.file_len
    }

    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.lookup
            .get(&normalize(name))
            .map(|&i| &self.entries[i])
    }

    /// Read one entry's bytes into memory.
    pub fn read(&self, name: &str) -> Result<Vec<u8>, PakError> {
        let entry = self
            .find(name)
            .ok_or_else(|| PakError::NotFound(name.to_string()))?;
        self.read_entry(entry)
    }

    pub fn read_entry(&self, entry: &Entry) -> Result<Vec<u8>, PakError> {
        let mut buf = vec![0u8; entry.size as usize];
        {
            let mut f = self.file.lock().map_err(|_| PakError::LockPoisoned)?;
            f.seek(SeekFrom::Start(entry.offset))?;
            f.read_exact(&mut buf)?;
        }
        for b in &mut buf {
            *b ^= XOR_KEY;
        }
        Ok(buf)
    }
}

/// Wraps a writer and applies the XOR as bytes pass through.
struct XorWriter<W: Write> {
    inner: W,
}

impl<W: Write> Write for XorWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let scrambled: Vec<u8> = buf.iter().map(|b| b ^ XOR_KEY).collect();
        self.inner.write_all(&scrambled)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn invalid(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, msg)
}

/// Write a PAK from (name, data) pairs, in the order given.
///
/// Names use forward or backward slashes and must be ASCII and at most 255
/// bytes. Stored filetimes are zero so builds are reproducible.
pub fn write_pak<W: Write>(out: W, files: &[(String, Vec<u8>)]) -> io::Result<()> {
    // Validate everything first so nothing is half-written on error.
    for (name, data) in files {
        if !name.is_ascii() {
            return Err(invalid(format!("name is not ASCII: {name}")));
        }
        if name.is_empty() || name.len() > 255 {
            return Err(invalid(format!("bad name length: {name}")));
        }
        if data.len() > u32::MAX as usize {
            return Err(invalid(format!("entry too large: {name}")));
        }
    }

    let mut w = XorWriter { inner: out };
    w.write_all(&MAGIC.to_le_bytes())?;
    w.write_all(&VERSION.to_le_bytes())?;
    for (name, data) in files {
        let stored: Vec<u8> = name
            .bytes()
            .map(|b| if b == b'/' { b'\\' } else { b })
            .collect();
        w.write_all(&[0x00, stored.len() as u8])?;
        w.write_all(&stored)?;
        w.write_all(&(data.len() as u32).to_le_bytes())?;
        w.write_all(&0u64.to_le_bytes())?;
    }
    w.write_all(&[FLAG_END])?;
    for (_, data) in files {
        w.write_all(data)?;
    }
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a tiny PAK in memory so the parser can be checked without any
    /// game data.
    fn build(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&MAGIC.to_le_bytes());
        out.extend_from_slice(&VERSION.to_le_bytes());
        for (name, data) in entries {
            out.push(0x00);
            out.push(name.len() as u8);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&0u64.to_le_bytes());
        }
        out.push(FLAG_END);
        for (_, data) in entries {
            out.extend_from_slice(data);
        }
        for b in &mut out {
            *b ^= XOR_KEY;
        }
        out
    }

    #[test]
    fn roundtrip() {
        let bytes = build(&[
            ("images\\a.png", b"hello"),
            ("sounds\\b.ogg", b"world!!"),
        ]);
        let path = std::env::temp_dir().join("geminator-pak_roundtrip_test.pak");
        std::fs::write(&path, bytes).unwrap();

        let pak = Pak::open(&path).unwrap();
        assert_eq!(pak.entries().len(), 2);
        assert_eq!(pak.read("images/A.PNG").unwrap(), b"hello");
        assert_eq!(pak.read("sounds\\b.ogg").unwrap(), b"world!!");
        assert!(matches!(pak.read("missing"), Err(PakError::NotFound(_))));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_then_read() {
        let files = vec![
            ("icons/new.png".to_string(), b"abc".to_vec()),
            ("music/empty.bin".to_string(), Vec::new()),
            ("fonts\\bomb.txt".to_string(), b"0123456789".to_vec()),
        ];
        let path = std::env::temp_dir().join("geminator-pak_write_test.pak");
        let file = File::create(&path).unwrap();
        write_pak(file, &files).unwrap();

        let pak = Pak::open(&path).unwrap();
        assert_eq!(pak.entries().len(), 3);
        // Names are stored with backslashes but looked up either way.
        assert_eq!(pak.entries()[0].name, "icons\\new.png");
        assert_eq!(pak.read("icons/new.png").unwrap(), b"abc");
        assert_eq!(pak.read("music/empty.bin").unwrap(), b"");
        assert_eq!(pak.read("fonts/bomb.txt").unwrap(), b"0123456789");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn writer_rejects_non_ascii_names() {
        let files = vec![("caf\u{e9}.png".to_string(), vec![1u8])];
        let mut sink = Vec::new();
        assert!(write_pak(&mut sink, &files).is_err());
    }

    #[test]
    fn rejects_non_pak() {
        let path = std::env::temp_dir().join("geminator-pak_bad_magic_test.pak");
        std::fs::write(&path, [0u8; 32]).unwrap();
        assert!(matches!(Pak::open(&path), Err(PakError::BadMagic(_))));
        let _ = std::fs::remove_file(&path);
    }
}
