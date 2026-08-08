//! Background thumbnail decoding with bounded in-memory and on-disk caches.
//!
//! Decoding images on the UI thread would stall frame rendering, so paths
//! are handed to a small pool of worker threads and results are collected
//! once per frame via [`ThumbService::drain`]. A single worker was not
//! enough: embedded JPEG covers take tens to hundreds of milliseconds each,
//! so scrolling a large music folder outran one serial decoder. Decoded
//! thumbs are also written through to a disk cache under
//! `$XDG_CACHE_HOME/lantern/thumbs/`, so covers survive app restarts.
//! Audio covers (FLAC PICTURE blocks, ID3v2 APIC frames) are parsed by hand
//! with bounded reads — audio files run to tens of megabytes and only their
//! leading metadata is relevant — then fed to the same `image`-crate
//! decoder as plain image files.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

/// Longest edge of a decoded thumbnail.
const THUMB_SIZE: u32 = 128;
/// Upper bound for any single buffer read from disk.
const READ_CAP: u64 = 32 * 1024 * 1024;
/// Unfinished requests beyond this many drop the OLDEST queued one: while
/// scrolling, the oldest pending paths are the rows most likely already
/// scrolled past, and the newest requests are the rows in view.
const MAX_PENDING: usize = 256;
/// Cache limits: exceeding either on insert evicts least-recently-used.
const MAX_CACHED: usize = 512;
const MAX_CACHE_BYTES: usize = 64 * 1024 * 1024;
/// Decode pool size: `available_parallelism` capped at this, at least 1.
const MAX_WORKERS: usize = 4;
/// Bound on the on-disk cache; enforced once per startup by the janitor.
const MAX_DISK_ENTRIES: usize = 4096;
/// Disk entry header: width u32 LE, height u32 LE, source size u64 LE,
/// source mtime secs u64 LE.
const DISK_HEADER: usize = 24;

/// A decoded thumbnail: tightly packed RGBA8 (not premultiplied).
#[derive(Clone, Debug)]
pub struct Thumb {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Decode `path` to a thumbnail, or `None` for unsupported/invalid input.
///
/// Pure function: the disk-cache read-through lives in the workers, not
/// here. Never reads more than [`READ_CAP`] bytes and never panics on
/// malformed data: every parse step uses checked slicing and bails out on
/// the first inconsistency.
pub fn decode_thumb(path: &Path) -> Option<Thumb> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let bytes = match extension.as_str() {
        "flac" => flac_picture(path)?,
        "mp3" => id3_picture(path)?,
        // Only PNG/JPEG decoders are compiled in (the approved `image`
        // feature set); keep the dispatch list in sync with it.
        "png" | "jpg" | "jpeg" => read_capped(path)?,
        _ => return None,
    };
    let image = image::load_from_memory(&bytes).ok()?;
    let thumb = image.thumbnail(THUMB_SIZE, THUMB_SIZE).to_rgba8();
    if thumb.width() == 0 || thumb.height() == 0 {
        return None;
    }
    Some(Thumb {
        width: thumb.width(),
        height: thumb.height(),
        rgba: thumb.into_raw(),
    })
}

/// Request queue shared between the service and the worker pool.
struct Shared {
    /// Queued-but-not-started requests, oldest first.
    queue: VecDeque<PathBuf>,
    /// Set by `Drop` so detached workers exit instead of parking forever.
    shutdown: bool,
}

/// Worker-pool background decode service with an LRU pixel cache.
///
/// Requests are pushed onto a shared `Mutex<VecDeque>` + `Condvar` that
/// `min(available_parallelism, 4)` workers pop from; finished thumbs flow
/// back on a single mpsc channel. When the pending count hits
/// [`MAX_PENDING`] the OLDEST queued request is dropped (and removed from
/// the dedupe set, so a re-request works) — the newest requests are the
/// rows the user is looking at. Workers read through a per-startup-pruned
/// disk cache before touching [`decode_thumb`].
pub struct ThumbService {
    shared: Arc<(Mutex<Shared>, Condvar)>,
    result_rx: Receiver<(PathBuf, Option<Thumb>)>,
    /// Sent-but-unfinished paths (queued or mid-decode), for O(1) dedupe.
    pending_set: HashSet<PathBuf>,
    cache: HashMap<String, Thumb>,
    /// Recency list holding each cached path exactly once,
    /// most-recently-used last.
    recency: VecDeque<String>,
    cache_bytes: usize,
    /// Paths that produced no thumbnail; avoids re-decoding them every frame.
    /// Memory-only: failures are never written to the disk cache.
    negative: HashSet<String>,
}

