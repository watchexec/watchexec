#!/bin/bash

set -euxo pipefail

watchexec=${WATCHEXEC_BIN:-watchexec}

timeout -s9 30s sh -c "sleep 10 | $watchexec --stdin-quit echo"
