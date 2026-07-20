# 06.2: The UART write is wrapped in a function. print_char is a leaf, so like square in 05 it needs no frame.

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
