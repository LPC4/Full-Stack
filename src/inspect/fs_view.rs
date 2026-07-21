//! Filesystem inspector: walks the in-memory FS image from guest memory into a
//! read-only tree with a file preview. Layout (kernel/fs/fs.hll; OS spec 8) is here.

use asm_to_binary::AssembledOutput;
use virtual_machine::bus::RAM_BASE;
use virtual_machine::virtual_machine::VirtualMachine;

// FS on-disk layout (kernel/fs/fs.hll). Must match the HLL constants.
const BLOCK_SIZE: u64 = 4096;
const INODE_SIZE: u64 = 128;
const INODE_BLOCKS: u64 = 44;
const DIRENTS_PER_BLOCK: u64 = 113;
const DIRENT_SIZE: u64 = 36;
const NAME_LEN: usize = 32;
const MAGIC32: u64 = 0x464C4C48; // "HLLF", first 4 bytes of the superblock.

// Inode field byte offsets within each 128-byte record.
const IN_SIZE: u64 = 4;
const IN_BLOCKS: u64 = 40;
// Dirent field byte offsets within each 36-byte record.
const DE_INODE: u64 = 32;
// Inode types.
const TYPE_DIR: u64 = 2;
// Fd-table entry layout (FdEntry; each field is a u64).
const FD_TABLE_SLOTS: u64 = 16;
const FDE_SIZE: u64 = 32;
const FDE_VALID: u64 = 0;
const FDE_INODE: u64 = 8;

// Bound recursion and the node count against a corrupt image.
const MAX_DEPTH: usize = 16;
const MAX_NODES: usize = 1024;
// Cap how much of a file the preview pane reads (one page past a page-aligned ELF
// payload still fits, so a binary's header and start of code both show).
const MAX_PREVIEW: u64 = 8192;

/// Guest-physical addresses of the kernel FS globals, resolved once at boot from
/// the linked symbol table (kernel data is identity-mapped: PA = `RAM_BASE` + off).
#[derive(Clone, Copy)]
pub struct FsSymbols {
    fs_image_base: u64, // address of the global holding the image base PA
    fs_fd_table_pa: u64,
}

impl FsSymbols {
    /// Resolve the FS globals, or `None` if the image lacks them (non-kernel
    /// binary, so the panel hides itself).
    pub fn from_kernel(assembled: &AssembledOutput) -> Option<Self> {
        let pa = |n: &str| assembled.symbol_address(n).map(|off| RAM_BASE + off);
        Some(Self {
            fs_image_base: pa("fs_image_base")?,
            fs_fd_table_pa: pa("fs_fd_table_pa")?,
        })
    }
}

/// One filesystem entry decoded for display.
pub struct FsNode {
    pub inode: u64,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub open_fds: u32,
    pub children: Vec<FsNode>,
}

fn rd_u16(vm: &VirtualMachine, pa: u64) -> u64 {
    let b = vm.peek_bytes_raw(pa, 2);
    u16::from_le_bytes(b.try_into().unwrap_or([0u8; 2])) as u64
}

fn rd_u32(vm: &VirtualMachine, pa: u64) -> u64 {
    let b = vm.peek_bytes_raw(pa, 4);
    u32::from_le_bytes(b.try_into().unwrap_or([0u8; 4])) as u64
}

fn rd_u64(vm: &VirtualMachine, pa: u64) -> u64 {
    let b = vm.peek_bytes_raw(pa, 8);
    u64::from_le_bytes(b.try_into().unwrap_or([0u8; 8]))
}

fn inode_pa(base: u64, idx: u64) -> u64 {
    base + BLOCK_SIZE + idx * INODE_SIZE
}

fn read_name(vm: &VirtualMachine, pa: u64) -> String {
    let bytes = vm.peek_bytes_raw(pa, NAME_LEN);
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn open_fd_counts(vm: &VirtualMachine, sym: &FsSymbols) -> std::collections::HashMap<u64, u32> {
    let table = rd_u64(vm, sym.fs_fd_table_pa);
    let mut counts = std::collections::HashMap::new();
    if table == 0 {
        return counts;
    }
    for fd in 0..FD_TABLE_SLOTS {
        let entry = table + fd * FDE_SIZE;
        if rd_u64(vm, entry + FDE_VALID) != 0 {
            *counts.entry(rd_u64(vm, entry + FDE_INODE)).or_insert(0) += 1;
        }
    }
    counts
}

// `budget` caps the node count so a cyclic image cannot spin the render.
fn read_node(
    vm: &VirtualMachine,
    base: u64,
    idx: u64,
    name: String,
    depth: usize,
    fds: &std::collections::HashMap<u64, u32>,
    budget: &mut usize,
) -> FsNode {
    let ipa = inode_pa(base, idx);
    let ty = rd_u16(vm, ipa) & 0xff;
    let size = rd_u32(vm, ipa + IN_SIZE);
    let is_dir = ty == TYPE_DIR;

    let mut node = FsNode {
        inode: idx,
        name,
        is_dir,
        size,
        open_fds: fds.get(&idx).copied().unwrap_or(0),
        children: Vec::new(),
    };

    if !is_dir || depth >= MAX_DEPTH {
        return node;
    }

    // Walk the directory's data blocks; each holds DIRENTS_PER_BLOCK slots, a
    // dirent being live when its first name byte is non-zero.
    for slot in 0..INODE_BLOCKS {
        let blk = rd_u16(vm, ipa + IN_BLOCKS + slot * 2);
        if blk == 0 {
            break;
        }
        for ent in 0..DIRENTS_PER_BLOCK {
            if *budget == 0 {
                return node;
            }
            let de = base + blk * BLOCK_SIZE + ent * DIRENT_SIZE;
            let name = read_name(vm, de);
            if name.is_empty() {
                continue;
            }
            *budget -= 1;
            let child = rd_u16(vm, de + DE_INODE);
            node.children
                .push(read_node(vm, base, child, name, depth + 1, fds, budget));
        }
    }
    node
}

/// Walk the FS image from the root directory (inode 0). Returns `None` if the FS
/// is not mounted yet or the superblock magic does not match.
pub fn capture(vm: &VirtualMachine, sym: &FsSymbols) -> Option<FsNode> {
    let base = rd_u64(vm, sym.fs_image_base);
    if base == 0 || rd_u32(vm, base) != MAGIC32 {
        return None;
    }
    let fds = open_fd_counts(vm, sym);
    let mut budget = MAX_NODES;
    Some(read_node(vm, base, 0, "/".to_owned(), 0, &fds, &mut budget))
}

fn read_file(vm: &VirtualMachine, base: u64, idx: u64, size: u64) -> Vec<u8> {
    let ipa = inode_pa(base, idx);
    let mut out = Vec::new();
    let mut remaining = size.min(MAX_PREVIEW);
    for slot in 0..INODE_BLOCKS {
        if remaining == 0 {
            break;
        }
        let blk = rd_u16(vm, ipa + IN_BLOCKS + slot * 2);
        if blk == 0 {
            break;
        }
        let take = remaining.min(BLOCK_SIZE) as usize;
        out.extend_from_slice(&vm.peek_bytes_raw(base + blk * BLOCK_SIZE, take));
        remaining -= take as u64;
    }
    out
}

/// Read the previewable prefix of a file node's contents (for tests and the
/// preview pane). Empty for a directory.
pub fn file_preview(vm: &VirtualMachine, sym: &FsSymbols, node: &FsNode) -> Vec<u8> {
    if node.is_dir {
        return Vec::new();
    }
    let base = rd_u64(vm, sym.fs_image_base);
    read_file(vm, base, node.inode, node.size)
}
