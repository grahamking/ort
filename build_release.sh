# Use this script to build the `ort` binary in --release mode.

# Build std from source to avoid unwinding infrastructure. Save over 60 KiB and a dynamic link to libgcc_s.
#cargo build --release -Zbuild-std="core,alloc"
cargo build --release

# Only saves about 1 KiB, not really worthwhile
if command -v llvm-strip >/dev/null 2>&1; then
    llvm-strip --strip-sections --strip-all target/release/ort
fi
