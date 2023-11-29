#!/bin/bash

set -euo pipefail

export WATCHEXEC_BIN=$(realpath ${WATCHEXEC_BIN:-$(which watchexec)})

cd "$(dirname "${BASH_SOURCE[0]}")/integration"
for test in *.sh; do
	echo
	echo
	echo "======= Testing $test ======="
	./$test
done
