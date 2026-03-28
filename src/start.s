//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025-2026 Graham King
//!
//! Entrypoint for the entire program.
//! We do not use libc so we must handle everything ourselves.
//! x86_64 assembly, Intel syntax

.global _start
_start:
    // argc
     mov rdi, [rsp]

    // argv. points to a list of argument pointers.
     mov rsi, rsp
     add rsi, 8

    // envp
    // This is after the args. Need to skip the args (argc * pointer_size),
    // and skip argc itself (8) and the NULL that ends the args (+8).
     lea rdx, [rsp + rdi * 8 + 16]

	// The whole program
     call main

	// exit
    // Return code from main
     mov edi, eax
    // exit syscall number
     mov eax, 60
     syscall
     ud2
