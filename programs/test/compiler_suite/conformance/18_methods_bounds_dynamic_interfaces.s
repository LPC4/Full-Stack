.section .text
; Function: Rect_make
Rect_make:
; --- Function Prologue ---
; Allocate stack frame: 48 bytes
	addi   sp, sp, -48
; Save return address (ra) at offset 40
	sd     ra, 40(sp)
; Save callee-saved register s2 at offset 24
	sd     s2, 24(sp)
; Save callee-saved register s3 at offset 32
	sd     s3, 32(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 48
; Move parameter '$width' from register a0 to allocated register
	addiw  s2, a0, 0
; Move parameter '$height' from register a1 to allocated register
	addiw  s3, a1, 0
; --- End Parameter Spills ---
; Basic Block: entry
Rect_make__entry:
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
	addi   t0, sp, 8
; Store i32 to memory
	sw     s3, 0(t0)
; Load i32 from memory into $$1
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   t0, sp, 16
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$2
	addi   t0, sp, 8
	lw     s2, 0(t0)
	addi   t0, sp, 20
; Store i32 to memory
	sw     s2, 0(t0)
	ld     a0, 16(sp)
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 32
	ld     s3, 32(sp)
; Restore callee-saved register s2 from offset 24
	ld     s2, 24(sp)
; Restore return address (ra) from offset 40
	ld     ra, 40(sp)
; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: Rect_area
Rect_area:
; --- Function Prologue ---
; Allocate stack frame: 32 bytes
	addi   sp, sp, -32
; Save return address (ra) at offset 24
	sd     ra, 24(sp)
; Save callee-saved register s2 at offset 8
	sd     s2, 8(sp)
; Save callee-saved register s3 at offset 16
	sd     s3, 16(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t1, sp, 32
; Move parameter '$self' from register a0 to allocated register
	addi   s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: entry
Rect_area__entry:
	addi   t0, sp, 0
; Store Rect* to memory
	sd     s2, 0(t0)
; Load { width: i32, height: i32 }* from memory into $$0
	addi   t0, sp, 0
; Access field 'width' at offset 0
; Load i32 from memory into $$1
	addi   t0, s2, 0
	lw     s2, 0(t0)
; Load { width: i32, height: i32 }* from memory into $$2
	addi   t0, sp, 0
	ld     s3, 0(t0)
; Access field 'height' at offset 4
; Load i32 from memory into $$3
	addi   t0, s3, 4
	lw     s3, 0(t0)
; mul operation on i32
	mul    s2, s2, s3
	addiw  s2, s2, 0
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 16
	ld     s3, 16(sp)
; Restore callee-saved register s2 from offset 8
	ld     s2, 8(sp)
; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
; Deallocate stack frame: 32 bytes
	addi   sp, sp, 32
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: Rect_scaled
Rect_scaled:
; --- Function Prologue ---
; Allocate stack frame: 48 bytes
	addi   sp, sp, -48
; Save return address (ra) at offset 32
	sd     ra, 32(sp)
; Save callee-saved register s2 at offset 16
	sd     s2, 16(sp)
; Save callee-saved register s3 at offset 24
	sd     s3, 24(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 48
; Move parameter '$self' from register a0 to allocated register
	addi   s2, a0, 0
; Move parameter '$k' from register a1 to allocated register
	addiw  s3, a1, 0
; --- End Parameter Spills ---
; Basic Block: entry
Rect_scaled__entry:
	addi   t0, sp, 0
; Store Rect* to memory
	sd     s2, 0(t0)
	addi   t0, sp, 8
; Store i32 to memory
	sw     s3, 0(t0)
; Load Rect* from memory into $$0
	addi   t0, sp, 0
; --- Function Call: Rect_area ---
; Passing 1 arguments
	addi   a0, s2, 0
	jal ra, Rect_area
	addiw  s2, a0, 0
; --- End Function Call: Rect_area ---
; Load i32 from memory into $$2
	addi   t0, sp, 8
	lw     s3, 0(t0)
; mul operation on i32
	mul    s2, s2, s3
	addiw  s2, s2, 0
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 24
	ld     s3, 24(sp)
; Restore callee-saved register s2 from offset 16
	ld     s2, 16(sp)
; Restore return address (ra) from offset 32
	ld     ra, 32(sp)
; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: Disc_area
Disc_area:
; --- Function Prologue ---
; Allocate stack frame: 32 bytes
	addi   sp, sp, -32
; Save return address (ra) at offset 24
	sd     ra, 24(sp)
; Save callee-saved register s2 at offset 8
	sd     s2, 8(sp)
; Save callee-saved register s3 at offset 16
	sd     s3, 16(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 32
; Move parameter '$self' from register a0 to allocated register
	addi   s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: entry
Disc_area__entry:
	addi   t0, sp, 0
; Store Disc* to memory
	sd     s2, 0(t0)
; Load { radius: i32 }* from memory into $$0
	addi   t0, sp, 0
; Access field 'radius' at offset 0
; Load i32 from memory into $$1
	addi   t0, s2, 0
	lw     s2, 0(t0)
; mul operation on i32
	addi   t0, zero, 3
	mul    s2, t0, s2
	addiw  s2, s2, 0
; Load { radius: i32 }* from memory into $$3
	addi   t0, sp, 0
	ld     s3, 0(t0)
; Access field 'radius' at offset 0
; Load i32 from memory into $$4
	addi   t0, s3, 0
	lw     s3, 0(t0)
; mul operation on i32
	mul    s2, s2, s3
	addiw  s2, s2, 0
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 16
	ld     s3, 16(sp)
; Restore callee-saved register s2 from offset 8
	ld     s2, 8(sp)
; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
; Deallocate stack frame: 32 bytes
	addi   sp, sp, 32
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: main
main:
; --- Function Prologue ---
; Allocate stack frame: 176 bytes
	addi   sp, sp, -176
; Save return address (ra) at offset 168
	sd     ra, 168(sp)
; Save callee-saved register s2 at offset 152
	sd     s2, 152(sp)
; Save callee-saved register s3 at offset 160
	sd     s3, 160(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: r
; --- Function Call: Rect_make ---
; Passing 2 arguments
	addi   t0, zero, 6
	addi   a0, t0, 0
	addi   t1, zero, 7
	addi   a1, t1, 0
	jal ra, Rect_make
; Unpacking small aggregate return from a0/a1
	sd     a0, 144(sp)
; --- End Function Call: Rect_make ---
	addi   t0, sp, 0
; Store Rect to memory
	addi   t1, a0, 0
	sd     t1, 0(t0)
; --- Function Call: Rect_area ---
; Passing 1 arguments
	addi   t0, sp, 0
	addi   a0, t0, 0
	jal ra, Rect_area
	addiw  s2, a0, 0
; --- End Function Call: Rect_area ---
	addi   t0, zero, 42
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_0
	j main__if_merge_2
; Basic Block: if_then_0
main__if_then_0:
	addi   t2, zero, 1
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 160
	ld     s3, 160(sp)
; Restore callee-saved register s2 from offset 152
	ld     s2, 152(sp)
; Restore return address (ra) from offset 168
	ld     ra, 168(sp)
; Deallocate stack frame: 176 bytes
	addi   sp, sp, 176
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_2
main__if_merge_2:
; --- Function Call: Rect_scaled ---
; Passing 2 arguments
	addi   t0, sp, 0
	addi   a0, t0, 0
	addi   t1, zero, 2
	addi   a1, t1, 0
	jal ra, Rect_scaled
	addiw  s2, a0, 0
; --- End Function Call: Rect_scaled ---
	addi   t0, zero, 84
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_3
	j main__if_merge_5
; Basic Block: if_then_3
main__if_then_3:
	addi   t2, zero, 2
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 160
	ld     s3, 160(sp)
; Restore callee-saved register s2 from offset 152
	ld     s2, 152(sp)
; Restore return address (ra) from offset 168
	ld     ra, 168(sp)
; Deallocate stack frame: 176 bytes
	addi   sp, sp, 176
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_5
main__if_merge_5:
; --- Function Call: double_area__Rect ---
; Passing 1 arguments
	addi   t0, sp, 0
	addi   a0, t0, 0
	jal ra, double_area__Rect
	addiw  s2, a0, 0
; --- End Function Call: double_area__Rect ---
	addi   t0, zero, 84
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_6
	j main__if_merge_8
; Basic Block: if_then_6
main__if_then_6:
	addi   t2, zero, 3
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 160
	ld     s3, 160(sp)
; Restore callee-saved register s2 from offset 152
	ld     s2, 152(sp)
; Restore return address (ra) from offset 168
	ld     ra, 168(sp)
; Deallocate stack frame: 176 bytes
	addi   sp, sp, 176
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_8
main__if_merge_8:
; local var: d
	addi   t0, sp, 16
; Store i32 to memory
	addi   t1, zero, 3
	sw     t1, 0(t0)
	addi   t0, sp, 8
; Store Disc to memory
	lw     t1, 16(sp)
	sw     t1, 0(t0)
; --- Function Call: double_area__Disc ---
; Passing 1 arguments
	addi   t0, sp, 8
	addi   a0, t0, 0
	jal ra, double_area__Disc
	addiw  s2, a0, 0
; --- End Function Call: double_area__Disc ---
	addi   t0, zero, 54
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_9
	j main__if_merge_11
; Basic Block: if_then_9
main__if_then_9:
	addi   t2, zero, 4
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 160
	ld     s3, 160(sp)
; Restore callee-saved register s2 from offset 152
	ld     s2, 152(sp)
; Restore return address (ra) from offset 168
	ld     ra, 168(sp)
; Deallocate stack frame: 176 bytes
	addi   sp, sp, 176
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_11
main__if_merge_11:
; local var: mixed
	addi   t0, sp, 0
	addi   s2, t0, 0
	addi   t0, sp, 56
; Store i8* to memory
	sd     s2, 0(t0)
	la s2, Measurable__dyn__Rect__area
	addi   t0, sp, 64
; Store fn(i8*) -> i32 to memory
	sd     s2, 0(t0)
	addi   t0, sp, 8
	addi   s2, t0, 0
	addi   t0, sp, 72
; Store i8* to memory
	sd     s2, 0(t0)
	la s2, Measurable__dyn__Disc__area
	addi   t0, sp, 80
; Store fn(i8*) -> i32 to memory
	sd     s2, 0(t0)
	addi   t0, sp, 88
; Store { __data: i8*, __fn_area: fn(i8*) -> i32 } to memory
	ld     t1, 56(sp)
	sd     t1, 0(t0)
	ld     t2, 64(sp)
	sd     t2, 8(t0)
	addi   t0, sp, 104
; Store { __data: i8*, __fn_area: fn(i8*) -> i32 } to memory
	ld     t1, 72(sp)
	sd     t1, 0(t0)
	addi   t2, s2, 0
	sd     t2, 8(t0)
	addi   t0, sp, 24
; Store Measurable__dyn[2] to memory
	ld     t1, 88(sp)
	sd     t1, 0(t0)
	ld     t2, 96(sp)
	sd     t2, 8(t0)
	ld     t3, 104(sp)
	sd     t3, 16(t0)
	ld     t4, 112(sp)
	sd     t4, 24(t0)
	addi   t0, sp, 24
	addi   t1, zero, 0
	addi   t3, zero, 16
	mul    t2, t1, t3
	add    s2, t0, t2
; --- Function Call: Measurable__dyn_area ---
; Passing 1 arguments
	addi   a0, s2, 0
	jal ra, Measurable__dyn_area
	addiw  s2, a0, 0
; --- End Function Call: Measurable__dyn_area ---
	addi   t0, zero, 42
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_12
	j main__if_merge_14
; Basic Block: if_then_12
main__if_then_12:
	addi   t2, zero, 5
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 160
	ld     s3, 160(sp)
; Restore callee-saved register s2 from offset 152
	ld     s2, 152(sp)
; Restore return address (ra) from offset 168
	ld     ra, 168(sp)
; Deallocate stack frame: 176 bytes
	addi   sp, sp, 176
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_14
main__if_merge_14:
	addi   t0, sp, 24
	addi   t1, zero, 1
	addi   t3, zero, 16
	mul    t2, t1, t3
	add    s2, t0, t2
; --- Function Call: Measurable__dyn_area ---
; Passing 1 arguments
	addi   a0, s2, 0
	jal ra, Measurable__dyn_area
	addiw  s2, a0, 0
; --- End Function Call: Measurable__dyn_area ---
	addi   t0, zero, 27
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_15
	j main__if_merge_17
; Basic Block: if_then_15
main__if_then_15:
	addi   t2, zero, 6
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 160
	ld     s3, 160(sp)
; Restore callee-saved register s2 from offset 152
	ld     s2, 152(sp)
; Restore return address (ra) from offset 168
	ld     ra, 168(sp)
; Deallocate stack frame: 176 bytes
	addi   sp, sp, 176
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_17
main__if_merge_17:
; local var: total
	addi   t0, sp, 120
; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
; inferred local var: i
	addi   t0, sp, 128
; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
; inferred local var: __for_end_0
	addi   t0, sp, 136
; Store i32 to memory
	addi   t1, zero, 2
	sw     t1, 0(t0)
	j main__while_cond_18
; Basic Block: while_cond_18
main__while_cond_18:
; Load i32 from memory into $$23
	addi   t0, sp, 128
	lw     s2, 0(t0)
; Load i32 from memory into $$24
	addi   t0, sp, 136
	lw     s3, 0(t0)
	slt    s2, s2, s3
	bne s2, zero, main__while_body_19
	j main__while_exit_20
; Basic Block: while_body_19
main__while_body_19:
; Load i32 from memory into $$26
	addi   t0, sp, 120
	lw     s2, 0(t0)
; Load i32 from memory into $$27
	addi   t0, sp, 128
	lw     s3, 0(t0)
	addi   t0, sp, 24
	addi   t2, zero, 16
	mul    t1, s3, t2
	add    s3, t0, t1
; --- Function Call: Measurable__dyn_area ---
; Passing 1 arguments
	addi   a0, s3, 0
	jal ra, Measurable__dyn_area
	addiw  s3, a0, 0
; --- End Function Call: Measurable__dyn_area ---
; add operation on i32
	add    s2, s2, s3
	addiw  s2, s2, 0
	addi   t0, sp, 120
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$31
	addi   t0, sp, 128
	lw     s2, 0(t0)
; add operation on i32
	addi   t0, zero, 1
	add    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, sp, 128
; Store i32 to memory
	sw     s2, 0(t0)
	j main__while_cond_18
; Basic Block: while_exit_20
main__while_exit_20:
; Load i32 from memory into $$33
	addi   t0, sp, 120
	lw     s2, 0(t0)
	addi   t0, zero, 69
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_21
	j main__if_merge_23
; Basic Block: if_then_21
main__if_then_21:
	addi   t2, zero, 7
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 160
	ld     s3, 160(sp)
; Restore callee-saved register s2 from offset 152
	ld     s2, 152(sp)
; Restore return address (ra) from offset 168
	ld     ra, 168(sp)
; Deallocate stack frame: 176 bytes
	addi   sp, sp, 176
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_23
main__if_merge_23:
	addi   t3, zero, 42
	addi   a0, t3, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 160
	ld     s3, 160(sp)
; Restore callee-saved register s2 from offset 152
	ld     s2, 152(sp)
; Restore return address (ra) from offset 168
	ld     ra, 168(sp)
; Deallocate stack frame: 176 bytes
	addi   sp, sp, 176
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: double_area__Rect
double_area__Rect:
; --- Function Prologue ---
; Allocate stack frame: 32 bytes
	addi   sp, sp, -32
; Save return address (ra) at offset 24
	sd     ra, 24(sp)
; Save callee-saved register s2 at offset 8
	sd     s2, 8(sp)
; Save callee-saved register s3 at offset 16
	sd     s3, 16(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t4, sp, 32
; Move parameter '$v' from register a0 to allocated register
	addi   s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: Rect__entry
double_area__Rect__entry:
	addi   t0, sp, 0
; Store Rect* to memory
	sd     s2, 0(t0)
; Load Rect* from memory into $$0
	addi   t0, sp, 0
; --- Function Call: Rect_area ---
; Passing 1 arguments
	addi   a0, s2, 0
	jal ra, Rect_area
	addiw  s2, a0, 0
; --- End Function Call: Rect_area ---
; Load Rect* from memory into $$2
	addi   t0, sp, 0
	ld     s3, 0(t0)
; --- Function Call: Rect_area ---
; Passing 1 arguments
	addi   a0, s3, 0
	jal ra, Rect_area
	addiw  s3, a0, 0
; --- End Function Call: Rect_area ---
; add operation on i32
	add    s2, s2, s3
	addiw  s2, s2, 0
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 16
	ld     s3, 16(sp)
; Restore callee-saved register s2 from offset 8
	ld     s2, 8(sp)
; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
; Deallocate stack frame: 32 bytes
	addi   sp, sp, 32
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: double_area__Disc
double_area__Disc:
; --- Function Prologue ---
; Allocate stack frame: 32 bytes
	addi   sp, sp, -32
; Save return address (ra) at offset 24
	sd     ra, 24(sp)
; Save callee-saved register s2 at offset 8
	sd     s2, 8(sp)
; Save callee-saved register s3 at offset 16
	sd     s3, 16(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 32
; Move parameter '$v' from register a0 to allocated register
	addi   s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: Disc__entry
double_area__Disc__entry:
	addi   t0, sp, 0
; Store Disc* to memory
	sd     s2, 0(t0)
; Load Disc* from memory into $$0
	addi   t0, sp, 0
; --- Function Call: Disc_area ---
; Passing 1 arguments
	addi   a0, s2, 0
	jal ra, Disc_area
	addiw  s2, a0, 0
; --- End Function Call: Disc_area ---
; Load Disc* from memory into $$2
	addi   t0, sp, 0
	ld     s3, 0(t0)
; --- Function Call: Disc_area ---
; Passing 1 arguments
	addi   a0, s3, 0
	jal ra, Disc_area
	addiw  s3, a0, 0
; --- End Function Call: Disc_area ---
; add operation on i32
	add    s2, s2, s3
	addiw  s2, s2, 0
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 16
	ld     s3, 16(sp)
; Restore callee-saved register s2 from offset 8
	ld     s2, 8(sp)
; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
; Deallocate stack frame: 32 bytes
	addi   sp, sp, 32
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: Measurable__dyn_area
Measurable__dyn_area:
; --- Function Prologue ---
; Allocate stack frame: 32 bytes
	addi   sp, sp, -32
; Save return address (ra) at offset 24
	sd     ra, 24(sp)
; Save callee-saved register s2 at offset 8
	sd     s2, 8(sp)
; Save callee-saved register s3 at offset 16
	sd     s3, 16(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 32
; Move parameter '$self' from register a0 to allocated register
	addi   s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: dyn_area__entry
Measurable__dyn_area__entry:
	addi   t0, sp, 0
; Store Measurable__dyn* to memory
	sd     s2, 0(t0)
; Load { __data: i8*, __fn_area: fn(i8*) -> i32 }* from memory into $$0
	addi   t0, sp, 0
; Access field '__fn_area' at offset 8
; Load fn(i8*) -> i32 from memory into $$1
	addi   t0, s2, 8
	ld     s2, 0(t0)
; Load { __data: i8*, __fn_area: fn(i8*) -> i32 }* from memory into $$2
	addi   t0, sp, 0
	ld     s3, 0(t0)
; Access field '__data' at offset 0
; Load i8* from memory into $$3
	addi   t0, s3, 0
	ld     s3, 0(t0)
; --- Indirect Function Call ---
; Passing 1 arguments
	addi   a0, s3, 0
	jalr   ra, 0(s2)
	addiw  s2, a0, 0
; --- End Indirect Function Call ---
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 16
	ld     s3, 16(sp)
; Restore callee-saved register s2 from offset 8
	ld     s2, 8(sp)
; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
; Deallocate stack frame: 32 bytes
	addi   sp, sp, 32
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: Measurable__dyn__Disc__area
Measurable__dyn__Disc__area:
; --- Function Prologue ---
; Allocate stack frame: 32 bytes
	addi   sp, sp, -32
; Save return address (ra) at offset 16
	sd     ra, 16(sp)
; Save callee-saved register s2 at offset 8
	sd     s2, 8(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 32
; Move parameter '$__obj' from register a0 to allocated register
	addi   s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: dyn__Disc__area__entry
Measurable__dyn__Disc__area__entry:
	addi   t0, sp, 0
; Store i8* to memory
	sd     s2, 0(t0)
; Load i8* from memory into $$0
	addi   t0, sp, 0
; --- Function Call: Disc_area ---
; Passing 1 arguments
	addi   a0, s2, 0
	jal ra, Disc_area
	addiw  s2, a0, 0
; --- End Function Call: Disc_area ---
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
; Function: Measurable__dyn__Rect__area
Measurable__dyn__Rect__area:
; --- Function Prologue ---
; Allocate stack frame: 32 bytes
	addi   sp, sp, -32
; Save return address (ra) at offset 16
	sd     ra, 16(sp)
; Save callee-saved register s2 at offset 8
	sd     s2, 8(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 32
; Move parameter '$__obj' from register a0 to allocated register
	addi   s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: dyn__Rect__area__entry
Measurable__dyn__Rect__area__entry:
	addi   t0, sp, 0
; Store i8* to memory
	sd     s2, 0(t0)
; Load i8* from memory into $$0
	addi   t0, sp, 0
; --- Function Call: Rect_area ---
; Passing 1 arguments
	addi   a0, s2, 0
	jal ra, Rect_area
	addiw  s2, a0, 0
; --- End Function Call: Rect_area ---
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