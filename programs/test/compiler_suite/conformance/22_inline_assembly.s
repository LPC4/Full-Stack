.section .text
; Function: main
main:
; --- Function Prologue ---
; Allocate stack frame: 48 bytes
	addi   sp, sp, -48
; Save return address (ra) at offset 32
	sd     ra, 32(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: before
	addi   t0, sp, 0
	sd     t0, 16(sp)
	addi   t0, sp, 0
; Store i64 to memory
	ld     t1, 16(sp)
	sd     t1, 0(t0)
	addi sp, sp, 0
; local var: after
	addi   t0, sp, 0
	sd     t0, 16(sp)
	addi   t0, sp, 8
; Store i64 to memory
	ld     t1, 16(sp)
	sd     t1, 0(t0)
; Load i64 from memory into $$2
	addi   t0, sp, 0
	ld     t1, 0(t0)
	sd     t1, 16(sp)
	addi   t0, t1, 0
	addi   t1, zero, 0
	slt    t3, t1, t0
	sltiu  t2, t3, 1
	sb     t2, 16(sp)
	lb     t4, 16(sp)
	bne t4, zero, main__if_then_0
	j main__if_merge_2
; Basic Block: if_then_0
main__if_then_0:
	addi   t5, zero, 1
	addi   a0, t5, 0
; --- Function Epilogue ---
; Restore return address (ra) from offset 32
	ld     ra, 32(sp)
; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_2
main__if_merge_2:
; Load i64 from memory into $$4
	addi   t0, sp, 8
	ld     t1, 0(t0)
	sd     t1, 16(sp)
; Load i64 from memory into $$5
	addi   t0, sp, 0
	ld     t1, 0(t0)
	sd     t1, 24(sp)
	ld     t0, 16(sp)
	sub    t3, t0, t1
	sltu   t2, zero, t3
	sb     t2, 16(sp)
	lb     t4, 16(sp)
	bne t4, zero, main__if_then_3
	j main__if_merge_5
; Basic Block: if_then_3
main__if_then_3:
	addi   t5, zero, 2
	addi   a0, t5, 0
; --- Function Epilogue ---
; Restore return address (ra) from offset 32
	ld     ra, 32(sp)
; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_5
main__if_merge_5:
	addi   t6, zero, 42
	addi   a0, t6, 0
; --- Function Epilogue ---
; Restore return address (ra) from offset 32
	ld     ra, 32(sp)
; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function