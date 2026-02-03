//! Minimal CPIO "newc" parser (read-only).
//!
//! - Supports the "newc" ASCII-hex header format (magic: "070701").
//! - Iterates entries, returning name, mode, and a slice of file data.
//! - Stops at "TRAILER!!!".
//! - No allocations for file data; names are validated UTF-8 and borrowed from the archive.
//!
//! This is meant for initrd/initramfs usage in a kernel.

#![no_std]

use core::{fmt, str};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpioError {
    UnexpectedEof,
    BadMagic,
    BadHex,
    BadUtf8,
    BadAlignment,
}

impl fmt::Display for CpioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use CpioError::*;
        let s = match self {
            UnexpectedEof => "unexpected end of archive",
            BadMagic => "bad cpio magic (expected 070701)",
            BadHex => "invalid hex field in header",
            BadUtf8 => "filename is not valid UTF-8",
            BadAlignment => "alignment overflow/invalid",
        };
        f.write_str(s)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CpioEntry<'a> {
    /// Path name (as stored in archive), UTF-8.
    pub name: &'a str,
    /// POSIX mode (includes file type bits).
    pub mode: u32,
    /// File payload (empty for directories and special entries).
    pub data: &'a [u8],
}

fn read_exact<'a>(buf: &'a [u8], off: &mut usize, n: usize) -> Result<&'a [u8], CpioError> {
    let start = *off;
    let end = start.checked_add(n).ok_or(CpioError::UnexpectedEof)?;
    if end > buf.len() {
        return Err(CpioError::UnexpectedEof);
    }
    *off = end;
    Ok(&buf[start..end])
}

fn align_up_4(x: usize) -> Result<usize, CpioError> {
    // (x + 3) & !3, with overflow checking
    let y = x.checked_add(3).ok_or(CpioError::BadAlignment)?;
    Ok(y & !3)
}

fn parse_hex_u32(field: &[u8]) -> Result<u32, CpioError> {
    // field is ASCII hex, typically 8 chars
    let s = core::str::from_utf8(field).map_err(|_| CpioError::BadHex)?;
    u32::from_str_radix(s, 16).map_err(|_| CpioError::BadHex)
}

/// Iterator over entries in a `newc` archive.
pub struct NewcIter<'a> {
    buf: &'a [u8],
    off: usize,
    done: bool,
}

impl<'a> NewcIter<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self {
            buf,
            off: 0,
            done: false,
        }
    }
}

impl<'a> Iterator for NewcIter<'a> {
    type Item = Result<CpioEntry<'a>, CpioError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        // Each newc header is 110 bytes:
        // magic[6] + 13 fields * 8 hex chars.
        const HDR_LEN: usize = 110;

        let hdr = match read_exact(self.buf, &mut self.off, HDR_LEN) {
            Ok(h) => h,
            Err(e) => return Some(Err(e)),
        };

        // Magic: "070701" (newc). (There is also "070702" for CRC; we reject it here.)
        if &hdr[0..6] != b"070701" {
            return Some(Err(CpioError::BadMagic));
        }

        // Parse only the fields we need:
        // offsets in header after magic:
        // ino      [6..14]
        // mode     [14..22]
        // uid      [22..30]
        // gid      [30..38]
        // nlink    [38..46]
        // mtime    [46..54]
        // filesize [54..62]
        // devmajor [62..70]
        // devminor [70..78]
        // rdevmaj  [78..86]
        // rdevmin  [86..94]
        // namesize [94..102]
        // check    [102..110]
        let mode = match parse_hex_u32(&hdr[14..22]) {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };
        let filesize = match parse_hex_u32(&hdr[54..62]) {
            Ok(v) => v as usize,
            Err(e) => return Some(Err(e)),
        };
        let namesize = match parse_hex_u32(&hdr[94..102]) {
            Ok(v) => v as usize,
            Err(e) => return Some(Err(e)),
        };

        // name includes a trailing NUL, and namesize is at least 1.
        let name_bytes = match read_exact(self.buf, &mut self.off, namesize) {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };

        if name_bytes.is_empty() || *name_bytes.last().unwrap() != 0 {
            // Not strictly required to error, but helps catch malformed archives.
            return Some(Err(CpioError::BadUtf8));
        }

        let name_nul_stripped = &name_bytes[..name_bytes.len() - 1];
        let name = match str::from_utf8(name_nul_stripped) {
            Ok(s) => s,
            Err(_) => return Some(Err(CpioError::BadUtf8)),
        };

        // Align to 4-byte boundary after name.
        let aligned = match align_up_4(self.off) {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };
        if aligned > self.buf.len() {
            return Some(Err(CpioError::UnexpectedEof));
        }
        self.off = aligned;

        // Read file data.
        let data = match read_exact(self.buf, &mut self.off, filesize) {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };

        // Align again after file data.
        let aligned = match align_up_4(self.off) {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };
        if aligned > self.buf.len() {
            return Some(Err(CpioError::UnexpectedEof));
        }
        self.off = aligned;

        // End marker
        if name == "TRAILER!!!" {
            self.done = true;
            return None;
        }

        Some(Ok(CpioEntry { name, mode, data }))
    }
}

/// Convenience: find a file by exact path and return its payload.
pub fn find_file<'a>(archive: &'a [u8], path: &str) -> Result<Option<&'a [u8]>, CpioError> {
    for ent in NewcIter::new(archive) {
        let ent = ent?;
        if ent.name == path {
            return Ok(Some(ent.data));
        }
    }
    Ok(None)
}

// Optional: file type helpers (POSIX mode bits)
pub const S_IFMT: u32 = 0o170000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFDIR: u32 = 0o040000;

pub fn is_dir(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFDIR
}
pub fn is_reg(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFREG
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty_archive() {
        // This test only checks that the iterator doesn't panic on empty input.
        assert!(NewcIter::new(&[]).next().unwrap().is_err());
    }

    // A minimal initrd cpio archive with:
    // - directory "."
    // - directory "bin/"
    // - regular file "bin/init" with mode 755 and a short payload.
    #[test]
    fn parses_minimal_archive() {
        let archive = include_bytes!("../tests/data/initrd-minimal.cpio");

        let mut iter = NewcIter::new(archive);
        assert_eq!(iter.count(), 3); // ., bin/, bin/init

        let mut iter = NewcIter::new(archive);
        let entry = iter
            .filter_map(|e| e.ok())
            .find(|e| e.name == "bin/init")
            .unwrap();

        assert_eq!(entry.mode, 0o100755);
        assert_eq!(
            entry.data,
            b"\x13\x05\x00\x00\x93\x08\xa0\x02\x73\x00\x00\x00\x6f\x00\x00\x00"
        );
    }
}
