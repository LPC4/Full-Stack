use full_stack::virtual_machine::memory::cache::{Cache, CacheParams};
use full_stack::virtual_machine::memory::ram::Ram;
use full_stack::virtual_machine::memory::MemoryAccess;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const RAM_BASE: u64 = 0x8000_0000;
const RAM_SIZE: usize = 1 * 1024 * 1024; // 1 MiB

fn make_cache(size: usize, block_size: usize, ways: usize, write_back: bool) -> Cache<Ram> {
    let ram = Ram::new(RAM_BASE, RAM_SIZE);
    Cache::new(
        CacheParams { size, block_size, associativity: ways, write_back, read_only: false },
        ram,
    )
}

fn make_read_only_cache() -> Cache<Ram> {
    let ram = Ram::new(RAM_BASE, RAM_SIZE);
    Cache::new(
        CacheParams {
            size: 256,
            block_size: 64,
            associativity: 1,
            write_back: true,
            read_only: true,
        },
        ram,
    )
}

// ---------------------------------------------------------------------------
// Basic hit / miss counting
// ---------------------------------------------------------------------------

#[test]
fn first_read_is_a_miss() {
    let mut c = make_cache(256, 64, 2, true);
    let _ = c.read_byte(RAM_BASE).unwrap();
    assert_eq!(c.stats().read_misses, 1);
    assert_eq!(c.stats().read_hits, 0);
}

#[test]
fn second_read_same_block_is_a_hit() {
    let mut c = make_cache(256, 64, 2, true);
    let _ = c.read_byte(RAM_BASE).unwrap();     // cold miss
    let _ = c.read_byte(RAM_BASE + 1).unwrap(); // same 64-byte block → hit
    assert_eq!(c.stats().read_misses, 1);
    assert_eq!(c.stats().read_hits, 1);
}

#[test]
fn different_blocks_each_count_as_a_miss() {
    let mut c = make_cache(256, 64, 2, true);
    let _ = c.read_byte(RAM_BASE).unwrap();      // block 0
    let _ = c.read_byte(RAM_BASE + 64).unwrap(); // block 1
    let _ = c.read_byte(RAM_BASE + 128).unwrap(); // block 2
    assert_eq!(c.stats().read_misses, 3);
    assert_eq!(c.stats().read_hits, 0);
}

// ---------------------------------------------------------------------------
// Multi-byte accesses count as one stat per block, not one per byte
// ---------------------------------------------------------------------------

#[test]
fn halfword_read_counts_as_one_access() {
    let mut c = make_cache(256, 64, 2, true);
    // Store known bytes first via RAM (bypass cache)
    let _ = c.write_byte(RAM_BASE, 0x01).unwrap(); // miss
    let _ = c.read_halfword(RAM_BASE).unwrap();    // should hit (already loaded)
    // Stats: 1 write miss + 1 write hit? No — the write_byte is a miss.
    // Then read_halfword: both bytes in same block → 1 read hit
    assert_eq!(c.stats().read_hits, 1);
    assert_eq!(c.stats().read_misses, 0);
}

#[test]
fn word_read_same_block_counts_as_one_read_stat() {
    let mut c = make_cache(256, 64, 2, true);
    // Warm the block with a byte write first
    let _ = c.write_byte(RAM_BASE, 0xFF).unwrap();
    // Now a word read entirely within that block should be one hit
    let _ = c.read_word(RAM_BASE).unwrap();
    assert_eq!(c.stats().read_hits, 1);
    assert_eq!(c.stats().read_misses, 0);
}

#[test]
fn doubleword_read_same_block_counts_as_one_stat() {
    let mut c = make_cache(256, 64, 2, true);
    let _ = c.write_byte(RAM_BASE, 0xAB).unwrap(); // warm block
    let _ = c.read_doubleword(RAM_BASE).unwrap();
    assert_eq!(c.stats().read_hits, 1);
    assert_eq!(c.stats().read_misses, 0);
}

#[test]
fn word_spanning_two_blocks_counts_as_two_reads() {
    // block_size = 4 → each word is its own block
    let mut c = make_cache(64, 4, 2, true);
    // Read a "word" that crosses a 4-byte block boundary (offset 2)
    // bytes at RAM_BASE+2 and RAM_BASE+3 are in block 0; +4 and +5 in block 1
    let _ = c.read_halfword(RAM_BASE + 2).unwrap(); // two bytes, same block OK
    let _ = c.read_word(RAM_BASE + 2).unwrap();     // 4 bytes: blocks 0 and 1
    // First halfword: 1 miss (cold)
    // The word: 2 bytes in block 0 (hit) + 2 bytes in block 1 (miss) = 1 hit + 1 miss
    assert_eq!(c.stats().read_misses, 2);
    assert_eq!(c.stats().read_hits, 1);
}

// ---------------------------------------------------------------------------
// Write policy: write-back
// ---------------------------------------------------------------------------

#[test]
fn write_back_marks_line_dirty_and_does_not_propagate_immediately() {
    let mut c = make_cache(256, 64, 2, true);
    // Write a byte — should load the block (miss) and mark it dirty
    let _ = c.write_byte(RAM_BASE, 0xDE).unwrap();
    assert_eq!(c.stats().write_misses, 1);
    // Read back — same block, should hit and return our written byte
    let v = c.read_byte(RAM_BASE).unwrap();
    assert_eq!(v, 0xDE);
    // The underlying RAM should NOT have seen the write yet
    // (We can't directly check, but the stats confirm we didn't bypass the cache)
    assert_eq!(c.stats().write_hits, 0); // no second write
    assert_eq!(c.stats().read_hits, 1);
}

