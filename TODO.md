## Future Enhancements

### Linker-script / base addresses (PARTIALLY ADDRESSED)
The assembler lays out sections starting at virtual address 0 (each section packed sequentially). When you load the VM's memory, you'll typically want .text at 0x8000_0000 (or wherever).

**Current State:**
- Assembler produces `AssembledOutput` with sections and symbol tables
- Symbol addresses are relative to section base (starting at 0)
- **Loader responsibility**: Add base offset when loading into VM memory

**Recommended Approach:**
Implement a simple loader in the VM that:
1. Reads the assembled sections
2. Adds a configurable base address (e.g., 0x8000_0000) to all section data
3. Adjusts the entry point accordingly

No assembler changes needed – this is purely a VM loading concern.

### Elf/image generation (FUTURE WORK)
If you eventually want to run a real OS kernel, you'll want to output ELF files. That's a future concern; for bringing up the VM, loading the raw SectionData bytes directly is fine.

**Potential Implementation:**
- Add ELF header generation to `AssembledOutput`
- Support program headers for LOAD segments
- Generate proper section headers
- Export as `.elf` binary format