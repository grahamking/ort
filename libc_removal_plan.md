# libc Replacement Plan

Assumptions: this ranking assumes the existing target remains x86_64 Linux, because `src/libc.rs` already hardcodes Linux syscall numbers and the x86_64 syscall ABI. It also assumes the libc-free build will provide its own `_start`, which is important because that startup path needs to capture `argc`, `argv`, and `envp` directly from the initial stack and replace the pieces of process setup that libc currently hides.

1. `sigemptyset`

This is the easiest one to remove. The current code only needs an empty signal mask in [`src/common/cancel_token.rs`](/home/graham/src/ort/src/common/cancel_token.rs#L49), so libc's helper can be replaced with direct zero-initialization of `sigset_t`, either by constructing `sigset_t { __val: [0; 16] }` or by using `core::ptr::write_bytes`. There is no libc-specific behavior here beyond filling the structure with zeroes.

2. `printf`

The current call sites only use very small fixed formats: `%s%s\n` in [`src/common/utils.rs`](/home/graham/src/ort/src/common/utils.rs#L148) and `ERROR: %s` in [`src/main.rs`](/home/graham/src/ort/src/main.rs#L60). That means you do not need a general `printf` implementation at all; replace those call sites with a tiny helper that writes the literal prefix and then the string bytes with the existing `write` syscall wrapper. This is mostly mechanical and removes a large libc surface area for almost no runtime work.

3. `freeaddrinfo`

This function becomes unnecessary as soon as `getaddrinfo` stops returning libc-managed linked lists. The clean replacement is to change the resolver boundary so it returns a Rust-owned `Vec<Ipv4Addr>` directly, which is already what [`src/common/resolver.rs`](/home/graham/src/ort/src/common/resolver.rs#L18) wants in the end. If you do that, `freeaddrinfo` simply disappears rather than being reimplemented.

4. `pthread_attr_init`

This is easy only because it should stop existing as a public concept. In [`src/common/thread.rs`](/home/graham/src/ort/src/common/thread.rs#L49), the attribute object is just a temporary vessel for stack metadata before `pthread_create`. A libc-free thread launcher should replace that flow with a small internal Rust struct, or just pass stack base and size directly to the clone wrapper, which makes `pthread_attr_init` either a no-op or dead code.

5. `pthread_attr_destroy`

This is the symmetric case to `pthread_attr_init`, and it is equally easy to remove. The current code does not rely on any destructor side effects in [`src/common/thread.rs`](/home/graham/src/ort/src/common/thread.rs#L57); it only calls the function because the pthread API requires it. Once the thread API is no longer modeled after pthread attributes, there is nothing to destroy.

9. `socket`

This is a straight syscall wrapper on x86_64 Linux. The current call site in [`src/net/socket.rs`](/home/graham/src/ort/src/net/socket.rs#L19) already uses a libc-shaped interface, so replacing it means adding `SYS_SOCKET` and an inline-asm wrapper exactly like the existing `read`, `write`, and `open` wrappers in [`src/libc.rs`](/home/graham/src/ort/src/libc.rs#L245). There is very little hidden behavior here.

10. `connect`

`connect` is the same class of work as `socket`: just add `SYS_CONNECT` and a typed wrapper that preserves the current signature used in [`src/net/socket.rs`](/home/graham/src/ort/src/net/socket.rs#L27). Because you already build the `sockaddr_in` manually, libc is not contributing any address marshalling here. The replacement is almost entirely mechanical.

11. `setsockopt`

This is another direct syscall wrapper. The current use is narrow, only enabling `TCP_FASTOPEN` in [`src/net/socket.rs`](/home/graham/src/ort/src/net/socket.rs#L68), so the libc-free version only has to pass raw pointers and lengths through to `SYS_SETSOCKOPT`. As with `socket` and `connect`, the ABI work is simple and the complexity is mostly just keeping constants correct.

12. `isatty`

`isatty` is still easy, but it does require a small design choice. The simplest faithful replacement is an `ioctl(TCGETS)` syscall wrapper that returns true on success and false on `ENOTTY`, which matches how libc usually answers this question for [`src/main.rs`](/home/graham/src/ort/src/main.rs#L55) and [`src/input/cli.rs`](/home/graham/src/ort/src/input/cli.rs#L49). A fallback based on `fstat` and `S_IFCHR` is possible, but `TCGETS` is the cleaner drop-in behavior.

13. `syscall`

The program should not keep a generic variadic `syscall` entry point once libc is gone. Today it is only used for futex operations in [`src/common/queue.rs`](/home/graham/src/ort/src/common/queue.rs#L167), so the clean replacement is to delete the generic API and add a dedicated `futex` wrapper with the exact argument pattern the queue needs. That keeps the inline asm type-safe and avoids rebuilding libc's catch-all syscall helper.

14. `pthread_attr_setstack`

This one is still simpler than real thread creation, but it forces you to settle the stack handoff contract. Right now [`src/common/thread.rs`](/home/graham/src/ort/src/common/thread.rs#L55) uses pthread attributes to tell libc which manually mapped stack to use. In a libc-free implementation, you either store that stack pointer and size in your own thread descriptor, or more directly pass the top-of-stack to a `clone` trampoline, so the function itself goes away but the underlying concept remains.

16. `getenv`

`getenv` looks simple but hides startup assumptions. The current helper in [`src/common/utils.rs`](/home/graham/src/ort/src/common/utils.rs#L208) relies on libc to expose the process environment, and removing libc means you also lose `libc::environ` as a trustworthy external symbol. Since you are planning to add your own `_start`, the clean replacement is to capture `envp` there, store it in a global, and implement `getenv` as a linear scan over `KEY=value` strings.

17. `sigaction`

This is harder than `sigemptyset` because raw signal installation on x86_64 Linux is not just one syscall. The direct replacement is `rt_sigaction`, but if you go that route you need to provide the kernel-compatible structure layout and usually a restorer trampoline plus `SA_RESTORER`, which libc normally hides. For this codebase, another viable route is to avoid asynchronous handlers entirely and switch the cancel path in [`src/common/cancel_token.rs`](/home/graham/src/ort/src/common/cancel_token.rs#L46) to a `signalfd`-based model, which may actually be simpler than reproducing libc's signal ABI details.

18. `getaddrinfo`

This is the largest non-threading functional replacement. The current resolver in [`src/common/resolver.rs`](/home/graham/src/ort/src/common/resolver.rs#L18) only needs IPv4 `SOCK_STREAM` results, so you do not need a general RFC-complete `getaddrinfo`; a focused DNS client is enough. A practical plan is to first accept dotted IPv4 literals directly, then check `/etc/hosts`, then send A-record DNS queries to configured servers or nameservers parsed from `/etc/resolv.conf`, and return `Vec<Ipv4Addr>` without any linked-list ABI.

19. `pthread_join`

Joining is harder than it looks because it depends on the exact way threads are created. The clean libc-free model is to maintain your own thread record containing the child TID and completion state, then wait with futex until the child exits and clears `child_tid`, which is the kernel-level primitive pthreads use under the hood. This is manageable, but it is tightly coupled to the eventual `clone` flags, thread exit path, and whether you need to preserve return values, so I would not tackle it before the simpler runtime pieces are in place.

20. `pthread_create`

This is the hardest replacement in the file. A real libc-free implementation means replacing the pthread runtime with a `clone` or `clone3`-based launcher, manually mapped stacks, a small entry trampoline that calls the Rust `extern "C"` function from [`src/input/prompt.rs`](/home/graham/src/ort/src/input/prompt.rs#L331), and enough bookkeeping to support `join`. This is also where hidden runtime assumptions show up, especially thread-local storage, signal state inheritance, and child TID/futex semantics, so I would treat it as the final stage after sockets, allocator behavior, environment access, and DNS resolution are already independent of libc.
