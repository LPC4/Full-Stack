//! Configurable set-associative cache with true LRU replacement.
//! Write-back / write-allocate policy for data caches.
//! Multi-byte accesses count one stat per unique cache block, not one per byte.

use crate::virtual_machine::error::VmError;
use crate::virtual_machine::memory::MemoryAccess;

pub struct CacheParams {
    pub size: usize,
    pub block_size: usize,
    pub associativity: usize,
    pub write_back: bool,
    pub read_only: bool,
}

struct CacheLine {
    valid: bool,
    dirty: bool,
    tag: u64,
    data: Vec<u8>,
    lru_age: u64,
}

struct CacheSet {
    ways: Vec<CacheLine>,
}

pub struct Cache<Next: MemoryAccess> {
    params: CacheParams,
    sets: Vec<CacheSet>,
    next: Next,
    stats: CacheStats,
    block_bits: u32,
    set_bits: u32,
    access_tick: u64,
}

#[derive(Default, Debug, Clone)]
pub struct CacheStats {
    pub read_hits: u64,
    pub read_misses: u64,
    pub write_hits: u64,
    pub write_misses: u64,
}

impl<Next: MemoryAccess> Cache<Next> {
    pub fn new(params: CacheParams, next: Next) -> Self {
        let block_bits = params.block_size.ilog2();
        let sets = params.size / (params.block_size * params.associativity);
        let set_bits = sets.ilog2();

        let sets_vec = (0..sets)
            .map(|_| CacheSet {
                ways: (0..params.associativity)
                    .map(|_| CacheLine {
                        valid: false,
                        dirty: false,
                        tag: 0,
                        data: vec![0u8; params.block_size],
                        lru_age: 0,
                    })
                    .collect(),
            })
            .collect();

        Self {
            params,
            sets: sets_vec,
            next,
            stats: CacheStats::default(),
            block_bits,
            set_bits,
            access_tick: 0,
        }
    }

    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    pub fn peek_next(&self) -> &Next {
        &self.next
    }

    fn address_fields(&self, addr: u64) -> (u64, u64, u64) {
        let tag = addr >> (self.block_bits + self.set_bits);
        let index = (addr >> self.block_bits) & ((1u64 << self.set_bits) - 1);
        let offset = addr & ((1u64 << self.block_bits) - 1);
        (tag, index, offset)
    }

    fn line_base_address(&self, tag: u64, index: u64) -> u64 {
        (tag << (self.block_bits + self.set_bits)) | (index << self.block_bits)
    }

    /// Resolve an address to a (set_idx, way_idx, was_miss) tuple.
    /// On a miss the block is fetched from the next level; dirty victims are written back.
    /// On a hit or after a fill, the LRU age of the line is updated.
    fn resolve_line(&mut self, addr: u64) -> Result<(usize, usize, bool), VmError> {
        let (tag, index, _) = self.address_fields(addr);
        let set_idx = index as usize;

        // Hit?
        let hit_way = self.sets[set_idx]
            .ways
            .iter()
            .position(|w| w.valid && w.tag == tag);
        if let Some(way_idx) = hit_way {
            self.access_tick += 1;
            self.sets[set_idx].ways[way_idx].lru_age = self.access_tick;
            return Ok((set_idx, way_idx, false));
        }

        // Miss: choose victim -- first invalid way, else true LRU (smallest age)
        let victim_idx = {
            let set = &self.sets[set_idx];
            set.ways
                .iter()
                .position(|w| !w.valid)
                .unwrap_or_else(|| {
                    set.ways
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, w)| w.lru_age)
                        .map(|(i, _)| i)
                        .unwrap_or(0)
                })
        };

        // Write back dirty victim
        let need_evict = {
            let line = &self.sets[set_idx].ways[victim_idx];
            line.valid && line.dirty
        };
        if need_evict {
            let (victim_tag, victim_data) = {
                let line = &self.sets[set_idx].ways[victim_idx];
                (line.tag, line.data.clone())
            };
            let base_old = self.line_base_address(victim_tag, index);
            for (i, &byte) in victim_data.iter().enumerate() {
                self.next.write_byte(base_old + i as u64, byte)?;
            }
        }

        // Fetch new block from the next level
        let block_mask = !(self.params.block_size as u64 - 1);
        let fetch_base = addr & block_mask;
        let mut new_data = vec![0u8; self.params.block_size];
        for i in 0..self.params.block_size {
            new_data[i] = self.next.read_byte(fetch_base + i as u64)?;
        }

        // Install and assign LRU age
        self.access_tick += 1;
        {
            let line = &mut self.sets[set_idx].ways[victim_idx];
            line.valid = true;
            line.dirty = false;
            line.tag = tag;
            line.data = new_data;
            line.lru_age = self.access_tick;
        }

        Ok((set_idx, victim_idx, true))
    }

    // ---------------------------------------------------------------------------
    // Internal multi-byte helpers -- count one stat per unique cache block touched
    // ---------------------------------------------------------------------------

    fn read_n(&mut self, addr: u64, n: usize) -> Result<u64, VmError> {
        let block_mask = !((1u64 << self.block_bits) - 1);
        let mut result = 0u64;
        let mut last_block = u64::MAX;
        let mut last_set = 0usize;
        let mut last_way = 0usize;

        for i in 0..n {
            let byte_addr = addr + i as u64;
            let block = byte_addr & block_mask;

            let (set_idx, way_idx) = if block != last_block {
                let (si, wi, miss) = self.resolve_line(byte_addr)?;
                if miss {
                    self.stats.read_misses += 1;
                } else {
                    self.stats.read_hits += 1;
                }
                last_block = block;
                last_set = si;
                last_way = wi;
                (si, wi)
            } else {
                (last_set, last_way)
            };

            let offset = (byte_addr & !block_mask) as usize;
            result |= (self.sets[set_idx].ways[way_idx].data[offset] as u64) << (i * 8);
        }
        Ok(result)
    }

    fn write_n(&mut self, addr: u64, data: u64, n: usize) -> Result<(), VmError> {
        if self.params.read_only {
            return Err(VmError::WriteToRom);
        }

        let block_mask = !((1u64 << self.block_bits) - 1);
        let mut last_block = u64::MAX;
        let mut last_set = 0usize;
        let mut last_way = 0usize;

        for i in 0..n {
            let byte_addr = addr + i as u64;
            let block = byte_addr & block_mask;

            let (set_idx, way_idx) = if block != last_block {
                let (si, wi, miss) = self.resolve_line(byte_addr)?;
                if miss {
                    self.stats.write_misses += 1;
                } else {
                    self.stats.write_hits += 1;
                }
                last_block = block;
                last_set = si;
                last_way = wi;
                (si, wi)
            } else {
                (last_set, last_way)
            };

            let offset = (byte_addr & !block_mask) as usize;
            let byte = ((data >> (i * 8)) & 0xFF) as u8;
            self.sets[set_idx].ways[way_idx].data[offset] = byte;

            if self.params.write_back {
                self.sets[set_idx].ways[way_idx].dirty = true;
            } else {
                self.next.write_byte(byte_addr, byte)?;
            }
        }
        Ok(())
    }
}

