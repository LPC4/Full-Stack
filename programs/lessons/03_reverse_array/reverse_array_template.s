# 03 template: reverse the array in place.

.data                 # your array goes here

.text
.globl _start         # specifies to the virtual machine where to begin execution
_start:
	                    # your code goes here

done:                 # spin here forever so the memory stays inspectable
	j    done
