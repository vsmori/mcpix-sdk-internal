#!/bin/bash
# Wrapper for GHA defaults.run.shell. Tees every step's stdout+stderr
# into a per-job log so the upload-artifact step can publish a
# downloadable file with real compiler/test output on failure.
#
# Usage (in workflow defaults.run.shell):
#   shell: bash .github/scripts/ci-shell.sh {0}
#
# GHA replaces {0} with the temp step script path; we receive it as $1
# and run it in a subshell piped through tee. PIPESTATUS[0] carries
# the exit code of the step command, not tee, so failures propagate.
#
# Bash 3.2+ compatible (macOS GHA runners ship system bash 3.2).
set -e
mkdir -p /tmp/ci-logs
bash -eo pipefail "$1" 2>&1 | tee -a /tmp/ci-logs/full.log
exit "${PIPESTATUS[0]}"
