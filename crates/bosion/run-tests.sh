#!/bin/bash

set -euo pipefail

for test in default no-git no-std; do
	echo "Testing $test"
	pushd examples/$test
	cargo check
	cargo test
	popd
done
