# Diagram notes

Technical context for unpublished diagrams. Move relevant explanations into the Guide when
publishing a figure; keep only short labels in the drawing.

## toolchain_flow

Compile to typed IR, then lower to RISC-V assembly. Assemble a relocatable ELF object with
symbols and relocations. Link the object with its runtime and imports. The kernel loads ELF
segments into a fresh address space. Compilation creates instructions; loading establishes a
process that can execute them.

## interp_flow

Literals and typed holes define the output. Compute the exact number of output bytes. Too small:
return an error before writing anything. Enough space: format once and return bytes used. The
destination is unchanged on a capacity error; no allocation is required.

## load_store_addressing

t4 contains 0x2000. 0x2000 + signed offset 8 = 0x2008. The eight bytes starting at 0x2008 encode
the value 7. t5 receives 7; t4 is unchanged. The offset is measured in bytes, not elements.
Address calculation precedes reading the value.

## cache_address_split

Five bits select one of 32 sets. Compare the requested tag with both valid ways. Six bits select
the byte within the matching 64-byte line. On a miss, fetch the line from L2 and replace an
invalid way or the least recently used way. These are physical address fields.

## sv39_walk

Read PTE at (satp.PPN << 12) + VPN[2] * 8. Read PTE at (next PPN << 12) + VPN[1] * 8. Read the
leaf PTE at (next PPN << 12) + VPN[0] * 8. Combine leaf PPN << 12 with byte offset, VA bits
11:0. Each table has 512 eight-byte PTEs. Check validity and permissions; upper-level leaves
describe superpages.

## fb_double_buffer

Guest pixel stores and FILL update the hidden frame. Publish the completed back buffer to the
front buffer. The host GUI samples a stable 320 by 240 RGBA frame. Control offsets: FILL +0,
PRESENT +4, DBMODE +8. Single-buffered mode has different visibility.

## trap_flow

ecall, interrupt, or fault transfers control to S-mode. Use sscratch to switch stacks; save the
trap frame. Handle the syscall, interrupt, or recoverable fault. Restore the frame and resume at
sepc. Syscalls advance sepc past ecall. A fatal fault or task exit does not return to that user
instruction.