impl ThumbService {
    /// Service backed by the system cache dir (`$XDG_CACHE_HOME` or
    /// `$HOME/.cache`, then `lantern/thumbs/`).
    pub fn new() -> ThumbService {
        Self::with_cache_dir(default_cache_dir())
    }

    /// Service backed by an explicit cache dir (tests, embedding).
    pub fn with_cache_dir(cache_dir: PathBuf) -> ThumbService {
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(MAX_WORKERS);
        Self::assemble(cache_dir, workers, true)
    }

    fn assemble(cache_dir: PathBuf, worker_count: usize, prune: bool) -> ThumbService {
        let _ = std::fs::create_dir_all(&cache_dir);
        let shared = Arc::new((
            Mutex::new(Shared {
                queue: VecDeque::new(),
                shutdown: false,
            }),
            Condvar::new(),
        ));
        let (result_tx, result_rx) = mpsc::channel::<(PathBuf, Option<Thumb>)>();
        for _ in 0..worker_count {
            let worker_shared = Arc::clone(&shared);
            let worker_dir = cache_dir.clone();
            let worker_tx = result_tx.clone();
            // JoinHandles are deliberately dropped (detached workers);
            // `Drop` flips the shutdown flag and wakes the pool, so the
            // threads exit promptly instead of parking on the condvar.
            thread::spawn(move || worker_loop(&worker_shared, &worker_dir, &worker_tx));
        }
        if prune {
            // Janitor: bound the on-disk cache once per startup, off the
            // caller's thread so startup never blocks on it.
            thread::spawn(move || prune_cache_dir(&cache_dir, MAX_DISK_ENTRIES));
        }
        ThumbService {
            shared,
            result_rx,
            pending_set: HashSet::new(),
            cache: HashMap::new(),
            recency: VecDeque::new(),
            cache_bytes: 0,
            negative: HashSet::new(),
        }
    }

    /// Queue `path` for decoding unless it is cached, negatively cached, or
    /// already queued/decoding. Over [`MAX_PENDING`] pending, the OLDEST
    /// queued request is dropped to make room (it leaves the dedupe set, so
    /// a later re-request re-queues it).
    pub fn request(&mut self, path: &str) {
        if self.cache.contains_key(path)
            || self.negative.contains(path)
            || self.pending_set.contains(Path::new(path))
        {
            return;
        }
        let owned = PathBuf::from(path);
        let (lock, condvar) = &*self.shared;
        {
            let mut shared = lock.lock().unwrap();
            if self.pending_set.len() >= MAX_PENDING {
                if let Some(dropped) = shared.queue.pop_front() {
                    self.pending_set.remove(&dropped);
                }
            }
            shared.queue.push_back(owned.clone());
        }
        condvar.notify_one();
        self.pending_set.insert(owned);
    }

    /// Collect everything finished since the last call, caching the results
    /// (and negatively caching failures). Non-blocking.
    pub fn drain(&mut self) -> Vec<(String, Thumb)> {
        let mut ready = Vec::new();
        while let Ok((path, thumb)) = self.result_rx.try_recv() {
            let key = path.to_string_lossy().into_owned();
            self.pending_set.remove(&path);
            match thumb {
                Some(thumb) => {
                    self.insert(key.clone(), thumb.clone());
                    ready.push((key, thumb));
                }
                None => {
                    self.negative.insert(key);
                }
            }
        }
        ready
    }

    /// Cache lookup; a hit moves the entry to most-recently-used.
    pub fn get(&mut self, path: &str) -> Option<&Thumb> {
        if self.cache.contains_key(path) {
            self.untrack(path);
            self.recency.push_back(path.to_string());
        }
        self.cache.get(path)
    }

    /// Drop queued-but-not-started requests and forget negative results.
    /// Called on directory navigation; the cache itself is kept — thumbs
    /// stay valid across directories. A request already mid-decode still
    /// lands in the result channel and is cached normally, which is
    /// harmless: it was wanted once.
    pub fn clear_pending(&mut self) {
        let (lock, _) = &*self.shared;
        {
            let mut shared = lock.lock().unwrap();
            for path in shared.queue.drain(..) {
                self.pending_set.remove(&path);
            }
        }
        self.negative.clear();
    }

    fn insert(&mut self, path: String, thumb: Thumb) {
        if let Some(old) = self.cache.insert(path.clone(), thumb) {
            // Re-insert: replace the old entry's byte share and recency slot.
            self.cache_bytes -= old.rgba.len();
            self.untrack(&path);
        }
        self.cache_bytes += self.cache[&path].rgba.len();
        self.recency.push_back(path);
        while self.cache.len() > MAX_CACHED || self.cache_bytes > MAX_CACHE_BYTES {
            let Some(oldest) = self.pop_oldest() else {
                break;
            };
            if let Some(evicted) = self.cache.remove(&oldest) {
                self.cache_bytes -= evicted.rgba.len();
            }
        }
    }

