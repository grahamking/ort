# libc Replacement Plan

Assumptions: this ranking assumes the existing target remains x86_64 Linux, because `src/libc.rs` already hardcodes Linux syscall numbers and the x86_64 syscall ABI. It also assumes the libc-free build will provide its own `_start`, which is important because that startup path needs to capture `argc`, `argv`, and `envp` directly from the initial stack and replace the pieces of process setup that libc currently hides.

12. `isatty`

`isatty` is still easy, but it does require a small design choice. The simplest faithful replacement is an `ioctl(TCGETS)` syscall wrapper that returns true on success and false on `ENOTTY`, which matches how libc usually answers this question for [`src/main.rs`](/home/graham/src/ort/src/main.rs#L55) and [`src/input/cli.rs`](/home/graham/src/ort/src/input/cli.rs#L49). A fallback based on `fstat` and `S_IFCHR` is possible, but `TCGETS` is the cleaner drop-in behavior.

16. `getenv`

`getenv` looks simple but hides startup assumptions. The current helper in [`src/common/utils.rs`](/home/graham/src/ort/src/common/utils.rs#L208) relies on libc to expose the process environment, and removing libc means you also lose `libc::environ` as a trustworthy external symbol. Since you are planning to add your own `_start`, the clean replacement is to capture `envp` there, store it in a global, and implement `getenv` as a linear scan over `KEY=value` strings.

