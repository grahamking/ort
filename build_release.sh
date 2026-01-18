# Use this script to build the `ort` binary in --release mode.
# `cargo build --release` WILL NOT WORK.

# Build std from source to avoid unwinding infrastructure. Save over 60 KiB and a dynamic link to libgcc_s.
cargo build --release -Zbuild-std="core,alloc"

# Saves about 3 KiB
if command -v llvm-strip >/dev/null 2>&1; then
    llvm-strip --strip-sections --strip-all target/release/ort
fi
