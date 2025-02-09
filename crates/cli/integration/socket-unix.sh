#!/bin/bash

set -euxo pipefail

watchexec=${WATCHEXEC_BIN:-watchexec}
test_socketfd=${TEST_SOCKETFD_BIN:-test-socketfd}

timeout -s9 10s sh -c "$watchexec --socket 18080 -1 -- $test_socketfd tcp"
timeout -s9 10s sh -c "$watchexec --socket udp::18080 -1 -- $test_socketfd udp"
timeout -s9 10s sh -c "$watchexec --socket 18080 --socket 28080 -1 -- $test_socketfd tcp tcp"
timeout -s9 10s sh -c "$watchexec --socket 18080 --socket 28080 --socket udp::38080 -1 -- $test_socketfd tcp tcp udp"
timeout -s9 10s sh -c "$watchexec --socket 127.0.1.1:18080 -1 -- $test_socketfd tcp"
timeout -s9 10s sh -c "$watchexec --socket udp:127.0.1.1:18080 -1 -- $test_socketfd udp"

