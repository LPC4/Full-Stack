// --- Filesystem layout ---

/// Canonical host-side filesystem layout. Shared constants mirror `FS_*` in
/// `kernel/fs/fs_types.hll`; record offsets mirror its typed disk structs.
pub mod fs_layout {
    /// Block size in bytes (`FS_BLOCK_SIZE`).
    pub const BLOCK_SIZE: usize = 4096;
    /// Inode size in bytes (`FS_INODE_SIZE`).
    pub const INODE_SIZE: usize = 128;
    /// Total inode count (`FS_MAX_INODES`).
    pub const INODE_COUNT: usize = 256;
    /// Blocks occupied by the inode table (`INODE_COUNT * INODE_SIZE / BLOCK_SIZE`).
    pub const INODE_TABLE_BLOCKS: usize = INODE_COUNT * INODE_SIZE / BLOCK_SIZE;
    /// Free-block bitmap block index (`FS_BITMAP_BLOCK`); follows the inode table.
    pub const BITMAP_BLOCK: usize = 1 + INODE_TABLE_BLOCKS;
    /// First data block index (`FS_DATA_BLOCK_START`); follows the bitmap.
    pub const DATA_BLOCK_START: usize = BITMAP_BLOCK + 1;
    /// Directory-entry size in bytes (`FS_DIRENT_SIZE`).
    pub const DIRENT_SIZE: usize = 36;
    /// Directory entries per block (`FS_DIRENTS_PER_BLOCK`).
    pub const DIRENTS_PER_BLOCK: usize = BLOCK_SIZE / DIRENT_SIZE;
    /// Direct block pointers per inode (`FS_INODE_BLOCKS`).
    pub const MAX_DIRECT_BLOCKS: usize = 44;
    /// Largest data-block count a built image may use; host-side bound from
    /// the boot RAM window (see `_OS_SPECIFICATION.md` 8.4), not a format cap.
    pub const MAX_DATA_BLOCKS: usize = 640;

    // DiskInode field offsets.
    pub const IN_TYPE: usize = 0;
    pub const IN_PARENT: usize = 2;
    pub const IN_SIZE: usize = 4;
    pub const IN_NAME: usize = 8;
    pub const IN_BLOCKS: usize = 40;

    // DiskDirEntry field offsets.
    pub const DE_NAME: usize = 0;
    pub const DE_INODE: usize = 32;

    // Superblock field offsets (FS_SB_*); MAGIC/VERSION/INODE_COUNT/ROOT_INODE are
    // host-only header fields the kernel reads positionally.
    pub const SB_MAGIC: usize = 0;
    pub const SB_VERSION: usize = 8;
    pub const SB_INODE_COUNT: usize = 10;
    pub const SB_BLOCK_COUNT: usize = 12;
    pub const SB_ROOT_INODE: usize = 14;
    pub const SB_FREE_INODES: usize = 16;
    pub const SB_FREE_BLOCKS: usize = 20;
    pub const SB_INODE_BITMAP: usize = 24;
}

// --- Filesystem image builder ---

/// An entry to include in a filesystem image built by [`build_fs_image`].
pub enum FsEntry<'a> {
    /// A directory at the given absolute path (e.g. `"/bin"`).
    Dir { path: &'a str },
    /// A file at the given absolute path with the given content.
    File { path: &'a str, data: &'a [u8] },
}

fn split_last_component(path: &str) -> (&str, &str) {
    if let Some(pos) = path.rfind('/') {
        let parent = if pos == 0 { "/" } else { &path[..pos] };
        let name = &path[pos + 1..];
        (parent, name)
    } else {
        ("/", path)
    }
}

