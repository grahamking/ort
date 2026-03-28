//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025-2026 Graham King
//!
//! These are usually provided by libc.

// fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8
// rdi = dest, rsi = src, rdx = n
// we do not touch any callee save registers
.global memcpy
memcpy:
	mov rax, rdi	// we must return dest, but rep movsb will move it
	// dest already in rdi
	// src already in rsi
	mov rcx, rdx	// rcx = n
	rep movsb
	ret

// fn memset(dest: *mut u8, c: i32, count: usize) -> *mut u8
// rdi = dest, rsi = c, rdx = count
// we do not touch any callee save registers
.global memset
memset:
	mov r8, rdi		// we must return dest, but rep stosb will move it
	mov eax, esi	// eax = c (only al is used), the byte to write
	mov rcx, rdx	// rcx = count
	rep stosb
	mov rax, r8		// return dest
	ret

// fn strlen(s *const c_char) -> usize
// rdi = s
// we do not touch any callee save registers
.global strlen
strlen:
	xor eax, eax	// search for byte 0
	mov rcx, -1		// loop counter. use max for unbounded scan.
	repne scasb
	// for length n, after the loop we have subtracted n+1 from rcx (+1 for the \0 byte)
	// so rcx = -1 - (n + 1) = -(n + 2)
	// in two's complement the inverse of -(n + 1) is n
	// so ...
	not rcx			// inverse
	dec rcx			// it was n + 2 not n + 1, so decrement
	mov rax, rcx	// length is in rcx, return it
	ret

// fn bcmp(s1: *const u8, s2: *const u8, n: usize) -> i32
// rdi = s1, rsi = s2, rdx = n
//
// bcmp is a simpler version of memcmp. llvm will call this instead
// of memcmp when it can.
// we do not touch any callee save registers
.global bcmp
bcmp:
	test rdx, rdx // if n == 0
	je .Lbcmp_zero

	mov rcx, rdx	// rcx = n
	repe cmpsb
	setne al		// check ZF (zero-flag). set al to 1 if the arrays are not equal.
	movzx eax, al	// zero-extend eax from al
	ret
.Lbcmp_zero:
	xor eax, eax
	ret

// fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32
// rdi = s1, rsi = s2, rdx = n
// we do not touch any callee save registers
.global memcmp
memcmp:
	test rdx, rdx // if n == 0
	je .Lmemcmp_same

	mov rcx, rdx		// rcx = n
	repe cmpsb

	// arrays match
	je .Lmemcmp_same

	// arrays differ, figure out sign, into eax
	movzx eax, byte ptr [rdi - 1]
	movzx edx, byte ptr [rsi - 1]
	sub eax, edx
	ret

.Lmemcmp_same:
	xor eax, eax		// return 0, arrays match
	ret

