#!/usr/bin/env bash
#
# horizon-run.sh – Automates Horizon setup and launch.
#
set -euo pipefail

# ----------------------------------------------------------------------
# Configuration
# ----------------------------------------------------------------------
REPO_URL="git@github.com:horizon-rust/horizon.git"
REPO_DIR="horizon"
LAUNCHER_BIN="horizon-launcher"
LAUNCHER_ARCHIVE="horizon-launcher-linux-x86_64.tar.gz"
MINECRAFT_DIR="${HORIZON_MINECRAFT_DIR:-$HOME/.minecraft}"
VERSION="26.3"

# ----------------------------------------------------------------------
# Options
#   -n   fetch + fast-forward pull from origin before building
#   -h   this help
# ----------------------------------------------------------------------
UPDATE=0
usage() {
    cat <<EOF
Usage: $0 [-n]

  -n   Update the repository (git pull --ff-only) before building.
       Without -n the working tree is left untouched and cargo will
       usually rebuild nothing.
  -h   Show this help.
EOF
}

while getopts ":nh" opt; do
    case "$opt" in
        n) UPDATE=1 ;;
        h) usage; exit 0 ;;
        \?) err "Unknown option: -$OPTARG" ;;
        :)  err "Option -$OPTARG requires an argument." ;;
    esac
done
shift $((OPTIND - 1))

# ----------------------------------------------------------------------
# Helpers
# ----------------------------------------------------------------------
log() { printf '\n[%s] %s\n' "$(date +%H:%M:%S)" "$*"; }
err() { printf '\n[ERROR] %s\n' "$*" >&2; exit 1; }

# ----------------------------------------------------------------------
# 1. Clone (first run) or optionally update (-n)
# ----------------------------------------------------------------------
if [[ ! -d "$REPO_DIR" ]]; then
    log "Cloning Horizon repository into '$REPO_DIR' ..."
    git clone --depth 1 "$REPO_URL" "$REPO_DIR" || err "Failed to clone."
elif [[ "$UPDATE" == "1" ]]; then
    log "Updating repository (-n) ..."
    ( cd "$REPO_DIR" && git pull --ff-only ) || err "Failed to pull."
else
    log "Skipping git pull (pass -n to update)."
fi
cd "$REPO_DIR"

# ----------------------------------------------------------------------
# 2. Install the launcher inside horizon/.local
# ----------------------------------------------------------------------
LAUNCHER_PATH=".local/${LAUNCHER_BIN}"
if [[ ! -x "$LAUNCHER_PATH" ]]; then
    log "Installing launcher into .local ..."
    mkdir -p .local
    API_URL="https://api.github.com/repos/horizon-rust/horizon/releases?per_page=20"
    DOWNLOAD_URL=$(
        curl -fsSL "$API_URL" |
        jq -r --arg A "$LAUNCHER_ARCHIVE" '
            .[] | select(.tag_name|startswith("launcher@"))
                | .assets[] | select(.name==$A) | .browser_download_url
        ' | head -n1
    )
    [[ -n "$DOWNLOAD_URL" ]] || err "Could not find launcher asset ($LAUNCHER_ARCHIVE)."
    log "Downloading: $DOWNLOAD_URL"
    curl -fsSL -o "$LAUNCHER_ARCHIVE" "$DOWNLOAD_URL" || err "Download failed."
    tar -xzf "$LAUNCHER_ARCHIVE" -C .local || err "Extraction failed."
    rm -f "$LAUNCHER_ARCHIVE"

    # The archive may unpack directly or into a subdirectory.
    if [[ ! -x "$LAUNCHER_PATH" ]]; then
        found=$(find .local -maxdepth 3 -type f -name "$LAUNCHER_BIN" | head -n1 || true)
        if [[ -n "$found" ]]; then
            chmod +x "$found"
            ln -sf "$(realpath --relative-to=.local "$found")" "$LAUNCHER_PATH" \
                || cp "$found" "$LAUNCHER_PATH"
        fi
    fi
    [[ -x "$LAUNCHER_PATH" ]] || err "Launcher binary not executable."
    log "Launcher ready at $LAUNCHER_PATH"
else
    log "Launcher already installed at $LAUNCHER_PATH"
fi

# ----------------------------------------------------------------------
# 3. Configure launcher (idempotent)
#    Syntax is `settings <KEY> [VALUE]` — NO `get` / `set` verb.
# ----------------------------------------------------------------------
log "Configuring launcher minecraft-dir ..."
current=$("$LAUNCHER_PATH" settings minecraft-dir 2>/dev/null || true)
if [[ "$current" == *"$MINECRAFT_DIR"* ]]; then
    log "minecraft-dir already set to $MINECRAFT_DIR"
