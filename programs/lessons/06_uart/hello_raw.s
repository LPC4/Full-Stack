# 06.1: printing without any helpers. Storing a byte to the UART.

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