impl<Next: MemoryAccess> MemoryAccess for Cache<Next> {
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError> {
        self.read_n(addr, 1).map(|v| v as u8)
    }

    fn read_halfword(&mut self, addr: u64) -> Result<u16, VmError> {
        self.read_n(addr, 2).map(|v| v as u16)
    }

    fn read_word(&mut self, addr: u64) -> Result<u32, VmError> {
        self.read_n(addr, 4).map(|v| v as u32)
    }

    fn read_doubleword(&mut self, addr: u64) -> Result<u64, VmError> {
        self.read_n(addr, 8)
    }

    fn write_byte(&mut self, addr: u64, data: u8) -> Result<(), VmError> {
        self.write_n(addr, data as u64, 1)
    }

    fn write_halfword(&mut self, addr: u64, data: u16) -> Result<(), VmError> {
        self.write_n(addr, data as u64, 2)
    }

    fn write_word(&mut self, addr: u64, data: u32) -> Result<(), VmError> {
        self.write_n(addr, data as u64, 4)
    }

    fn write_doubleword(&mut self, addr: u64, data: u64) -> Result<(), VmError> {
        self.write_n(addr, data, 8)
    }
}

// ---------------------------------------------------------------------------
// Snapshot types for the debug UI
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct CacheParamsSnapshot {
    pub size: usize,
    pub block_size: usize,
    pub associativity: usize,
    pub write_back: bool,
    pub read_only: bool,
}

#[derive(Clone, Debug)]
pub struct CacheLineSnapshot {
    pub valid: bool,
    pub dirty: bool,
    pub tag: u64,
}

#[derive(Clone, Debug, Default)]
pub struct CacheSnapshot {
    pub params: CacheParamsSnapshot,
    pub sets: Vec<Vec<CacheLineSnapshot>>,
    pub stats: CacheStats,
}

impl<Next: MemoryAccess> Cache<Next> {
    pub fn snapshot(&self) -> CacheSnapshot {
        let sets = self
            .sets
            .iter()
            .map(|s| {
                s.ways
                    .iter()
                    .map(|w| CacheLineSnapshot {
                        valid: w.valid,
                        dirty: w.dirty,
                        tag: w.tag,
                    })
                    .collect()
            })
            .collect();

        CacheSnapshot {
            params: CacheParamsSnapshot {
                size: self.params.size,
                block_size: self.params.block_size,
                associativity: self.params.associativity,
                write_back: self.params.write_back,
                read_only: self.params.read_only,
            },
            sets,
            stats: self.stats.clone(),
        }
    }
}