else
    "$LAUNCHER_PATH" settings minecraft-dir "$MINECRAFT_DIR" \
        || err "Failed to set minecraft-dir. Try: $LAUNCHER_PATH settings --help"
    log "minecraft-dir → $MINECRAFT_DIR"
fi

# ----------------------------------------------------------------------
# 4. Ensure the client + server jars are present
# ----------------------------------------------------------------------
client_jar="$MINECRAFT_DIR/versions/$VERSION/$VERSION.jar"

find_server_jar() {
    if [[ -n "${HORIZON_SERVER_JAR:-}" && -f "${HORIZON_SERVER_JAR}" ]]; then
        printf '%s' "$HORIZON_SERVER_JAR"; return 0
    fi
    find .local/minecraft "$MINECRAFT_DIR" -type f -name 'server*.jar' 2>/dev/null | head -n1
}
server_jar="$(find_server_jar || true)"

# If either is missing, ask the launcher to fetch (it can only do this
# when signed in — the exact subcommand varies between builds, so try
# the common spellings; a miss just falls through to the jar check).
if [[ ! -f "$client_jar" || -z "$server_jar" ]]; then
    log "Missing jar(s) — asking launcher to fetch (requires sign-in) ..."
    for sub in download install fetch prepare setup; do
        if "$LAUNCHER_PATH" "$sub" &>/dev/null; then
            log "Launcher '$sub' completed."
            break
        fi
    done
    client_jar="$MINECRAFT_DIR/versions/$VERSION/$VERSION.jar"
    server_jar="$(find_server_jar || true)"
fi

[[ -f "$client_jar" ]] || err "Client jar missing: $client_jar
  Sign in with '$LAUNCHER_PATH', or play once from the official launcher."
[[ -n "$server_jar" && -f "$server_jar" ]] || err "Server jar missing (searched .local/minecraft and $MINECRAFT_DIR).
  Sign in so the launcher can download it, or set HORIZON_SERVER_JAR=/path/to/server.jar."
log "Client jar: $client_jar"
log "Server jar: $server_jar"

# ----------------------------------------------------------------------
# 5. Prepare resources (only when missing, stale, or forced)
# ----------------------------------------------------------------------
PREPARED_DIR=".local/horizon-client/$VERSION"

needs_prepare() {
    [[ "${HORIZON_FORCE_PREPARE:-0}" == "1" ]] && return 0

    # Missing or empty → prepare
    [[ ! -d "$PREPARED_DIR" ]] && return 0
    [[ -z "$(ls -A "$PREPARED_DIR" 2>/dev/null)" ]] && return 0

    # Inputs newer than the prepared tree → re-prepare
    [[ "$client_jar" -nt "$PREPARED_DIR" ]] && return 0
    [[ "$server_jar" -nt "$PREPARED_DIR" ]] && return 0

    return 1
}

if needs_prepare; then
    if [[ "${HORIZON_FORCE_PREPARE:-0}" == "1" ]]; then
        log "Preparing Horizon resources (forced) ..."
    elif [[ -d "$PREPARED_DIR" && -n "$(ls -A "$PREPARED_DIR" 2>/dev/null)" ]]; then
        log "Preparing Horizon resources (inputs changed) ..."
    else
        log "Preparing Horizon resources (first run) ..."
    fi

    cargo run --release -p horizon-prepare -- \
        --prepare --minecraft-dir "$MINECRAFT_DIR" --server-jar "$server_jar" \
        || err "horizon-prepare failed."
else
    log "Resources already prepared at $PREPARED_DIR – skipping."
    log "  (set HORIZON_FORCE_PREPARE=1 to force a re-run)"
fi

# ----------------------------------------------------------------------
# 6. Build the client
# ----------------------------------------------------------------------
log "Building horizon-client (first build can take ~15 min) ..."
cargo build --release -p horizon-client || err "Build failed."

# ----------------------------------------------------------------------
# 7. Run
# ----------------------------------------------------------------------
log "Launching Horizon client ..."
if [[ -n "${HORIZON_SESSION_FILE:-}" && -f "${HORIZON_SESSION_FILE}" ]]; then
    log "Using session file: $HORIZON_SESSION_FILE"
    cargo run --release -p horizon-client -- --session-file "$HORIZON_SESSION_FILE" "$@"
else
    log "Tip: export HORIZON_SESSION_FILE=/path/to/session.json to pass --session-file."
    cargo run --release -p horizon-client -- "$@"
fi
