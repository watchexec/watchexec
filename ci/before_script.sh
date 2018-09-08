#!/usr/bin/env bash

rustup target add $TARGET
cargo clean --target $TARGET --verbose
