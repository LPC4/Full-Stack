.section .text
; Function: main
main:
; --- Function Prologue ---
; Allocate stack frame: 48 bytes
	addi   sp, sp, -48
; Save return address (ra) at offset 32
	sd     ra, 32(sp)
; Save callee-saved register s2 at offset 24
	sd     s2, 24(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: boxed
	addi   a0, zero, 4
	addi   sp, sp, -16
	sd     a0, 0(sp)
	call malloc
	ld     a1, 0(sp)
	addi   sp, sp, 16
	addi   s2, a0, 0
	beq a0, zero, .Lheap_zero_done_0
	beq a1, zero, .Lheap_zero_done_0
	addi   t0, a0, 0
.Lheap_zero_0:
	sb     zero, 0(t0)
	addi   t0, t0, 1
	addi   a1, a1, -1
	bne a1, zero, .Lheap_zero_0
.Lheap_zero_done_0:
	addi   t0, sp, 0
; Store Box<i32>* to memory
	sd     s2, 0(t0)
; local var: legacy
	addi   a0, zero, 4
	addi   sp, sp, -16
	sd     a0, 0(sp)
	call malloc
	ld     a1, 0(sp)
	addi   sp, sp, 16
	addi   s2, a0, 0
	beq a0, zero, .Lheap_zero_done_1
	beq a1, zero, .Lheap_zero_done_1
	addi   t0, a0, 0
.Lheap_zero_1:
	sb     zero, 0(t0)
	addi   t0, t0, 1
	addi   a1, a1, -1
	bne a1, zero, .Lheap_zero_1
.Lheap_zero_done_1:
	addi   t0, sp, 8
; Store Box_i32* to memory
	sd     s2, 0(t0)
; local var: double
	addi   a0, zero, 4
	addi   sp, sp, -16
	sd     a0, 0(sp)
	call malloc
	ld     a1, 0(sp)
	addi   sp, sp, 16
	addi   s2, a0, 0
	beq a0, zero, .Lheap_zero_done_2
	beq a1, zero, .Lheap_zero_done_2
	addi   t0, a0, 0
.Lheap_zero_2:
	sb     zero, 0(t0)
	addi   t0, t0, 1
	addi   a1, a1, -1
	bne a1, zero, .Lheap_zero_2
.Lheap_zero_done_2:
	addi   t0, sp, 16
; Store Box<Box<i32>>* to memory
	sd     s2, 0(t0)
; Load Box<i32>* from memory into $$3
	addi   t0, sp, 0
	ld     s2, 0(t0)
	addi   a0, s2, 0
	call free
; Load Box_i32* from memory into $$4
	addi   t0, sp, 8
	ld     s2, 0(t0)
	addi   a0, s2, 0
	call free
; Load Box<Box<i32>>* from memory into $$5
	addi   t0, sp, 16
	ld     s2, 0(t0)
	addi   a0, s2, 0
	call free
	addi   t0, zero, 0
	addi   a0, t0, 0
; --- Function Epilogue ---
; Restore callee-saved register s2 from offset 24
	ld     s2, 24(sp)
; Restore return address (ra) from offset 32
	ld     ra, 32(sp)
; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function