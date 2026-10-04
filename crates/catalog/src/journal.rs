//! Crash-safe catalog persistence: an append-only op log plus periodic snapshots.
//!
//! Files (in a [`Store`]):
//! - `catalog.snap` — `{"format":"lightcraft-catalog","version":1,"seq":N,"catalog":{…}}`, the
//!   state after op `N`; replaced atomically (temp + fsync + rename).
//! - `catalog.log` — JSON lines, one per op applied after the snapshot:
//!   `{"seq":N,"crc":C,"op":{…}}` where `C` is the CRC-32 of the op's JSON text. Appends are
//!   fsynced before [`Journal::append`] returns.
//!
//! Loading = snapshot + replay of the records with `seq > snapshot seq`. Recovery rules:
//! - a **torn final record** (crash mid-append: truncated line or bad CRC at the end) is dropped
//!   and the file is cut back to the last good record, so later appends start on a clean line;
//! - records already covered by the snapshot (crash between writing the snapshot and resetting
//!   the log) are skipped;
//! - a bad record **followed by good ones** is real damage: replay stops there, the log is kept
//!   as `catalog.log.damaged-<seq>`, and a fresh snapshot is written so the library stays usable.

use crate::store::Store;
use crate::{Catalog, CatalogError, Op, Result};

pub const SNAPSHOT: &str = "catalog.snap";
pub const LOG: &str = "catalog.log";
const FORMAT: &str = "lightcraft-catalog";
const VERSION: u32 = 1;

/// When [`Journal::wants_snapshot`] says it's time to compact the log.
#[derive(Clone, Copy, Debug)]
pub struct SnapshotPolicy {
    pub max_records: u64,
    pub max_bytes: u64,
}

impl Default for SnapshotPolicy {
    fn default() -> Self {
        SnapshotPolicy { max_records: 2000, max_bytes: 16 << 20 }
    }
}

/// What happened while loading.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadReport {
    /// `seq` of the snapshot that was loaded (0 = none).
    pub snapshot_seq: u64,
    /// Log records applied on top of the snapshot.
    pub replayed: usize,
    /// Records skipped because the snapshot already contained them.
    pub stale: usize,
    /// Records whose op failed to apply (should never happen; reported, not fatal).
    pub failed: usize,
    /// Bytes of a torn final record that were dropped.
    pub torn_bytes: u64,
    /// The damaged log was preserved under this name.
    pub damaged: Option<String>,
    /// No catalog files existed (a new library).
    pub created: bool,
}

/// Where persistence time goes (reported by `library.info` → `persistence`; printed to stderr
/// per write under `LIGHTCRAFT_PROFILE`). Times are wall-clock milliseconds on the calling
/// thread, i.e. how long the caller (the UI thread, for the app) was blocked.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistStats {
    /// [`Journal::append`] calls that wrote something.
    pub appends: u64,
    /// Encode + write + `sync_data` of the last / slowest append.
    pub last_append_ms: f64,
    pub max_append_ms: f64,
    /// Snapshots written (compactions, plus the ones on close / repair).
    pub snapshots: u64,
    /// The last snapshot, by stage.
    pub last_snapshot: SnapshotTiming,
    /// Total time of the slowest snapshot.
    pub max_snapshot_ms: f64,
}

/// One snapshot (compaction), by stage.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotTiming {
    /// Serialising the catalog to JSON.
    pub serialize_ms: f64,
    /// Writing, flushing, `sync_all` and the atomic rename of `catalog.snap`.
    pub write_sync_ms: f64,
    /// Resetting `catalog.log` (an atomic rewrite to empty, fsynced).
    pub reset_ms: f64,
    pub total_ms: f64,
    /// Size of `catalog.snap`.
    pub bytes: u64,
    /// Log records the snapshot compacted.
    pub records: u64,
}

/// `LIGHTCRAFT_PROFILE` is set: print persistence timings to stderr.
fn profiling() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LIGHTCRAFT_PROFILE").is_some())
}

