#!/bin/bash

set -euo pipefail

export WATCHEXEC_BIN=$(realpath ${WATCHEXEC_BIN:-$(which watchexec)})
export TEST_SOCKETFD_BIN=$(realpath ${TEST_SOCKETFD_BIN:-$(which test-socketfd)})

platform="${1:-unix}"

cd "$(dirname "${BASH_SOURCE[0]}")/integration"
for test in *.sh; do
	if [[ "$test" == *-unix.sh && "$platform" = "windows" ]]; then
		echo "Skipping $test as it requires unix"
		continue
	fi
	if [[ "$test" == *-win.sh && "$platform" != "windows" ]]; then
		echo "Skipping $test as it requires windows"
		continue
	fi

	echo
	echo
	echo "======= Testing $test ======="
	./$test
done
