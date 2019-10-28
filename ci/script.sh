#!/usr/bin/env bash

if [[ ! -z "$CARGO_AUDIT" ]]; then
    cargo check --target $TARGET
    cargo audit
elif [[ ! -z "$CARGO_CLIPPY" ]]; then
    cargo clippy --target $TARGET
else
    cargo test --target $TARGET
fi
