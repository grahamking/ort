# Use this script to build the `ort` binary. It must be built from the `cli/` directory so that we use the .cargo/config.toml in there.
#
# The tests use the std lib, but cli does not.
# Also the cli re-builds parts of the Rust stdlib (see cli/.cargo/config.toml). This causes a conflict with 'cargo test'.
# Hence we split the project into two crates, one that has all the weird stuff (cli/) and one that has all the code (core/) and tests.
#
pushd cli
cargo build --release
popd
dirs -c

# Saves about 3 KiB
if command -v llvm-strip >/dev/null 2>&1; then
    llvm-strip --strip-sections --strip-all target/release/ort
fi
