# 03 solution: reverse an array in memory, in place.

.data                 # what follows is data placed in memory, not code
array:                # a label for the first byte of the array
	.dword 10
	.dword 20
	.dword 30
	.dword 40
array_end:            # a label just past the last element

.text
.globl _start         # specifies to the virtual machine where to begin execution
_start:
	la   t0, array      # t0 = pointer to the first element
	la   t1, array_end  # t1 = the address just past the array
	addi t1, t1, -8     # move it back onto the last element
loop:
	bge  t0, t1, done   # pointers met or crossed: the array is reversed
	ld   t2, 0(t0)      # t2 = front element
	ld   t3, 0(t1)      # t3 = back element
	sd   t3, 0(t0)      # front slot = back element
	sd   t2, 0(t1)      # back slot = front element
	addi t0, t0, 8      # move the front pointer one element forward
	addi t1, t1, -8     # move the back pointer one element backward
	j    loop
done:                 # spin here forever so the memory stays inspectable
	j    done