    /// Drop `path` from the recency list (it holds each cached path once).
    fn untrack(&mut self, path: &str) {
        if let Some(position) = self.recency.iter().position(|p| p == path) {
            self.recency.remove(position);
        }
    }

    /// Oldest live entry of the recency list.
    fn pop_oldest(&mut self) -> Option<String> {
        while let Some(path) = self.recency.pop_front() {
            if self.cache.contains_key(&path) {
                return Some(path);
            }
        }
        None
    }
}

impl Default for ThumbService {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ThumbService {
    fn drop(&mut self) {
        let (lock, condvar) = &*self.shared;
        lock.lock().unwrap().shutdown = true;
        condvar.notify_all();
    }
}

fn worker_loop(
    shared: &Arc<(Mutex<Shared>, Condvar)>,
    cache_dir: &Path,
    result_tx: &Sender<(PathBuf, Option<Thumb>)>,
) {
    let (lock, condvar) = &**shared;
    loop {
        let path = {
            let mut shared = lock.lock().unwrap();
            loop {
                if shared.shutdown {
                    return;
                }
                if let Some(path) = shared.queue.pop_front() {
                    break path;
                }
                shared = condvar.wait(shared).unwrap();
            }
        };
        // `Option` already absorbs all parse/decode failures; catch_unwind
        // is insurance against panics inside the decoder so one corrupt
        // file can never kill the pool.
        let thumb = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cached_thumb(cache_dir, &path)
        }))
        .ok()
        .flatten();
        if result_tx.send((path, thumb)).is_err() {
            return;
        }
    }
}

// ---- on-disk cache --------------------------------------------------------

fn default_cache_dir() -> PathBuf {
    let base = std::env::var("XDG_CACHE_HOME")
        .unwrap_or_else(|_| crate::path::join(&crate::path::home_dir(), ".cache"));
    PathBuf::from(base).join("lantern").join("thumbs")
}

/// Two FNV-1a-64 passes (different seeds) over the path bytes, hex-joined
/// into a pseudo-128-bit key. The source file's size/mtime are NOT part of
/// the key — they are stored inside the entry and validated on read — so a
/// cover can still be served when the file itself is gone (deleted track,
/// unmounted share).
fn cache_key(path: &Path) -> String {
    let bytes = path.to_string_lossy();
    let bytes = bytes.as_bytes();
    format!(
        "{:016x}{:016x}",
        fnv1a(0xcbf2_9ce4_8422_2325, bytes),
        fnv1a(0x8422_2325_cbf2_9ce4, bytes)
    )
}

fn fnv1a(seed: u64, bytes: &[u8]) -> u64 {
    let mut hash = seed;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Current (size, mtime_secs) stamp of `path`, if it can be stat'ed.
fn stat_stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some((meta.len(), mtime))
}

/// Read a cached thumbnail. An entry whose stored stamp disagrees with the
/// file on disk is stale: treated as a miss (the re-decode overwrites it).
/// An unverifiable entry (source deleted or unstat-able) is accepted — a
/// possibly-stale cover beats none. Any structural inconsistency is a miss.
fn disk_read(dir: &Path, key: &str, path: &Path) -> Option<Thumb> {
    let bytes = std::fs::read(dir.join(key)).ok()?;
    let mut cur = &bytes[..];
    let width = take_u32_le(&mut cur)?;
    let height = take_u32_le(&mut cur)?;
    let size = take_u64_le(&mut cur)?;
    let mtime = take_u64_le(&mut cur)?;
    if width == 0 || width > THUMB_SIZE || height == 0 || height > THUMB_SIZE {
        return None;
    }
    if cur.len() != width as usize * height as usize * 4 {
        return None;
    }
    if let Some(stamp) = stat_stamp(path) {
        if stamp != (size, mtime) {
            return None;
        }
    }
    Some(Thumb {
        width,
        height,
        rgba: cur.to_vec(),
    })
}

/// Write a decoded thumb to the disk cache. Best-effort: I/O errors are
/// ignored. Overwrites whatever (possibly corrupt) entry was there.
fn disk_write(dir: &Path, key: &str, path: &Path, thumb: &Thumb) {
    let Some((size, mtime)) = stat_stamp(path) else {
        return;
    };
    let mut bytes = Vec::with_capacity(DISK_HEADER + thumb.rgba.len());
    bytes.extend_from_slice(&thumb.width.to_le_bytes());
    bytes.extend_from_slice(&thumb.height.to_le_bytes());
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.extend_from_slice(&mtime.to_le_bytes());
    bytes.extend_from_slice(&thumb.rgba);
    let _ = std::fs::write(dir.join(key), bytes);
}