fn ms_since(t: web_time::Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

pub struct Journal {
    store: Box<dyn Store>,
    /// `seq` of the last durable op.
    seq: u64,
    snapshot_seq: u64,
    log_records: u64,
    log_bytes: u64,
    pub policy: SnapshotPolicy,
    stats: PersistStats,
}

fn io(e: std::io::Error) -> CatalogError {
    CatalogError::Io(e.to_string())
}

/// One log line (without the newline) for op number `seq`.
pub fn encode_record(seq: u64, op: &Op) -> String {
    let body = serde_json::to_string(op).unwrap_or_default();
    let crc = crc32fast::hash(body.as_bytes());
    format!("{{\"seq\":{seq},\"crc\":{crc},\"op\":{body}}}")
}

/// Parse one log line (without the newline). `None` if it's malformed or the CRC doesn't match.
pub fn decode_record(line: &str) -> Option<(u64, Op)> {
    let rest = line.strip_prefix("{\"seq\":")?;
    let (seq, rest) = rest.split_once(",\"crc\":")?;
    let (crc, rest) = rest.split_once(",\"op\":")?;
    let body = rest.strip_suffix('}')?;
    let seq: u64 = seq.parse().ok()?;
    let crc: u32 = crc.parse().ok()?;
    if crc32fast::hash(body.as_bytes()) != crc {
        return None;
    }
    serde_json::from_str(body).ok().map(|op| (seq, op))
}

#[derive(serde::Deserialize)]
struct SnapFile {
    format: String,
    version: u32,
    seq: u64,
    catalog: Catalog,
}

impl Journal {
    /// Open (or create) the catalog in `store`: load the snapshot, replay the log, repair a torn
    /// tail. Fails only if the snapshot itself is unreadable (then nothing is modified).
    pub fn open(mut store: Box<dyn Store>) -> Result<(Journal, Catalog, LoadReport)> {
        let mut report = LoadReport::default();
        let snap = store.read(SNAPSHOT).map_err(io)?;
        let log = store.read(LOG).map_err(io)?;
        report.created = snap.is_none() && log.is_none();
        let (mut catalog, snapshot_seq) = match snap {
            Some(bytes) => {
                let s: SnapFile = serde_json::from_slice(&bytes).map_err(|e| CatalogError::Corrupt(format!("{SNAPSHOT}: {e}")))?;
                if s.format != FORMAT || s.version > VERSION {
                    return Err(CatalogError::Corrupt(format!("{SNAPSHOT}: unsupported format {} v{}", s.format, s.version)));
                }
                (s.catalog, s.seq)
            }
            None => (Catalog::new(), 0),
        };
        report.snapshot_seq = snapshot_seq;
        let mut j = Journal { store, seq: snapshot_seq, snapshot_seq, log_records: 0, log_bytes: 0, policy: SnapshotPolicy::default(), stats: PersistStats::default() };
        let log = log.unwrap_or_default();

        // Scan records; `good_end` is the byte offset just past the last good record.
        let mut pos = 0usize;
        let mut good_end = 0usize;
        let mut damaged_at: Option<usize> = None;
        let mut needs_newline = false;
        while pos < log.len() {
            let (line_end, next, has_nl) = match log[pos..].iter().position(|b| *b == b'\n') {
                Some(i) => (pos + i, pos + i + 1, true),
                None => (log.len(), log.len(), false),
            };
            let line = std::str::from_utf8(&log[pos..line_end]).ok().map(|l| l.trim_end_matches('\r'));
            if line.is_some_and(|l| l.trim().is_empty()) {
                pos = next;
                good_end = next;
                continue;
            }
            match line.and_then(decode_record) {
                Some((seq, op)) if seq <= j.seq => {
                    // already in the snapshot
                    let _ = op;
                    report.stale += 1;
                }
                Some((seq, op)) if seq == j.seq + 1 => {
                    if catalog.apply(op).is_err() {
                        report.failed += 1;
                    } else {
                        report.replayed += 1;
                    }
                    j.seq = seq;
                }
                _ => {
                    // bad record (or a gap): torn tail if nothing good follows, else damage
                    let rest_has_good = log[next..].split(|b| *b == b'\n').any(|l| std::str::from_utf8(l).ok().and_then(decode_record).is_some());
                    if rest_has_good {
                        damaged_at = Some(pos);
                    }
                    break;
                }
            }
            j.log_records += 1;
            pos = next;
            good_end = next;
            needs_newline = !has_nl;
        }

        if let Some(at) = damaged_at {
            let name = format!("{LOG}.damaged-{}", j.seq);
            j.store.write_atomic(&name, &log).map_err(io)?;
            log::warn!("catalog log damaged at byte {at}; kept as {name}; state recovered up to op {}", j.seq);
            report.damaged = Some(name);
            j.snapshot(&catalog)?;
        } else {
            if good_end < log.len() {
                report.torn_bytes = (log.len() - good_end) as u64;
                log::warn!("catalog log: dropped a torn final record ({} bytes)", report.torn_bytes);
                j.store.truncate(LOG, good_end as u64).map_err(io)?;
            } else if needs_newline {
                j.store.append(LOG, b"\n").map_err(io)?;
            }
            j.log_bytes = good_end as u64;
        }
        catalog.revision = 0;
        Ok((j, catalog, report))
    }

    /// Append ops (already applied to the live catalog) durably, in order.
    pub fn append(&mut self, ops: &[Op]) -> Result<()> {
        if ops.is_empty() {
            return Ok(());
        }
        let t0 = web_time::Instant::now();
        let mut buf = String::new();
        let mut seq = self.seq;
        for op in ops {
            seq += 1;
            buf.push_str(&encode_record(seq, op));
            buf.push('\n');
        }
        self.store.append(LOG, buf.as_bytes()).map_err(io)?;
        self.seq = seq;
        self.log_records += ops.len() as u64;
        self.log_bytes += buf.len() as u64;
        let ms = ms_since(t0);
        self.stats.appends += 1;
        self.stats.last_append_ms = ms;
        self.stats.max_append_ms = self.stats.max_append_ms.max(ms);
        if profiling() {
            eprintln!("catalog: append {} op(s), {} B: {ms:.2} ms", ops.len(), buf.len());
        }
        Ok(())
    }

    /// Write a snapshot of `catalog` (which must reflect every appended op) and reset the log.
    pub fn snapshot(&mut self, catalog: &Catalog) -> Result<()> {
        let t0 = web_time::Instant::now();
        let body = serde_json::to_string(catalog).map_err(|e| CatalogError::Invalid(e.to_string()))?;
        let file = format!("{{\"format\":\"{FORMAT}\",\"version\":{VERSION},\"seq\":{},\"catalog\":{body}}}\n", self.seq);
        let serialize_ms = ms_since(t0);
        let t1 = web_time::Instant::now();
        self.store.write_atomic(SNAPSHOT, file.as_bytes()).map_err(io)?;
        let write_sync_ms = ms_since(t1);
        let t2 = web_time::Instant::now();
        // A crash here leaves old records in the log; they are skipped by seq on load.
        self.store.write_atomic(LOG, b"").map_err(io)?;
        let reset_ms = ms_since(t2);
        let timing =
            SnapshotTiming { serialize_ms, write_sync_ms, reset_ms, total_ms: ms_since(t0), bytes: file.len() as u64, records: self.log_records };
        self.record_snapshot(timing);
        self.snapshot_seq = self.seq;
        self.log_records = 0;
        self.log_bytes = 0;
        Ok(())
    }

    fn record_snapshot(&mut self, t: SnapshotTiming) {
        self.stats.snapshots += 1;
        self.stats.last_snapshot = t;
        self.stats.max_snapshot_ms = self.stats.max_snapshot_ms.max(t.total_ms);
        if profiling() {
            eprintln!(
                "catalog: snapshot of {} records, {} B: serialize {:.1} ms, write+sync {:.1} ms, log reset {:.1} ms, total {:.1} ms",
                t.records, t.bytes, t.serialize_ms, t.write_sync_ms, t.reset_ms, t.total_ms
            );
        }
    }

    /// Where persistence time went so far.
    pub fn stats(&self) -> PersistStats {
        self.stats
    }

    /// The log is long enough to be worth compacting.
    pub fn wants_snapshot(&self) -> bool {
        self.log_records >= self.policy.max_records || self.log_bytes >= self.policy.max_bytes
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }
    pub fn snapshot_seq(&self) -> u64 {
        self.snapshot_seq
    }
    pub fn log_records(&self) -> u64 {
        self.log_records
    }
    pub fn log_bytes(&self) -> u64 {
        self.log_bytes
    }
    pub fn describe(&self) -> String {
        self.store.describe()
    }
}
