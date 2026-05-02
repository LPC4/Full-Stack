//! Configurable set‑associative cache – models L1I, L1D, L2, L3.
//! Write‑back / write‑allocate policy for data caches.

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
}

#[derive(Default, Debug)]
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

        let mut sets_vec = Vec::with_capacity(sets);
        for _ in 0..sets {
            let ways = (0..params.associativity)
                .map(|_| CacheLine {
                    valid: false,
                    dirty: false,
                    tag: 0,
                    data: vec![0u8; params.block_size],
                })
                .collect();
            sets_vec.push(CacheSet { ways });
        }

        Self {
            params,
            sets: sets_vec,
            next,
            stats: CacheStats::default(),
            block_bits,
            set_bits,
        }
    }

    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Peek at the underlying memory without affecting cache state or statistics.
    /// This is useful for debugging/inspection purposes.
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

    /// Resolve an address: return the index of the matching line and whether a
    /// miss occurred (and was handled). On miss, the line is fetched from the
    /// next level and any dirty victim is written back.
    fn resolve_line(&mut self, addr: u64) -> Result<(usize, usize, bool), VmError> {
        let (tag, index, _) = self.address_fields(addr);
        let set_idx = index as usize;

        // Hit?
        {
            let set = &self.sets[set_idx];
            if let Some(way_idx) = set.ways.iter().position(|w| w.valid && w.tag == tag) {
                return Ok((set_idx, way_idx, false));
            }
        }

        // Miss, choose victim (first invalid way, otherwise way 0)
        let victim_idx = {
            let set = &self.sets[set_idx];
            set.ways.iter().position(|w| !w.valid).unwrap_or(0)
        };

        // Evict if dirty
        let need_evict = {
            let set = &self.sets[set_idx];
            set.ways[victim_idx].valid && set.ways[victim_idx].dirty
        };

        if need_evict {
            // Copy out the dirty line to avoid borrowing conflicts
            let (victim_tag, victim_data) = {
                let set = &self.sets[set_idx];
                let line = &set.ways[victim_idx];
                (line.tag, line.data.clone())
            };
            let base_old = self.line_base_address(victim_tag, index);
            for (i, &byte) in victim_data.iter().enumerate() {
                self.next.write_byte(base_old + i as u64, byte)?;
            }
        }

        // Fetch new block from next level
        let block_mask = !(self.params.block_size as u64 - 1);
        let fetch_base = addr & block_mask;
        let mut new_data = vec![0u8; self.params.block_size];
        for i in 0..self.params.block_size {
            new_data[i] = self.next.read_byte(fetch_base + i as u64)?;
        }

        // Install the new line
        {
            let set = &mut self.sets[set_idx];
            let line = &mut set.ways[victim_idx];
            line.valid = true;
            line.dirty = false;
            line.tag = tag;
            line.data = new_data;
        }

        Ok((set_idx, victim_idx, true))
    }

    fn read_byte_inner(&mut self, addr: u64) -> Result<u8, VmError> {
        let (set_idx, way_idx, miss) = self.resolve_line(addr)?;
        let offset = (addr & ((1u64 << self.block_bits) - 1)) as usize;
        if miss {
            self.stats.read_misses += 1;
        } else {
            self.stats.read_hits += 1;
        }
        Ok(self.sets[set_idx].ways[way_idx].data[offset])
    }

    fn write_byte_inner(&mut self, addr: u64, value: u8) -> Result<(), VmError> {
        if self.params.read_only {
            return Err(VmError::WriteToRom);
        }

        let (set_idx, way_idx, miss) = self.resolve_line(addr)?;
        let offset = (addr & ((1u64 << self.block_bits) - 1)) as usize;
        let line = &mut self.sets[set_idx].ways[way_idx];
        line.data[offset] = value;

        if miss {
            self.stats.write_misses += 1;
        } else {
            self.stats.write_hits += 1;
        }

        if self.params.write_back {
            line.dirty = true;
        } else {
            // write‑through: forward to next level
            self.next.write_byte(addr, value)?;
        }
        Ok(())
    }
}

impl<Next: MemoryAccess> MemoryAccess for Cache<Next> {
    fn read_byte(&mut self, addr: u64) -> Result<u8, VmError> {
        self.read_byte_inner(addr)
    }

    fn read_halfword(&mut self, addr: u64) -> Result<u16, VmError> {
        // Read two bytes directly to avoid double-counting in stats
        let lo = self.read_byte_inner(addr)? as u16;
        let hi = self.read_byte_inner(addr + 1)? as u16;
        Ok(lo | (hi << 8))
    }

    fn read_word(&mut self, addr: u64) -> Result<u32, VmError> {
        // Read four bytes directly to avoid double-counting in stats
        let b0 = self.read_byte_inner(addr)? as u32;
        let b1 = self.read_byte_inner(addr + 1)? as u32;
        let b2 = self.read_byte_inner(addr + 2)? as u32;
        let b3 = self.read_byte_inner(addr + 3)? as u32;
        Ok(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
    }

    fn read_doubleword(&mut self, addr: u64) -> Result<u64, VmError> {
        // Read eight bytes directly to avoid double-counting in stats
        let w0 = self.read_word(addr)? as u64;
        let w1 = self.read_word(addr + 4)? as u64;
        Ok(w0 | (w1 << 32))
    }

    fn write_byte(&mut self, addr: u64, data: u8) -> Result<(), VmError> {
        self.write_byte_inner(addr, data)
    }

    fn write_halfword(&mut self, addr: u64, data: u16) -> Result<(), VmError> {
        // Write two bytes directly to avoid double-counting in stats
        self.write_byte_inner(addr, data as u8)?;
        self.write_byte_inner(addr + 1, (data >> 8) as u8)
    }

    fn write_word(&mut self, addr: u64, data: u32) -> Result<(), VmError> {
        // Write four bytes directly to avoid double-counting in stats
        self.write_byte_inner(addr, data as u8)?;
        self.write_byte_inner(addr + 1, (data >> 8) as u8)?;
        self.write_byte_inner(addr + 2, (data >> 16) as u8)?;
        self.write_byte_inner(addr + 3, (data >> 24) as u8)
    }

    fn write_doubleword(&mut self, addr: u64, data: u64) -> Result<(), VmError> {
        // Write eight bytes directly to avoid double-counting in stats
        self.write_word(addr, data as u32)?;
        self.write_word(addr + 4, (data >> 32) as u32)
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

/// A full snapshot of cache state (sets × ways) plus cumulative stats.
#[derive(Clone, Debug, Default)]
pub struct CacheSnapshot {
    pub params: CacheParamsSnapshot,
    /// `sets[set_index][way_index]`
    pub sets: Vec<Vec<CacheLineSnapshot>>,
    pub stats: CacheStats,
}

impl Clone for CacheStats {
    fn clone(&self) -> Self {
        Self {
            read_hits: self.read_hits,
            read_misses: self.read_misses,
            write_hits: self.write_hits,
            write_misses: self.write_misses,
        }
    }
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
