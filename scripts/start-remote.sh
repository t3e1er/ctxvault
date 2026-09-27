#!/usr/bin/env bash
# groundcontrol Remote Services Launcher for macOS and Linux
# Starts the background MCP daemon and GraphView dashboard bound to 0.0.0.0,
# detecting local LAN IP and outputting all remote connection endpoints.

set -e

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DAEMON_PORT=9090
GRAPHVIEW_PORT=9091
SYNC_FLAG=""
WATCH_FLAG=""
BG_GRAPHVIEW=false
OVERRIDE_IP=""

while [ $# -gt 0 ]; do
    case "$1" in
        --daemon-port)
            DAEMON_PORT="$2"
            shift 2
            ;;
        --graphview-port)
            GRAPHVIEW_PORT="$2"
            shift 2
            ;;
        --sync)
            SYNC_FLAG="--sync"
            shift
            ;;
        --watch)
            WATCH_FLAG="--watch"
            shift
            ;;
        --background)
            BG_GRAPHVIEW=true
            shift
            ;;
        --ip)
            OVERRIDE_IP="$2"
            shift 2
            ;;
        *)
            shift
            ;;
    esac
done

# 1. Resolve GroundControl executable
if command -v groundcontrol >/dev/null 2>&1; then
    GC_BIN="groundcontrol"
elif [ -x "$HOME/.local/bin/groundcontrol" ]; then
    GC_BIN="$HOME/.local/bin/groundcontrol"
elif [ -x "$REPO_ROOT/target/release/groundcontrol" ]; then
    GC_BIN="$REPO_ROOT/target/release/groundcontrol"
else
    echo "[ERROR] groundcontrol binary not found. Run 'scripts/install-local.sh' first." >&2
    exit 1
fi

# 2. Start MCP Background Daemon on 0.0.0.0
echo ""
echo "[*] Starting GroundControl MCP daemon on 0.0.0.0:${DAEMON_PORT}..."
"$GC_BIN" daemon start --bind "0.0.0.0:${DAEMON_PORT}" $SYNC_FLAG $WATCH_FLAG

# 3. Detect Active Network IP Addresses
if [ -n "$OVERRIDE_IP" ]; then
    HOST_IP="$OVERRIDE_IP"
else
    if command -v ip >/dev/null 2>&1; then
        HOST_IP=$(ip -4 route get 1.1.1.1 2>/dev/null | awk '{print $7; exit}')
    fi
    if [ -z "$HOST_IP" ] && command -v hostname >/dev/null 2>&1; then
        HOST_IP=$(hostname -I 2>/dev/null | awk '{print $1}')
    fi
    if [ -z "$HOST_IP" ]; then
        HOST_IP="127.0.0.1"
    fi
fi

# 4. Display Connection Card
echo ""
echo "=========================================================================="
echo "  GroundControl Remote Services Ready"
echo "=========================================================================="
echo "  * Host IP Address     : ${HOST_IP}"
echo ""
echo "  --- Remote Access Endpoints ---"
echo "  * GraphView 3D Web UI : http://${HOST_IP}:${GRAPHVIEW_PORT}"
echo "  * MCP SSE Stream      : http://${HOST_IP}:${DAEMON_PORT}/sse"
echo "  * MCP JSON-RPC Stream : http://${HOST_IP}:${DAEMON_PORT}/mcp"
echo "  * Health Check Probe  : http://${HOST_IP}:${DAEMON_PORT}/health"
echo ""
echo "  --- Remote Agent MCP Configuration (.agents/mcp_config.json) ---"
echo "  {"
echo "    \"mcpServers\": {"
echo "      \"groundcontrol\": {"
echo "        \"command\": \"groundcontrol\","
echo "        \"args\": ["
echo "          \"--mode\", \"proxy\","
echo "          \"--server\", \"http://${HOST_IP}:${DAEMON_PORT}\""
echo "        ]"
echo "      }"
echo "    }"
echo "  }"
echo "=========================================================================="
echo ""

# 5. Launch GraphView
if [ "$BG_GRAPHVIEW" = true ]; then
    echo "[*] Launching GraphView in background on port ${GRAPHVIEW_PORT}..."
    nohup "$GC_BIN" graphview --bind "0.0.0.0:${GRAPHVIEW_PORT}" --daemon "http://127.0.0.1:${DAEMON_PORT}" >/dev/null 2>&1 &
    echo "[+] GraphView started in background (PID $!)."
else
    echo "[*] Launching GraphView (Press Ctrl+C to close dashboard; daemon remains running)..."
    exec "$GC_BIN" graphview --bind "0.0.0.0:${GRAPHVIEW_PORT}" --daemon "http://127.0.0.1:${DAEMON_PORT}"
fi