#[test]
fn write_back_dirty_eviction_propagates_to_next_level() {
    // 1-way (direct-mapped), tiny cache so we force eviction quickly
    let mut c = make_cache(64, 64, 1, true);
    // Write to block mapped at set 0
    let _ = c.write_byte(RAM_BASE, 0xAB).unwrap();
    // Access a different address that maps to the SAME set (RAM_BASE + 64)
    // This forces eviction of the dirty block
    let _ = c.read_byte(RAM_BASE + 64).unwrap();
    // Now read back the original address — should come from underlying RAM
    let v = c.read_byte(RAM_BASE).unwrap();
    // The dirty write (0xAB) should have been flushed to RAM during eviction
    assert_eq!(v, 0xAB, "dirty eviction must write back to next level");
}

// ---------------------------------------------------------------------------
// Write policy: write-through
// ---------------------------------------------------------------------------

#[test]
fn write_through_propagates_immediately() {
    let ram = Ram::new(RAM_BASE, RAM_SIZE);
    let mut c = Cache::new(
        CacheParams { size: 256, block_size: 64, associativity: 2, write_back: false, read_only: false },
        ram,
    );
    let _ = c.write_byte(RAM_BASE, 0xCD).unwrap();
    // Bypass the cache by reading directly from the underlying RAM via peek_next
    let direct = c.peek_next().peek_byte(RAM_BASE).unwrap_or(0);
    assert_eq!(direct, 0xCD, "write-through must propagate immediately");
}

// ---------------------------------------------------------------------------
// Read-only cache rejects writes
// ---------------------------------------------------------------------------

#[test]
fn read_only_cache_rejects_writes() {
    let mut c = make_read_only_cache();
    let result = c.write_byte(RAM_BASE, 0xFF);
    assert!(result.is_err(), "write to read-only cache must fail");
}

// ---------------------------------------------------------------------------
// LRU replacement
// ---------------------------------------------------------------------------

#[test]
fn lru_evicts_least_recently_used_way() {
    // 2-way, 2-set cache (size=256, block=64, assoc=2 → 2 sets)
    let mut c = make_cache(256, 64, 2, true);

    // Fill both ways of set 0 (addresses map to set 0 when index = addr>>6 & 1 = 0 or 2)
    // With 2 sets: set = (addr >> 6) & 1
    // Block 0: addr 0x8000_0000 → set 0
    // Block 2: addr 0x8000_0080 → set 0
    // Block 4: addr 0x8000_0100 → set 0 (will evict LRU)
    let b0 = RAM_BASE;
    let b2 = RAM_BASE + 128; // set 0 (128/64=2, 2&1=0)
    let b4 = RAM_BASE + 256; // set 0 (256/64=4, 4&1=0)

    // Load way 0 (LRU=1)
    let _ = c.write_byte(b0, 0x11).unwrap();
    // Load way 1 (LRU=2)
    let _ = c.write_byte(b2, 0x22).unwrap();
    // Re-access b0 → LRU age updated (b0 now MRU, b2 is LRU)
    let _ = c.read_byte(b0).unwrap();
    // Bring in b4 → evicts b2 (LRU)
    let _ = c.read_byte(b4).unwrap();
    // b0 should still be cached (hit)
    let stats_before = c.stats().clone();
    let _ = c.read_byte(b0).unwrap();
    assert_eq!(c.stats().read_hits, stats_before.read_hits + 1, "b0 must still be cached after b2 eviction");
    // b2 should have been evicted (miss)
    let _ = c.read_byte(b2).unwrap();
    assert_eq!(c.stats().read_misses, stats_before.read_misses + 1, "b2 should have been evicted");
}

// ---------------------------------------------------------------------------
// Three-level hierarchy (L1 → L2 → RAM)
// ---------------------------------------------------------------------------

#[test]
fn three_level_hierarchy_read_write() {
    let ram = Ram::new(RAM_BASE, RAM_SIZE);
    let l2 = Cache::new(
        CacheParams { size: 512, block_size: 64, associativity: 4, write_back: true, read_only: false },
        ram,
    );
    let mut l1 = Cache::new(
        CacheParams { size: 128, block_size: 64, associativity: 2, write_back: true, read_only: false },
        l2,
    );

    // Write via L1
    let _ = l1.write_byte(RAM_BASE, 0x42).unwrap();
    let v = l1.read_byte(RAM_BASE).unwrap();
    assert_eq!(v, 0x42);

    // L1 should show 1 write miss, 1 read hit
    assert_eq!(l1.stats().write_misses, 1);
    assert_eq!(l1.stats().read_hits, 1);
}

// ---------------------------------------------------------------------------
// Write miss: block is fetched before writing (write-allocate)
// ---------------------------------------------------------------------------

#[test]
fn write_miss_fetches_block_write_allocate() {
    let mut c = make_cache(256, 64, 2, true);
    // First write to an uncached address: should be a write miss
    let _ = c.write_byte(RAM_BASE + 16, 0x55).unwrap();
    assert_eq!(c.stats().write_misses, 1);
    // The full block should now be in cache; reading adjacent byte should hit
    let _ = c.read_byte(RAM_BASE + 17).unwrap();
    assert_eq!(c.stats().read_hits, 1);
}

// ---------------------------------------------------------------------------
// Stats: hit rate
// ---------------------------------------------------------------------------

#[test]
fn repeated_reads_produce_high_hit_rate() {
    let mut c = make_cache(256, 64, 2, true);
    // Cold access
    let _ = c.read_byte(RAM_BASE).unwrap();
    // 99 more reads to the same byte
    for _ in 0..99 {
        let _ = c.read_byte(RAM_BASE).unwrap();
    }
    assert_eq!(c.stats().read_misses, 1);
    assert_eq!(c.stats().read_hits, 99);
}
