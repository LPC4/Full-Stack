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

## pipeline_stages

IF fetches, ID decodes, EX executes, MEM accesses data memory, and WB writes back. Cycles
increase to the right. The dependent instruction waits one cycle for the load result. This
example has added memory latency disabled.

## branch_mispredict

Cycles increase to the right. The branch was predicted not taken but is actually taken.
EX resolves it, discards the younger instructions marked X, and restarts fetch at the target.

## predictor_fsm

The two-bit saturating counter starts at 01. T means the branch was actually taken; N means
it was not taken. Without a BTB entry, the predicted target defaults to PC + 4.

## process_states

READY is the initial queued state. EXITED retains the exit status until waitpid reaps the
slot, and cannot run. Waking a blocked task returns it to READY.

## regalloc_coloring

An edge joins values that are live at the same time. Those values need different registers.
The lifetimes of a and d do not overlap, so both can use s2.

## vec_growth

Length counts live elements; capacity counts available slots. A dot marks spare capacity.
When the buffer is full, growth allocates a larger heap buffer, copies the live elements,
and releases the old buffer.

## stack_frame

Higher addresses are at the top. The compiler frame contains padding, saved registers
including ra, and locals or spill slots. Its size is a multiple of 16 bytes. The adjusted
sp is also the compiler's fp. The regions are schematic, not to scale.

## stack_heap_growth

The diagram uses process virtual addresses, with higher addresses at the top. Reserving a
stack frame moves sp down within the mapped stack. brk grows the heap upward and may map
new pages. Accessing the unmapped gap faults. Region sizes are schematic.

## user_va_space

Higher process virtual addresses are at the top. Four stack pages sit below 0x8000_0000;
the heap starts at 0x6000_0000 and the image at 0x4000_0000. The gap is unmapped and faults
on access. This is a schematic layout, not a physical memory map.

## user_heap_brk

The program break is a byte boundary. Page mappings can extend beyond it, leaving a mapped
tail above the bytes assigned to the heap. Growing the break can require new pages. The
diagram shows virtual addresses with higher addresses at the top, not to scale.

## instruction_encoding

The ABI names t0, t1, and t2 refer to x5, x6, and x7. The encoding of add t2, t0, t1 is
0x006283B3. Bit 31 is on the left; bit 0 is on the right.

## elf_linking

main.o calls square, which util.o defines. The linker lays out sections, resolves the
undefined symbol, and patches the call relocation. The ELF records the entry address and
the loadable segments.
