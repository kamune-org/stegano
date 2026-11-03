//! The plaintext that gets encrypted into a carrier: a list of text and file
//! entries, optionally DEFLATE-compressed.
//!
//! ```text
//! u8   version (1)
//! u8   flags (bit 0: entries are deflated)
//! ...  entries, possibly deflated:
//!      u32 count
//!      per entry: u8 kind, u16 name length, name (UTF-8), u64 mtime (0 = none),
//!                 u64 data length, data
//! ```

use crate::error::{Error, Result};

const VERSION: u8 = 1;
const FLAG_DEFLATE: u8 = 1;
const MAX_ENTRIES: usize = 65_535;
const MAX_NAME: usize = 1024;
/// Upper bound on the inflated size, so a hostile carrier can't exhaust memory.
const MAX_INFLATED: usize = 1 << 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Text,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: EntryKind,
    /// File name for files; empty for text.
    pub name: String,
    /// Modification time in seconds since the Unix epoch, if known.
    pub modified: Option<u64>,
    pub data: Vec<u8>,
}

impl Entry {
    pub fn text(text: impl Into<String>) -> Self {
        Entry {
            kind: EntryKind::Text,
            name: String::new(),
            modified: None,
            data: text.into().into_bytes(),
        }
    }

    pub fn file(name: impl Into<String>, data: Vec<u8>, modified: Option<u64>) -> Self {
        Entry {
            kind: EntryKind::File,
            name: name.into(),
            modified,
            data,
        }
    }

    /// The text of a text entry (lossy if it somehow isn't UTF-8).
    pub fn as_text(&self) -> String {
        String::from_utf8_lossy(&self.data).into_owned()
    }

    /// A name that is safe to join onto a destination directory: no directories,
    /// no `..`, no control characters, never empty.
    pub fn safe_file_name(&self) -> String {
        let base = self
            .name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default()
            .chars()
            .filter(|c| !c.is_control() && !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
            .collect::<String>();
        let base = base.trim().trim_start_matches('.').to_string();
        if base.is_empty() {
            "extracted.bin".into()
        } else {
            base
        }
    }

    fn encoded_len(&self) -> usize {
        1 + 2 + self.name.len() + 8 + 8 + self.data.len()
    }
}

/// Exact size of the uncompressed container for these entries. Useful for
/// capacity checks before doing any work.
pub fn encoded_size(entries: &[Entry]) -> usize {
    2 + 4 + entries.iter().map(Entry::encoded_len).sum::<usize>()
}

pub(crate) fn encode(entries: &[Entry], compress: bool) -> Result<Vec<u8>> {
    if entries.is_empty() {
        return Err(Error::InvalidPayload("nothing to hide".into()));
    }
    if entries.len() > MAX_ENTRIES {
        return Err(Error::InvalidPayload(format!(
            "more than {MAX_ENTRIES} entries"
        )));
    }

    let mut body = Vec::with_capacity(encoded_size(entries));
    body.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    for e in entries {
        if e.name.len() > MAX_NAME {
            return Err(Error::InvalidPayload(format!(
                "file name longer than {MAX_NAME} bytes"
            )));
        }
        body.push(match e.kind {
            EntryKind::Text => 0,
            EntryKind::File => 1,
        });
        body.extend_from_slice(&(e.name.len() as u16).to_be_bytes());
        body.extend_from_slice(e.name.as_bytes());
        body.extend_from_slice(&e.modified.unwrap_or(0).to_be_bytes());
        body.extend_from_slice(&(e.data.len() as u64).to_be_bytes());
        body.extend_from_slice(&e.data);
    }

    let mut out = vec![VERSION, 0];
    if compress {
        let deflated = miniz_oxide::deflate::compress_to_vec(&body, 9);
        if deflated.len() < body.len() {
            out[1] |= FLAG_DEFLATE;
            out.extend_from_slice(&deflated);
            return Ok(out);
        }
    }
    out.extend_from_slice(&body);
    Ok(out)
}

pub(crate) fn decode(data: &[u8]) -> Result<Vec<Entry>> {
    let [version, flags, rest @ ..] = data else {
        return Err(Error::Corrupt("container too short".into()));
    };
    if *version != VERSION {
        return Err(Error::Corrupt(format!(
            "unknown container version {version}"
        )));
    }
    let inflated;
    let mut r = Reader(if flags & FLAG_DEFLATE != 0 {
        inflated = miniz_oxide::inflate::decompress_to_vec_with_limit(rest, MAX_INFLATED)
            .map_err(|e| Error::Corrupt(format!("decompression failed: {e:?}")))?;
        &inflated
    } else {
        rest
    });

    let count = u32::from_be_bytes(r.take()?) as usize;
    if count > MAX_ENTRIES {
        return Err(Error::Corrupt("too many entries".into()));
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = match r.take::<1>()?[0] {
            0 => EntryKind::Text,
            1 => EntryKind::File,
            k => return Err(Error::Corrupt(format!("unknown entry kind {k}"))),
        };
        let name_len = u16::from_be_bytes(r.take()?) as usize;
        let name = String::from_utf8(r.bytes(name_len)?.to_vec())
            .map_err(|_| Error::Corrupt("file name is not UTF-8".into()))?;
        let modified = Some(u64::from_be_bytes(r.take()?)).filter(|&t| t != 0);
        let len = usize::try_from(u64::from_be_bytes(r.take()?))
            .map_err(|_| Error::Corrupt("entry too large".into()))?;
        let data = r.bytes(len)?.to_vec();
        entries.push(Entry {
            kind,
            name,
            modified,
            data,
        });
    }
    if !r.0.is_empty() {
        return Err(Error::Corrupt("trailing bytes after entries".into()));
    }
    Ok(entries)
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.0.len() < n {
            return Err(Error::Corrupt("unexpected end of data".into()));
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Ok(head)
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.bytes(N)?.try_into().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Entry> {
        vec![
            Entry::text("meet at noon"),
            Entry::file("notes.txt", b"a".repeat(5000), Some(1_700_000_000)),
            Entry::file("empty.bin", vec![], None),
        ]
    }

    #[test]
    fn roundtrip_plain_and_deflated() {
        for compress in [false, true] {
            let bytes = encode(&sample(), compress).unwrap();
            assert_eq!(decode(&bytes).unwrap(), sample());
        }
        assert_eq!(
            encode(&sample(), false).unwrap().len(),
            encoded_size(&sample())
        );
        assert!(encode(&sample(), true).unwrap().len() < 1000);
    }

    #[test]
    fn rejects_truncated() {
        let bytes = encode(&sample(), false).unwrap();
        assert!(decode(&bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn sanitizes_names() {
        let mut e = Entry::file("../../etc/passwd", vec![], None);
        assert_eq!(e.safe_file_name(), "passwd");
        e.name = "C:\\Users\\x\\..".into();
        assert_eq!(e.safe_file_name(), "extracted.bin");
        e.name = ".bashrc".into();
        assert_eq!(e.safe_file_name(), "bashrc");
    }
}
