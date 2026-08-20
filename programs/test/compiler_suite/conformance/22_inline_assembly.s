.section .text
; Function: asm_add
asm_add:
; --- Function Prologue ---
; Allocate stack frame: 64 bytes
	addi   sp, sp, -64
; Save return address (ra) at offset 56
	sd     ra, 56(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 64
; Spill parameter '$left' from register a0 to stack slot 40
	sd     a0, 40(sp)
; Spill parameter '$right' from register a1 to stack slot 48
	sd     a1, 48(sp)
; --- End Parameter Spills ---
; Basic Block: entry
asm_add__entry:
	addi   t0, sp, 16
; Store i64 to memory
	ld     t1, 40(sp)
	sd     t1, 0(t0)
	addi   t0, sp, 24
; Store i64 to memory
	ld     t1, 48(sp)
	sd     t1, 0(t0)
; local var: result
	addi   t0, sp, 32
; Store i64 to memory
	addi   t1, zero, 0
	sd     t1, 0(t0)
	addi   a0, sp, 16
	ld     a0, 0(a0)
	addi   a1, sp, 24
	ld     a1, 0(a1)
	add a0, a0, a1
	sd     a0, 32(sp)
; Load i64 from memory into $$0
	addi   t0, sp, 32
	addi   t1, a0, 0
	sd     t1, 40(sp)
	addi   t2, t1, 0
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore return address (ra) from offset 56
	ld     ra, 56(sp)
; Deallocate stack frame: 64 bytes
	addi   sp, sp, 64
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: main
main:
; --- Function Prologue ---
; Allocate stack frame: 80 bytes
	addi   sp, sp, -80
; Save return address (ra) at offset 64
	sd     ra, 64(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: before
	addi   t0, sp, 16
; Store i64 to memory
	addi   t1, zero, 0
	sd     t1, 0(t0)
; local var: after
	addi   t0, sp, 24
; Store i64 to memory
	addi   t1, zero, 0
	sd     t1, 0(t0)
	sd     sp, 16(sp)
	addi sp, sp, 0
	sd     sp, 24(sp)
; Load i64 from memory into $$0
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 48(sp)
	addi   t0, t1, 0
	addi   t1, zero, 0
	slt    t3, t1, t0
	sltiu  t2, t3, 1
	sb     t2, 48(sp)
	lb     t4, 48(sp)
	bne t4, zero, main__if_then_0
	j main__if_merge_2
; Basic Block: if_then_0
main__if_then_0:
	addi   t5, zero, 1
	addi   a0, t5, 0
; --- Function Epilogue ---
; Restore return address (ra) from offset 64
	ld     ra, 64(sp)
; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_2
main__if_merge_2:
; Load i64 from memory into $$2
	addi   t0, sp, 24
	ld     t1, 0(t0)
	sd     t1, 48(sp)
; Load i64 from memory into $$3
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 56(sp)
	ld     t0, 48(sp)
	sub    t3, t0, t1
	sltu   t2, zero, t3
	sb     t2, 48(sp)
	lb     t4, 48(sp)
	bne t4, zero, main__if_then_3
	j main__if_merge_5
; Basic Block: if_then_3
main__if_then_3:
	addi   t5, zero, 2
	addi   a0, t5, 0
; --- Function Epilogue ---
; Restore return address (ra) from offset 64
	ld     ra, 64(sp)
; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_5
main__if_merge_5:
; local var: value
; --- Function Call: asm_add ---
; Passing 2 arguments
	addi   t0, zero, 12
	addi   a0, t0, 0
	addi   t1, zero, 30
	addi   a1, t1, 0
	jal ra, asm_add
	sd     a0, 48(sp)
; --- End Function Call: asm_add ---
	addi   t0, sp, 32
; Store i64 to memory
	addi   t1, a0, 0
	sd     t1, 0(t0)
; local var: scratch
	addi   t0, sp, 40
; Store i64 to memory
	addi   t1, zero, 0
	sd     t1, 0(t0)
	li t0, 9
	addi   a0, sp, 32
	ld     a0, 0(a0)
	sd     t0, 0(sp)
	sd     t1, 8(sp)
	la t0, bridge_global
	sd     a0, 0(t0)
	ld     t1, 8(sp)
	ld     t0, 0(sp)
	la a1, bridge_global
	ld a2, 0(a1)
	sd     a2, 32(sp)
	sd     t0, 40(sp)
	la t0, bridge_global
	sd     t0, 48(sp)
; Load i64 from memory into $$7
	ld     t1, 0(t0)
	sd     t1, 48(sp)
	addi   t0, t1, 0
	addi   t1, zero, 42
	sub    t3, t0, t1
	sltu   t2, zero, t3
	sb     t2, 48(sp)
	lb     t4, 48(sp)
	bne t4, zero, main__if_then_6
	j main__if_merge_8
; Basic Block: if_then_6
main__if_then_6:
	addi   t5, zero, 3
	addi   a0, t5, 0
; --- Function Epilogue ---
; Restore return address (ra) from offset 64
	ld     ra, 64(sp)
; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_8
main__if_merge_8:
; Load i64 from memory into $$9
	addi   t0, sp, 32
	ld     t1, 0(t0)
	sd     t1, 48(sp)
	addi   t0, t1, 0
	addi   t1, zero, 42
	sub    t3, t0, t1
	sltu   t2, zero, t3
	sb     t2, 48(sp)
	lb     t4, 48(sp)
	bne t4, zero, main__if_then_9
	j main__if_merge_11
; Basic Block: if_then_9
main__if_then_9:
	addi   t5, zero, 4
	addi   a0, t5, 0
; --- Function Epilogue ---
; Restore return address (ra) from offset 64
	ld     ra, 64(sp)
; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_11
main__if_merge_11:
; Load i64 from memory into $$11
	addi   t0, sp, 40
	ld     t1, 0(t0)
	sd     t1, 48(sp)
	addi   t0, t1, 0
	addi   t1, zero, 9
	sub    t3, t0, t1
	sltu   t2, zero, t3
	sb     t2, 48(sp)
	lb     t4, 48(sp)
	bne t4, zero, main__if_then_12
	j main__if_merge_14
; Basic Block: if_then_12
main__if_then_12:
	addi   t5, zero, 5
	addi   a0, t5, 0
; --- Function Epilogue ---
; Restore return address (ra) from offset 64
	ld     ra, 64(sp)
; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_14
main__if_merge_14:
	addi   t6, zero, 42
	addi   a0, t6, 0
; --- Function Epilogue ---
; Restore return address (ra) from offset 64
	ld     ra, 64(sp)
; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
.section .bss
.balign 8
bridge_global:
	.space 8
.section .text
