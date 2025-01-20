#!/bin/bash

set -euo pipefail

for test in examples/*; do
	echo "Testing $test"
	pushd $test
	if ! test -f Cargo.toml; then
		popd
		continue
	fi

	cargo check
	cargo test

	popd
done
