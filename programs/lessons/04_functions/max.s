# 04.1: call a function. Arguments go in a0/a1, the result comes back in a0.

.globl _start
_start:
	li   a0, 7          # first argument
	li   a1, 12         # second argument
	jal  ra, max        # a0 = max(7, 12); ra = the address of the next line
	li   a1, 3
	jal  ra, max        # a0 = max(12, 3): the first result feeds the second call
done:
	j    done

max:                  # max(a, b): return the larger of a0 and a1
	bge  a0, a1, max_a  # if a >= b the answer is already in a0
	mv   a0, a1         # otherwise the answer is b
max_a:
	ret                 # jump back to the address in ra (ret = jalr x0, ra, 0)
