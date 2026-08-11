I started off creating Minecraft mods for fun, and ever since, I've wanted to understand what a computer
*actually* does. Not just how to use a programming language, but how an application really gets run. At some point
I realized I had no idea how Java itself worked underneath.

That question is why I started working on multiple projects in my free time: different virtual machines, custom assembly and
programming languages. Eventually, after much trial and error, I arrived at [this project](https://github.com/LPC4/Full-Stack).

Full-Stack goes over the entire execution stack of computing: from binary instructions to a running operating system.
It includes a virtual machine (VM), a compiler, assembler, and linker for a custom language, and a kernel that runs inside the VM.

In these documents we'll understand every layer and how it fits together, giving you what you need to build a
similar project from scratch.

Throughout these documents there will be multiple code snippets. Complete examples have a Run button, so
you can modify and execute them directly on the page. You can also open the larger lesson programs in
[the project's own IDE](https://lpc4.github.io/Full-Stack/) to use the debugger and machine inspectors.

This is written for anyone interested in systems programming. I assume you already know how to code and understand
concepts like pointers, but not what lives below that level. After this you'll be able to understand the full stack
and make your own compiler, virtual machine, and even a small kernel.

This is what we'll be seeing:

| Chapter | What it adds |
| --- | --- |
| [CPUs and Assembly](cpus-and-assembly/) | Start at the CPU: registers, memory, instruction encoding, branches, functions, and stack frames. |
| [Memory Mapped Input / Output](memory-mapped-input-output/) | Reach devices through ordinary memory addresses, print through a UART, and stop the machine with SYSCON. |
| [The same code on QEMU](the-same-code-on-qemu/) | Move the assembly to another machine by supplying its stack, device protocol, linker layout, and build steps. |
| [High-Level Languages](high-level-languages/) | Follow HLL through IR and RISC-V assembly into object files, a linked ELF, and the debugger. |
| [Runtimes](runtimes/) | Trace `_start`, system calls, traps, and privilege levels down to the hardware. |
| [Standard Library](standard-library/) | Manage dynamic memory and see how reusable library code hides low-level allocation details. |

Let's begin with the basics.

## CPUs and Assembly

At the very bottom of the project sits an emulated RISC-V VM. That sounds really scary, but it's nothing
more than a machine that reads and executes very simple instructions.

RISC-V is an *instruction set architecture* (ISA), which basically means it specifies the basic instructions a CPU
can perform: add two things, subtract values from each other, and so on. These are the smallest possible steps a program
can be broken down into. Your computer at home does exactly the same thing, although most Intel CPUs use x86, a different ISA.
x86 has more complex instructions, like adding with offsets or doing multiple things in a single instruction.
RISC-V takes the opposite mindset: keep the CPU instructions minimal and let the programmer handle the rest. That's also
what the name means: RISC stands for Reduced Instruction Set Computer, and the V is there because it's the fifth
RISC design.

I chose RISC-V because its instruction formats are much more regular, which makes it simpler to emulate, and because
I agree with the mindset of keeping the assembly simple.

For now, we'll treat the CPU as the simplest machine possible. Real CPUs do all kinds of clever optimisations to run faster,
but none of that is needed yet. Let's just understand the assembly first.

### Registers

To actually add or subtract numbers, the CPU needs a place to store them. These are *registers*. You can see them
as variables, except the CPU has a fixed amount of them, and each register holds exactly one number. To add two
numbers you would, for example, specify `r1 = r2 + r3`, and the CPU would execute it. In RISC-V assembly that
looks like this:

```asm
add x1, x2, x3    # x1 = x2 + x3    "add": sum two registers into a third
```

Most instructions follow the same pattern: the instruction name, then the destination register, followed by the inputs.
Here are the basic ones:

```asm,runnable expect=t0:5|t1:3|t2:8|t3:2|t4:15|t5:1|t6:15
li   t0, 5          # t0 = 5             "load immediate": put a constant in a register
li   t1, 3          # t1 = 3
add  t2, t0, t1     # t2 = t0 + t1 = 8
sub  t3, t0, t1     # t3 = t0 - t1 = 2   "subtract"
mul  t4, t0, t1     # t4 = t0 * t1 = 15  "multiply"
div  t5, t0, t1     # t5 = t0 / t1 = 1   "divide": integer division, remainder dropped
addi t6, t0, 10     # t6 = t0 + 10 = 15  "add immediate": the constant is baked in
```

Notice how every instruction does exactly one tiny step. There are no expressions in assembly: something like
`a = (b + c) * d` has to be split into steps yourself:

```asm
# a = (b + c) * d, with b in t0, c in t1, d in t2
add t3, t0, t1      # t3 = b + c
mul t3, t3, t2      # a = t3 * d
```

One register is special: `x0` is hardwired to always contain 0. Writing to it does nothing, and reading it
always gives 0. That might sound useless, but it's used to make RISC-V need a lot less instructions: lots of
instructions can be formed by just using `x0`, so the CPU doesn't need a separate encoding for them. For example, there
is no dedicated `mov` instruction in the hardware, moving a value from one register to another is just adding it to x0.
We'll see another example of this later.

You can find the RISC-V specifications as used in this project
[here](repo:crates/asm-to-binary/_RISCV_SPECIFICATIONS.md). It's not necessary to start memorizing them: most of it
is pretty logical, and it'll come with time. I'm only putting it here in case you don't understand an instruction.

RISC-V gives you 32 of these registers. And you can probably already see the problem: only 32? What if you need
more? What if you want to store a large string, a very large number, or an array of 50 numbers?

### Memory

This is why you also need *memory*: RAM. RAM is much slower than a register, but it has way more space; gigabytes
instead of a few dozen numbers. That is the entire tradeoff between registers and memory: with less storage it can
be way faster, and with more storage it gets way slower. So we always try to avoid using RAM and keep as much as
possible in registers.

The CPU can't do math on numbers that live in RAM directly. To use RAM you need special instructions that move
values between memory and registers: *loads* and *stores*.

Memory operands are written as offset(register), meaning
"the memory located offset bytes from the address stored in the register."

```asm
ld t5, 0(t4)    # t5 = memory at t4 + 0    "load doubleword": read 8 bytes from memory
sd t5, 8(sp)    # memory at sp + 8 = t5    "store doubleword": write 8 bytes to memory
```

![Loads move data from RAM into registers; stores move it back.](figures/load_store_data_flow.svg)

One thing to know before reading real assembly: the 32 registers have nicknames that hint at how they're used.
`t0`-`t6` are temporaries, `a0`-`a7` hold function arguments, and `sp` is the stack pointer. We'll get to the
details later; for now, they're all just registers.

### The stack and the heap

So RAM gives us lots of space, but how do we organize it? Two classic answers: the *stack* and the *heap*.

The stack is a region of memory managed by a pointer that grows dynamically. The `sp` register holds the address of the
current top of the stack, and the stack grows downwards, from high addresses to low ones.
So if you want a stack frame of 64 bytes, you move `sp` down: the new `sp` is the old `sp` minus 64. Everything between
the new and the old `sp` is now yours to use.

The heap is a contiguous region of memory that you can use however you want, and usually it grows the other way: upwards.
Every allocation pushes the end of the heap towards higher addresses.

![The stack at high addresses and the heap growing toward it](figures/the_stack_and_the_heap.svg)

You can probably see the danger: if the stack uses more memory than was reserved for it, that is a *stack overflow*.
In this simple layout, the stack and heap might meet and start overwriting each other.

Let's use the stack in a small example. We reserve some stack space, store two numbers in it, load them
back, and add them:

```asm,runnable expect=a0:12
	addi sp, sp, -16    # move sp down: reserve 16 bytes of stack space
	li   t0, 5          # t0 = 5
	sd   t0, 0(sp)      # store the 5 at sp + 0
	li   t1, 7          # t1 = 7
	sd   t1, 8(sp)      # store the 7 at sp + 8
	ld   t2, 0(sp)      # load the 5 back into t2
	ld   t3, 8(sp)      # load the 7 back into t3
	add  a0, t2, t3     # a0 = 5 + 7 = 12
	addi sp, sp, 16     # move sp back up: give the stack space back
```

Of course, nobody would store two numbers in RAM just to add them; registers alone would do fine here. But this
pattern, reserve stack space, work in it, give it back, is the blueprint of every function you'll ever see in
assembly.

### How the CPU reads instructions

Okay, so before we get into writing more assembly code, you've got to understand how the CPU actually reads
instructions, and where it reads them from. Everything is stored in memory: instructions and data. The CPU
continuously fetches new instructions out of RAM. The code is usually placed at the bottom of memory, at the
low addresses, below the heap. So the full picture of the diagram from earlier looks like this:

![How the CPU reads instructions](figures/how_the_cpu_reads_instructions.svg)

In RISC-V these instructions are stored as 4-byte codes, with each instruction having its own encoding. For
the register-to-register instructions we've seen (add, sub, mul, ...), the 32 bits are split up like this:

![The 32 bits of an instruction split into fields](figures/how_the_cpu_reads_instructions_2.svg)

`opcode` selects the instruction family, `rd` is the destination register's number, `rs1` and `rs2` are the
source registers, and `funct3`/`funct7` pick the exact operation. So `add x1, x2, x3` encodes to:

![The encoding of `add x1, x2, x3`](figures/how_the_cpu_reads_instructions_3.svg)

The fields don't line up with byte boundaries, so regroup those same 32 bits into 4 bytes:

![The same 32 bits regrouped into four bytes](figures/how_the_cpu_reads_instructions_4.svg)

Compare that to x86, where instructions vary from 1 to 15 bytes and there are many different encodings:

```x86
push rax                 # 1 byte:   50
add  eax, ebx            # 2 bytes:  01 D8
mov  rax, 0x123456789    # 10 bytes: 48 B8 89 67 45 23 01 00 00 00
```

This is why I chose to use RISC-V in this project.

The CPU keeps track of where it's at using a special register: `pc` (program counter). This register always
contains the address of the instruction that's currently being executed, so by writing a different address
into this `pc` register, the CPU can jump around to different instructions.

So the basic loop your CPU keeps executing is:
1. load an instruction from memory into the CPU (*fetch*)
2. decode the 4 bytes into an actual instruction (*decode*)
3. execute it (add, multiply, jump, ...) (*execute*)
4. add 4 to the program counter to move to the next instruction (because RISC-V instructions are 4 bytes per instruction),
   unless the instruction was a jump.

![The CPU repeats fetch, decode, execute, then advances the program counter.](figures/fetch_decode_execute_loop.svg)

### Endianness

Because RISC-V is a *little-endian* architecture, the bytes are ordered in memory with the lowest byte first:
byte 0 goes at the lowest address, byte 3 at the highest. In a big-endian architecture the bytes would be the
other way around. So if our `add` is stored at address 00, memory looks like this:

![The bytes of an add instruction in memory](figures/endianness.svg)

This is the same reason the number 0x1234 would be stored as 34 12 in memory.

You'll never have to do this by hand, though. It's the assembler's task to parse the instructions as text and
give them back as encoded bytes.

### Labels and branches

So far every program has been running from the top down, but real programs need to make decisions (ifs) or repeat
things (while/for loops). This is where labels and branch instructions come into play.

A label is just a name for a position in your code, and it doesn't take up any space. It's just a human-readable
alias for the address the next instruction is stored at:

```asm
# This loop does t0 += 1 forever
loop:               # a label named "loop"
	addi t0, t0, 1
	j    loop         # "jump": always go back to the label, so this loops forever
```

`j` is an unconditional jump: when it's read, you jump to the label it points to. All it does is set `pc` to
the target address. You also have conditional jumps,
which compare two registers before deciding to jump to the label or just to move on:

```asm
beq t0, t1, somewhere   # if t0 == t1, jump to "somewhere"  "branch if equal"
bne t0, t1, somewhere   # if t0 != t1                       "branch if not equal"
blt t0, t1, somewhere   # if t0 <  t1                       "branch if less than"
bge t0, t1, somewhere   # if t0 >= t1                       "branch if greater or equal"
```

And that's all the control flow there is: everything else can be made out of these instructions.

So let's first write a simple assembly program. No printing "hello world" just yet, we'll start by making a program that
sums the numbers from 1 to 10 and then stops.

```asm,runnable expect=t0:55
	li   t0, 0          # t0 = sum, start at 0
	li   t1, 1          # t1 = i, start at 1
	li   t2, 10         # t2 = the limit
loop:
	add  t0, t0, t1     # sum = sum + i
	addi t1, t1, 1      # i = i + 1
	ble  t1, t2, loop   # if i <= 10, go around again   "branch if less than or equal"
	                    # when we fall through here, t0 holds 55
```

Note that we didn't see `ble` (branch if less than or equal) earlier: it's a *pseudo-instruction*. It doesn't
actually exist to the CPU; the assembler translates it into `bge t2, t1, loop`, the same comparison with the
operands swapped. Pseudo-instructions like this exist to make coding in assembly easier and more human-readable.

The `j` from earlier is also a pseudo-instruction: it's really `jal x0, label`. `jal` ("jump and link") jumps
to the label and stores the address of the instruction after it into a destination register, so you can later
jump back to where you came from. That's what function calls are built on, but that will come later. By using
`x0` as the destination, the stored address is thrown away, and all that's left is a plain jump. This
is one of those instructions from before formed by using the hardwired `x0`.

So now t0 contains 55, and since we don't have any way to see a returned value or anything, we'll have to inspect
the registers. This can be done through the IDE:

1. Boot up [the IDE](https://lpc4.github.io/Full-Stack/). At the top left, click `open folder` and pick `lessons`.
2. In the file tree, open lesson one:
   [`01_sum/sum.s`](repo:programs/lessons/01_sum/sum.s). It's this exact program, plus a couple of
   extra lines that we'll get to in a moment.
3. In the top right, select the run configuration `01: sum 1..10`.
4. Click `debug`: the machine starts paused on the very first instruction.
5. Open the Inspect tool in the left rail and pick the `CPU` pane: that's the register file.
6. Step through the program with the step controls.
7. When the loop falls through to `done` (one of those extra lines), `t0` reads `37`. The debugger shows registers in hexadecimal, and
   0x37 is 55 in decimal.

Registers are only one view of the machine, the debugger has some more. The `[debug]` tab that opened when you
clicked `debug` has four views along the top: `code`, `disasm`, `memory` and `stack`. `code` is the one you've been
looking at: a read-only copy of the source that follows the pc around. Switch to `disasm` and you're looking at
the *disassembly*: the instructions as they actually sit in memory, one per row, with the address on the left,
then the raw 32-bit encoding of the instruction, then its decoded form. The arrow in the gutter marks the
instruction the pc points at, and it moves along as you step.

The disassembly is where pseudo-instructions stop being invisible. Look at the loop: where the source says
`ble t1, t2, loop`, the disassembly reads `bge t2, t1, loop`, the swapped-operand branch the assembler really
emitted. And the `j done` at the end shows up as `jal zero, done`. What you write and what the CPU runs are
close, but not the same thing. The disassembly always shows you exactly what the CPU is executing.

### Arrays and sections

Okay, so now we've made a sum from 1 to 10, but what if you want to sum up a random array? An array can be any
size, so we can't just put it all in registers, or use math tricks to sum them all (which is what you would try
to do if writing really optimized assembly). So we'll have to put the array into memory, which for our purposes
is basically unlimited: all 32 registers together hold just 256 bytes, while the VM in this project has 128 MiB
of RAM, and your own PC has gigabytes of it.

We can do this through *sections*: `.text` and `.data`. These are directives that tell the *assembler* what the
following lines are: everything after `.text` is code, and everything after `.data` is data that gets placed
into memory before the program starts. Along with that we get a new instruction, `la` ("load address"), which
puts the address of a label into a register:

```asm,runnable expect=t1:4|t2:8
.data                 # what follows is data placed in memory, not code
array:                # a label for the first byte of the array
	.dword 4            # "emit a doubleword": place a 64-bit value in memory here
	.dword 8
	.dword 15

.text                 # back to code
	la   t0, array      # t0 = the address of "array"        "load address"
	ld   t1, 0(t0)      # t1 = 4, the first element
	ld   t2, 8(t0)      # t2 = 8, the second element, 8 bytes further
```

A label in `.data` works exactly like a label in `.text`: it's just a name for an address. The only difference
is what lives at that address: data instead of instructions.

You can write the sections in any order in the source file; here `.data` comes first. In memory, though, the
code always ends up first: `.text` is placed at the start, at lower addresses, and `.data` right after it, at
higher addresses:

![Program sections laid out in the address space](figures/arrays_and_sections.svg)

This layout isn't fixed though: it's the linker (which we'll see later) that decides where each section ends up, and on other
systems the sections can be placed differently or moved around. In this project it's always `.text` first and
`.data` right after. If you have multiple assembly files that you want to combine to use together (for example if
you want to use a library written by someone else), the *linker* combines their sections and makes it into one
combined executable, with all the code and data sections placed properly.

### Breakpoints and the memory view

This is a good moment to meet the rest of the debugger, because registers alone won't show you an array. Load
[`02_array_sum/array_sum.s`](repo:programs/lessons/02_array_sum/array_sum.s), run configuration
`02: sum an array`, and click `debug` again: it's the
same idea as the snippet above, extended to four elements (10, 20, 30 and 40) and a summing loop.

Stepping one instruction at a time gets old fast, so let's set a *breakpoint*: a marked instruction where the
machine pauses on its own. Click in the gutter next to the `done` line (a dot appears there) and press resume (F5).
The machine now runs at full speed and stops the moment the pc reaches that instruction, with the finished sum sitting in `t0`.
ctrl+clicking a line's gutter or a disassembly row runs to it once without leaving a dot behind, and the `Debugger` panel
on the right keeps a list of all breakpoints, where you can add one by typing a label name like `done`
(or `done+0x8` for an offset past it) and clear them all at once. That panel also has a bulk `step` control
that steps N lines, instructions or cycles in one click, which beats spamming the step button when you want
to land somewhere a hundred instructions away. The remaining step buttons (step over, into, out) are about
function calls, so just use step over by default for now.

To see the array itself, switch the `[debug]` tab to its `memory` view. It shows a page of memory as raw
bytes: each row is 16 bytes, printed in hex on the left and as ASCII characters on the right. The address
field at the top picks where to look, and the `pc` and `sp` chips jump to the program counter and the stack
pointer. So: step past the `la t1, array` line, read the address out of `t1` in the CPU pane, and type it
into the address field. There's your array. (The `va`/`pa` toggle next to the field switches between virtual
and physical addresses; our programs don't use virtual memory yet, so both read the same bytes, and you can
ignore it until we get to operating systems.) We'll go over the `stack` view later.

Sorry for so much boring debugger documentation without any diagrams, illustrations or code. To make up for it,
here's an exercise.

### Exercise

Now, I'd like for you to try to make your own assembly program: one that creates an array with the values
10, 20, 30 and 40, and then reverses it in place, so memory ends up holding 40, 30, 20, 10. The classic
approach uses two pointers: one at the first element and one at the last. Swap the elements they point at,
move the pointers towards each other, and stop once they meet or cross. You'll need `sd` from the stack
section to write elements back into the array. You can use the built-in debugger to debug and test your
code, and you can find a blank template under
[`03_reverse_array/reverse_array_template.s`](repo:programs/lessons/03_reverse_array/reverse_array_template.s),
with a matching run configuration `03.1 (template): reverse an array`.

![Two pointers move inward as the array is reversed in place.](figures/reverse_array_two_pointer_swap.svg)

The template contains the boilerplate we haven't talked about yet. `.globl _start` plus the `_start` label
tell the machine which instruction to begin execution at; these were also the extra lines in lesson one. And
the `done: j done` at the end is an endless loop that keeps the machine spinning, so you can still inspect
the registers and memory after your program is finished. Write your array in `.data` and your code between
`_start` and `done`. If everything works, the memory view at your array's address reads 28 where 0a used to
be: the elements in reverse order, shown in hex.

If you get stuck, a finished version sits right next to the template:
[`03_reverse_array/reverse_array.s`](repo:programs/lessons/03_reverse_array/reverse_array.s),
with a matching run configuration `03.2 (solution): reverse an array`.

### Functions

With everything we've seen so far we can basically code anything we want in assembly, although it'll still
be pretty difficult, because we haven't seen one of the most important, but also most frustrating parts:
functions. The same functions you'd see in normal code can exist in assembly too, using only what we've
already covered. But in assembly we have only labels and jumps: no way to pass arguments, no way to return.
So for functions to work we'll need some *calling conventions*.

The convention for arguments: the caller puts them in `a0` through `a7`, in order, and the function leaves
its result in `a0`. If eight registers aren't enough for all the arguments, we push the rest onto the stack.
Now, a function has arguments and knows where to find them. None of this is some sort of golden rule you
have to follow in assembly, by the way: you can code your own functions that take arguments through memory,
or through temporaries, or through whatever you want. But these are conventions, and they're what most if
not all production code uses, whether it was made by a compiler or by a human.

Now that we have arguments we also need a way to return out of the function, and this is where the `jal` from
before comes back. It jumps *and* puts the address of the next instruction into the register you specify.
That's usually `ra`, the "return address" register, but it can be any register (the `j` pseudo you've been
using is just `jal` with `zero`, throwing the address away). So when we're inside a function and we're done,
we can return by jumping to the address in `ra`, which is exactly what `ret` does. A simple function that
takes its arguments in registers and needs no stack looks like this:

```asm,runnable=bare expect=a0:12
_start:
	li   a0, 7          # first argument
	li   a1, 12         # second argument
	jal  ra, max        # a0 = max(7, 12); ra = the address of the next line
	li   a1, 3
	jal  ra, max        # a0 = max(12, 3): the first result feeds the second call
done:
	j    done           # spin here forever; the final result stays in a0

max:                  # max(a, b): return the larger of a0 and a1
	bge  a0, a1, max_a  # if a >= b the answer is already in a0
	mv   a0, a1         # otherwise the answer is b   "move": pseudo for add a0, a1, x0
max_a:
	ret                 # jump back to the address in ra (ret = jalr x0, ra, 0)
```

![`jal` transfers control to the function; `ret` returns to the saved address.](figures/function_call_and_return.svg)

You'll find this one under
[`04_functions/max.s`](repo:programs/lessons/04_functions/max.s), run configuration
`04.1: call a leaf function`. Notice
how the second call reuses the result of the first: the answer is already sitting in `a0`, exactly where the
first argument goes.

But then there's an issue. What if a function wants to call another function? It'll do a `jal` of its own,
the return address in `ra` gets overwritten, and now the outer function can't return to its caller
anymore.

### Stack frames

This is where a really important concept comes in: the *function stack frame*. At the
top of the function, in what's called the *prologue*, you move `sp` down by the size of your frame (a
multiple of 16, since the convention keeps `sp` aligned to 16 bytes), and then you `sd` everything that has
to survive into the slots you just reserved: `ra` first and foremost (so it can be restored later),
plus any `s` registers you plan to use.

