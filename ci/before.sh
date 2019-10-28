#!/usr/bin/env bash

set -e

rustup target add $TARGET
cargo clean --target $TARGET --verbose

if [ $TRAVIS_OS_NAME = windows ]; then
    choco install windows-sdk-10.1
fi

if [[ ! -z "$CARGO_AUDIT" ]]; then
    which cargo-audit || cargo install --debug cargo-audit
    # --debug for faster build at the minimal expense of runtime speed
elif [[ ! -z "$CARGO_CLIPPY" ]]; then
    rustup component add clippy
fi

