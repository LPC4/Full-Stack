# 02: sum an array of numbers stored in memory.

.data                 # what follows is data placed in memory, not code
array:                # a label for the first byte of the array
	.dword 10           # "emit a doubleword": place a 64-bit value in memory here
	.dword 20
	.dword 30
	.dword 40
array_end:            # a label just past the last element

.text                 # back to code
.globl _start         # specifies to the virtual machine where to begin execution
_start:
	li   t0, 0          # t0 = sum, start at 0
	la   t1, array      # "load address": t1 = pointer to the current element
	la   t2, array_end  # t2 = the address just past the array
loop:
	ld   t3, 0(t1)      # t3 = the element t1 points at
	add  t0, t0, t3     # sum = sum + element
	addi t1, t1, 8      # move the pointer one element (8 bytes) forward
	blt  t1, t2, loop   # keep looping while the pointer is still inside the array
done:                 # we fall through here with t0 = 100 (0x64 in the debugger)
	j    done           # spin here forever so the registers stay inspectable
