`ort` is a tiny CLI for openrouter.ai. It sends a prompt to an AI and displays the result.

It is an opinionated, statically linked nightly Rust binary for Linux x86_64 that is `no_std`, has no external crate dependencies and does not use libc. It does direct syscalls in asm. Prioritize very small binary size and high performance.

It has three commands:
- Prompt: `ort -m <model-id> "The prompt"`. Default mode.
- List: `ort list` to show available models.
- Info: `ort info <model>` to learn about a model.

Separately, there is a minimal agent harness in binary `src/bin/art` which does use the standard library.

After changing a Rust file call these in order. Do not call them for a code review or a documentation-only change. If you anticipate making several changes, only call after the final change:
- `cargo fmt`
- `cargo clippy`
- `cargo test`
