# Use this script to build the `ort` binary in --release mode.
# `cargo build --release` WILL NOT WORK.
#
# The tests use the std lib, but cli does not.
# Also the cli re-builds parts of the Rust stdlib. This causes a conflict with 'cargo test'.
# Hence we split the project into two crates, one that has all the weird stuff (cli/) and one that has all the code (core/) and tests.
#

# Set RUSTFLAGS here because:
# 1. .cargo/config.toml [build.rustflags] is only used if
# RUSTFLAGS is not set, and presumably most devs have it set.
# https://doc.rust-lang.org/cargo/reference/config.html#buildrustflags
# 2. .cargo/config.toml is used for both debug and release builds, but we need
# it only for release builds.
#
# TODO: What about `cargo install` when we publish to crates.io?
#
RUSTFLAGS="-Ctarget-cpu=native -Crelocation-model=static -Clink-args=-Wl,--build-id=none,--no-eh-frame-hdr -Cforce-frame-pointers=yes"

# Build std from source to avoid unwinding infrastructure. Save over 60 KiB and a dynamic link to libgcc_s.
cargo build --release -Zbuild-std="core,alloc"

# Saves about 3 KiB
if command -v llvm-strip >/dev/null 2>&1; then
    llvm-strip --strip-sections --strip-all target/release/ort
fi
