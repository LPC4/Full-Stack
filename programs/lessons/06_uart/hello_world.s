.data
msg:
  .asciz "Hello, world!\n"   # "asciz": the bytes in ASCII + a trailing zero (c-string)

.text
.globl _start
_start:
  la   a0, msg
  jal  ra, print_string
  li   a0, 0           # exit code 0: nothing went wrong
  jal  ra, exit

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
  beqz a0, ps_done     # a zero byte ends the string
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

exit:                  # exit(code): halt the machine with the code in a0
  li   t0, 0x10010000  # SYSCON, the power-off device
  sd   a0, 0(t0)       # the machine stops here: nothing below this runs
