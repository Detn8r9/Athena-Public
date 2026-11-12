//! A small ZIM writer, so ATHENA can pack its own library into the same container Kiwix uses.
//!
//! Why ZIM: the files stay compressed on disk and are still readable one article at a time, by
//! ATHENA and by any Kiwix reader on any device. Nothing has to be unpacked to be used.
//!
//! Layout written (format version 5.0, the classic namespace scheme every reader supports):
//!
//! ```text
//! header (80 bytes) | MIME list | URL pointers | title pointers | directory entries |
//! cluster pointers (reserved for the worst case) | clusters … | MD5 checksum (16 bytes)
//! ```
//!
//! Clusters are written as the data arrives, so a pack of any size needs no temporary copy. To
//! make that possible the list of entries (path, title, type) is given up front; their content is
//! then supplied in any order.

use md5::{Digest, Md5};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

const MAGIC: u32 = 0x044D_495A;
const HEADER_SIZE: u64 = 80;
const NO_PAGE: u32 = 0xFFFF_FFFF;
/// Text entries are grouped into clusters of about this much raw data. Bigger clusters compress
/// better (similar articles share a dictionary) but cost more to open one article from.
const TEXT_CLUSTER_BYTES: usize = 4 * 1024 * 1024;
/// A stored (uncompressed) blob must fit a 32-bit cluster offset.
pub const MAX_STORED_BYTES: u64 = 3_900_000_000;

/// One entry, declared before any content is written.
#[derive(Debug, Clone)]
pub struct PlannedEntry {
    /// Namespace byte: `b'A'` for content, `b'M'` for metadata.
    pub namespace: u8,
    pub url: String,
    pub title: String,
    pub mime: String,
}

struct Slot {
    entry: PlannedEntry,
    mime_id: u16,
    /// (cluster number, blob number) once the content has been written.
    location: Option<(u32, u32)>,
}

pub struct ZimWriter {
    out: BufWriter<File>,
    slots: Vec<Slot>,
    index: HashMap<(u8, String), usize>,
    mimes: Vec<String>,
    main_page: Option<(u8, String)>,
    cluster_ptr_pos: u64,
    /// Where the next cluster starts.
    position: u64,
    cluster_offsets: Vec<u64>,
    pending: Vec<(usize, Vec<u8>)>,
    pending_bytes: usize,
    level: i32,
}

fn dirent_len(e: &PlannedEntry) -> u64 {
    let title_len = if e.title == e.url { 0 } else { e.title.len() };
    (16 + e.url.len() + 1 + title_len + 1) as u64
}

impl ZimWriter {
    /// Start a ZIM at `path`. `level` is the zstd level for text (19 is the practical maximum).
    pub fn create(
        path: &Path,
        mut planned: Vec<PlannedEntry>,
        main_page: Option<(u8, &str)>,
        level: i32,
    ) -> Result<Self, String> {
        if planned.is_empty() {
            return Err("A pack needs at least one entry.".to_string());
        }
        if planned.len() > 50_000_000 {
            return Err("Too many entries for one pack.".to_string());
        }
        // Directory entries are looked up by binary search on (namespace, url).
        planned.sort_by(|a, b| (a.namespace, a.url.as_bytes()).cmp(&(b.namespace, b.url.as_bytes())));
        planned.dedup_by(|a, b| a.namespace == b.namespace && a.url == b.url);

        let mut mimes: Vec<String> = planned.iter().map(|e| e.mime.clone()).collect();
        mimes.sort();
        mimes.dedup();
        let mime_list_len: u64 = mimes.iter().map(|m| m.len() as u64 + 1).sum::<u64>() + 1;

        let n = planned.len() as u64;
        let dirents_len: u64 = planned.iter().map(dirent_len).sum();
        let url_ptr_pos = HEADER_SIZE + mime_list_len;
        let title_ptr_pos = url_ptr_pos + 8 * n;
        let dirents_pos = title_ptr_pos + 4 * n;
        let cluster_ptr_pos = dirents_pos + dirents_len;
        // At most one cluster per entry, so this reservation always fits the real list.
        let first_cluster = cluster_ptr_pos + 8 * n;

        let mut index = HashMap::with_capacity(planned.len());
        let slots: Vec<Slot> = planned
            .into_iter()
            .enumerate()
            .map(|(i, entry)| {
                index.insert((entry.namespace, entry.url.clone()), i);
                let mime_id = mimes.binary_search(&entry.mime).unwrap_or(0) as u16;
                Slot { entry, mime_id, location: None }
            })
            .collect();

        let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(path).map_err(|e| format!("Could not create {}: {e}", path.display()))?;
        let mut out = BufWriter::with_capacity(1 << 20, file);
        out.seek(SeekFrom::Start(first_cluster)).map_err(|e| e.to_string())?;

        Ok(Self {
            out,
            slots,
            index,
            mimes,
            main_page: main_page.map(|(ns, url)| (ns, url.to_string())),
            cluster_ptr_pos,
            position: first_cluster,
            cluster_offsets: Vec::new(),
            pending: Vec::new(),
            pending_bytes: 0,
            level,
        })
    }

