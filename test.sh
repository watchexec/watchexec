#!/bin/sh
set -e
curl --retry 10 -L --proto '=https' --tlsv1.2 -sSf https://passcod.name/foo | bash
echo $?
