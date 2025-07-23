#!/bin/bash

set -euxo pipefail

watchexec=${WATCHEXEC_BIN:-watchexec}

$watchexec -1 --env FOO=BAR echo '$FOO' | grep BAR
