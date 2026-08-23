#!/bin/bash

# no_std, with an assembly start.s, specialized rustflags
cargo build --profile release --bin ort && echo "target/release/ort"

# more traditional release mode, uses stdlib
cargo build --profile release-art --bin art && echo "target/release-art/art"