That last part is the other half of the convention: a call is allowed to clobber (or overwrite) any `a` or `t`
register, but the `s` registers are "saved" registers that must come out of a call unchanged,
so if you want to use one, you have to save its old value first, because it belongs to your caller. At the end of the
function, the *epilogue* undoes all of it in reverse: `ld` the saved registers back, move `sp` up, `ret`.

Here's a function that needs all of it, `max3`, which finds the largest of three numbers by calling `max`
twice:

```asm
max3:                 # max3(a, b, c): the largest of three, via two calls to max
	addi sp, sp, -16    # prologue: grow the stack by two 8-byte slots
	sd   ra, 8(sp)      # save our return address; without this, ret loops forever
	sd   s0, 0(sp)      # save s0 before using it: its value belongs to our caller
	mv   s0, a2         # move c into s0: a call may clobber any a or t register
	jal  ra, max        # a0 = max(a, b)
	mv   a1, s0         # bring c back as the second argument
	jal  ra, max        # a0 = max(max(a, b), c)
	ld   s0, 0(sp)      # epilogue: restore what we saved, in reverse
	ld   ra, 8(sp)
	addi sp, sp, 16     # give the stack space back
	ret
```

That's [`04_functions/max3.s`](repo:programs/lessons/04_functions/max3.s), run configuration
`04.2: nested calls`. Its frame looks
like this on the stack:

![The stack with frames from nested calls](figures/functions_2.svg)

Understanding function frames was one of the hardest things for me personally: I couldn't really visualize
it properly. So I hope these diagrams made it a bit cleaner. You can also watch it happen live: run
`max3`, put a breakpoint on its `jal ra, max` line, and use the `sp` chip in the memory view to jump to the
stack, where the saved `ra` and `s0` are sitting as raw bytes. This is also where the remaining step buttons
from last lesson come into play: *step into* follows a `jal` into the function, *step out* runs until the
current function returns, and *step over* runs the whole call and lands on the next line. And while paused,
the `Inspect` tool on the left has a `Call Stack` pane: one row per active function, newest on top, and clicking
a row jumps the editor to that call site.

### The frame pointer

There's one more piece of the convention worth knowing. It reserves a register, `fp` (the "frame pointer",
which is just another name for `s0`), to hold the base of the frame: the value `sp` had when the function
was entered. The point is that offsets from `fp` stay fixed for the whole function, while `sp` can change in
the middle of one, for example when you push extra arguments for a call. Without `fp`, finding your locals
after `sp` moved means recalculating every offset from wherever `sp` happens to be now; with it, a local is
just always at, say, `-24(fp)`. So for manual assembly programming the frame pointer is a nice tool to keep
around.

This is a lot of theory, so here's what a frame with locals and a frame pointer looks like, to really make it clear:

```asm
example:
	addi sp, sp, -32    # prologue: frame of four 8-byte slots

	# We will be overwriting the caller's FP (which is a saved register) so we need to push it to stack.
	sd   fp, 16(sp)     # save the caller's frame pointer (fp is s0)
	addi fp, sp, 32     # fp = base of this frame: the old sp (we did -32 in the prologue)
	sd   ra, -8(fp)     # save the return address at -8(fp)
	li   t0, 42
	sd   t0, -24(fp)    # a local variable; same slot as 8(sp), for now
	addi sp, sp, -16    # sp moves mid-function...
	ld   t1, -24(fp)    # ...but the local is still at -24(fp)
	addi sp, sp, 16
	ld   ra, -8(fp)     # epilogue: restore in reverse
	ld   fp, -16(fp)
	addi sp, sp, 32
	ret
```

![A stack frame addressed from the frame pointer](figures/the_frame_pointer.svg)

Most compilers drop `fp` and track the offsets themselves to be more efficient: no need to burn a register and two
extra instructions per function keeping it up to date. Our examples from before do the compiler thing, pure `sp` offsets,
which is also why `max3` was free to use `s0` as an ordinary saved register.

Now let's tie the whole thing together with one last example: factorial. It's a good fit, because
`fact(n) = n * fact(n - 1)` makes a call, and it has to carry something across it: the original `n`, so it can
multiply by it once the call comes back. It can't be trusted to a register, because the `jal` into `fact` is
allowed to clobber it. So each call keeps its own `n` in its frame, addressed off `fp`. Here's the whole thing,
we'll be going over it piece by piece. It's
[`05_stack_frames/factorial_stack.s`](repo:programs/lessons/05_stack_frames/factorial_stack.s), run configuration
`05.1: factorial with a frame`:

```asm,runnable=bare expect=a0:120
_start:
	li   a0, 5           # n
	jal  ra, fact        # a0 = fact(5) = 120
done:
	j    done            # spin here forever; the result stays in a0

fact:                  # fact(n): n < 2 returns 1, else n * fact(n - 1)
	addi sp, sp, -32     # prologue: reserve a 32-byte frame
	sd   fp, 16(sp)      # save the caller's frame pointer before we overwrite fp
	addi fp, sp, 32      # fp = base of this frame: the old sp
	sd   ra, -8(fp)      # save the return address at -8(fp)
	li   t0, 2
	blt  a0, t0, base    # n < 2: fact(n) = 1, so there is nothing to compute
	sd   a0, -24(fp)     # keep n in a local, so the call cannot lose it
	addi a0, a0, -1
	jal  ra, fact        # a0 = fact(n - 1)
	ld   t1, -24(fp)     # reload n
	mul  a0, a0, t1      # a0 = n * fact(n - 1)
	j    epilogue
base:
	li   a0, 1           # fact(n) = 1 for n < 2
epilogue:
	ld   ra, -8(fp)      # restore what we saved, in reverse
	ld   fp, -16(fp)
	addi sp, sp, 32
	ret
```

Let's read it in parts. The prologue reserves four slots and sets `fp` as early as it can, so everything after
it can address the frame off `fp`. The one thing that has to happen first is saving the caller's `fp`: once
`addi fp, sp, 32` runs, that old value is gone, so we `sd` it while `fp` still holds it (that lone `16(sp)` is
the same slot as `-16(fp)`). From there `fp` is the base of the frame and the return address goes to `-8(fp)`.

```asm
	addi sp, sp, -32     # prologue: reserve a 32-byte frame
	sd   fp, 16(sp)      # save the caller's frame pointer before we overwrite fp
	addi fp, sp, 32      # fp = base of this frame: the old sp
	sd   ra, -8(fp)      # save the return address at -8(fp)
```

This is still all just saving registers on the stack to use them ourselves. The third slot, at `-24(fp)`, is our local: `n`.
The fourth, at `-32(fp)`, goes unused: `ra`, `fp`, and `n` only need three 8-byte slots, but frames have to be a multiple of 16,
so we round up to 32 anyway. It's the same shape as the frame in the example just above, unused slot and all. So the
whole frame `fact` builds looks like this, with both names for every slot, `fp` offsets on the left and `sp` offsets on the right:

![The `fact` frame with `fp` offsets on the left and `sp` offsets on the right](figures/the_frame_pointer_2.svg)

Next comes the base case, which is the only reason the recursion ever stops:

```asm
	li   t0, 2
	blt  a0, t0, base    # n < 2: fact(n) = 1, so there is nothing to compute
```

If `n` is 0 or 1 then `fact(n)` is 1. That value isn't already sitting in `a0` (`a0` still holds `n`), so the
base case has to set it explicitly and jump past the multiply that follows it. Otherwise, we make the call:

```asm
	sd   a0, -24(fp)     # keep n in a local, so the call cannot lose it
	addi a0, a0, -1
	jal  ra, fact        # a0 = fact(n - 1)
	ld   t1, -24(fp)     # reload n
	mul  a0, a0, t1      # a0 = n * fact(n - 1)
```

Before the call we put `n` in `-24(fp)`, because the call is about to overwrite `a0`. When it returns `a0`
holds `fact(n - 1)`, so we reload `n` from its local (this is the whole reason we saved it) and multiply the
two together, leaving the product in `a0`. The epilogue restores `fp` and `ra` and hands the frame back, just
like before. Notice that `-24(fp)` names the same local the whole way through, even with a call running in the
middle.