/// Worker-side read-through: a disk hit skips decoding entirely; a decode
/// is written behind. Negative results are never written.
fn cached_thumb(cache_dir: &Path, path: &Path) -> Option<Thumb> {
    let key = cache_key(path);
    if let Some(thumb) = disk_read(cache_dir, &key, path) {
        return Some(thumb);
    }
    let thumb = decode_thumb(path)?;
    disk_write(cache_dir, &key, path, &thumb);
    Some(thumb)
}

/// Delete oldest-by-mtime files until `dir` holds at most `max_entries`.
/// Best-effort: every I/O error is ignored.
fn prune_cache_dir(dir: &Path, max_entries: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(PathBuf, SystemTime)> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|entry| {
            let mtime = entry.metadata().and_then(|m| m.modified()).ok()?;
            Some((entry.path(), mtime))
        })
        .collect();
    if files.len() <= max_entries {
        return;
    }
    files.sort_by_key(|(_, mtime)| *mtime);
    let excess = files.len() - max_entries;
    for (path, _) in files.into_iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
}

fn read_capped(path: &Path) -> Option<Vec<u8>> {
    let file = File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(READ_CAP).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

// ---- checked byte-slice helpers ------------------------------------------

fn take<'a>(buf: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
    if buf.len() < n {
        return None;
    }
    let (head, tail) = buf.split_at(n);
    *buf = tail;
    Some(head)
}

fn take_u32(buf: &mut &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(take(buf, 4)?.try_into().ok()?))
}

fn take_u32_le(buf: &mut &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(take(buf, 4)?.try_into().ok()?))
}

fn take_u64_le(buf: &mut &[u8]) -> Option<u64> {
    Some(u64::from_le_bytes(take(buf, 8)?.try_into().ok()?))
}

fn syncsafe(bytes: &[u8]) -> Option<u32> {
    if bytes.len() != 4 || bytes.iter().any(|b| b & 0x80 != 0) {
        return None;
    }
    Some(bytes.iter().fold(0u32, |acc, b| (acc << 7) | u32::from(*b)))
}

// ---- FLAC -----------------------------------------------------------------

/// Extract the first PICTURE metadata block from a FLAC stream. Only the
/// block headers and the one picture block are read; everything else is
/// sought past.
fn flac_picture(path: &Path) -> Option<Vec<u8>> {
    let mut file = File::open(path).ok()?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).ok()?;
    if &magic != b"fLaC" {
        return None;
    }
    loop {
        let mut header = [0u8; 4];
        file.read_exact(&mut header).ok()?;
        let last = header[0] & 0x80 != 0;
        let block_type = header[0] & 0x7f;
        let length = u64::from(u32::from_be_bytes([0, header[1], header[2], header[3]]));
        if block_type == 6 {
            return flac_picture_block(file, length);
        }
        file.seek(SeekFrom::Current(length as i64)).ok()?;
        if last {
            return None;
        }
    }
}

/// Parse a PICTURE block body (at most 24-bit long, so reading it whole is
/// bounded): picture type, MIME, description, dimensions, depth, colors,
/// then the image data itself.
fn flac_picture_block(mut file: File, length: u64) -> Option<Vec<u8>> {
    let mut body = vec![0u8; length as usize];
    file.read_exact(&mut body).ok()?;
    let mut cur = &body[..];
    let _picture_type = take_u32(&mut cur)?;
    let mime_len = take_u32(&mut cur)? as usize;
    take(&mut cur, mime_len)?;
    let description_len = take_u32(&mut cur)? as usize;
    take(&mut cur, description_len)?;
    let _width = take_u32(&mut cur)?;
    let _height = take_u32(&mut cur)?;
    let _depth = take_u32(&mut cur)?;
    let _colors = take_u32(&mut cur)?;
    let data_len = take_u32(&mut cur)? as usize;
    if data_len as u64 >= READ_CAP {
        return None;
    }
    Some(take(&mut cur, data_len)?.to_vec())
}

// ---- ID3v2 ----------------------------------------------------------------

