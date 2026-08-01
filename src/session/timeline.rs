//! Time-travel history: periodic architectural snapshots with delta-compressed
//! RAM (sparse keyframes + per-frame page deltas). See the module README/SPEC.

use virtual_machine::VmSnapshot;

const PAGE: usize = 4096;

enum RamStore {
    // Non-zero pages relative to a zeroed buffer of `len` bytes.
    Keyframe {
        len: usize,
        pages: Vec<(u32, Vec<u8>)>,
    },
    // Pages that differ from the immediately previous frame.
    Delta(Vec<(u32, Vec<u8>)>),
}

// Independent of the VM so encoding and replay unit-test with plain byte buffers.
struct RamLog {
    frames: Vec<RamStore>,
    // Full RAM of the newest frame; the delta base for the next push.
    prev: Vec<u8>,
    since_keyframe: usize,
    keyframe_stride: usize,
}

impl RamLog {
    fn new(keyframe_stride: usize) -> Self {
        Self {
            frames: Vec::new(),
            prev: Vec::new(),
            since_keyframe: 0,
            keyframe_stride: keyframe_stride.max(1),
        }
    }

    fn clear(&mut self) {
        self.frames.clear();
        self.prev = Vec::new();
        self.since_keyframe = 0;
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.frames.len()
    }

    fn push(&mut self, ram: &[u8]) {
        let store = if self.frames.is_empty() || self.since_keyframe >= self.keyframe_stride {
            self.since_keyframe = 0;
            RamStore::Keyframe {
                len: ram.len(),
                pages: changed_pages(&[], ram),
            }
        } else {
            self.since_keyframe += 1;
            RamStore::Delta(changed_pages(&self.prev, ram))
        };
        self.prev.clear();
        self.prev.extend_from_slice(ram);
        self.frames.push(store);
    }

    fn reconstruct(&self, i: usize) -> Option<Vec<u8>> {
        if i >= self.frames.len() {
            return None;
        }
        let mut key = i;
        while !matches!(self.frames[key], RamStore::Keyframe { .. }) {
            key = key.checked_sub(1)?;
        }
        let RamStore::Keyframe { len, pages } = &self.frames[key] else {
            return None;
        };
        let mut buf = vec![0u8; *len];
        apply_pages(&mut buf, pages);
        for frame in &self.frames[key + 1..=i] {
            if let RamStore::Delta(pages) = frame {
                apply_pages(&mut buf, pages);
            }
        }
        Some(buf)
    }

    // Rebases the chain so the next push diffs against the surviving newest frame.
    fn truncate_after(&mut self, i: usize) {
        if i + 1 >= self.frames.len() {
            return;
        }
        self.frames.truncate(i + 1);
        self.prev = self.reconstruct(i).unwrap_or_default();
        let mut key = self.frames.len() - 1;
        while key > 0 && !matches!(self.frames[key], RamStore::Keyframe { .. }) {
            key -= 1;
        }
        self.since_keyframe = self.frames.len() - 1 - key;
    }

    // Promotes the new front to a keyframe first so no surviving delta is orphaned.
    fn evict_front(&mut self) {
        if self.frames.len() < 2 {
            self.frames.clear();
            return;
        }
        if matches!(self.frames[1], RamStore::Delta(_))
            && let Some(full) = self.reconstruct(1)
        {
            self.frames[1] = RamStore::Keyframe {
                len: full.len(),
                pages: changed_pages(&[], &full),
            };
        }
        self.frames.remove(0);
    }
}

const MAX_INTERVAL_GROWTH: u64 = 64;

/// A ring of captured architectural snapshots for scrubbing through execution;
/// RAM and framebuffer planes each in a delta-compressed `RamLog`.
pub struct TimeMachine {
    ram: RamLog,
    fb: RamLog,
    lights: Vec<VmSnapshot>,
    retired: Vec<u64>,
    last_retired: Option<u64>,
    interval: u64,
    base_interval: u64,
    evicted_since_growth: usize,
    max_frames: usize,
    // Frames after the restore point are stale future history, dropped on next capture.
    pending_truncate: Option<usize>,
    enabled: bool,
}

