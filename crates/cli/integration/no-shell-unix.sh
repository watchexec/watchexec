#!/bin/bash

set -euxo pipefail

watchexec=${WATCHEXEC_BIN:-watchexec}

$watchexec -1 -n echo 'foo  bar' | grep 'foo  bar'
