.section .text
; Function: control_suite
.globl control_suite
control_suite:
; --- Function Prologue ---
; Allocate stack frame: 48 bytes
	addi   sp, sp, -48
; Save return address (ra) at offset 24
	sd     ra, 24(sp)
; Save callee-saved register s2 at offset 16
	sd     s2, 16(sp)
; Save callee-saved register s0 at offset 32
	sd     s0, 32(sp)
; Set up frame pointer
	addi   s0, sp, 0
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, s0, 48
; Move parameter '$param' from register a0 to allocated register
	addiw  s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: entry
control_suite__entry:
; bind parameter: param
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
; local var: sum
	addi   t0, sp, 8
; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
	j control_suite__label_0
; Basic Block: label_0
control_suite__label_0:
; while condition
; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   t0, zero, 0
	slt    s2, t0, s2
	bne s2, zero, control_suite__label_1
	j control_suite__label_2
; Basic Block: label_1
control_suite__label_1:
; if condition
; Load i32 from memory into $$2
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   t0, zero, 5
	sub    t1, s2, t0
	sltiu  s2, t1, 1
	bne s2, zero, control_suite__label_3
	j control_suite__label_4
; Basic Block: label_3
control_suite__label_3:
; assignment
; Load i32 from memory into $$4
	addi   t0, sp, 8
	lw     s2, 0(t0)
; add operation on i32
	addi   t0, zero, 10
	add    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, sp, 8
; Store i32 to memory
	sw     s2, 0(t0)
	j control_suite__label_5
; Basic Block: label_4
control_suite__label_4:
; assignment
; Load i32 from memory into $$6
	addi   t0, sp, 8
	lw     s2, 0(t0)
; add operation on i32
	addi   t0, zero, 1
	add    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, sp, 8
; Store i32 to memory
	sw     s2, 0(t0)
	j control_suite__label_5
; Basic Block: label_5
control_suite__label_5:
; assignment
; Load i32 from memory into $$8
	addi   t0, sp, 0
	lw     s2, 0(t0)
; sub operation on i32
	addi   t0, zero, 1
	sub    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
	j control_suite__label_0
; Basic Block: label_2
control_suite__label_2:
; if condition
; Load i32 from memory into $$10
	addi   t0, sp, 8
	lw     s2, 0(t0)
	addi   t0, zero, 0
	slt    s2, t0, s2
	bne s2, zero, control_suite__label_6
	j control_suite__label_8
; Basic Block: label_6
control_suite__label_6:
; Load i32 from memory into $$12
	addi   t0, sp, 8
	lw     s2, 0(t0)
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s0 from offset 32
	ld     s0, 32(sp)
; Restore callee-saved register s2 from offset 16
	ld     s2, 16(sp)
; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: label_8
control_suite__label_8:
	addi   t1, zero, 0
	addi   a0, t1, 0
; --- Function Epilogue ---
; Restore callee-saved register s0 from offset 32
	ld     s0, 32(sp)
; Restore callee-saved register s2 from offset 16
	ld     s2, 16(sp)
; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function