impl TimeMachine {
    pub fn new(interval: u64, keyframe_stride: usize, max_frames: usize) -> Self {
        Self {
            ram: RamLog::new(keyframe_stride),
            fb: RamLog::new(keyframe_stride),
            lights: Vec::new(),
            retired: Vec::new(),
            last_retired: None,
            interval: interval.max(1),
            base_interval: interval.max(1),
            evicted_since_growth: 0,
            max_frames: max_frames.max(2),
            pending_truncate: None,
            enabled: false,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.clear();
        }
    }

    pub fn clear(&mut self) {
        self.ram.clear();
        self.fb.clear();
        self.lights.clear();
        self.retired.clear();
        self.last_retired = None;
        self.interval = self.base_interval;
        self.evicted_since_growth = 0;
        self.pending_truncate = None;
    }

    pub fn len(&self) -> usize {
        self.lights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lights.is_empty()
    }

    /// Retired-instruction count of frame `i`, for the scrubber tick labels.
    pub fn retired_at(&self, i: usize) -> Option<u64> {
        self.retired.get(i).copied()
    }

    /// Whether a capture is due at `retired` (enabled and past the interval).
    pub fn should_capture(&self, retired: u64) -> bool {
        self.enabled
            && self
                .last_retired
                .is_none_or(|last| retired.saturating_sub(last) >= self.interval)
    }

    /// Rebase history on restored frame `i`: the throttle restarts from its retired
    /// count and the divergent frames after it drop at the next capture, not before.
    pub fn rewind_to(&mut self, i: usize) {
        if i >= self.lights.len() {
            return;
        }
        self.last_retired = self.retired.get(i).copied();
        self.pending_truncate = Some(i);
    }

    /// Store `snap` (its RAM and framebuffer delta-compressed) as the newest frame.
    pub fn capture(&mut self, snap: &VmSnapshot, retired: u64) {
        if let Some(i) = self.pending_truncate.take() {
            self.ram.truncate_after(i);
            self.fb.truncate_after(i);
            self.lights.truncate(i + 1);
            self.retired.truncate(i + 1);
        }
        self.ram.push(snap.ram());
        self.fb.push(&snap.fb_image());
        self.lights.push(snap.without_bulk());
        self.retired.push(retired);
        self.last_retired = Some(retired);
        while self.lights.len() > self.max_frames {
            self.ram.evict_front();
            self.fb.evict_front();
            self.lights.remove(0);
            self.retired.remove(0);
            self.grow_interval_on_eviction();
        }
    }

    // Once evictions replace half the ring, double the interval (capped) so long
    // runs keep a wider, sparser window.
    fn grow_interval_on_eviction(&mut self) {
        self.evicted_since_growth += 1;
        if self.evicted_since_growth >= self.max_frames / 2 {
            self.evicted_since_growth = 0;
            self.interval = (self.interval * 2).min(self.base_interval * MAX_INTERVAL_GROWTH);
        }
    }

    /// Rebuild a restorable snapshot for frame `i`, or None if out of range.
    pub fn snapshot_at(&self, i: usize) -> Option<VmSnapshot> {
        let ram = self.ram.reconstruct(i)?;
        let fb = self.fb.reconstruct(i)?;
        Some(self.lights.get(i)?.with_bulk(ram, &fb))
    }
}

// Empty `base` means diff against zero; the tail page may be short.
fn changed_pages(base: &[u8], cur: &[u8]) -> Vec<(u32, Vec<u8>)> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    let mut page = 0u32;
    while offset < cur.len() {
        let end = (offset + PAGE).min(cur.len());
        let cur_page = &cur[offset..end];
        let differs = match base.get(offset..end) {
            Some(b) => b != cur_page,
            None => cur_page.iter().any(|&b| b != 0),
        };
        if differs {
            out.push((page, cur_page.to_vec()));
        }
        offset = end;
        page += 1;
    }
    out
}

