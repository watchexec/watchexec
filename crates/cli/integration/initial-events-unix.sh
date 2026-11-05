#!/bin/bash

set -euo pipefail

watchexec=${WATCHEXEC_BIN:-watchexec}
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT
mkdir "$fixture/watched"
touch "$fixture/watched/existing"

output=$(timeout 10 "$watchexec" -1 --watch "$fixture/watched" \
	--emit-events-to=stdio -n -- cat)
test -z "$output"

output=$(timeout 10 "$watchexec" -1 --watch "$fixture/watched" \
	--initial-events=create --emit-events-to=stdio -n -- cat)
if ! printf '%s\n' "$output" | grep -Fx "create:$fixture/watched/existing"; then
	printf 'First emission is missing the pre-existing path; output was: %s\n' "$output" >&2
	exit 1
fi

output=$(timeout 10 "$watchexec" -1 --watch "$fixture/watched" \
	--initial-events=create --emit-events-to=json-stdio -n -- cat)
printf '%s\n' "$output" | grep -F "$fixture/watched/existing"

for mode in file json-file; do
	output=$(timeout 10 "$watchexec" -1 --watch "$fixture/watched" \
		--initial-events=create --emit-events-to="$mode" -n -- \
		sh -c 'cat "$WATCHEXEC_EVENTS_FILE"')
	printf '%s\n' "$output" | grep -F "$fixture/watched/existing"
done

output=$(timeout 10 "$watchexec" -1 --watch "$fixture/watched" \
	--initial-events=create --emit-events-to=environment -n -- \
	sh -c 'printf "%s\n" "$WATCHEXEC_CREATED_PATH"')
printf '%s\n' "$output" | grep -F 'watched/existing'

output=$(timeout 10 "$watchexec" -1 --watch "$fixture/watched" \
	--initial-events=create --fs-events=modify --emit-events-to=stdio -n -- cat)
printf '%s\n' "$output" | grep -Fx "create:$fixture/watched/existing"