/// Extract the first APIC (v2.3/v2.4) or PIC (v2.2) frame from the ID3v2
/// tag at the start of an MP3 file. The tag size caps the read.
fn id3_picture(path: &Path) -> Option<Vec<u8>> {
    let mut file = File::open(path).ok()?;
    let mut header = [0u8; 10];
    file.read_exact(&mut header).ok()?;
    if &header[..3] != b"ID3" {
        return None;
    }
    let major = header[3];
    if !(2..=4).contains(&major) || header[4] == 0xff {
        return None;
    }
    let flags = header[5];
    let tag_size = syncsafe(&header[6..10])? as u64;
    if tag_size >= READ_CAP {
        return None;
    }
    let mut tag = vec![0u8; tag_size as usize];
    file.read_exact(&mut tag).ok()?;
    let mut cur = &tag[..];

    if flags & 0x40 != 0 {
        // Extended header: v2.3 stores a big-endian size excluding the four
        // size bytes; v2.4 a syncsafe size including them. v2.2 has none.
        match major {
            3 => {
                let size = take_u32(&mut cur)? as usize;
                take(&mut cur, size)?;
            }
            4 => {
                let size = syncsafe(take(&mut cur, 4)?)? as usize;
                take(&mut cur, size.checked_sub(4)?)?;
            }
            _ => {}
        }
    }

    match major {
        2 => id3_v22_frames(cur),
        _ => id3_frames(cur, major),
    }
}

/// Walk v2.3/v2.4 frames: 4-byte id, 4-byte size (v2.3 big-endian, v2.4
/// syncsafe), 2 flag bytes, body.
fn id3_frames(mut cur: &[u8], major: u8) -> Option<Vec<u8>> {
    loop {
        let header = take(&mut cur, 10)?;
        if header.iter().all(|&b| b == 0) {
            return None; // padding: no picture found
        }
        let id = &header[..4];
        if !id.iter().all(u8::is_ascii_alphanumeric) {
            return None;
        }
        let size = if major == 3 {
            u32::from_be_bytes(header[4..8].try_into().ok()?) as usize
        } else {
            syncsafe(&header[4..8])? as usize
        };
        let body = take(&mut cur, size)?;
        if id == b"APIC" {
            return apic_picture(body);
        }
    }
}

/// Walk v2.2 frames: 3-byte id, 3-byte big-endian size, body.
fn id3_v22_frames(mut cur: &[u8]) -> Option<Vec<u8>> {
    loop {
        let header = take(&mut cur, 6)?;
        if header.iter().all(|&b| b == 0) {
            return None;
        }
        let id = &header[..3];
        if !id.iter().all(u8::is_ascii_alphanumeric) {
            return None;
        }
        let size = u32::from_be_bytes([0, header[3], header[4], header[5]]) as usize;
        let body = take(&mut cur, size)?;
        if id == b"PIC" {
            return pic_picture(body);
        }
    }
}

/// APIC body: text encoding byte, NUL-terminated MIME (always single-byte),
/// picture type byte, description (terminated per encoding), picture data.
fn apic_picture(body: &[u8]) -> Option<Vec<u8>> {
    let (&encoding, rest) = body.split_first()?;
    if encoding > 3 {
        return None;
    }
    let (_mime, rest) = latin1_terminated(rest)?;
    let (&_picture_type, rest) = rest.split_first()?;
    let (_description, data) = text_terminated(rest, encoding)?;
    if data.is_empty() {
        return None;
    }
    Some(data.to_vec())
}

/// PIC body (v2.2): like APIC but with a fixed 3-byte format ("PNG"/"JPG")
/// instead of a MIME string.
fn pic_picture(body: &[u8]) -> Option<Vec<u8>> {
    let (&encoding, rest) = body.split_first()?;
    if encoding > 3 {
        return None;
    }
    let mut rest = rest;
    take(&mut rest, 3)?; // image format
    let (&_picture_type, rest) = rest.split_first()?;
    let (_description, data) = text_terminated(rest, encoding)?;
    if data.is_empty() {
        return None;
    }
    Some(data.to_vec())
}

fn latin1_terminated(buf: &[u8]) -> Option<(&[u8], &[u8])> {
    let end = buf.iter().position(|&b| b == 0)?;
    Some((&buf[..end], &buf[end + 1..]))
}