fn apply_pages(buf: &mut [u8], pages: &[(u32, Vec<u8>)]) {
    for (idx, bytes) in pages {
        let start = *idx as usize * PAGE;
        if start < buf.len() {
            let end = (start + bytes.len()).min(buf.len());
            buf[start..end].copy_from_slice(&bytes[..end - start]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(len: usize, edits: &[(usize, u8)]) -> Vec<u8> {
        let mut v = vec![0u8; len];
        for &(i, b) in edits {
            v[i] = b;
        }
        v
    }

    #[test]
    fn ramlog_reconstructs_every_frame_across_keyframes() {
        // Stride 2 forces frequent keyframes; each frame edits a distinct byte
        // in a different page so deltas and replay both matter.
        let mut log = RamLog::new(2);
        let len = PAGE * 4;
        let frames = [
            buf(len, &[(0, 1)]),
            buf(len, &[(0, 1), (PAGE, 2)]),
            buf(len, &[(0, 1), (PAGE, 2), (PAGE * 2, 3)]),
            buf(len, &[(0, 9), (PAGE, 2), (PAGE * 3, 4)]),
        ];
        for f in &frames {
            log.push(f);
        }
        for (i, expected) in frames.iter().enumerate() {
            assert_eq!(log.reconstruct(i).as_ref(), Some(expected), "frame {i}");
        }
    }

    #[test]
    fn ramlog_survives_eviction_of_a_keyframe() {
        let mut log = RamLog::new(2);
        let len = PAGE * 2;
        let frames: Vec<Vec<u8>> = (0..6).map(|i| buf(len, &[(0, i as u8)])).collect();
        for f in &frames {
            log.push(f);
        }
        // Evict the two oldest; remaining frames must still reconstruct.
        log.evict_front();
        log.evict_front();
        assert_eq!(log.len(), 4);
        // Frame index 0 now corresponds to original frame 2.
        assert_eq!(log.reconstruct(0).as_ref(), Some(&frames[2]));
        assert_eq!(log.reconstruct(3).as_ref(), Some(&frames[5]));
    }

    #[test]
    fn ramlog_truncate_after_rebases_the_delta_chain() {
        let mut log = RamLog::new(4);
        let len = PAGE * 2;
        let frames: Vec<Vec<u8>> = (0..5).map(|i| buf(len, &[(0, i as u8 + 1)])).collect();
        for f in &frames {
            log.push(f);
        }
        log.truncate_after(1);
        assert_eq!(log.len(), 2);
        assert_eq!(log.reconstruct(1).as_ref(), Some(&frames[1]));
        // A new frame must delta against surviving frame 1, not dropped frame 4.
        let divergent = buf(len, &[(0, 9), (PAGE, 7)]);
        log.push(&divergent);
        assert_eq!(log.reconstruct(2).as_ref(), Some(&divergent));
        assert_eq!(log.reconstruct(1).as_ref(), Some(&frames[1]));
    }

    #[test]
    fn interval_doubles_per_half_ring_of_evictions_and_caps() {
        let mut tm = TimeMachine::new(100, 8, 8);
        assert_eq!(tm.interval, 100);
        // Half the ring (4 evictions) doubles the interval once.
        for _ in 0..4 {
            tm.grow_interval_on_eviction();
        }
        assert_eq!(tm.interval, 200);
        // Growth is capped at MAX_INTERVAL_GROWTH times the base.
        for _ in 0..1000 {
            tm.grow_interval_on_eviction();
        }
        assert_eq!(tm.interval, 100 * MAX_INTERVAL_GROWTH);
        // Clearing resets the cadence to the base interval.
        tm.clear();
        assert_eq!(tm.interval, 100);
    }

    #[test]
    fn changed_pages_detects_only_touched_pages() {
        let a = buf(PAGE * 3, &[(0, 1), (PAGE * 2, 5)]);
        let b = buf(PAGE * 3, &[(0, 1), (PAGE, 7), (PAGE * 2, 5)]);
        let diff = changed_pages(&a, &b);
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].0, 1);
    }
}