Now run it. The run configuration opens straight into the debugger with a breakpoint on
`fact`, so you land on the very first call. Open the `stack` view (it sits next to `code`, `disasm`, and
`memory`): it draws the live stack from the top down to `sp`, one 8-byte slot per row. Each frame gets a header
with the function's name and its size, the saved `ra` and saved `fp` are labelled, the local shows up as
`local` (and reads `(unused)` until it's actually been written), and the `fp` and `sp` arrows mark the current
frame. Continue past the breakpoint, or use *step into* to follow each `jal`, and watch the frames pile up.

At its deepest, `fact(5)` walks all the way down, `fact(5)` -> `fact(4)` -> `fact(3)` -> `fact(2)` ->
`fact(1)`, where `fact(1)` finally hits the base case. Five frames are live at once, each an identical 32-byte
block, and the stack looks like this:

![Five live `fact` frames on the stack](figures/the_frame_pointer_3.svg)

The innermost `fact(1)` took the base-case branch before storing anything, so its local slot is still
`(unused)`; every frame above it has its `n` saved and is parked on its `jal`, waiting for the call below to
return. As those returns come in, `ret` and the epilogue pop the frames off one at a time, and the answers
multiply their way back up to `fact(5) = 120`.

Now we can see how recursion can cause a *stack overflow*. Each call pushes the stack a little further down, and a
long enough chain eventually uses more memory than was reserved for the stack. In this layout, it can then
reach the heap and corrupt its data.

There's also a less known sibling error: *stack underflow*. The stack grows downwards, so the place it started from is the
highest address it will ever use, and an underflow is `sp` climbing back past that starting point: popping more than
was ever pushed. This doesn't happen often, but it can occur when writing assembly by hand and messing up the stack
offsets, an epilogue that gives back more than its prologue reserved.

### Exercise

Now that we've seen all of these examples, I'd like for you to try and write a function yourself, following the
conventions this time. The function is `sos(a, b) = square(a) + square(b)`, and `square` is already written
for you: it's a *leaf* function, one that calls nothing else, so it needs no frame at all. `sos` is the
opposite: it calls `square` twice, and every call is free to clobber `a0` and `a1`, so anything you still
need after a call has to live in the frame. Both `b` and the result of the first `square` have to survive a
call, so both get a slot.

The template is
[`05_stack_frames/sum_of_squares_template.s`](repo:programs/lessons/05_stack_frames/sum_of_squares_template.s),
run configuration
`05.2 (template): sum of squares`, again in the [browser IDE](https://lpc4.github.io/Full-Stack/). It gives
you `_start`, `square`, and a `sos` split into four commented steps to fill in, with the frame plan written
out in the comments above it:

![The planned stack frame for `sos`](figures/the_frame_pointer_4.svg)

Same 32-byte shape as `fact`, except this time there is no unused slot. Write the prologue, the two calls
with their saves and reloads in between, and the epilogue. If it works, `a0` holds 25 when the program
reaches `done`: `sos(3, 4) = 9 + 16`. Use the `stack` view while you're in there, it will show you your own
frame being built slot by slot, and a wrong offset is a lot easier to see there than in the registers.

The solution sits next to it as
[`05_stack_frames/sum_of_squares.s`](repo:programs/lessons/05_stack_frames/sum_of_squares.s), run configuration
`05.3 (solution): sum of squares`, which opens with a breakpoint on `sos`.

And that's the whole assembly chapter. You can now read and write RISC-V by hand: registers, memory, the
stack, branches, sections, and functions that call other functions without losing their place. Everything
from here on is built on top of exactly this.

## Memory Mapped Input / Output

So far every program we've made has been kind of useless. If you tried to run one on your own computer and
watched the console, nothing would happen. We only know they work because we can inspect the registers while
they execute. And at the end of our programs the machine just keeps spinning idly forever. We need something
that actually starts outputting stuff. This is where *memory mapped input / output* (MMIO) comes in.

To understand this fully, we'll have to start looking into more of the computer's hardware.

### Bit Representations

Every register we've used so far is 64 bits wide. That's the entire storage: 64 switches, each on or off.
What those 64 bits mean is not stored anywhere. Read them as an unsigned integer and they count from 0 to
18,446,744,073,709,551,615. Read the exact same bits as a signed integer and they run from
-9,223,372,036,854,775,808 to 9,223,372,036,854,775,807.

Take this bit pattern:

![The same eight bytes read as string, signed, and unsigned](figures/bit_representations.svg)

All three are correct. Those eight bytes are of a string we're going to print in a moment, but if you load the same
bits into a floating-point register, a floating-point instruction will happily tell you they're a number with
forty-one digits in front of the decimal point, and it isn't wrong. Nothing in either register says "this is text".

That's why `add` and `fadd` are separate instructions using different register files, why `ld` and `lb` load
different amounts of data, and why there are signed and unsigned comparisons. A register only stores bits. The
meaning arrives with the instruction you pick.

### An address is also just a number

Here's where that matters for memory. An address is a number in a register, and registers are 64 bits wide,
so the biggest address we can even write down is that same 18,446,744,073,709,551,615. That's 16 exbibytes
of addressable memory, about 18.4 million terabytes.

Nobody has that. A high-end machine today has something like 32-64 GiB of RAM. 64 GiB out of a full
64-bit address space is one 268,435,456th of it, roughly 0.0000004%. We are using an almost unimaginably
small part of the addresses we're able to name.

So the addresses are essentially free, and that changes how you use them. Instead of packing everything
together at the bottom, a machine spreads things out and gives each one its own region, with massive empty
gaps in between. That's called a *memory map*, and it's decided by whoever builds the machine. Here's mine,
straight out of the
[`bus.rs`](repo:crates/virtual-machine/src/bus.rs#L45) memory map, where every one of these bases is a constant the bus
uses to decide which device an address belongs to:

![The memory map: RAM, ROM, and devices](figures/an_address_is_also_just_a_number.svg)

Only two of those rows are memory. `ROM` and `RAM` hold bytes. Every other row is a *device*, and it lives in the same
address space, reachable with the same `ld` and `sd` instructions we've been using all along.

Which means loading and storing to certain special addresses has side effects. Store a byte to the UART address and it
comes out on the console. Store one to the SYSCON address and the machine turns off. So let's start with our first
MMIO device: the UART, and after that SYSCON. The framebuffer, keyboard, CLINT
and PLIC will be covered once we get to operating systems.

### UART

UART stands for Universal Asynchronous Receiver/Transmitter, and it's essentially just a black box we can throw
bytes into.

In real hardware it's a chip on the board, the thing sitting behind an actual serial port. The classic one is the
NS16550A. You hand it a byte, and it shifts that byte out one bit at a time over a single wire, then does the same
in reverse for anything coming back in. You talk to it through a small bank of registers at a fixed address. The first
byte of that bank is the transmit register, and writing to it is what sends a character.

We're emulating that chip in
[`uart.rs`](repo:crates/virtual-machine/src/devices/uart.rs), a subset of the NS16550A. There's no
wire and no bits being shifted, obviously: writing a byte to the transmit register appends it to a buffer, and the
IDE drains that buffer into the console window. From the program's side nothing about that is visible. It stores a
byte to an address, and a character appears.

So how do we actually use this in our code? The UART sits at `0x1000_0000`, and its transmit register is the first
byte there, so the whole thing is two instructions: put the address in a register, store a byte to it. That's
[`06_uart/hello_raw.s`](repo:programs/lessons/06_uart/hello_raw.s):

![`sb` writes a byte to the UART transmit address, which appears in the console.](figures/uart_mmio_output.svg)

```asm,runnable=bare
.globl _start
_start:
	li   t0, 0x10000000  # UART transmit register, fixed by the machine
	li   t1, 72          # 'H'
	sb   t1, 0(t0)
	li   t1, 105         # 'i'
	sb   t1, 0(t0)
	li   t1, 10          # newline character
	sb   t1, 0(t0)
done:
	j    done
```

Note that we're using `sb`, store byte, and not the `sd` we've used everywhere else. The transmit register is one
byte wide, and a character is one byte: 72 is `H` in ASCII, 105 is `i`, 10 is a newline. Writing eight bytes at once
here would make no sense, we're feeding the device one character at a time.

Let's test it in our IDE. Open the file, pick its run configuration, and press run, same as the lessons before. No
debug mode needed this time: the console pops up and prints

```text
Hi
```

Alright, let's now put this behind some abstraction, so we don't always need to load these magic numbers everywhere.
This is [`06_uart/print_char.s`](repo:programs/lessons/06_uart/print_char.s), and it's nothing new: the UART
store moved into a function. `print_char` calls
nothing else, so it's a leaf, and just like `square` back in lesson 05 it needs no frame at all:

```asm,runnable=bare
.globl _start
_start:
	li   a0, 72          # 'H'
	jal  ra, print_char
	li   a0, 105         # 'i'
	jal  ra, print_char
	li   a0, 10          # '\n'
	jal  ra, print_char
done:
	j    done

print_char:            # print_char(c): send the byte in a0 to the console
	li   t0, 0x10000000
	sb   a0, 0(t0)
	ret
```

Same output, but the address only appears once now, and everything above it just talks about characters.

And now let's create a full print function, combining everything we've seen up until now: stack frames, memory, and
our new IO. `print_string` takes a pointer and prints bytes until it hits a zero. It calls `print_char`, so it is
not a leaf: the call clobbers `ra`, and it clobbers `a0`, which is also where our pointer lives. Both have to
survive, so both get a slot in a frame:

```asm,runnable=bare
.data
msg:
	.asciz "Hello, world!\n"   # "asciz": the bytes in ASCII + a trailing zero (c-string)

.text
.globl _start
_start:
	la   a0, msg
	jal  ra, print_string
done:
	j    done

print_char:            # print_char(c): send the byte in a0 to the console
	li   t0, 0x10000000
	sb   a0, 0(t0)
	ret

print_string:          # print_string(s): print bytes from s until the zero
	addi sp, sp, -32     # prologue, exactly like lesson 05
	sd   fp, 16(sp)
	addi fp, sp, 32
	sd   ra, -8(fp)      # print_char overwrites ra, so save ours first
	sd   a0, -32(fp)     # and keep the pointer: a0 is about to be the character
ps_loop:
	ld   t1, -32(fp)     # t1 = current pointer
	lb   a0, 0(t1)       # "load byte": a0 = the byte it points at
	beqz a0, ps_done     # "branch if equal to zero": jump if a0 is 0, so a zero byte ends the string (also a pseudo instruction)
	jal  ra, print_char
	ld   t1, -32(fp)     # reload: the call may have clobbered t1
	addi t1, t1, 1       # advance one byte
	sd   t1, -32(fp)
	j    ps_loop
ps_done:
	ld   ra, -8(fp)      # epilogue
	ld   fp, -16(fp)
	addi sp, sp, 32
	ret
```

`.asciz` is the string version of the `.dword` we used for arrays: it drops the characters into `.data` and adds a
zero byte at the end, so the loop has something to stop on.

Only one thing left: that ugly j done loop we've been using in every program up until now.

### SYSCON

SYSCON is short for system controller. In real hardware it does a lot of different things: it's less a single device
than a bunch of control registers glued together, handling resets, clocks, power domains, and whatever other board-specific
stuff the chip designers needed somewhere to put. We're only interested in one of those things, the power management.

`0x1001_0000` is the SYSCON device's address (see the memory map), and storing to it shuts the machine off, with
whatever value we store becoming the program's exit code. So let's do that. Wrapped in a function, exactly like we
did with the UART:

```asm
exit:                  # exit(code): halt the machine with the code in a0
	li   t0, 0x10010000  # SYSCON, the power-off device
	sd   a0, 0(t0)       # the machine stops here: nothing below this runs
```

There's no `ret` at the end, and that isn't an oversight. `exit` never returns: the store is the last instruction the
machine ever executes, so there's nothing to return to.

So `_start` no longer needs anywhere to park the machine. The spin loop goes, and a call to `exit` takes its place:

```asm
_start:
	la   a0, msg
	jal  ra, print_string
	li   a0, 0           # exit code 0: nothing went wrong
	jal  ra, exit
```

The finished program, with `print_char`, `print_string` and `exit` all together, is
[`06_uart/hello_world.s`](repo:programs/lessons/06_uart/hello_world.s).

And finally, after six lessons and a fair amount of hard work, we've printed hello world. Who would have thought
printing hello world could be this complicated.

## The same code on QEMU

So far everything we've written has been tested on my own virtual machine, so you might wonder if this runs on real hardware
too. That's what we'll be looking at now. Let me preface this by saying that I won't really be going over any new concepts in
this chapter, I'll just show you how the code we've been running on my emulated RV64 CPU would run on different hardware.

To demonstrate this we'll be using QEMU. QEMU is an emulator you've probably heard of. Most people use it as something
that runs Linux on their laptop, but that's only part of what it does. In that mode it emulates a whole PC, but it can
also emulate a specific board: a CPU wired with a set of devices at specific addresses. And that's
exactly the mode we want. It is the same job my VM has been doing the entire time, just written by
people who have been doing it way longer.

The board we'll use is called `virt`. It doesn't correspond to any chip you can buy: it's a board the QEMU authors
invented, with a RISC-V CPU and a small set of standard devices at fixed addresses. That sounds like cheating,
but the CPU is a real RV64 implementation and the devices are real device designs, so code written for `virt` is written
against the same rules as code for silicon. The board is fictional, but it's CPU and devices follow the same 
specifications as real hardware.

The port is [`07_qemu/hello_world.s`](repo:programs/lessons/07_qemu/hello_world.s). Two things change
from our previous hello world program.

### Setting the stack pointer

Something has to get your program from a file on disk into the machine's memory and start it running, and that
something is called a *loader*. It copies the program's bytes to the addresses they belong at, and while it's in
there it can set a few things up on your behalf.

That's what has been happening behind the scenes this whole time: our loader has been setting `sp` to the top of
the stack before any of our code runs. We can do that because we always know how memory is divided, since it's emulated
and I chose the layout myself. QEMU makes no such promises. It hands you a machine and starts executing, and whatever is
in `sp` is not something you can rely on. So we have to set it up ourselves.

```asm
_start:
	li   sp, 0x88000000  # virt gives 128 MB of RAM at 0x80000000, so this is the top
	la   a0, msg
	jal  ra, print_string
	li   a0, 0
	jal  ra, exit
```

### The power-off device

The UART in our emulated environment is exactly the one QEMU has: the same device, at the same address. So
`print_char` and `print_string` port with no changes at all. SYSCON is where our machine is a simplification. QEMU
virt's power-off device is the SiFive test finisher and it's mounted at `0x10_0000`. There you'll find a 32-bit register
that takes magic values instead of raw exit codes:

| Value | Meaning |
| --- | --- |
| `0x5555` | pass, QEMU exits 0 |
| `0x3333` | fail, with the exit code in the upper 16 bits |
| `0x7777` | reset |

So we'll have to change `exit` accordingly.

```asm
exit:                  # exit(code): virt's poweroff device wants magic values, not raw codes
	li   t0, 0x100000    # the SiFive test finisher
	beqz a0, exit_pass   # if zero pass 0x5555
	slli a0, a0, 16      # a code sits in the upper 16 bits
	li   t1, 0x3333      # 0x3333 = "fail, code attached"
	or   a0, a0, t1      # or the exit code (upper 16 bits) so we have a0=[exit:16][magic:16] (can't use ori here!)
	sw   a0, 0(t0)       # sw instead of sd
exit_pass:
	li   a0, 0x5555      # 0x5555 = "pass", qemu exits 0
	sw   a0, 0(t0)       # sw instead of sd
```

The `li` and `or` pair is pretty interesting, because the obvious version doesn't assemble. You might expect
`ori a0, a0, 0x3333`. But `ori` is an I-type instruction, and in the specifications we can see that I-type
has exactly 12 bits for its immediate. Signed, that's a maximum of 2047, and `0x3333` is 13107. It doesn't fit in the
instruction, so the constant has to be built in a register first (with the li pseudo-instruction) and combined from there.

![An I-type immediate has 12 signed bits, so `0x3333` must be built in a register.](figures/i_type_immediate_range.svg)

### The linker script

There's one more file next to the assembly:
[`link.ld`](repo:programs/lessons/07_qemu/link.ld). Up until now the IDE has been deciding where our code and
data end up in memory. With QEMU, we need to do this ourselves.

```ld
ENTRY(_start)

SECTIONS
{
	. = 0x80000000;
	.text   : { *(.text*)   }
	.rodata : { *(.rodata*) }
	.data   : { *(.data*)   }
	.bss    : { *(.bss*) *(COMMON) }
}
```

That's all of it, but the syntax is its own little language, so it's worth taking apart.

`ENTRY(_start)` records which label the program begins at. `SECTIONS { ... }` is the block that describes the layout,
and everything interesting happens inside it.

`.` is the *location counter*: the address we're currently placing things at. Setting it to `0x80000000` says "start
putting things here", and it advances on its own as sections get placed, so we only have to set it once at the top.
That address isn't a free choice either, it's just where the RAM happens to be. If you put the
program anywhere else, it might land on a device, or on nothing at all.

Then each line places one section, and the two halves mean different things:

![A linker script placing sections](figures/the_linker_script.svg)

So the left of the `:` is the section we're producing, and the braces say what goes into it. Inside, the `*` before
the bracket is a filename wildcard meaning "every input file we were given". We only have one file here, but a real
program is built from dozens, and every one of them contributes its own `.text` to the single `.text` that ends up in
memory. Gathering those pieces and combining them is most of what a linker is for.

![The linker combines matching sections from multiple input files.](figures/linker_section_merge.svg)

The `*` at the end of `.text*` is a second wildcard, this time over section names: take `.text`, but also anything
starting with `.text`. Our handwritten file only ever produces a plain `.text`, but compilers often split code into
one section per function, with names like `.text.print_char`, so that unused ones can be dropped. The wildcard means
we catch those too.

`*(COMMON)` on the `.bss` line is a leftover from old C, which had a way of declaring a variable without quite
defining it. Those go into a category called `COMMON`, and this sweeps them in with the rest of the zeroed data.
Nothing in this chapter produces any, but it only costs one line to be safe.

`.text` is our instructions and `.data` is the `msg` string, both of which we've used before.

The other two are new, and both are empty in this program. They're worth going over anyway, because they're the two
other places data can go, and you'll see them the moment you write something bigger than hello world.

`.rodata` is read-only data: constants the program reads but never writes. String literals usually live here rather
than in `.data`, and on a machine with memory protection this section can be marked unwritable, so a bug that tries
to scribble on a constant gets caught instead of quietly corrupting it. We have no protection set up, so for us, it
would behave exactly like `.data`.

`.bss` is the interesting one. The name is an acronym from 70 years ago that no longer describes anything (I looked it
up, and it stands for "block started by symbol"), so just think of it as the section for data that starts out as all zeroes.
Say you want an array of a thousand elements, all initialized to 0. You could put a thousand `.dword 0` lines in
`.data`, and the assembler would write eight thousand zero bytes into your executable file. That's a lot of
filesize for nothing. `.bss` is the fix, and it comes with one new directive:

```asm
.bss
buf:                  # a label, like in .data
	.space 8000         # "reserve 8000 bytes here"
```

`.space` doesn't emit anything. It records a size, so all that ends up in the file is a note saying this program
needs 8000 bytes of zeroes at `buf`. Whoever loads the program is expected to provide them by zeroing that region
before the first instruction runs.

Here, QEMU is that loader. When we give it `hello.elf`, it reads the sizes stored in the file and fills `buf`'s
place in RAM with 8000 zero bytes before starting the program. So `buf` is guaranteed to contain zeroes when
`_start` begins.

That changes if you boot a raw binary, because a raw file has no ELF segment metadata telling a loader which extra
memory should start at zero. A boot protocol could do this separately, but otherwise the startup code must clear `.bss`
before using it.

So both sections stay in the script even though they're empty. Leaving them out doesn't make the program smaller,
so it's nice to have.

To summarize: All the script does is choose where our own program sits inside a map that already exists.
Without the script the linker falls back on its defaults, which are meant for programs running under an operating
system. And for those types of programs the entire layout is way different.

### Building it

One more thing changes here: the toolchain isn't mine anymore. Every program so far went through the assembler I wrote
for this project, which means my assembler was the only thing that has ever agreed the code was valid.

Here we use the GNU assembler and linker, the same ones used to build Linux itself. They have no idea this
project exists. And they take the file unchanged: the pseudo-instructions, the section directives, `.asciz`, all of
it assembles as written, because none of it was ever mine. It's just RISC-V.

```sh
sudo apt install qemu-system-misc gcc-riscv64-linux-gnu
```

```sh
riscv64-linux-gnu-gcc -nostdlib -no-pie -T link.ld \
	-Wl,--build-id=none,--no-warn-rwx-segments \
	-o hello.elf hello_world.s
```

Four flags, and they all exist for the same reason: `gcc` assumes it's building a normal program for a normal
operating system, and each one turns off an assumption that isn't true here. The first three are ordinary enough that
[the gcc](https://gcc.gnu.org/onlinedocs/gcc/Link-Options.html) and
[ld](https://sourceware.org/binutils/docs/ld/Options.html) documentation will tell you more than I can: `-nostdlib`
drops the C runtime so nothing is added around `_start`, `-no-pie` says our program lives at one fixed address rather
than wherever it gets loaded, and `-T link.ld` uses our layout instead of the default one. `--build-id=none` removes an
identifying prefix the linker usually adds to programs.

(`--no-warn-rwx-segments` only silences a warning about our one memory region being readable, writable and executable
at once. Could be a concern on a machine with memory protection, which we don't have.)

```sh
qemu-system-riscv64 -machine virt -nographic -bios none -kernel hello.elf
```

These are QEMU's own options, and its
[documentation](https://www.qemu.org/docs/master/system/target-riscv.html) covers them properly.
But `-bios none` is worth knowing here. Normally QEMU boots a small firmware called OpenSBI before
handing control to our program. `-bios none` skips that firmware, but QEMU's reset stub still runs
first and transfers control to our ELF entry point. Our code is the first program that runs, not
literally the machine's first instruction.

And now we see that the machine prints, powers off and puts us back into the shell with exit code 0. Everything works!

### How real is real enough

The QEMU virt machine still isn't real hardware. It's a well-behaved RV64 machine that doesn't exist in silicon, and
a fair objection is that I've swapped one emulator for another.

QEMU does have machines that model chips you could actually buy. `sifive_u` emulates the FU540-C000, the chip on the
HiFive Unleashed board. It's still RV64, and its RAM still starts at `0x80000000`, so the assembly in this chapter is
the right kind of assembly for it. What it doesn't have is our devices. Its console is a different UART design, at a
different address, driven a different way, so `print_char` would need rewriting. And it has no power-off device at
all, so `exit` can't exist on it: the machine has no way for a program to ask to be turned off.

![The project VM, QEMU virt, and QEMU's `sifive_u` model share RV64 but expose different devices.](figures/riscv_machine_comparison.svg)

Which is the actual lesson here. The instructions port perfectly. Everything else depends on the specific
machine you're on. Adding two numbers is the same everywhere RISC-V is, printing a character or exiting is not,
because those aren't really CPU operations at all, they're requests to hardware that happens to be sitting at an address
someone chose.

So we've got to abstract this out even further. An operating system can hide hardware differences behind a stable interface:
when you call `println`, your program does not need to know which UART the machine has or where it lives. But even with
that interface, writing everything in assembly takes a lot of effort. So next we'll move up to a higher-level language and
build the compiler that turns readable programs into the instructions we now understand.

## High-Level Languages

As I said before, writing code in assembly does not scale very well. It is slow to write, difficult to debug and
has a lot of repetition. That does not make large assembly programs impossible: the original
[RollerCoaster Tycoon](https://www.chrissawyer.com/faq3.htm) was written almost entirely in x86 assembly.
It just takes a lot more work.

That is why people started inventing higher-level languages. Early languages such as Lisp and COBOL already made
programs easier to express than assembly, and C later made it practical to write readable systems software
without spelling out every CPU instruction by hand.

We will not be going over C in this chapter, though. Instead, we will use a custom language I invented: HLL, which
stands for High-Level Language (I know, original). You can find its full
[language specification here](repo:crates/hll-to-ir/_LANG_SPECIFICATIONS.md).

It started as a hobby project, and after a lot of iteration it finally feels ready to showcase. So let's have a look.

### Compilers and IR

Before we take a look at HLL, we first need to know what compilers do, and how they do it. This makes some of the design
decisions easier to understand.

A compiler translates a higher-level language into something a machine can execute. For a native program that
usually ends in machine code for an instruction set such as x86 or RISC-V. Other compilers target bytecode for a
virtual machine instead, like WebAssembly. This translation is necessary because a CPU cannot execute
higher-level instructions directly of course.

The translation is not very straightforward. The compiler has to turn expressions into individual instructions,
decide where values live, pass function arguments, create stack frames and do all the other work we previously had
to do ourselves in assembly.

It is possible to translate HLL directly into RISC-V, but doing everything in one step would get messy very quickly.
Instead, the compiler first translates it into an *intermediate representation*, or IR. You can think of IR as a
small language between HLL and assembly. It makes each translation step simpler and gives the compiler a convenient
place to optimize the program.

![HLL moves through IR and RISC-V assembly before becoming machine code.](figures/compiler_pipeline.svg)

This separation also makes the compiler easier to port. If I wanted HLL programs to run on an x86 machine, the HLL
front end could stay the same. I would only need to replace the back end that translates IR into RISC-V with one that
translates IR into x86.

### An example

All right, let's make this pipeline more concrete. Here is about the smallest useful HLL program we can write:

```hll, runnable
main: () -> i32 {
	return 1 + 1
}
```

[Open the complete compiler-pipeline lesson in the IDE](repo:programs/lessons/08_hll/compiler_pipeline.hll).
The link opens the Lessons workspace and selects its matching `08: HLL compiler pipeline` run configuration. The
folder also contains the [IR code](repo:programs/lessons/08_hll/compiler_pipeline.ir), the
[RISC-V code](repo:programs/lessons/08_hll/compiler_pipeline.s) and the
[build manifest](repo:programs/lessons/08_hll/compiler_pipeline.build).

This defines a function named `main`, which is where the program starts. The empty parentheses mean it takes no
arguments, `i32` is its return type and the body returns the result of `1 + 1`.

The first stage of the compiler translates that program into this IR:

```ir
define main() -> i32 {
entry:
	$0 = add i32 1, 1
	ret $0
}
```

The expression is already broken into simpler steps. `add i32 1, 1` adds two 32-bit integers and stores the result
in the temporary `$0`. The next instruction returns that temporary. IR still has useful concepts such as types and
an unlimited supply of temporary values, but every operation is now explicit and easier to translate.

The back end then translates that IR into RISC-V assembly. With the compiler's extra comments removed, the
generated function looks like this:

```asm
.section .text
main:
	addi sp, sp, -16
	sd   ra, 8(sp)
	sd   s2, 0(sp)

main__entry:
	addi  t0, zero, 1
	addi  t1, zero, 1
	add   s2, t0, t1
	addiw s2, s2, 0
	addi  a0, s2, 0

	ld   s2, 0(sp)
	ld   ra, 8(sp)
	addi sp, sp, 16
	jalr zero, 0(ra)
```

The middle of the function is the translation we were looking for: load both `1`s, add them and move the result
into `a0`, the return-value register. The rest is the function prologue and epilogue we saw earlier. Even this tiny
program already shows why having the IR as a separate step is useful.

The manifest sets `out = "out"`. I've already built this lesson with the native `fsc` tool, so the exact
[compiler generated IR](repo:programs/lessons/08_hll/out/ir/compiler_pipeline.ir) and
[compiler generated assembly](repo:programs/lessons/08_hll/out/asm/compiler_pipeline.s)
are in that folder and can be opened in the browser too.

### From HLL to an executable

When you press `build` on a HLL file in the IDE, the spine on the left side of the code lights up. This spine shows you
all the intermediate stages that the code goes through, from HLL code to an executable.

Our pipeline diagram from before ended at machine code, but the IDE has one more button next to `IR` and `ASM`: `ELF`. ELF
stands for *Executable and Linkable Format*, and it is the final file produced by the linker.

Before, we encoded our assembly directly, placed those bytes into a CPU and started executing them. There were
no extra layers because our assembly was the entire program and it already knew the exact machine it was
running on. We can still do that, but it is not a great way to build reusable programs. Every program would have to
set up everything itself and know details such as the stack location, MMIO addresses and how to stop the machine.

Instead, we can put a small amount of supporting code around the program. It prepares whatever the program needs
before `main` begins and deals with the result after `main` returns. This supporting environment is called a
*runtime*.

A reusable collection of this supporting code and other common functionality is called a *standard library*.
Later, an operating system will provide another layer between programs and the hardware. We will build all of that
later. For now, the important point is that they keep machine-specific details out of our HLL file.

`main` is not actually the first function that runs. The real entry point is `_start`, and its address is recorded in
the ELF. Depending on the target, `_start` could prepare the stack pointer, initialize memory or set up access to
devices before calling `main`. In this example the VM has already prepared the stack, so `_start` calls `main` and
waits for it to return. When `main` returns `2`, execution goes back to `_start`, which stops the machine with exit
code `2`.

The compiler turns our HLL program and the supporting code into separate *object files*. Each one contains machine
code and data, but it is not a complete executable on its own. The linker combines those objects, resolves references
between them, lays out their sections and produces one ELF file. Besides the instructions and data, the ELF records
the instruction set it contains, its entry address, the sections and memory segments, their sizes and their flags.
The complete structure is described in the
[ELF specification](https://gabi.xinuos.com/elf/).

![Linking the program and its supporting code into one ELF.](figures/linking_hll_executable.svg)

This is also why the HLL source can be modular and reusable while the ELF is still machine-specific. ELF files can
contain x86, RISC-V and many other kinds of machine code, but a CPU cannot execute all of them. An ELF is also still
a file, so it has to be placed into memory before the CPU can run it. That is the loader's job. It reads the ELF
header and checks whether it understands that file. This ELF is built for RISC-V and this project's machine
environment. To run the same program on a RISC-V QEMU machine, we would port the supporting library to QEMU's memory
map and devices and link it again. The HLL source itself could stay the same.

### Watching it run

The nice thing about the debugger is that it runs the final linked ELF, not just the instructions generated for our
HLL file. By stepping one instruction at a time, we can see what the standard library does before and after `main`,
and exactly where control moves between its code and ours.

![Instruction stepping moves from library code into main and back.](figures/stdlib_execution_path.svg)

To watch it happen, open the [example](repo:programs/lessons/08_hll/compiler_pipeline.hll) and click `build`. The
spine on the left lights up, and its buttons let you inspect the HLL, IR, assembly and final ELF.

Then click `debug`, switch to `disasm`, select `instr` and keep the step count at `1`. Execution begins in `_start`,
which belongs to the standard library. Keep stepping and you will see it call `main`, where the instructions from our
HLL file calculate `1 + 1`. When `main` returns, `a0` contains `2` and execution moves back into the standard library,
which uses that value to stop the machine with exit code `2`. This is the boundary the linker normally hides: in the
disassembly, you can walk across it one instruction at a time.

### Another example

Our first program showed the pipeline, but it did not show what happens when one function calls another. For that,
let's revisit the sum-of-squares program from the stack-frame lesson, this time written in HLL:

```hll,runnable registers=false
square: (value: i32) -> i32 {
	return value * value
}

sum_of_squares: (a: i32, b: i32) -> i32 {
	return square(a) + square(b)
}

main: () -> i32 {
	return sum_of_squares(3, 4)
}
```

[Open the lesson 09 source in the IDE](repo:programs/lessons/09_hll_functions/sum_of_squares.hll).
`main` calls `sum_of_squares` with `3` and `4`. That function squares both arguments, adds the results and returns
`25`. You can check the complete pipeline from the command line:

```sh
fsc run programs/lessons/09_hll_functions/sum_of_squares.hll
```

The program stops with exit code `25`. Now let's look at the
[generated IR](repo:programs/lessons/09_hll_functions/sum_of_squares.ir):

```ir
define square($value: i32) -> i32 {
entry:
	$value_ptr = stack_alloc i32
	store i32 $value, $value_ptr
	$0 = load i32, $value_ptr
	$1 = load i32, $value_ptr
	$2 = mul i32 $0, $1
	ret $2
}

define sum_of_squares($a: i32, $b: i32) -> i32 {
entry:
	$a_ptr = stack_alloc i32
	store i32 $a, $a_ptr
	$b_ptr = stack_alloc i32
	store i32 $b, $b_ptr
	$0 = load i32, $a_ptr
	$1 = call square($0)
	$2 = load i32, $b_ptr
	$3 = call square($2)
	$4 = add i32 $1, $3
	ret $4
}

define main() -> i32 {
entry:
	$0 = call sum_of_squares(3, 4)
	ret $0
}
```

Each HLL function becomes an IR function with the same parameters and return type. The `call` instructions name
their arguments explicitly and produce new temporary values, just like `add` and `mul`. Parameters are copied into
stack slots before they are used. That looks redundant here, and an optimizer could remove much of it, but we are
leaving optimization disabled so every lowering step remains visible.

The back end then decides how to express those operations using the RISC-V calling convention.
Below is the compiler's generated instruction stream with its explanatory comments removed. The
hosted standard library normally supplies `_start`, the small harness before `square` takes its
place here by calling the generated `main` and writing its return value to SYSCON. Everything from
`square` onward comes directly from the
[complete generated assembly](repo:programs/lessons/09_hll_functions/sum_of_squares.s).

```asm,runnable=bare expect=a0:25
.section .text
.globl _start
_start:
	jal    ra, main
	li     t0, 0x10010000
	sd     a0, 0(t0)

square:
	addi   sp, sp, -32
	sd     ra, 24(sp)
	sd     s2, 8(sp)
	sd     s3, 16(sp)
	addi   t0, sp, 32
	addiw  s2, a0, 0
square__entry:
	addi   t0, sp, 0
	sw     s2, 0(t0)
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   t0, sp, 0
	lw     s3, 0(t0)
	mul    s2, s2, s3
	addiw  s2, s2, 0
	addi   a0, s2, 0
	ld     s3, 16(sp)
	ld     s2, 8(sp)
	ld     ra, 24(sp)
	addi   sp, sp, 32
	jalr   zero, 0(ra)

sum_of_squares:
	addi   sp, sp, -48
	sd     ra, 32(sp)
	sd     s2, 16(sp)
	sd     s3, 24(sp)
	addi   t0, sp, 48
	addiw  s2, a0, 0
	addiw  s3, a1, 0
sum_of_squares__entry:
	addi   t0, sp, 0
	sw     s2, 0(t0)
	addi   t0, sp, 8
	sw     s3, 0(t0)
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   a0, s2, 0
	jal    ra, square
	addiw  s2, a0, 0
	addi   t0, sp, 8
	lw     s3, 0(t0)
	addi   a0, s3, 0
	jal    ra, square
	addiw  s3, a0, 0
	add    s2, s2, s3
	addiw  s2, s2, 0
	addi   a0, s2, 0
	ld     s3, 24(sp)
	ld     s2, 16(sp)
	ld     ra, 32(sp)
	addi   sp, sp, 48
	jalr   zero, 0(ra)

main:
	addi   sp, sp, -16
	sd     ra, 8(sp)
	sd     s2, 0(sp)
main__entry:
	addi   t0, zero, 3
	addi   a0, t0, 0
	addi   t1, zero, 4
	addi   a1, t1, 0
	jal    ra, sum_of_squares
	addiw  s2, a0, 0
	addi   a0, s2, 0
	ld     s2, 0(sp)
	ld     ra, 8(sp)
	addi   sp, sp, 16
	jalr   zero, 0(ra)
```

The arguments arrive in `a0` and `a1`, just as they did in our handwritten assembly. Before each call to `square`,
the next argument is placed in `a0`. Afterward, the return value is copied out of `a0`. The two results stay in
`s2` and `s3` until `add` combines them, then the final result goes back into `a0` for the caller.

Because `jal` overwrites `ra`, the prologue saves it before either call. It also saves `s2` and `s3`, since those
registers belong to the caller, and the epilogue restores all three. This is the same calling-convention
we wrote by hand in the earlier lessons. The difference is that the compiler has now derived it itself.

You can immediately see that this code isn't optimized: it has lots of repetition and unnecessary stack usage.
This was one of the issues with early compilers: for many programs, handwritten assembly was faster. But as
compilers progressed, so did optimization techniques. Today, modern compilers are difficult to beat across an
entire program, although handwritten assembly can sometimes still win in niche performance-critical scenarios.

### HLL examples

The next few chapters use HLL, but they will focus on what happens below the source code rather than teaching
general programming. For a practical language reference, open the `demo workspace` in the IDE and expand
`programs/example`. Every folder there is a complete program with its own run configuration, so you can run,
edit and experiment with the examples directly.

Start with [core basics](repo:programs/example/core_basics/core_basics.hll) for declarations, functions and
control flow. The rest of the folder divides the language into focused examples:

- [operators](repo:programs/example/operators/operators.hll)
- [arrays, slices and ranges](repo:programs/example/arrays_slices_and_ranges/arrays_slices_and_ranges.hll)
- [structs and binding](repo:programs/example/structs_and_binding/structs_and_binding.hll)
- [pointers and places](repo:programs/example/pointers_and_places/pointers_and_places.hll)
- [generics](repo:programs/example/generics/generics.hll)
- [enums, match and results](repo:programs/example/enums_match_and_result/enums_match_and_result.hll)
- [strings and iteration](repo:programs/example/strings_and_iteration/strings_and_iteration.hll)
- [compile-time math](repo:programs/example/compile_time_math/compile_time_math.hll)
- [advanced language features](repo:programs/example/hll2_completion_showcase/hll2_completion_showcase.hll)

Most HLL constructs should already look familiar. These are the conventions most likely to be new:

| Syntax | Meaning |
| --- | --- |
| `; comment` | A line comment. Semicolons are not statement terminators. |
| `name: Type = value` | A binding with an explicit type. |
| `name := value` | A binding whose type is inferred. |
| `name = value` | Assignment to an existing binding. |
| `name: (arg: Type) -> Return { ... }` | A function. Leave out `-> Return` when it returns nothing. |
| `T*`, `&place`, `@pointer` | A pointer type, taking an address and reading or writing a whole pointee. |
| `value as Type` | An explicit postfix cast. |
| `T[N]`, `T[]` | A fixed array and a bounds-checked slice. |

The [language specification](repo:crates/hll-to-ir/_LANG_SPECIFICATIONS.md) is the complete reference when an
example is not enough.

### Exercise

Now it's your turn. Write a function that calculates the `n`th Fibonacci number, where the sequence starts
with `0, 1, 1, 2, 3, 5`. Do it with a loop rather than recursion: handle `0` and `1` first, then keep track of
the previous number, the current number and which position you have reached.

The template is
[`09_hll_functions/fibonacci_template.hll`](repo:programs/lessons/09_hll_functions/fibonacci_template.hll), with
the run configuration `09.2 (template): fibonacci`. It checks the first two numbers before calling
`fibonacci(12)`, so the finished program should stop with exit code `144`.

If you get stuck, the finished version is
[`09_hll_functions/fibonacci.hll`](repo:programs/lessons/09_hll_functions/fibonacci.hll), with the run configuration
`09.3 (solution): fibonacci`.

For a quick reminder of the terminology used so far, see the [glossary](../../glossary/).

## Runtimes

Open the [`08: HLL compiler pipeline`](repo:programs/lessons/08_hll/compiler_pipeline.hll) example
again and click debug, but instead of pressing run, advance one instruction at a time. The first
instruction is not in `main`. You land in a file called
[`runtime.hll`](repo:crates/os-runtime/stdlib/hosted/runtime.hll). That's the *runtime*, a small piece
of code that wraps your program so it can start and stop cleanly without you having to interact with
any type of hardware or assembly.

Let's go over it.

### Entry stub

The label at the very top of the runtime is `_start`. This is the true entry point recorded in the ELF. The loader
jumps here at execution, not to `main`. Here's what `_start` looks like:

```hll
external main: () -> i32
sys := import("sys")

arg_ptr: u8* = null
arg_len: u64 = 0

export runtime_arg: () -> u8[] {
	empty: u8[1] = []
	arg: u8[] = empty[0..0]
	arg.ptr = arg_ptr
	arg.len = arg_len
	return arg
}

export _start: (entry_arg_ptr: u8*, entry_arg_len: u64) {
	arg_ptr = entry_arg_ptr
	arg_len = entry_arg_len
	code: i32 = main()
	sys.exit(code as i64)
}
```

`external main` tells the compiler that somebody else will provide `main`. That somebody else is you! The runtime calls
`main`, captures whatever it returns into `code`, and then calls `sys.exit(code)`. Under the kernel,
the loader places the process argument pointer and length in `a0` and `a1`, `runtime.arg()` keeps
that bounded slice for tools that consume an argument.

Continue stepping until `jal ra, main` is next. Step once more and the machine jumps into the small
`main` function that does 1 + 1 and returns 2. Step through it and you're back in `_start`, about to
call `sys.exit`. That is all the runtime does here: call main, collect its return value, and stop the machine.


### Exit function

Step into `sys.exit`. You'll find a small assembly wrapper in the standard library:

```hll
export sys_exit: (code: i64) {
	_a0 = code as u64
	asm {
		la t0, _a0
		ld a0, 0(t0)
		li a7, 93
		ecall
	}
}
```

The exit code is stored in a global variable `_a0`. Inline assembly loads it into register `a0`, puts the number 93
into `a7`, and then executes an instruction we haven't seen before: `ecall`.

`ecall` is short for *environment call*. Think of it like a `jal` where you don't know the destination. Instead of jumping
to a label in your own code, the CPU pauses your program and jumps to a handler address that was configured during startup. 
The number in `a7` tells that handler which service you want, in this case it's exiting with a specific code.

In our hosted setup, that handler lives in ROM and only knows a few basic services. Syscall 93 means "exit".
When the `ecall` fires, the handler reads the exit code and writes it to the SYSCON device, shutting the machine off
exactly like the `exit` function we wrote by hand in the assembly chapter. The difference is that we never had to know
the SYSCON address: the standard library and the ROM handler hide that from us.

![Returning from main flows through the runtime, ecall handler, and SYSCON.](figures/runtime_exit_path.svg)

### The handler is just code

The handler isn't a CPU feature, or some piece of silicon that "knows" what syscall 93 means. `ecall` only does one
thing, it's a trap. It stops your program, saves enough state to
resume later, and jumps to one fixed address that was set up ahead of time. What happens at that address is just
normal assembly code, written by whoever built the machine. Here's the relevant part of our
[real ROM handler](repo:crates/os-runtime/boot/trap.s):

```asm
_m_trap:
	csrr t0, mcause             # trap cause; we'll revisit the others in the kernel chapter
	li t1, 8
	beq t0, t1, _dispatch_ecall
	li t1, 9
	beq t0, t1, _dispatch_ecall
	li t1, 11
	beq t0, t1, _dispatch_ecall
	mret                        # not something we care about, so continue

_dispatch_ecall:
	li t0, 64
	beq a7, t0, sys_write
	li t0, 93
	beq a7, t0, sys_exit
	li t0, 94
	beq a7, t0, sys_exit
	li t0, 214
	beq a7, t0, sys_brk
	j sys_unknown

sys_exit:
	li t0, 268500992            # = 0x1001_0000 = SYSCON
	sd a0, 0(t0)
	j _advance_mepc_and_mret

_advance_mepc_and_mret:
	csrr t0, mepc
	addi t0, t0, 4
	csrw mepc, t0
	mret                        # resume program
```

The first three comparisons check `mcause` (we'll return to it in the kernel chapter). Once the
handler knows it's actually looking at an environment call, it compares `a7` against each syscall
number and branches to whichever one matches, exactly the same `if`/`match` chain you'd write in
HLL. `sys_exit` rebuilds `0x1001_0000` in a register, writes the exit code
from `a0` to that SYSCON address, and then bumps `mepc` past the `ecall` so that `mret` would resume at the next
instruction, if the machine were still running by that point.

Nobody wrote a syscall table into the hardware. Someone (in this case, me) wrote this code. You could've
used a jump table instead, same idea, and `ecall` just guarantees you land at the top of it with `a7` and `a0`
untouched. Everything past that point is regular RISC-V code.

Which also means: the handler can do whatever it wants with syscall 93. Ours treats it as "exit". Nothing about
`ecall` itself enforces that, a different handler could interpret 93 as something else entirely, or not recognize
it at all. The convention only decides where numbers go, `a7` for the syscall, `a0`-`a6` for arguments. What happens
after is decided by whoever creates the system.

### Why abstract it?

You might wonder: if we can just write to `0x10010000` and stop the machine, why not do that directly in HLL?
The answer is *protection*.

Right now, every program we write runs in the most privileged mode the CPU offers (M-mode). Any instruction can
touch any memory address, including the devices. That's fine when you're the only program on the machine. But a
real computer runs many programs at once. If one of them could write to the power-off device, it could kill every
other program instantly. If it could read the keyboard buffer directly, it could steal your passwords.

To stop that, CPUs have *privilege modes*: a small set of rings that decide what code is allowed to do:

| Mode | What runs there | What it can touch                             |
|------|-----------------|-----------------------------------------------|
| M-mode (machine) | Firmware, early boot code | Everything, all memory, all devices           |
| S-mode (supervisor) | The operating-system kernel | Memory and devices the firmware allows        |
| U-mode (user) | Ordinary programs | Only what the kernel explicitly maps for them |

![U-mode requests kernel service; higher modes control which hardware it can reach.](figures/privilege_service_path.svg)

Our earlier assembly programs ran in M-mode because there was nothing beneath them. The hosted examples in this chapter
also run in M-mode, but they use `ecall` to ask the ROM handler to do privileged things like shutting down. This isn't
true protection, though, the program could still reach SYSCON if it really wanted to, but it's the same pattern a
protected system uses. When we boot our kernel later, programs will run in U-mode. An `ecall` from there will trap into
the S-mode kernel handler, which checks whether the request is allowed before touching any hardware (or maybe doing another
`ecall` into the M-mode handler if needed). That's how a machine stays stable and doesn't just hard-crash when a program
tries to do something it's not supposed to.

And the hardware does enforce this. If a U-mode program tries to use a `j` to jump to M-mode code, or tries to write
to a protected memory address, the CPU hardware blocks it and throws an error. `ecall` is the only way to jump upward
in privilege.

The important take-away is: `ecall` is a protected jump to code that runs at a higher privilege level.
It might lead to the ROM (hosted mode) or to the kernel (OS mode), but from the program's point of view it's the same
instruction.

### Watch it happen

To watch the same layers do more than return a number, modify the program to print a message
before returning:

```hll
console := import("console")

main: () -> i32 {
	console.println("Hello from the runtime!")
	return 0
}
```

Now, let's follow the whole path in the debugger. Click Debug, and choose `instr` as the step unit:

1. Step through `_start`, into `main`, and back to `_start`. (Here you can view how the stdlib dispatches println)
2. Step into `sys.exit`. In the disassembly view, watch its inline assembly load the return value into `a0` and the
   syscall number `93` into `a7`.
3. Step into the `ecall`. Instead of halting immediately, the CPU jumps to the ROM handler.
4. Continue stepping through the ROM until you reach the final two instructions:

```asm
00000184  lui  t0, 0x10010
00000188  sd   a0, 0(t0)
```

The instructions before these are part of the privilege machinery. We will come back to permission checking later.
For now, the important instruction is the `lui`: it builds the SYSCON address `0x1001_0000` in `t0`. The `sd` then
writes the exit code from `a0` to that address, and the machine halts.

And that is the whole path: HLL, runtime, standard library, ecall, ROM, and finally the hardware.

Assembly gives you total control, but it also makes you manage every address and convention by hand. The runtime and standard
library collect the work shared by every program, starting, stopping, and talking to hardware, behind a couple of HLL
functions. The lower layers remain available in the debugger, but from here on we can stay in HLL unless we
choose to look beneath them.

## Standard Library

Every language ends up needing the same handful of things: printing, reading input, growing arrays, managing
memory. Rewriting all of that for every single program would be pointless, so most languages ship it prebuilt as
a standard library. HLL does so too, this chapter goes over some of the most useful pieces of mine.

The library lives under [crates/os-runtime/stdlib](repo:crates/os-runtime/stdlib/hosted/runtime.hll). You've already
seen two of its modules: [runtime.hll](repo:crates/os-runtime/stdlib/hosted/runtime.hll) wraps `main`, and [console.hll](repo:crates/os-runtime/stdlib/hosted/console.hll)
is where the print functions live. First we'll look at memory.

### Dynamic memory allocation

There's still a large elephant in the room: heap management. We haven't really used the heap
in any of our programs. The heap supports dynamic memory allocation: choosing an allocation's
size at runtime instead of at compile time.

For example, a dynamic array needs to grow whenever it becomes full. A function's stack frame
has a fixed size, and is freed when the function returns, so someone outside the function can't
use the stack-allocated memory. A heap allocation remains valid until the program explicitly
frees it, which makes it possible for another function to use it too.

The complete example is [lesson 10](repo:programs/lessons/10_stdlib/build_sequence.hll), run
configuration `10: dynamic memory allocation`. It allocates an array in one function, returns it
to `main`, and frees it when `main` exits:

```hll,runnable registers=false
console := import("console")
memory := import("memory_allocator")

build_sequence: (size: u64) -> i32* {
	bytes: u64 = size * size_of<i32>()
	seq: i32* = memory.malloc(bytes) as i32*
	if seq == null { return null }

	i: u64 = 0
	while i < size {
		seq[i] = ((i + 1) * 10) as i32
		i += 1
	}
	return seq
}

main: () -> i32 {
	my_data: i32* = build_sequence(5)
	if my_data == null { return 1 }
	defer memory.free(my_data as u8*)

	; The array still lives after build_sequence returned.
	total: i32 = my_data[0] + my_data[1] + my_data[2] + my_data[3] + my_data[4]
	console.print_int(total)  ; 150
	return 0
}
```

`malloc` takes a size in bytes, so `build_sequence` multiplies the number of elements by
`size_of<i32>()` (= 4). It returns a raw byte pointer, which we cast to `i32*` before treating the memory
as an array. It can also return `null`, so we check that before using the pointer.

`new(i32, size)` is the shorter version of the same thing. It calculates the byte count,
calls `malloc`, clears the allocated bytes to zero, and returns an `i32*`. Here we call `malloc`
directly so you can see those steps yourself.

Both only allocate the bytes you ask for, so if you accidentally access memory outside that range,
you get undefined behavior. It's called undefined because you don't know what'll actually be there:
you could be overwriting another allocation sitting right next to yours, writing into a device,
overwriting another program, or you could get lucky and have it run perfectly fine anyway. The
point is that with raw pointers you have to be careful and stay in bounds.

This is also why returning a pointer to a stack-allocated array would've been a bug in
`build_sequence`. You allocate it on the function's stack frame, free the frame, and then return
the pointer to the freed memory. Then, once you go into a new function, its stack frame will
overwrite the old one, overwriting the array data, and you'll get a dangling pointer.

![A pointer keeps its address after its block is freed.](figures/heap_allocation_lifetime.svg)

With the pointer from `malloc`, we build the array `[10, 20, 30, 40, 50]` and hand it back to
`main`. From there, `defer memory.free(my_data as u8*)` says: right before `main` returns, run
`memory.free(my_data as u8*)` and give that memory back so it can be reused. You could just call
`memory.free` directly at the end of `main` instead, but `defer` is cleaner, especially when a
function has more than one return point.

In a normal operating-system process, you don't strictly have to free memory immediately before
exiting because the OS reclaims the process's entire address space. In this hosted lesson, the whole
VM stops, so its memory disappears too. But it's still good practice to free what you allocate: it
makes your intent explicit, and it's the only option when a long-running program needs that memory
back while it is still alive.

Next we'll look inside the allocator to see how it decides where a new allocation actually lands.

### Malloc

Alright, so what does `malloc` actually do?

We split the heap into blocks. Each block describes one allocation: where it starts, how large it
is, and whether it's currently free. The allocator keeps track of those blocks, searches them for
enough free space, and grows the heap when it can't reuse an existing one.

The actual allocator has a lot of tiny details that would distract from the basic idea, so we'll use a
smaller version to see how the internals work. You can open the example malloc at
[`10_stdlib/simple_malloc.hll`](repo:programs/lessons/10_stdlib/simple_malloc.hll) with run
configuration `10: debug a small allocator`.

We'll start with the constants and the information stored for each block:

```hll
const HEAP_SIZE = 256
const MAX_BLOCKS = 8
const ALIGNMENT = 8

struct LessonBlock {
	ptr: u8*,
	size: u64,
	is_free: bool,
}
```

`HEAP_SIZE` gives us 256 bytes of pretend heap space, and `MAX_BLOCKS` means we can keep track of
up to eight allocations. `ALIGNMENT` makes every block start on an 8-byte boundary. The
`LessonBlock` struct is the allocator's record of an allocation: its address, size, and whether it
can be reused.

Next come the heap itself and the tracking variables:

```hll
heap: u8[HEAP_SIZE] = []
blocks: LessonBlock[MAX_BLOCKS] = []
block_count: u64 = 0
heap_used: u64 = 0
```

Here, `heap` is just a byte array. A real allocator gets its heap memory from the runtime or the
operating system, but a fixed array keeps this example small. `blocks` stores the records we just
defined, `block_count` tells us how many records are in use, and `heap_used` points to the first
unused byte at the end of the heap.

Now we can start `simple_malloc`. Before growing the heap, it searches the block list for memory
that was freed earlier:

```hll
simple_malloc: (size: u64) -> u8* {
	if size == 0 { return null }

	i: u64 = 0
	while i < block_count {
		if blocks[i].is_free and blocks[i].size >= size {
			blocks[i].is_free = false
			return blocks[i].ptr
		}
		i += 1
	}
```

We search through the entire list until we find a free block that's big enough. Then we mark it as
taken and return its address. That's basically it. This is called a *first-fit* search because we
reuse the first suitable block we find.

If none of the old blocks can be reused, we take more space from the unused end of the heap:

```hll
	if block_count == MAX_BLOCKS { return null }
	if size + ALIGNMENT - 1 < size { return null }
	aligned_size: u64 = ((size + ALIGNMENT - 1) / ALIGNMENT) * ALIGNMENT
	if aligned_size > HEAP_SIZE - heap_used { return null }

	ptr: u8* = &heap[heap_used]
	blocks[block_count] = {
		.ptr = ptr,
		.size = aligned_size,
		.is_free = false,
	}
	block_count += 1
	heap_used += aligned_size
	return ptr
}
```

First, we make sure there's room for another block record. The second check catches an integer
overflow before the size is rounded up to the next multiple of eight. We then check whether that
aligned block still fits in the heap.

If it fits, `&heap[heap_used]` is the address of the new block. We save its details, then move
`heap_used` forward so the next new block starts after it. Notice that the allocator returns a
pointer into `heap`; it doesn't copy or move any of the values stored there.

Freeing memory is the reverse:

```hll
simple_free: (ptr: u8*) {
	i: u64 = 0
	while i < block_count {
		if blocks[i].ptr == ptr {
			blocks[i].is_free = true
			return
		}
		i += 1
	}
}
```

Real allocators are more complex than this simple implementation. Different allocations split one
large region of memory into smaller blocks. When some of them are freed, the allocator can't always
combine that space into one large block again because one or more blocks between them are still in
use. So even if there's enough free memory in total, it might be spread across gaps that are all too
small for the next allocation. This is called *fragmentation*. Real allocators use all kinds of
clever tricks to work around it and avoid wasting those gaps. You can see how this can get really
complex.

![Allocations split one free region into gaps that cannot yet be recombined.](figures/heap_fragmentation.svg)

I won't be diving into the different allocation methods in this guide. I just want you to
understand the concept: keep track of which parts of the heap are in use, reuse freed blocks when
possible, and grow the heap when necessary. There are loads of ways to actually implement it.

You can find this project's actual allocator in
[`memory_allocator.hll`](repo:crates/os-runtime/stdlib/core/memory_allocator.hll). It uses a linked
list instead of the fixed `blocks` array, but the basic search and reuse are the same, and it also
reuses each block whole. You can step into the standard library from either lesson and watch a similar
allocator run.
