# 01: sum the numbers 1 to 10.

.globl _start         # specifies to the virtual machine where to begin execution
_start:
  li   t0, 0          # t0 = sum, start at 0
  li   t1, 1          # t1 = i, start at 1
  li   t2, 10         # t2 = the limit
loop:
  add  t0, t0, t1     # sum = sum + i
  addi t1, t1, 1      # i = i + 1
  ble  t1, t2, loop   # if i <= 10, go around again   "branch if less than or equal"
done:                 # we fall through here with t0 = 55 (the debugger
                      # shows registers in hex, so it reads 0x37)
  j    done           # spin here forever so the registers stay inspectable
