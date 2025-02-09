#!/bin/bash

set -euxo pipefail

watchexec=${WATCHEXEC_BIN:-watchexec}
test_socketfd=${TEST_SOCKETFD_BIN:-test-socketfd}

$watchexec --socket 18080 -1 -- $test_socketfd tcp
$watchexec --socket udp::18080 -1 -- $test_socketfd udp
$watchexec --socket 18080 --socket 28080 -1 -- $test_socketfd tcp tcp
$watchexec --socket 18080 --socket 28080 --socket udp::38080 -1 -- $test_socketfd tcp tcp udp

if [[ "$TEST_PLATFORM" = "linux" ]]; then
	$watchexec --socket 127.0.1.1:18080 -1 -- $test_socketfd tcp
fi

