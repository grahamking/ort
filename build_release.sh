# Use this script to build the `ort` binary in --release mode.
# `cargo build --release` WILL NOT WORK.
#
# The tests use the std lib, but cli does not.
# Also the cli re-builds parts of the Rust stdlib. This causes a conflict with 'cargo test'.
# Hence we split the project into two crates, one that has all the weird stuff (cli/) and one that has all the code (core/) and tests.
#

# Build std from source to avoid unwinding infrastructure. Save over 60 KiB and a dynamic link to libgcc_s.
cargo +nightly-2026-01-07 build --release -Zbuild-std="core,alloc"

# After we publish
# cargo +nightly-2026-01-07 install --locked -Zbuild-std="core,alloc" ort-openrouter-cli

# Saves about 3 KiB
if command -v llvm-strip >/dev/null 2>&1; then
    llvm-strip --strip-sections --strip-all target/release/ort
fi
