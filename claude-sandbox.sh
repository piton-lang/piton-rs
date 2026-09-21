#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

PROJECT_DIR="$PWD"
MODE="claude"

# Usage:
#
#   claude-sandbox
#   claude-sandbox ./some-project
#   claude-sandbox ./some-project --dangerously-skip-permissions
#   claude-sandbox --shell
#   claude-sandbox ./some-project --shell
#

if [[ $# -gt 0 && "$1" != -* ]]; then
    PROJECT_DIR="$(realpath "$1")"
    shift
fi

if [[ "${1:-}" == "--shell" ]]; then
    MODE="shell"
    shift
fi

if [[ ! -d "$PROJECT_DIR" ]]; then
    echo "Project directory does not exist: $PROJECT_DIR" >&2
    exit 1
fi

export PROJECT_DIR
export LOCAL_UID="$(id -u)"
export LOCAL_GID="$(id -g)"

cd "$SCRIPT_DIR"

# Build/update the sandbox image. Docker's layer cache makes this cheap
# when nothing has changed.
docker compose build

# NOTE: in `docker compose run SERVICE [COMMAND] [ARGS...]` the first
# trailing word REPLACES the image CMD. So the command has to be named
# explicitly, otherwise `claude-sandbox --some-flag` tries to exec the
# flag itself as a program.
if [[ "$MODE" == "shell" ]]; then
    exec docker compose run \
        --rm \
        --entrypoint /bin/bash \
        claude \
        "$@"
else
    exec docker compose run \
        --rm \
        claude \
        claude "$@"
fi
