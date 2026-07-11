# Shared environment loader for the tools/ scripts. Not executable — source it:
#   . "$(dirname "$0")/env.sh"
# Resolves the repo root, sources <repo>/.env if present (copy .env.example there
# and fill in your paths), and provides require_env for mandatory variables.

DSVITA_TOOLS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DSVITA_ROOT="$(dirname "$DSVITA_TOOLS_DIR")"

if [ -f "$DSVITA_ROOT/.env" ]; then
    set -a
    . "$DSVITA_ROOT/.env"
    set +a
fi

require_env() {
    for var in "$@"; do
        if [ -z "$(eval echo "\$$var")" ]; then
            echo "error: $var is not set — copy .env.example to .env in the repo root and fill it in" >&2
            exit 1
        fi
    done
}

# Defaults that work on most setups; override in .env if needed.
: "${DSVITA_DISPLAY:=:0}"
: "${DSVITA_PI_BIN:=~/claude/dsvita/dsvita}"
: "${DSVITA_PI_RUNTIME_DIR:=/run/user/1000}"
: "${DSVITA_PI_WAYLAND_DISPLAY:=wayland-0}"
: "${DSVITA_PI_WTYPE:=wtype}"
