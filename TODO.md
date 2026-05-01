Relocation for call, tail, la
The pseudo‑instructions call symbol, tail symbol, and la rd, symbol expand to auipc/jalr or lui/addi with zero immediate, i.e. they are not resolved. The current parser immediately expands them into RealInstructions without recording the symbol, so the encode pass has no chance to fill in the offsets.
If you plan to use those pseudo‑instructions, you’ll need a relocation step (or a linker). For now you can:

Avoid them and build address arithmetic manually (e.g., auipc rd, %hi(label) → addi rd, rd, %lo(label)), but the assembler doesn’t support %hi/%lo yet either.

Wait until you need multi‑file linking; for a simple VM test environment, direct jal label and j label are usually enough.

Linker‑script / base addresses
The assembler lays out sections starting at virtual address 0 (each section packed sequentially). When you load the VM’s memory, you’ll typically want .text at 0x8000_0000 (or wherever). You can simply add that base offset to all symbol addresses after loading. No assembler change needed – just a “loader” step in the VM that sets the base and copies the bytes.

Elf/image generation
If you eventually want to run a real OS kernel, you’ll want to output ELF files. That’s a future concern; for bringing up the VM, loading the raw SectionData bytes directly is fine.