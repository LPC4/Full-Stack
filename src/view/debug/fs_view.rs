//! Filesystem inspector: walks the in-memory FS image from guest memory into a
//! read-only tree with a file preview. Layout (kernel/fs.hll; OS spec 8) is here.

use asm_to_binary::AssembledOutput;
use eframe::egui::{self, Color32, RichText};
use virtual_machine::bus::RAM_BASE;
use virtual_machine::virtual_machine::VirtualMachine;

// FS on-disk layout (kernel/fs.hll). Must match the HLL constants.
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

/// Read a fixed 32-byte inline name, trimmed at its terminator.
fn read_name(vm: &VirtualMachine, pa: u64) -> String {
    let bytes = vm.peek_bytes_raw(pa, NAME_LEN);
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// Count valid open descriptors per inode by scanning the 16-slot fd table.
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

/// Recursively decode the inode at `idx` into a node, descending into directories.
/// `budget` caps the total node count so a cyclic image cannot spin the render.
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

/// Read up to `MAX_PREVIEW` bytes of a file by walking its inode block pointers.
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

fn mono(text: impl Into<String>, col: Color32) -> RichText {
    RichText::new(text.into()).monospace().size(11.0).color(col)
}

/// Render one row and recurse. Directories are collapsible; files are selectable
/// and drive the preview pane. Returns the inode the user just clicked, if any.
fn render_node(ui: &mut egui::Ui, node: &FsNode, selected: u64, clicked: &mut Option<u64>) {
    let dir_col = Color32::from_rgb(150, 190, 255);
    let file_col = Color32::LIGHT_GRAY;
    let dim = Color32::from_rgb(120, 120, 120);

    let fd_tag = if node.open_fds > 0 {
        format!("  [{} fd]", node.open_fds)
    } else {
        String::new()
    };

    if node.is_dir {
        let label = format!("{}/  (inode {}){}", node.name, node.inode, fd_tag);
        egui::CollapsingHeader::new(mono(label, dir_col))
            .id_salt(("fs_dir", node.inode))
            .default_open(node.inode == 0)
            .show(ui, |ui| {
                for child in &node.children {
                    render_node(ui, child, selected, clicked);
                }
            });
    } else {
        let label = format!(
            "{}  ({} B, inode {}){}",
            node.name, node.size, node.inode, fd_tag
        );
        if ui
            .selectable_label(node.inode == selected, mono(label, file_col))
            .clicked()
        {
            *clicked = Some(node.inode);
        }
    }

    // Empty directory marker, only at the immediate level (children render their own).
    if node.is_dir && node.children.is_empty() {
        ui.indent(("fs_empty", node.inode), |ui| {
            ui.label(mono("(empty)", dim));
        });
    }
}

/// Find a node by inode in the tree (for resolving the preview selection).
fn find(node: &FsNode, inode: u64) -> Option<&FsNode> {
    if node.inode == inode {
        return Some(node);
    }
    node.children.iter().find_map(|c| find(c, inode))
}

/// Offset-prefixed hex dump that collapses runs of all-zero rows into a single
/// `*` marker (xxd style), so an ELF's page-alignment padding stays legible.
fn hex_dump(data: &[u8]) -> String {
    let rows: Vec<&[u8]> = data.chunks(16).collect();
    let mut out = String::new();
    let mut i = 0;
    while i < rows.len() {
        let row = rows[i];
        out.push_str(&format!("{:06x}  ", i * 16));
        for b in row {
            out.push_str(&format!("{b:02x} "));
        }
        out.push('\n');
        // Collapse two or more consecutive all-zero rows after this one.
        if row.iter().all(|&b| b == 0) {
            let start = i;
            while i + 1 < rows.len() && rows[i + 1].iter().all(|&b| b == 0) {
                i += 1;
            }
            if i > start {
                out.push_str("*\n");
            }
        }
        i += 1;
    }
    out
}

/// Render a hex/text preview of the selected file's first bytes.
fn render_preview(ui: &mut egui::Ui, vm: &VirtualMachine, sym: &FsSymbols, node: &FsNode) {
    let head = Color32::from_rgb(160, 160, 160);
    ui.label(mono(
        format!("preview: {} ({} bytes)", node.name, node.size),
        head,
    ));
    let data = file_preview(vm, sym, node);
    if data.is_empty() {
        ui.label(mono("(empty file)", Color32::GRAY));
        return;
    }

    // Printable ASCII renders as text; anything else falls back to a hex dump.
    let printable = data
        .iter()
        .all(|&b| b == b'\n' || b == b'\t' || b == b'\r' || (0x20..0x7f).contains(&b));
    if printable {
        let text = String::from_utf8_lossy(&data);
        ui.label(mono(text.into_owned(), Color32::LIGHT_GRAY));
    } else {
        ui.label(mono(hex_dump(&data), Color32::LIGHT_GRAY));
    }
    if node.size > MAX_PREVIEW {
        ui.label(mono(
            format!("... (showing first {MAX_PREVIEW} bytes)"),
            head,
        ));
    }
}

/// Render the filesystem tree and the selected file's preview into the Debug tab.
pub fn render(ui: &mut egui::Ui, vm: &VirtualMachine, sym: &FsSymbols) {
    let Some(root) = capture(vm, sym) else {
        ui.label(mono("filesystem not mounted yet", Color32::GRAY));
        return;
    };

    let sel_id = ui.make_persistent_id("mw_fs_selected_inode");
    let mut selected = ui.data(|d| d.get_temp::<u64>(sel_id)).unwrap_or(u64::MAX);
    let mut clicked = None;

    render_node(ui, &root, selected, &mut clicked);
    if let Some(inode) = clicked {
        selected = inode;
        ui.data_mut(|d| d.insert_temp(sel_id, selected));
    }

    if let Some(node) = find(&root, selected)
        && !node.is_dir
    {
        ui.add_space(6.0);
        ui.separator();
        render_preview(ui, vm, sym, node);
    }
}

#[cfg(test)]
mod tests {
    use super::hex_dump;

    #[test]
    fn hex_dump_collapses_zero_runs() {
        // Header row, then 3 all-zero rows, then a data row. The zero run after
        // the first zero row collapses to a single '*'; offsets stay accurate.
        let mut data = vec![0x7fu8, 0x45, 0x4c, 0x46];
        data.resize(64, 0); // header row (0) + three all-zero rows (1..4)
        data.extend_from_slice(&[0xaa; 16]); // a non-zero row at offset 0x40

        let dump = hex_dump(&data);
        assert!(dump.contains("000000  7f 45 4c 46"), "header row missing");
        assert_eq!(
            dump.matches('*').count(),
            1,
            "zero run should collapse to one '*'"
        );
        assert!(
            dump.contains("000040  aa aa"),
            "data row offset wrong after collapse"
        );
    }

    #[test]
    fn hex_dump_keeps_lone_zero_row() {
        // A single zero row between data must not get a '*' (nothing to collapse).
        let mut data = vec![0x11u8; 16];
        data.extend_from_slice(&[0u8; 16]);
        data.extend_from_slice(&[0x22u8; 16]);
        assert_eq!(hex_dump(&data).matches('*').count(), 0);
    }
}