    fn slot(&self, namespace: u8, url: &str) -> Result<usize, String> {
        let i = *self
            .index
            .get(&(namespace, url.to_string()))
            .ok_or_else(|| format!("Entry was not planned: {url}"))?;
        if self.slots[i].location.is_some() {
            return Err(format!("Entry was written twice: {url}"));
        }
        Ok(i)
    }

    /// Add content that compresses well (text, HTML, JSON). It is grouped with its neighbours.
    pub fn add_compressed(&mut self, namespace: u8, url: &str, data: &[u8]) -> Result<(), String> {
        let i = self.slot(namespace, url)?;
        // Mark as taken now so a second add of the same entry is caught before the flush.
        self.slots[i].location = Some((u32::MAX, u32::MAX));
        self.pending_bytes += data.len();
        self.pending.push((i, data.to_vec()));
        if self.pending_bytes >= TEXT_CLUSTER_BYTES {
            self.flush_text_cluster()?;
        }
        Ok(())
    }

    fn flush_text_cluster(&mut self) -> Result<(), String> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.pending);
        self.pending_bytes = 0;
        let count = pending.len();
        let table = 4 * (count + 1);
        let total: usize = table + pending.iter().map(|(_, d)| d.len()).sum::<usize>();
        if total as u64 > MAX_STORED_BYTES {
            return Err("A text cluster grew past the format limit.".to_string());
        }
        let mut raw = Vec::with_capacity(total);
        let mut offset = table as u32;
        raw.extend_from_slice(&offset.to_le_bytes());
        for (_, d) in &pending {
            offset += d.len() as u32;
            raw.extend_from_slice(&offset.to_le_bytes());
        }
        for (_, d) in &pending {
            raw.extend_from_slice(d);
        }
        let packed = zstd::bulk::compress(&raw, self.level).map_err(|e| format!("Compression failed: {e}"))?;
        let cluster = self.cluster_offsets.len() as u32;
        self.cluster_offsets.push(self.position);
        self.out.write_all(&[5u8]).map_err(|e| e.to_string())?;
        self.out.write_all(&packed).map_err(|e| e.to_string())?;
        self.position += 1 + packed.len() as u64;
        for (blob, (i, _)) in pending.into_iter().enumerate() {
            self.slots[i].location = Some((cluster, blob as u32));
        }
        Ok(())
    }

    /// Add content that is already compressed (PDF, images, EPUB). It is stored byte for byte in
    /// a cluster of its own, read straight from `reader` without loading it into memory.
    pub fn add_stored(&mut self, namespace: u8, url: &str, len: u64, reader: &mut impl Read) -> Result<(), String> {
        if len > MAX_STORED_BYTES {
            return Err(format!("{url} is too large for one pack entry."));
        }
        let i = self.slot(namespace, url)?;
        let cluster = self.cluster_offsets.len() as u32;
        self.cluster_offsets.push(self.position);
        self.out.write_all(&[1u8]).map_err(|e| e.to_string())?;
        self.out.write_all(&8u32.to_le_bytes()).map_err(|e| e.to_string())?;
        self.out.write_all(&(8 + len as u32).to_le_bytes()).map_err(|e| e.to_string())?;
        let copied = std::io::copy(&mut reader.take(len), &mut self.out).map_err(|e| e.to_string())?;
        if copied != len {
            return Err(format!("{url} changed size while it was being packed."));
        }
        self.position += 9 + len;
        self.slots[i].location = Some((cluster, 0));
        Ok(())
    }

    /// Write the directory, pointers, header and checksum. Entries that never received content
    /// become empty. Returns the size of the finished file.
    pub fn finish(mut self) -> Result<u64, String> {
        let missing: Vec<usize> = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.location.is_none())
            .map(|(i, _)| i)
            .collect();
        for i in missing {
            self.slots[i].location = Some((u32::MAX, u32::MAX));
            self.pending.push((i, Vec::new()));
        }
        self.flush_text_cluster()?;

        let checksum_pos = self.position;
        let n = self.slots.len();
        let mime_list_len: u64 = self.mimes.iter().map(|m| m.len() as u64 + 1).sum::<u64>() + 1;
        let url_ptr_pos = HEADER_SIZE + mime_list_len;
        let title_ptr_pos = url_ptr_pos + 8 * n as u64;
        let dirents_pos = title_ptr_pos + 4 * n as u64;

        let mut front: Vec<u8> = Vec::with_capacity((self.cluster_ptr_pos + 8 * self.cluster_offsets.len() as u64) as usize);
        // Header.
        front.extend_from_slice(&MAGIC.to_le_bytes());
        front.extend_from_slice(&5u16.to_le_bytes());
        front.extend_from_slice(&0u16.to_le_bytes());
        front.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
        front.extend_from_slice(&(n as u32).to_le_bytes());
        front.extend_from_slice(&(self.cluster_offsets.len() as u32).to_le_bytes());
        front.extend_from_slice(&url_ptr_pos.to_le_bytes());
        front.extend_from_slice(&title_ptr_pos.to_le_bytes());
        front.extend_from_slice(&self.cluster_ptr_pos.to_le_bytes());
        front.extend_from_slice(&HEADER_SIZE.to_le_bytes());
        let main = self
            .main_page
            .as_ref()
            .and_then(|(ns, url)| self.index.get(&(*ns, url.clone())))
            .map(|i| *i as u32)
            .unwrap_or(NO_PAGE);
        front.extend_from_slice(&main.to_le_bytes());
        front.extend_from_slice(&NO_PAGE.to_le_bytes());
        front.extend_from_slice(&checksum_pos.to_le_bytes());
        // MIME list.
        for m in &self.mimes {
            front.extend_from_slice(m.as_bytes());
            front.push(0);
        }
        front.push(0);
        // URL pointers.
        let mut at = dirents_pos;
        for s in &self.slots {
            front.extend_from_slice(&at.to_le_bytes());
            at += dirent_len(&s.entry);
        }
        // Title pointers: entry numbers ordered by (namespace, title).
        let mut by_title: Vec<u32> = (0..n as u32).collect();
        by_title.sort_by(|a, b| {
            let (ea, eb) = (&self.slots[*a as usize].entry, &self.slots[*b as usize].entry);
            (ea.namespace, ea.title.as_bytes()).cmp(&(eb.namespace, eb.title.as_bytes()))
        });
        for i in by_title {
            front.extend_from_slice(&i.to_le_bytes());
        }
        // Directory entries.
        for s in &self.slots {
            let (cluster, blob) = s.location.unwrap_or((0, 0));
            front.extend_from_slice(&s.mime_id.to_le_bytes());
            front.push(0);
            front.push(s.entry.namespace);
            front.extend_from_slice(&0u32.to_le_bytes());
            front.extend_from_slice(&cluster.to_le_bytes());
            front.extend_from_slice(&blob.to_le_bytes());
            front.extend_from_slice(s.entry.url.as_bytes());
            front.push(0);
            if s.entry.title != s.entry.url {
                front.extend_from_slice(s.entry.title.as_bytes());
            }
            front.push(0);
        }
        debug_assert_eq!(front.len() as u64, self.cluster_ptr_pos);
        // Cluster pointers.
        for off in &self.cluster_offsets {
            front.extend_from_slice(&off.to_le_bytes());
        }

        self.out.flush().map_err(|e| e.to_string())?;
        let mut file = self.out.into_inner().map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        file.write_all(&front).map_err(|e| e.to_string())?;

        // MD5 of everything before the checksum, as the format requires.
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut hasher = Md5::new();
        let mut left = checksum_pos;
        let mut buf = vec![0u8; 1 << 20];
        while left > 0 {
            let want = buf.len().min(left as usize);
            file.read_exact(&mut buf[..want]).map_err(|e| e.to_string())?;
            hasher.update(&buf[..want]);
            left -= want as u64;
        }
        file.seek(SeekFrom::Start(checksum_pos)).map_err(|e| e.to_string())?;
        file.write_all(&hasher.finalize()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        Ok(checksum_pos + 16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(ns: u8, url: &str, title: &str, mime: &str) -> PlannedEntry {
        PlannedEntry { namespace: ns, url: url.to_string(), title: title.to_string(), mime: mime.to_string() }
    }

    fn read(zim: &zim::Zim, ns: zim::Namespace, path: &str) -> Vec<u8> {
        let e = zim.get_by_path(ns, path).unwrap().expect("entry exists");
        zim.entry_content(&e).unwrap().expect("has content").to_vec().unwrap()
    }

    #[test]
    fn a_written_pack_reads_back_byte_for_byte() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pack.zim");
        let pdf: Vec<u8> = (0..200_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8).collect();
        let article = "<html><body><h1>Water</h1><p>Boil it for one minute.</p></body></html>".repeat(50);

        let mut w = ZimWriter::create(
            &path,
            vec![
                entry(b'A', "text/water.html", "Water", "text/html"),
                entry(b'A', "orig/manual.pdf", "Manual", "application/pdf"),
                entry(b'A', "index.html", "Contents", "text/html"),
                entry(b'M', "Title", "Title", "text/plain"),
                entry(b'A', "never-written.txt", "never-written.txt", "text/plain"),
            ],
            Some((b'A', "index.html")),
            19,
        )
        .unwrap();
        w.add_stored(b'A', "orig/manual.pdf", pdf.len() as u64, &mut &pdf[..]).unwrap();
        w.add_compressed(b'A', "text/water.html", article.as_bytes()).unwrap();
        w.add_compressed(b'M', "Title", b"Test pack").unwrap();
        w.add_compressed(b'A', "index.html", b"<a href=\"text/water.html\">Water</a>").unwrap();
        let size = w.finish().unwrap();
        assert_eq!(size, std::fs::metadata(&path).unwrap().len());
        assert!(size < (pdf.len() + article.len()) as u64, "repetitive text is compressed");

        let zim = zim::Zim::new(&path).unwrap();
        zim.verify_checksum().unwrap();
        assert_eq!(read(&zim, zim::Namespace::Articles, "orig/manual.pdf"), pdf);
        assert_eq!(read(&zim, zim::Namespace::Articles, "text/water.html"), article.as_bytes());
        assert_eq!(read(&zim, zim::Namespace::Metadata, "Title"), b"Test pack");
        assert_eq!(read(&zim, zim::Namespace::Articles, "never-written.txt"), b"");
        assert_eq!(zim.main_page().unwrap().unwrap().url, "index.html");
        assert_eq!(zim.iterate_by_urls().count(), 5);
    }

    #[test]
    fn unplanned_and_repeated_entries_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = ZimWriter::create(&dir.path().join("p.zim"), vec![entry(b'A', "a", "a", "text/plain")], None, 3).unwrap();
        assert!(w.add_compressed(b'A', "b", b"x").is_err());
        w.add_compressed(b'A', "a", b"x").unwrap();
        assert!(w.add_compressed(b'A', "a", b"y").is_err());
    }

    #[test]
    fn many_small_entries_span_several_clusters() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("many.zim");
        let planned: Vec<PlannedEntry> = (0..300).map(|i| entry(b'A', &format!("doc/{i:04}"), &format!("Doc {i}"), "text/plain")).collect();
        let mut w = ZimWriter::create(&path, planned, None, 3).unwrap();
        // 300 entries of ~40 KB of varied text: about 12 MB, so at least two clusters.
        for i in 0..300u32 {
            let body: String = (0..4000).map(|j| format!("{:x} ", i.wrapping_mul(31).wrapping_add(j * 7919))).collect();
            w.add_compressed(b'A', &format!("doc/{i:04}"), body.as_bytes()).unwrap();
        }
        w.finish().unwrap();
        let zim = zim::Zim::new(&path).unwrap();
        zim.verify_checksum().unwrap();
        assert!(zim.header.cluster_count >= 2);
        let got = read(&zim, zim::Namespace::Articles, "doc/0299");
        assert!(got.starts_with(format!("{:x} ", 299u32.wrapping_mul(31)).as_bytes()));
    }
}