/// Serialise `entries` into the on-disk filesystem image format. Block layout is in
/// _`OS_SPECIFICATION.md`, and the [`fs_layout`] constants are checked against `fs_types.hll`.
pub fn build_fs_image(entries: &[FsEntry<'_>]) -> Vec<u8> {
    use fs_layout::{
        BITMAP_BLOCK, BLOCK_SIZE, DATA_BLOCK_START, DE_INODE, DE_NAME, DIRENT_SIZE, IN_BLOCKS,
        IN_NAME, IN_PARENT, IN_SIZE, IN_TYPE, INODE_COUNT, INODE_SIZE, MAX_DATA_BLOCKS,
        MAX_DIRECT_BLOCKS, SB_BLOCK_COUNT, SB_FREE_BLOCKS, SB_FREE_INODES, SB_INODE_BITMAP,
        SB_INODE_COUNT, SB_ROOT_INODE, SB_VERSION,
    };

    // One block per file (rounded up) plus one per directory and the root, then
    // a margin of free blocks so the running FS can create and grow files.
    let mut needed_data_blocks: usize = 1; // root directory block
    for entry in entries {
        match entry {
            FsEntry::File { data, .. } => {
                needed_data_blocks += data.len().div_ceil(BLOCK_SIZE).max(1);
            }
            FsEntry::Dir { .. } => needed_data_blocks += 1,
        }
    }
    needed_data_blocks += 56;
    let total_data_blocks = needed_data_blocks.min(MAX_DATA_BLOCKS);
    assert!(
        needed_data_blocks <= MAX_DATA_BLOCKS,
        "boot FS image needs {needed_data_blocks} data blocks but the layout caps at \
         {MAX_DATA_BLOCKS}; reduce the bundled file sizes"
    );
    let total_blocks = DATA_BLOCK_START + total_data_blocks;
    let image_size = total_blocks * BLOCK_SIZE;

    let mut image = vec![0u8; image_size];

    // --- Helpers ---

    let inode_offset = |idx: usize| -> usize { BLOCK_SIZE + idx * INODE_SIZE };
    let block_offset = |blk: usize| -> usize { blk * BLOCK_SIZE };

    let write_u16 = |buf: &mut Vec<u8>, off: usize, val: u16| {
        buf[off] = (val & 0xFF) as u8;
        buf[off + 1] = (val >> 8) as u8;
    };
    let write_u32 = |buf: &mut Vec<u8>, off: usize, val: u32| {
        buf[off] = (val & 0xFF) as u8;
        buf[off + 1] = ((val >> 8) & 0xFF) as u8;
        buf[off + 2] = ((val >> 16) & 0xFF) as u8;
        buf[off + 3] = ((val >> 24) & 0xFF) as u8;
    };
    // next_inode and next_data_block are mutable state we thread through.
    let mut next_inode: usize = 1; // 0 = root
    let mut next_data_block: usize = 0; // relative to DATA_BLOCK_START

    // Allocate a data block and return its absolute block index.
    let alloc_block = |next: &mut usize| -> usize {
        let blk = DATA_BLOCK_START + *next;
        *next += 1;
        blk
    };

    // Allocate an inode index.
    let alloc_inode = |next: &mut usize| -> usize {
        let idx = *next;
        *next += 1;
        idx
    };

    // Write a name into a 32-byte field at buf[off..off+32].
    let write_name = |buf: &mut Vec<u8>, off: usize, name: &str| {
        let bytes = name.as_bytes();
        let len = bytes.len().min(31);
        buf[off..off + len].copy_from_slice(&bytes[..len]);
        buf[off + len] = 0;
    };

    // Root directory (inode 0)
    let root_data_blk = alloc_block(&mut next_data_block);
    {
        let off = inode_offset(0);
        image[off + IN_TYPE] = 2; // directory
        write_u16(&mut image, off + IN_PARENT, 0); // root's parent = self
        write_u32(&mut image, off + IN_SIZE, 0); // entry count updated later
        write_name(&mut image, off + IN_NAME, "/");
        write_u16(&mut image, off + IN_BLOCKS, root_data_blk as u16);
    }

    // Add a DirEntry to a directory inode's data block
    // Returns false if block is full (not handled for simplicity; 113 entries per block).
    let add_dirent = |image: &mut Vec<u8>, dir_inode: usize, child_inode: usize, name: &str| {
        let off = inode_offset(dir_inode);
        let count = u32::from_le_bytes([
            image[off + IN_SIZE],
            image[off + IN_SIZE + 1],
            image[off + IN_SIZE + 2],
            image[off + IN_SIZE + 3],
        ]) as usize;

        // Get first data block of directory.
        let blk_idx =
            u16::from_le_bytes([image[off + IN_BLOCKS], image[off + IN_BLOCKS + 1]]) as usize;
        let blk_off = block_offset(blk_idx);
        let de_off = blk_off + count * DIRENT_SIZE;

        // Write name field (DE_NAME, 32 bytes).
        let name_bytes = name.as_bytes();
        let name_len = name_bytes.len().min(31);
        image[de_off + DE_NAME..de_off + DE_NAME + name_len]
            .copy_from_slice(&name_bytes[..name_len]);
        image[de_off + DE_NAME + name_len] = 0;
        // Write the child inode index (u16) at DE_INODE.
        image[de_off + DE_INODE] = (child_inode & 0xFF) as u8;
        image[de_off + DE_INODE + 1] = ((child_inode >> 8) & 0xFF) as u8;

        // Increment parent size.
        let new_count = (count + 1) as u32;
        image[off + IN_SIZE] = (new_count & 0xFF) as u8;
        image[off + IN_SIZE + 1] = ((new_count >> 8) & 0xFF) as u8;
        image[off + IN_SIZE + 2] = ((new_count >> 16) & 0xFF) as u8;
        image[off + IN_SIZE + 3] = ((new_count >> 24) & 0xFF) as u8;
    };

    // --- Process entries in order (dirs before files) ---

    // Map from absolute path -> inode index, seeded with root.
    let mut path_to_inode: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    path_to_inode.insert("/".to_owned(), 0);

    // Sort: directories first, then files, both sorted by path length so parents come first.
    let mut sorted: Vec<&FsEntry<'_>> = entries.iter().collect();
    sorted.sort_by_key(|e| match e {
        FsEntry::Dir { path } => (0usize, path.len(), *path),
        FsEntry::File { path, .. } => (1usize, path.len(), *path),
    });

    for entry in sorted {
        match entry {
            FsEntry::Dir { path } => {
                if *path == "/" {
                    continue; // root already created
                }
                let (parent_path, name) = split_last_component(path);
                let parent_inode = *path_to_inode.get(parent_path).unwrap_or(&0);

                let dir_inode = alloc_inode(&mut next_inode);
                let dir_data_blk = alloc_block(&mut next_data_block);
                {
                    let off = inode_offset(dir_inode);
                    image[off + IN_TYPE] = 2;
                    write_u16(&mut image, off + IN_PARENT, parent_inode as u16);
                    write_u32(&mut image, off + IN_SIZE, 0);
                    write_name(&mut image, off + IN_NAME, name);
                    write_u16(&mut image, off + IN_BLOCKS, dir_data_blk as u16);
                }
                add_dirent(&mut image, parent_inode, dir_inode, name);
                path_to_inode.insert(path.to_string(), dir_inode);
            }
            FsEntry::File { path, data } => {
                let (parent_path, name) = split_last_component(path);
                let parent_inode = *path_to_inode.get(parent_path).unwrap_or(&0);

                let file_inode = alloc_inode(&mut next_inode);
                let num_blocks = data.len().div_ceil(BLOCK_SIZE);
                let num_blocks = num_blocks.max(1); // at least one block even for empty files
                assert!(
                    num_blocks <= MAX_DIRECT_BLOCKS,
                    "file {path:?} needs {num_blocks} blocks but the inode holds at most \
                     {MAX_DIRECT_BLOCKS} direct blocks ({} bytes)",
                    MAX_DIRECT_BLOCKS * BLOCK_SIZE
                );

                {
                    let off = inode_offset(file_inode);
                    image[off + IN_TYPE] = 1;
                    write_u16(&mut image, off + IN_PARENT, parent_inode as u16);
                    write_u32(&mut image, off + IN_SIZE, data.len() as u32);
                    write_name(&mut image, off + IN_NAME, name);

                    for b in 0..num_blocks {
                        let blk = alloc_block(&mut next_data_block);
                        let blk_slot_off = off + IN_BLOCKS + b * 2;
                        image[blk_slot_off] = (blk & 0xFF) as u8;
                        image[blk_slot_off + 1] = ((blk >> 8) & 0xFF) as u8;

                        let src_start = b * BLOCK_SIZE;
                        let src_end = (src_start + BLOCK_SIZE).min(data.len());
                        let blk_off = block_offset(blk);
                        if src_start < data.len() {
                            let chunk = &data[src_start..src_end];
                            image[blk_off..blk_off + chunk.len()].copy_from_slice(chunk);
                        }
                    }
                }
                add_dirent(&mut image, parent_inode, file_inode, name);
                path_to_inode.insert(path.to_string(), file_inode);
            }
        }
    }

    // --- Superblock ---
    {
        let off = 0usize;
        // Magic: "HLLFS\0\1\0"
        let magic: [u8; 8] = [0x48, 0x4C, 0x4C, 0x46, 0x53, 0x00, 0x01, 0x00];
        image[off..off + 8].copy_from_slice(&magic);
        write_u16(&mut image, off + SB_VERSION, 1);
        write_u16(&mut image, off + SB_INODE_COUNT, INODE_COUNT as u16);
        write_u16(&mut image, off + SB_BLOCK_COUNT, total_data_blocks as u16);
        write_u16(&mut image, off + SB_ROOT_INODE, 0);

        let used_inodes = next_inode;
        let free_inodes = (INODE_COUNT - used_inodes) as u32;
        write_u32(&mut image, off + SB_FREE_INODES, free_inodes);

        let used_data_blocks = next_data_block;
        let free_blocks = (total_data_blocks.saturating_sub(used_data_blocks)) as u32;
        write_u32(&mut image, off + SB_FREE_BLOCKS, free_blocks);

        // Inode bitmap: 256 bits (32 bytes). 1 = free.
        for i in 0..INODE_COUNT {
            let byte = i / 8;
            let bit = i % 8;
            if i >= used_inodes {
                image[off + SB_INODE_BITMAP + byte] |= 1 << bit;
            }
        }
    }

    // --- Free-block bitmap (BITMAP_BLOCK) ---
    // One bit per data block starting at DATA_BLOCK_START; bit 0 of byte 0 = first data block.
    {
        let bmap_off = block_offset(BITMAP_BLOCK);
        for i in 0..total_data_blocks {
            let byte = i / 8;
            let bit = i % 8;
            if i >= next_data_block {
                image[bmap_off + byte] |= 1 << bit;
            }
        }
    }

    image
}

#[cfg(test)]
mod tests {
    use super::*;

    // Parse a `const FS_NAME = <int>` declaration out of fs_types.hll. Handles decimal
    // and 0x-prefixed hex.
    fn fs_hll_const(src: &str, name: &str) -> i64 {
        for line in src.lines() {
            let line = line.trim();
            let line = line.strip_prefix("export ").unwrap_or(line);
            let rest = match line.strip_prefix("const ") {
                Some(r) => r,
                None => continue,
            };
            let (lhs, rhs) = match rest.split_once('=') {
                Some(p) => p,
                None => continue,
            };
            if lhs.trim() != name {
                continue;
            }
            let val = rhs.split(';').next().unwrap_or("").trim();
            return if let Some(hex) = val.strip_prefix("0x") {
                i64::from_str_radix(hex, 16).expect("hex filesystem constant")
            } else {
                val.parse().expect("decimal filesystem constant")
            };
        }
        panic!("const {name} not found in fs_types.hll");
    }

    // Shared host-builder and kernel constants must not drift.
    #[test]
    fn fs_layout_matches_fs_hll() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/os-runtime/kernel/fs/fs_types.hll"
        );
        let src = std::fs::read_to_string(path).expect("read fs_types.hll");

        let pairs: &[(&str, i64)] = &[
            ("FS_BLOCK_SIZE", fs_layout::BLOCK_SIZE as i64),
            ("FS_INODE_SIZE", fs_layout::INODE_SIZE as i64),
            ("FS_MAX_INODES", fs_layout::INODE_COUNT as i64),
            ("FS_BITMAP_BLOCK", fs_layout::BITMAP_BLOCK as i64),
            ("FS_DATA_BLOCK_START", fs_layout::DATA_BLOCK_START as i64),
            ("FS_DIRENT_SIZE", fs_layout::DIRENT_SIZE as i64),
            ("FS_DIRENTS_PER_BLOCK", fs_layout::DIRENTS_PER_BLOCK as i64),
            ("FS_INODE_BLOCKS", fs_layout::MAX_DIRECT_BLOCKS as i64),
            ("FS_SB_BLOCK_COUNT", fs_layout::SB_BLOCK_COUNT as i64),
            ("FS_SB_FREE_INODES", fs_layout::SB_FREE_INODES as i64),
            ("FS_SB_FREE_BLOCKS", fs_layout::SB_FREE_BLOCKS as i64),
            ("FS_SB_INODE_BITMAP", fs_layout::SB_INODE_BITMAP as i64),
        ];

        for (name, rust_val) in pairs {
            let hll_val = fs_hll_const(&src, name);
            assert_eq!(
                hll_val, *rust_val,
                "{name}: fs_types.hll has {hll_val} but fs_layout has {rust_val}"
            );
        }
    }

    // Regression: the image must be sized for the sum of all files' blocks, not
    // just the largest one, or the copy loop runs past the data region.
    #[test]
    fn build_fs_image_sizes_for_total_of_all_files() {
        const BLOCK: usize = 4096;
        // Four 30-block files sum to 120 blocks, well past any single file.
        let big = vec![7u8; 30 * BLOCK];
        let entries = vec![
            FsEntry::Dir { path: "/bin" },
            FsEntry::File {
                path: "/bin/a.elf",
                data: &big,
            },
            FsEntry::File {
                path: "/bin/b.elf",
                data: &big,
            },
            FsEntry::File {
                path: "/bin/c.elf",
                data: &big,
            },
            FsEntry::Dir { path: "/home" },
            FsEntry::File {
                path: "/home/d.elf",
                data: &big,
            },
        ];

        // Must not panic and must hold all 120 data blocks plus metadata.
        let image = build_fs_image(&entries);
        assert!(
            image.len() >= 120 * BLOCK,
            "image too small ({} bytes) for the summed file contents",
            image.len()
        );
    }
}
