# Use this script to build the `ort` binary. It must be built from the `cli/` directory so that we use the .cargo/config.toml in there.
#
# The tests use the std lib, but cli does not.
# Also the cli re-builds parts of the Rust stdlib (see cli/.cargo/config.toml). This causes a conflict with 'cargo test'.
# Hence we split the project into two crates, one that has all the weird stuff (cli/) and one that has all the code (core/) and tests.
#

# Set RUSTFLAGS here because .cargo/config.toml [build.rustflags] is only used if
# RUSTFLAGS is not set, and presumably most devs have it set.
# https://doc.rust-lang.org/cargo/reference/config.html#buildrustflags
#
# However, `cargo install` is not going to use this script, so _also_ set it in the config. Keep them in sync.
#
RUSTFLAGS="-Ctarget-cpu=native -Crelocation-model=static -Clink-args=-Wl,--build-id=none,--no-eh-frame-hdr -Cforce-frame-pointers=yes"

pushd cli
cargo build --release
popd
dirs -c

# Saves about 3 KiB
if command -v llvm-strip >/dev/null 2>&1; then
    llvm-strip --strip-sections --strip-all target/release/ort
fi
