.section .text
; Function: main
main:
; --- Function Prologue ---
; Allocate stack frame: 32 bytes
	addi   sp, sp, -32
; Save return address (ra) at offset 16
	sd     ra, 16(sp)
; Save callee-saved register s2 at offset 8
	sd     s2, 8(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: c
; add operation on i32
	addi   t0, zero, 10
	addi   t1, zero, 20
	add    s2, t0, t1
	addiw  s2, s2, 0
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
	j main__while_cond_0
; Basic Block: while_cond_0
main__while_cond_0:
; Load i32 from memory into $$1
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   t0, zero, 50
	slt    s2, s2, t0
	bne s2, zero, main__while_body_1
	j main__while_exit_2
; Basic Block: while_body_1
main__while_body_1:
; Load i32 from memory into $$3
	addi   t0, sp, 0
	lw     s2, 0(t0)
; add operation on i32
	addi   t0, zero, 1
	add    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$5
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   t0, zero, 40
	sub    t1, s2, t0
	sltiu  s2, t1, 1
	bne s2, zero, main__if_then_3
	j main__if_merge_5
; Basic Block: if_then_3
main__if_then_3:
	j main__while_exit_2
; Basic Block: if_merge_5
main__if_merge_5:
; Load i32 from memory into $$7
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   t0, zero, 35
	sub    t1, s2, t0
	sltiu  s2, t1, 1
	bne s2, zero, main__if_then_6
	j main__if_merge_8
; Basic Block: if_then_6
main__if_then_6:
	j main__while_cond_0
; Basic Block: if_merge_8
main__if_merge_8:
	j main__while_cond_0
; Basic Block: while_exit_2
main__while_exit_2:
; Load i32 from memory into $$9
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s2 from offset 8
	ld     s2, 8(sp)
; Restore return address (ra) from offset 16
	ld     ra, 16(sp)
; Deallocate stack frame: 32 bytes
	addi   sp, sp, 32
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function