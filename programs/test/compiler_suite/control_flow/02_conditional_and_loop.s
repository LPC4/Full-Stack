.section .text
	; ========================================
	; Function: control_suite
	; ========================================
.globl control_suite
control_suite:
	; --- Function Prologue ---
	; Allocate stack frame: 80 bytes
	addi   sp, sp, -80
	; Save return address (ra) at offset 64
	sd     ra, 64(sp)
	; Save callee-saved register s8 at offset 72
	sd     s0, 72(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Function Parameter Spills ---
	addi   t0, s0, 80
	; Spill parameter '$param' from register a0 to stack slot 0
	sw     a0, 0(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
control_suite__entry:
	; bind parameter: param
	addi   t0, sp, 4
	; Store i32 to memory
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; local var: sum
	addi   t0, sp, 8
	; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
	j control_suite__label_0
	; --- Basic Block: label_0 ---
control_suite__label_0:
	; while condition
	; Load i32 from memory into $$0
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 12(sp)
	lw     t0, 12(sp)
	addi   t1, zero, 0
	slt    t2, t1, t0
	sb     t2, 16(sp)
	lb     t3, 16(sp)
	bne t3, zero, control_suite__label_1
	j control_suite__label_2
	; --- Basic Block: label_1 ---
control_suite__label_1:
	; if condition
	; Load i32 from memory into $$2
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 20(sp)
	lw     t0, 20(sp)
	addi   t1, zero, 5
	sub    t3, t0, t1
	sltiu  t2, t3, 1
	sb     t2, 24(sp)
	lb     t4, 24(sp)
	bne t4, zero, control_suite__label_3
	j control_suite__label_4
	; --- Basic Block: label_3 ---
control_suite__label_3:
	; assignment
	; Load i32 from memory into $$4
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 28(sp)
	; add operation on i32
	lw     t0, 28(sp)
	addi   t1, zero, 10
	add    t2, t0, t1
	sw     t2, 32(sp)
	addi   t0, sp, 8
	; Store i32 to memory
	lw     t1, 32(sp)
	sw     t1, 0(t0)
	j control_suite__label_5
	; --- Basic Block: label_4 ---
control_suite__label_4:
	; assignment
	; Load i32 from memory into $$6
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 36(sp)
	; add operation on i32
	lw     t0, 36(sp)
	addi   t1, zero, 1
	add    t2, t0, t1
	sw     t2, 40(sp)
	addi   t0, sp, 8
	; Store i32 to memory
	lw     t1, 40(sp)
	sw     t1, 0(t0)
	j control_suite__label_5
	; --- Basic Block: label_5 ---
control_suite__label_5:
	; assignment
	; Load i32 from memory into $$8
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 44(sp)
	; sub operation on i32
	lw     t0, 44(sp)
	addi   t1, zero, 1
	sub    t2, t0, t1
	sw     t2, 48(sp)
	addi   t0, sp, 4
	; Store i32 to memory
	lw     t1, 48(sp)
	sw     t1, 0(t0)
	j control_suite__label_0
	; --- Basic Block: label_2 ---
control_suite__label_2:
	; if condition
	; Load i32 from memory into $$10
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 52(sp)
	lw     t0, 52(sp)
	addi   t1, zero, 0
	slt    t2, t1, t0
	sb     t2, 56(sp)
	lb     t3, 56(sp)
	bne t3, zero, control_suite__label_6
	j control_suite__label_8
	; --- Basic Block: label_6 ---
control_suite__label_6:
	; Load i32 from memory into $$12
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 60(sp)
	lw     t2, 60(sp)
	addi   a0, t2, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 72
	ld     s0, 72(sp)
	; Restore return address (ra) from offset 64
	ld     ra, 64(sp)
	; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; --- Basic Block: label_8 ---
control_suite__label_8:
	addi   t3, zero, 0
	addi   a0, t3, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 72
	ld     s0, 72(sp)
	; Restore return address (ra) from offset 64
	ld     ra, 64(sp)
	; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function