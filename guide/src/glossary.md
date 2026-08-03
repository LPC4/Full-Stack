Terms are italicized when they are introduced in the Guide. This table collects those terms and the related
vocabulary that is useful when moving between source code, IR, assembly and the running machine.

| Term | Meaning |
| --- | --- |
| *Address* | A number that identifies a location in memory or a memory-mapped device. The address itself is just a value; the machine's memory map decides what exists there. |
| *Application binary interface (ABI)* | The machine-level contract that lets separately compiled code work together. It includes the calling convention, data layout, register use and object-file conventions. |
| *Assembler* | A program that translates assembly source into encoded machine instructions and object-file data. It also resolves local labels and expands pseudo-instructions. |
| *Assembly* | A readable text form of machine instructions. It names operations, registers and labels, but an assembler must still encode it before a CPU can execute it. |
| *Back end* | The compiler stage that translates IR into code for a particular instruction set, such as RISC-V. It chooses instructions, registers and stack layouts. |
| *Basic block* | A straight-line sequence of instructions with one entry and no branches except at the end. Conditions and loops become multiple basic blocks connected by branches and jumps. |
| *Branch* | An instruction that may change the program counter based on a condition. Branches implement source-level decisions such as `if` and loop conditions. |
| *Breakpoint* | A marked instruction where the debugger pauses execution before running it, allowing the current registers, memory and call stack to be inspected. |
| *Byte* | Eight bits. Memory is addressed by byte even when an instruction reads or writes a larger value. |
| *Callee* | The function being called. Its calling convention determines which registers it may overwrite and which it must preserve. |
| *Caller* | The function making a call. It prepares the arguments and must protect any caller-owned values it still needs afterward. |
| *Calling convention* | Shared rules for function calls: where arguments and return values go, which registers a callee may overwrite, which it must restore and how the stack stays aligned. Without these rules, separately written functions could not safely call one another. |
| *Compiler* | A program that translates source code into a lower-level form. In this project the path is HLL to IR to RISC-V assembly. |
| *Decode* | The stage where the CPU interprets an instruction's encoded bits to determine its operation, registers and immediate value. |
| *Device* | Hardware, or emulated hardware, that the CPU communicates with. A memory-mapped device responds to loads and stores in its assigned address range. |
| *Disassembly* | Machine instructions decoded from memory and displayed as readable assembly. It shows what the CPU actually received, after labels and pseudo-instructions have been resolved. |
| *Entry point* | The address of the first instruction that should run. An executable records this address, often for a startup function such as `_start` rather than `main`. |
| *Epilogue* | The end of a function that restores saved registers, releases its stack frame and returns to the saved return address. It must undo the prologue's work in a compatible order. |
| *Executable and Linkable Format (ELF)* | A container for executable code, data and metadata such as the target instruction set, entry point, sections and loadable segments. The linker creates it and the loader uses it to place the program in memory. |
| *Execute* | The stage where the CPU performs a decoded instruction, such as adding values, accessing memory or changing the program counter. |
| *Fetch* | The stage where the CPU reads the next encoded instruction from the address in the program counter. |
| *Frame pointer (`fp`)* | A register that keeps a stable reference point for the current stack frame even if `sp` moves during the function. RISC-V uses `s0` as `fp` when a frame pointer is needed. |
| *Front end* | The compiler stages that understand the source language. They parse HLL, check its meaning and types, and produce IR without depending on the final CPU. |
| *Function stack frame* | The part of the stack owned by one active function call. It can hold the return address, saved registers, local values and temporary call data; recursive calls each receive a separate frame. |
| *Heap* | A memory region used for dynamically allocated values whose lifetime is not tied to one function call. An allocator tracks which parts are free and in use. |
| *Immediate* | A constant encoded directly inside an instruction rather than read from a register. Its bit field has a fixed size, so large constants may require several instructions. |
| *Instruction* | One operation the CPU can decode and execute. A RISC-V instruction in this Guide is encoded as four bytes. |
| *Instruction set architecture (ISA)* | The contract implemented by a CPU: its instructions, registers, encodings and machine-level behavior. Programs compiled for one ISA do not automatically run on another. |
| *Intermediate representation (IR)* | The compiler's explicit language between HLL and assembly. It keeps useful information such as types and unlimited temporary values while expressing control flow as basic blocks, branches and jumps. |
| *Label* | A source-level name for an address. The assembler replaces references to it with the distance or address required by the encoded instruction. |
| *Leaf function* | A function that calls no other functions. Because it makes no calls, it often does not need to save a return address or create a stack frame. |
| *Linker* | A program that combines object files, resolves symbols between them, applies relocations and lays out sections to produce an executable. This is where separately compiled code becomes one program. |
| *Load* | An instruction that copies a value from memory or a mapped device into a register. The instruction determines how many bytes are read. |
| *Loader* | A program or system component that reads an executable, copies its loadable segments to their required addresses, prepares any zero-filled memory and starts execution at the entry point. |
| *Location counter* | The linker's current output address while it lays out sections. In a linker script, `.` refers to this address and advances as content is placed. |
| *Machine code* | The encoded bytes a CPU executes directly. Assembly is a readable representation of these bytes, not the bytes themselves. |
| *Memory* | Addressable storage that holds program instructions and data. RAM is larger but slower than the CPU's registers. |
| *Memory map* | The assignment of address ranges to RAM, ROM and devices. The same numeric address must have a defined owner before a load or store has a useful meaning. |
| *Memory mapped input/output (MMIO)* | A way to control devices using ordinary load and store instructions at designated addresses. These accesses can have side effects, such as printing a byte or stopping the machine. |
| *Object file* | A compiled unit containing machine code, data, symbols and relocation information. It is not a complete program until the linker combines it with the other required objects. |
| *Program counter (`pc`)* | The register containing the address of the current instruction. Normal execution advances it; branches, jumps and calls replace it with another address. |
| *Prologue* | The start of a function that reserves stack space and saves state that must survive the call, commonly `ra`, `fp` and saved registers. |
| *Pseudo-instruction* | Convenient assembly syntax that does not have its own hardware encoding. The assembler expands it into one or more real instructions, so the disassembly may look different from the source. |
| *QEMU* | An emulator that can model CPUs and complete machines. Its RISC-V `virt` board implements the same ISA as this project but defines its own memory map and devices. |
| *Register* | A small, fixed-width storage location directly inside the CPU. Instructions name registers explicitly, making them much faster but far fewer than memory locations. |
| *Relocation* | A placeholder in an object file for an address or offset that cannot be known until linking. The linker patches it once every symbol's final location is known. |
| *Return address (`ra`)* | The address where execution should continue after a function finishes. A call writes it to `ra`, so a function making another call must preserve its own return address. |
| *RISC-V* | The instruction set architecture used throughout the Guide. RV64 means its general-purpose registers and address space are 64 bits wide. |
| *Runtime* | Supporting code that establishes the environment around a source program. It can initialize memory, call `main`, handle its result and provide operations that source code cannot perform alone. |
| *Section* | A named group of related code or data in an object or executable file. Common sections include `.text` for code, `.rodata` for constants, `.data` for initialized writable data and `.bss` for zero-initialized data. |
| *Segment* | A range of an executable that a loader maps into memory with particular read, write and execute permissions. Sections organize files for tools; segments describe what the running process needs. |
| *Stack* | A memory region that grows and shrinks in last-in, first-out order. Function calls commonly reserve stack frames there because releasing the newest call first matches return order. |
| *Stack overflow* | The stack growing beyond the memory reserved for it, often because too many calls or very large frames are active at once. |
| *Stack pointer (`sp`)* | The register marking the current end of the stack. Reserving a frame moves it downward; releasing the frame restores it upward. |
| *Stack underflow* | The stack pointer moving back past its starting position by releasing more space than was reserved. It usually indicates mismatched stack operations. |
| *Standard library* | Reusable functions and types provided to programs, such as console or memory helpers. It uses the runtime environment but is not the same thing as startup and shutdown code. |
| *Step into* | A debugger action that follows a call into the first instruction of the called function. |
| *Step out* | A debugger action that runs until the current function returns to its caller. |
| *Step over* | A debugger action that runs an entire called function and pauses at the following source line. |
| *Store* | An instruction that copies a value from a register into memory or a mapped device. The instruction determines how many bytes are written. |
| *Symbol* | A named address or value recorded in assembly or an object file. Linkers use symbols to connect references in one object to definitions in another. |
| *SYSCON* | A system-controller device. In this project's VM, storing an exit code at its mapped address stops the machine. |
| *UART* | A Universal Asynchronous Receiver/Transmitter, traditionally used for serial communication. In the Guide, storing a byte in its transmit register writes a character to the console. |
| *Virtual machine (VM)* | Software that models a CPU, memory and devices closely enough to execute machine code. The VM still follows the same ISA and memory-map rules as hardware. |
