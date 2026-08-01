I started off creating Minecraft mods for fun, and ever since, I've wanted to understand what a computer
*actually* does. Not just how to use a programming language, but how an application really gets run on a
computer. I realized I had no idea how Java itself worked underneath.

That question is why I started working on multiple projects in my free time: different virtual machines, custom assembly and
programming languages. Eventually, after much trial and error, I arrived at [this project](https://github.com/LPC4/Full-Stack).

Full-Stack goes over the entire execution stack of computing: from binary instructions to a running operating system.
It includes a virtual machine (VM), a compiler, assembler, and linker for a custom language, and a kernel that runs inside the VM.

In these documents we'll be going over all the things you'd need to know to build a similar project from scratch.

Throughout these documents there will be multiple code snippets. You can run and modify these yourself,
online and without any downloads, in [the project's own IDE](https://lpc4.github.io/Full-Stack/).

This is written for anyone interested in systems programming. I assume you already know how to code and understand
concepts like pointers, but not what lives below that level. After this you'll be able to understand the full stack
and make your own compiler, virtual machine, and even a small kernel.

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

I chose RISC-V because it's way simpler to emulate (fewer instructions) and because I agree with the mindset of
keeping the assembly simple. (x86 actually also gets broken down into a RISC-like microcode, inside the CPU itself.)

You can find the RISC-V specifications as used in this project
[here](../crates/asm-to-binary/_RISCV_SPECIFICATIONS.md). It's not necessary to start memorizing them: most of it
is pretty logical, and it'll come with time. I'm only putting it here in case you don't understand an instruction.

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

```asm
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

![](figures/the_stack_and_the_heap.svg)

You can probably see the danger: if the stack grows down too far, or the heap grows up too far, they meet and start
overwriting each other. That is an overflow. This is exactly the *stack overflow* error you've probably seen.

Let's use the stack in a small example. We reserve some stack space, store two numbers in it, load them
back, and add them:

```asm
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

![](figures/how_the_cpu_reads_instructions.svg)

In RISC-V these instructions are stored as 4-byte codes, with each instruction having its own encoding. For
the register-to-register instructions we've seen (add, sub, mul, ...), the 32 bits are split up like this:

![](figures/how_the_cpu_reads_instructions_2.svg)

`opcode` selects the instruction family, `rd` is the destination register's number, `rs1` and `rs2` are the
source registers, and `funct3`/`funct7` pick the exact operation. So `add x1, x2, x3` encodes to:

![](figures/how_the_cpu_reads_instructions_3.svg)

The fields don't line up with byte boundaries, so regroup those same 32 bits into 4 bytes:

![](figures/how_the_cpu_reads_instructions_4.svg)

Compare that to x86, where an instruction can be any amount of bytes and there are many different encodings:

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

![](figures/endianness.svg)

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

```asm
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
2. In the file tree, open lesson one: `01_sum/sum.s`. It's this exact program, plus a couple of
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

```asm
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

![](figures/arrays_and_sections.svg)

This layout isn't fixed though: it's the linker (which we'll see later) that decides where each section ends up, and on other
systems the sections can be placed differently or moved around. In this project it's always `.text` first and
`.data` right after. If you have multiple assembly files that you want to combine to use together (for example if
you want to use a library written by someone else), the *linker* combines their sections and makes it into one
combined executable, with all the code and data sections placed properly.

### Breakpoints and the memory view

This is a good moment to meet the rest of the debugger, because registers alone won't show you an array. Load
`02_array_sum/array_sum.s`, run configuration `02: sum an array`, and click `debug` again: it's the
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
code, and you can find a blank template under `03_reverse_array/reverse_array_template.s`, with a matching
run configuration `03.1 (template): reverse an array`. There is nothing to install for any of this: the
[IDE runs in your browser](https://lpc4.github.io/Full-Stack/), lessons and all.

![Two pointers move inward as the array is reversed in place.](figures/reverse_array_two_pointer_swap.svg)

The template contains the boilerplate we haven't talked about yet. `.globl _start` plus the `_start` label
tell the machine which instruction to begin execution at; these were also the extra lines in lesson one. And
the `done: j done` at the end is an endless loop that keeps the machine spinning, so you can still inspect
the registers and memory after your program is finished. Write your array in `.data` and your code between
`_start` and `done`. If everything works, the memory view at your array's address reads 28 where 0a used to
be: the elements in reverse order, shown in hex.

If you get stuck, a finished version sits right next to the template: `03_reverse_array/reverse_array.s`,
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

```asm
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

You'll find this one under `04_functions/max.s`, run configuration `04.1: call a leaf function`. Notice
how the second call reuses the result of the first: the answer is already sitting in `a0`, exactly where the
first argument goes.

But then there's an issue. What if a function wants to call another function? It'll do a `jal` of its own,
the return address in `ra` gets overwritten, and now the outer function can't return to its caller
anymore. This is where a really important concept comes in: the *function stack frame*. At the
top of the function, in what's called the *prologue*, you move `sp` down by the size of your frame (a
multiple of 16, since the convention keeps `sp` aligned to 16 bytes), and then you `sd` everything that has
to survive into the slots you just reserved: `ra` first and foremost (so it can be restored later),
plus any `s` registers you plan to use. That last part is the other half of the convention: a call is allowed to clobber
(or overwrite) any `a` or `t` register, but the `s` registers are "saved" registers that must come out of a call unchanged,
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

That's `04_functions/max3.s`, run configuration `04.2: nested calls`. Its frame looks
like this on the stack:

![](figures/functions_2.svg)

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

![](figures/the_frame_pointer.svg)

Most compilers drop `fp` and track the offsets themselves to be more efficient: no need to burn a register and two
extra instructions per function keeping it up to date. Our examples from before do the compiler thing, pure `sp` offsets,
which is also why `max3` was free to use `s0` as an ordinary saved register.

Now let's tie the whole thing together with one last example: factorial. It's a good fit, because
`fact(n) = n * fact(n - 1)` makes a call, and it has to carry something across it: the original `n`, so it can
multiply by it once the call comes back. It can't be trusted to a register, because the `jal` into `fact` is
allowed to clobber it. So each call keeps its own `n` in its frame, addressed off `fp`. Here's the whole thing,
we'll be going over it piece by piece. It's `05_stack_frames/factorial_stack.s`, run configuration
`05.1: factorial with a frame`:

```asm
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

![](figures/the_frame_pointer_2.svg)

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

![](figures/the_frame_pointer_3.svg)

The innermost `fact(1)` took the base-case branch before storing anything, so its local slot is still
`(unused)`; every frame above it has its `n` saved and is parked on its `jal`, waiting for the call below to
return. As those returns come in, `ret` and the epilogue pop the frames off one at a time, and the answers
multiply their way back up to `fact(5) = 120`.

Now we understand fully where that *stack overflow* error comes from. When a chain of calls becomes too long, each
frame pushes the stack a little further down, until it reaches the heap and the two start overwriting each other.

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

The template is `05_stack_frames/sum_of_squares_template.s`, run configuration
`05.2 (template): sum of squares`, again in the [browser IDE](https://lpc4.github.io/Full-Stack/). It gives
you `_start`, `square`, and a `sos` split into four commented steps to fill in, with the frame plan written
out in the comments above it:

![](figures/the_frame_pointer_4.svg)

Same 32-byte shape as `fact`, except this time there is no unused slot. Write the prologue, the two calls
with their saves and reloads in between, and the epilogue. If it works, `a0` holds 25 when the program
reaches `done`: `sos(3, 4) = 9 + 16`. Use the `stack` view while you're in there, it will show you your own
frame being built slot by slot, and a wrong offset is a lot easier to see there than in the registers.

The solution sits next to it as `05_stack_frames/sum_of_squares.s`, run configuration
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

![](figures/bit_representations.svg)

All three are correct. Those eight bytes are of a string we're going to print in a moment,
but if you hand them to a float instruction it will happily tell you they're a number
with forty-one digits in front of the decimal point, and it isn't wrong. Nothing in the register
says "this is text".

That's why `add` and `fadd` are separate instructions even though both "add two registers", and why `ld` and
`lb` are separate, and why there are signed and unsigned comparisons. The register only stores bits. The
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
straight out of `crates/virtual-machine/src/bus.rs`, where every one of these bases is a constant the bus
uses to decide which device an address belongs to:

![](figures/an_address_is_also_just_a_number.svg)

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

We're emulating that chip in `crates/virtual-machine/src/devices/uart.rs`, a subset of the NS16550A. There's no
wire and no bits being shifted, obviously: writing a byte to the transmit register appends it to a buffer, and the
IDE drains that buffer into the console window. From the program's side nothing about that is visible. It stores a
byte to an address, and a character appears.

So how do we actually use this in our code? The UART sits at `0x1000_0000`, and its transmit register is the first
byte there, so the whole thing is two instructions: put the address in a register, store a byte to it. That's
`06_uart/hello_raw.s`:

![`sb` writes a byte to the UART transmit address, which appears in the console.](figures/uart_mmio_output.svg)

```asm
.globl _start
_start:
  li   t0, 0x10000000  # UART transmit register, fixed by the machine
  li   t1, 72          # 'H'
  sb   t1, 0(t0)
  li   t1, 105         # 'i'
  sb   t1, 0(t0)
  li   t1, 10          # '\n'
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
This is `06_uart/print_char.s`, and it's nothing new: the UART store moved into a function. `print_char` calls
nothing else, so it's a leaf, and just like `square` back in lesson 05 it needs no frame at all:

```asm
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

```asm
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

The finished program, with `print_char`, `print_string` and `exit` all together, is `06_uart/hello_world.s`.

And finally, after six lessons and a fair amount of hard work, we've printed hello world. Who would have thought
printing hello world could be this complicated.

## The same code on QEMU

So far everything we've written has been tested on my own virtual machine, so you might wonder if this runs on real hardware
too. That's what we'll be looking at now. Let me preface this by saying that I won't really be going over any new concepts in
this chapter, I'll just show you how the code we've been running on my emulated RV64 CPU would run on different hardware.

To demonstrate this we'll be using QEMU. QEMU is an emulator you've probably heard of. Most people use it as something
that runs Linux on their laptop, but that's only part of what it does. In that mode it emulates a whole PC, but it can
also emulate a specific board: a CPU wired with a set of devices at specific addresses. And that's exactly the mode we
want, and it's the same job my VM has been doing the entire time, just written by people who have doing it way longer.

The board we'll use is called `virt`. It doesn't correspond to any chip you can buy: it's a board the QEMU authors
invented, with a RISC-V CPU and a small set of standard devices at fixed addresses. That sounds like cheating,
but the CPU is a real RV64 implementation and the devices are real device designs, so code written for `virt` is written
against the same rules as code for silicon. It's basically a real machine that just hasn't been manufactured, but if you
really wanted to you could.

The port is `07_qemu/hello_world.s`. Two things change from our previous hello world program.

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
  slli a0, a0, 16      # a code rides in the upper 16 bits
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

There's one more file next to the assembly: `link.ld`. Up until now the IDE has been deciding where our code and data
end up in memory. With QEMU, we need to do this ourselves.

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
That address isn't a free choice either, it's just where the RAM happens to be. If you put the program anywhere else and it
might land on a device, or on nothing at all.

Then each line places one section, and the two halves mean different things:

![](figures/the_linker_script.svg)

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

That last part is worth looking into, because there is no loader here to do it. QEMU copies our program into RAM and
jumps straight to `_start`. Nothing in that sequence zeroes anything.

In practice, you can put something in `.bss` on QEMU and read zeroes back, but only by luck: the emulator hands the
machine a block of memory that happens to already be zero. Nothing promised us that. Real hardware powers on
with whatever was in the RAM, and the next machine you try might not be so clean. A program that depends on `.bss`
being zero without zeroing it is relying on the emulator/machine being nice to it.

So the rule is that whoever loads a program is responsible for clearing its `.bss`, and here nobody is, which means
it's us. Zeroing `.bss` is one of the first things a kernel does on boot, and now you know what it's for.

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

These are QEMU's own options and its [documentation](https://www.qemu.org/docs/master/system/target-riscv.html)
covers them properly, but `-bios none` is worth knowing here. Normally QEMU boots a small firmware called OpenSBI
first, which initializes the machine before handing over. `-bios none` throws that away, so our first instruction
really is the machine's first instruction, exactly as in my VM.

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
that interface, writing everything in assembly does not scale. So next we'll move up to a higher-level language and
build the compiler that turns readable programs into the instructions we now understand.