/// Split off a string terminated per its text encoding: encodings 0
/// (latin1) and 3 (UTF-8) end at one NUL, 1/2 (UTF-16) at an aligned NUL
/// pair.
fn text_terminated(buf: &[u8], encoding: u8) -> Option<(&[u8], &[u8])> {
    match encoding {
        1 | 2 => {
            let mut i = 0;
            while i + 1 < buf.len() {
                if buf[i] == 0 && buf[i + 1] == 0 {
                    return Some((&buf[..i], &buf[i + 2..]));
                }
                i += 2;
            }
            None
        }
        _ => latin1_terminated(buf),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A valid `width`×`height` PNG, encoded per test via the same crate.
    fn fixture_png(width: u32, height: u32) -> Vec<u8> {
        let mut img = image::RgbaImage::new(width, height);
        for pixel in img.pixels_mut() {
            *pixel = image::Rgba([10, 20, 30, 255]);
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lantern-thumbs-test-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// `fLaC` + STREAMINFO (34 fake bytes) + one PICTURE block carrying
    /// `png`; `truncate` cuts the PICTURE block body short.
    fn flac_fixture(png: Option<&[u8]>, truncate: bool) -> Vec<u8> {
        let mut out = b"fLaC".to_vec();
        let solo = png.is_none();
        out.push(if solo { 0x80 } else { 0 }); // STREAMINFO, last iff alone
        out.extend_from_slice(&[0, 0, 34]);
        out.extend_from_slice(&[0u8; 34]);
        if let Some(png) = png {
            let mut body = Vec::new();
            body.extend_from_slice(&3u32.to_be_bytes()); // cover (front)
            body.extend_from_slice(&9u32.to_be_bytes());
            body.extend_from_slice(b"image/png");
            body.extend_from_slice(&0u32.to_be_bytes()); // empty description
            body.extend_from_slice(&8u32.to_be_bytes());
            body.extend_from_slice(&4u32.to_be_bytes());
            body.extend_from_slice(&32u32.to_be_bytes());
            body.extend_from_slice(&0u32.to_be_bytes());
            body.extend_from_slice(&(png.len() as u32).to_be_bytes());
            body.extend_from_slice(png);
            if truncate {
                body.truncate(body.len() / 2);
            }
            out.push(0x80 | 6);
            let len = (body.len() as u32).to_be_bytes();
            out.extend_from_slice(&len[1..]);
            out.extend_from_slice(&body);
        }
        out
    }

    /// Minimal ID3v2.3 tag with one APIC frame (MIME image/png, type 3,
    /// empty description) carrying `png`.
    fn mp3_fixture(png: &[u8]) -> Vec<u8> {
        let mut frame_body = vec![0u8]; // latin1
        frame_body.extend_from_slice(b"image/png");
        frame_body.push(0u8);
        frame_body.push(3u8); // cover (front)
        frame_body.push(0u8); // empty description terminator
        frame_body.extend_from_slice(png);
        let mut frame = b"APIC".to_vec();
        frame.extend_from_slice(&(frame_body.len() as u32).to_be_bytes());
        frame.extend_from_slice(&[0, 0]);
        frame.extend_from_slice(&frame_body);
        let mut out = b"ID3".to_vec();
        out.extend_from_slice(&[3, 0, 0]); // v2.3.0, no flags
        let size = frame.len();
        out.extend_from_slice(&[
            ((size >> 21) & 0x7f) as u8,
            ((size >> 14) & 0x7f) as u8,
            ((size >> 7) & 0x7f) as u8,
            (size & 0x7f) as u8,
        ]);
        out.extend_from_slice(&frame);
        out
    }

    #[test]
    fn decodes_plain_png() {
        let dir = temp_dir("png");
        let file = dir.join("cover.png");
        std::fs::write(&file, fixture_png(8, 4)).unwrap();
        let thumb = decode_thumb(&file).expect("png should decode");
        assert_eq!((thumb.width, thumb.height), (128, 64));
        assert_eq!(thumb.rgba.len(), (128 * 64 * 4) as usize);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn flac_with_picture_decodes() {
        let dir = temp_dir("flac");
        let file = dir.join("song.flac");
        std::fs::write(&file, flac_fixture(Some(&fixture_png(8, 4)), false)).unwrap();
        let thumb = decode_thumb(&file).expect("embedded cover should decode");
        assert!(thumb.width <= 128 && thumb.height <= 128);
        assert_eq!(thumb.width, thumb.height * 2); // 8:4 aspect preserved
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn flac_without_picture_yields_none() {
        let dir = temp_dir("flac-none");
        let file = dir.join("song.flac");
        std::fs::write(&file, flac_fixture(None, false)).unwrap();
        assert!(decode_thumb(&file).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn flac_truncated_picture_yields_none() {
        let dir = temp_dir("flac-trunc");
        let file = dir.join("song.flac");
        std::fs::write(&file, flac_fixture(Some(&fixture_png(8, 4)), true)).unwrap();
        assert!(decode_thumb(&file).is_none()); // must not panic
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mp3_with_apic_decodes() {
        let dir = temp_dir("mp3");
        let file = dir.join("song.mp3");
        std::fs::write(&file, mp3_fixture(&fixture_png(8, 4))).unwrap();
        let thumb = decode_thumb(&file).expect("APIC cover should decode");
        assert_eq!((thumb.width, thumb.height), (128, 64));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mp3_bad_magic_yields_none() {
        let dir = temp_dir("mp3-bad");
        let file = dir.join("song.mp3");
        std::fs::write(&file, b"NOT-AN-ID3-TAG".as_slice()).unwrap();
        assert!(decode_thumb(&file).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unsupported_extension_yields_none() {
        let dir = temp_dir("txt");
        let file = dir.join("notes.txt");
        std::fs::write(&file, fixture_png(8, 4)).unwrap();
        assert!(decode_thumb(&file).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Poll `drain` until `want` results arrived or ~1s passed.
    fn drain_until(service: &mut ThumbService, want: usize) -> Vec<(String, Thumb)> {
        let mut all = Vec::new();
        for _ in 0..100 {
            all.extend(service.drain());
            if all.len() >= want {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        all
    }

    #[test]
    fn service_roundtrip_and_dedupe() {
        let dir = temp_dir("svc");
        let file = dir.join("photo.png");
        std::fs::write(&file, fixture_png(8, 4)).unwrap();
        let path = file.to_str().unwrap().to_string();

        let mut service = ThumbService::with_cache_dir(dir.join("cache"));
        service.request(&path);
        service.request(&path); // duplicate while pending: ignored
        assert_eq!(service.pending_set.len(), 1);

        let results = drain_until(&mut service, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, path);
        assert_eq!((results[0].1.width, results[0].1.height), (128, 64));

        let thumb = service.get(&path).expect("result must be cached");
        assert_eq!(thumb.width, 128);
        service.request(&path); // cached: not re-queued
        assert!(service.pending_set.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn service_negative_caching() {
        let dir = temp_dir("svc-neg");
        let file = dir.join("notes.txt");
        std::fs::write(&file, b"plain text".as_slice()).unwrap();
        let path = file.to_str().unwrap().to_string();

        let mut service = ThumbService::with_cache_dir(dir.join("cache"));
        service.request(&path);
        let results = drain_until(&mut service, 1);
        assert!(results.is_empty(), "text file yields no thumbnail");
        assert!(service.negative.contains(&path));

        // Negatively cached: a second request is not re-queued.
        service.request(&path);
        assert!(service.pending_set.is_empty());
        assert!(drain_until(&mut service, 1).is_empty());

        // clear_pending forgets failures so a refresh may retry.
        service.clear_pending();
        assert!(!service.negative.contains(&path));
        service.request(&path);
        assert_eq!(service.pending_set.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn service_clear_pending_drops_queue() {
        let dir = temp_dir("svc-clear");
        let file = dir.join("photo.png");
        std::fs::write(&file, fixture_png(8, 4)).unwrap();
        let path = file.to_str().unwrap().to_string();

        let mut service = ThumbService::with_cache_dir(dir.join("cache"));
        service.request(&path);
        service.clear_pending();
        assert!(service.shared.0.lock().unwrap().queue.is_empty());

        // Re-requesting after a clear works again: either the request was
        // dequeued before a worker took it and is re-queued now, or it is
        // mid-decode (still deduped) and its result arrives anyway.
        service.request(&path);
        assert_eq!(service.pending_set.len(), 1);
        let results = drain_until(&mut service, 1);
        assert_eq!(results.len(), 1);
        assert!(service.get(&path).is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn pending_overflow_drops_oldest() {
        let dir = temp_dir("flood");
        // No workers: the queue stays put, making the flood deterministic.
        let mut service = ThumbService::assemble(dir.join("cache"), 0, false);
        let total = MAX_PENDING + 4;
        for i in 0..total {
            service.request(&format!("/some/dir/{i}.png"));
        }
        assert_eq!(service.pending_set.len(), MAX_PENDING);
        {
            let shared = service.shared.0.lock().unwrap();
            assert_eq!(shared.queue.len(), MAX_PENDING);
            assert!(shared.queue.contains(&PathBuf::from("/some/dir/4.png")));
        }
        // The four oldest were dropped; the newest survived.
        for i in 0..4 {
            let dropped = format!("/some/dir/{i}.png");
            assert!(!service.pending_set.contains(Path::new(&dropped)));
        }
        let newest = format!("/some/dir/{}.png", total - 1);
        assert!(service.pending_set.contains(Path::new(&newest)));

        // A dropped path can be re-requested; it re-enters at the back and
        // evicts the next-oldest.
        service.request("/some/dir/0.png");
        assert!(service.pending_set.contains(Path::new("/some/dir/0.png")));
        assert!(!service.pending_set.contains(Path::new("/some/dir/4.png")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn disk_cache_roundtrip_survives_source_delete() {
        let dir = temp_dir("disk");
        let cache = dir.join("cache");
        let file = dir.join("cover.png");
        std::fs::write(&file, fixture_png(8, 4)).unwrap();
        let path = file.to_str().unwrap().to_string();

        let mut first = ThumbService::with_cache_dir(cache.clone());
        first.request(&path);
        assert_eq!(drain_until(&mut first, 1).len(), 1);
        drop(first);

        // Source gone: the second service must still serve the cover.
        std::fs::remove_file(&file).unwrap();
        let mut second = ThumbService::with_cache_dir(cache);
        second.request(&path);
        let results = drain_until(&mut second, 1);
        assert_eq!(results.len(), 1);
        assert_eq!((results[0].1.width, results[0].1.height), (128, 64));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn disk_cache_invalidates_on_source_change() {
        let dir = temp_dir("disk-mtime");
        let cache = dir.join("cache");
        let file = dir.join("cover.png");
        std::fs::write(&file, fixture_png(8, 4)).unwrap();
        let path = file.to_str().unwrap().to_string();

        let mut first = ThumbService::with_cache_dir(cache.clone());
        first.request(&path);
        let results = drain_until(&mut first, 1);
        assert_eq!((results[0].1.width, results[0].1.height), (128, 64));
        drop(first);

        // Rewrite with a different aspect and an explicitly bumped mtime.
        std::fs::write(&file, fixture_png(4, 8)).unwrap();
        File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(SystemTime::now() + Duration::from_secs(10))
            .unwrap();

        let mut second = ThumbService::with_cache_dir(cache);
        second.request(&path);
        let results = drain_until(&mut second, 1);
        assert_eq!(results.len(), 1);
        assert_eq!((results[0].1.width, results[0].1.height), (64, 128));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_cache_entry_is_miss_and_replaced() {
        let dir = temp_dir("disk-corrupt");
        let cache = dir.join("cache");
        let file = dir.join("photo.png");
        std::fs::write(&file, fixture_png(8, 4)).unwrap();
        let key = cache_key(&file);
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join(&key), b"garbage-not-a-thumb".as_slice()).unwrap();

        let mut service = ThumbService::with_cache_dir(cache.clone());
        let path = file.to_str().unwrap().to_string();
        service.request(&path);
        let results = drain_until(&mut service, 1);
        assert_eq!(results.len(), 1);
        assert_eq!((results[0].1.width, results[0].1.height), (128, 64));

        // The garbage entry was overwritten with a valid one.
        let bytes = std::fs::read(cache.join(&key)).unwrap();
        assert!(bytes.len() > DISK_HEADER);
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 128);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 64);
        assert_eq!(bytes.len(), DISK_HEADER + 128 * 64 * 4);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn janitor_prunes_oldest_entries() {
        let dir = temp_dir("janitor");
        let base = UNIX_EPOCH + Duration::from_secs(1_000_000);
        for i in 0..20u64 {
            let file = dir.join(format!("entry{i:02}"));
            std::fs::write(&file, b"x".as_slice()).unwrap();
            File::options()
                .write(true)
                .open(&file)
                .unwrap()
                .set_modified(base + Duration::from_secs(i))
                .unwrap();
        }
        prune_cache_dir(&dir, 5);
        let remaining = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(remaining, 5);
        for i in 15..20u64 {
            assert!(dir.join(format!("entry{i:02}")).exists());
        }
        assert!(!dir.join("entry00").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn tiny_thumb() -> Thumb {
        Thumb {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        }
    }

    #[test]
    fn cache_evicts_least_recently_used_over_count_cap() {
        let dir = temp_dir("lru-count");
        let mut service = ThumbService::with_cache_dir(dir.join("cache"));
        for i in 0..MAX_CACHED {
            service.insert(format!("p{i}"), tiny_thumb());
        }
        // Refresh p0 so p1 becomes the least-recently-used entry.
        assert!(service.get("p0").is_some());
        service.insert("overflow".into(), tiny_thumb());
        assert_eq!(service.cache.len(), MAX_CACHED);
        assert!(service.cache.contains_key("p0"));
        assert!(!service.cache.contains_key("p1"));
        assert!(service.cache.contains_key("overflow"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cache_evicts_over_byte_cap() {
        let dir = temp_dir("lru-bytes");
        let mut service = ThumbService::with_cache_dir(dir.join("cache"));
        let big = || Thumb {
            width: 256,
            height: 256,
            rgba: vec![0; 40 * 1024 * 1024],
        };
        service.insert("a".into(), big());
        service.insert("b".into(), big()); // 80 MiB > 64 MiB cap: evicts "a"
        assert!(!service.cache.contains_key("a"));
        assert!(service.cache.contains_key("b"));
        assert_eq!(service.cache_bytes, 40 * 1024 * 1024);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
