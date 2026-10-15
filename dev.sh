#!/usr/bin/env bash
#
# PatientSpace dev startup script.
#
# Starts the Rust hub + Rust core + Svelte frontend, all in the background.
# Logs go to /tmp/patientspace-*.log
#
# Usage:
#   ./dev.sh          # start everything
#   ./dev.sh stop      # stop everything
#   ./dev.sh restart   # restart everything
#   ./dev.sh status     # check what's running
#   ./dev.sh seed       # seed 50 synthetic patients
#   ./dev.sh logs       # tail all logs

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
PID_DIR="/tmp/patientspace-pids"
LOG_DIR="/tmp/patientspace-logs"
mkdir -p "$PID_DIR" "$LOG_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; }

# ---- Helpers ----

is_running() {
    local pidfile="$1"
    if [[ -f "$pidfile" ]]; then
        local pid
        pid=$(cat "$pidfile")
        if kill -0 "$pid" 2>/dev/null; then
            return 0
        fi
    fi
    return 1
}

start_process() {
    local name="$1"
    local pidfile="$PID_DIR/$name.pid"
    local logfile="$LOG_DIR/$name.log"
    shift 3

    if is_running "$pidfile"; then
        warn "$name is already running (PID $(cat "$pidfile"))"
        return 0
    fi

    info "Starting $name..."
    "$@" >"$logfile" 2>&1 &
    echo $! >"$pidfile"
    sleep 1

    if is_running "$pidfile"; then
        info "  $name started (PID $(cat "$pidfile")) — logs: $logfile"
    else
        error "  $name failed to start. Check $logfile"
        return 1
    fi
}

stop_process() {
    local name="$1"
    local pidfile="$PID_DIR/$name.pid"

    if is_running "$pidfile"; then
        local pid
        pid=$(cat "$pidfile")
        info "Stopping $name (PID $pid)..."
        kill "$pid" 2>/dev/null || true
        sleep 2
        kill -9 "$pid" 2>/dev/null || true
        rm -f "$pidfile"
        info "  $name stopped"
    else
        info "$name is not running"
        rm -f "$pidfile"
    fi
}

status_process() {
    local name="$1"
    local pidfile="$PID_DIR/$name.pid"

    if is_running "$pidfile"; then
        info "  $name: running (PID $(cat "$pidfile"))"
    else
        warn "  $name: not running"
    fi
}

# ---- Commands ----

cmd_start() {
    info "=== Starting PatientSpace dev environment ==="

    # Check for Rust
    if ! command -v cargo &>/dev/null; then
        if [[ -f "$HOME/.cargo/env" ]]; then
            source "$HOME/.cargo/env"
        else
            error "Rust is not installed. Install with: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
            exit 1
        fi
    fi

    # Check for bun
    if ! command -v bun &>/dev/null; then
        error "Bun is not installed. Install from https://bun.sh"
        exit 1
    fi

    cd "$ROOT_DIR"

    # 1. Build Rust binaries (if needed)
    info "Building Rust binaries..."
    cargo build -p patientspace-hub -p patientspace-core 2>&1 | tail -1
    info "  Rust build complete"

    # 2. Start hub
    start_process "hub" "$LOG_DIR/hub.log" \
        cargo run -p patientspace-hub

    # Wait for hub to be ready
    info "Waiting for hub to be ready..."
    for i in $(seq 1 10); do
        if curl -sf http://127.0.0.1:8081/health &>/dev/null; then
            info "  Hub is ready"
            break
        fi
        sleep 1
    done

    # 3. Start core
    PATIENTSPACE_FACILITY_ID=F-DEV1 \
    PATIENTSPACE_BIND=0.0.0.0:8080 \
    PATIENTSPACE_DATABASE_URL="sqlite:${ROOT_DIR}/patientspace-dev.db?mode=rwc" \
    PATIENTSPACE_HUB_URL=http://127.0.0.1:8081 \
    RUST_LOG=info \
    start_process "core" "$LOG_DIR/core.log" \
        cargo run -p patientspace-core

    # Wait for core to be ready
    info "Waiting for core to be ready..."
    for i in $(seq 1 10); do
        if curl -sf http://127.0.0.1:8080/health &>/dev/null; then
            info "  Core is ready"
            break
        fi
        sleep 1
    done

    # 4. Install frontend deps (if needed)
    if [[ ! -d "$ROOT_DIR/frontend/node_modules" ]]; then
        info "Installing frontend dependencies..."
        cd "$ROOT_DIR/frontend"
        bun install
        cd "$ROOT_DIR"
    fi

    # 5. Start frontend
    cd "$ROOT_DIR/frontend"
    start_process "frontend" "$LOG_DIR/frontend.log" \
        bun run dev -- --host 0.0.0.0
    cd "$ROOT_DIR"

    echo ""
    info "=== PatientSpace is running ==="
    echo ""
    echo "  Frontend:  http://localhost:5173"
    echo "  Core API:  http://localhost:8080/health"
    echo "  Hub:       http://localhost:8081/health"
    echo ""
    echo "  Logs:      $LOG_DIR/{hub,core,frontend}.log"
    echo "  Stop:      ./dev.sh stop"
    echo "  Seed data: ./dev.sh seed"
    echo ""
}

cmd_stop() {
    info "=== Stopping PatientSpace ==="
    stop_process "frontend"
    stop_process "core"
    stop_process "hub"
    info "All stopped."
}

cmd_restart() {
    cmd_stop
    echo ""
    cmd_start
}

cmd_status() {
    info "=== PatientSpace status ==="
    status_process "hub"
    status_process "core"
    status_process "frontend"
    echo ""

    if is_running "$PID_DIR/core.pid"; then
        local count
        count=$(curl -sf http://127.0.0.1:8080/health 2>/dev/null | grep -o '"envelope_count":[0-9]*' | grep -o '[0-9]*' || echo "?")
        info "Records in core: $count"
    fi

    if is_running "$PID_DIR/hub.pid"; then
        if curl -sf http://127.0.0.1:8081/health &>/dev/null; then
            info "Hub health: OK"
        else
            warn "Hub health: not responding"
        fi
    fi
}

cmd_seed() {
    info "=== Seeding 50 synthetic patients ==="
    if ! is_running "$PID_DIR/core.pid"; then
        error "Core is not running. Start with: ./dev.sh start"
        exit 1
    fi

    if command -v bun &>/dev/null; then
        bun run "$ROOT_DIR/scripts/seed.ts" --url http://localhost:8080
    else
        error "Bun is not installed. Install from https://bun.sh"
        exit 1
    fi
}

cmd_logs() {
    info "=== Tailing logs (Ctrl+C to exit) ==="
    echo ""
    tail -f "$LOG_DIR/hub.log" "$LOG_DIR/core.log" "$LOG_DIR/frontend.log" 2>/dev/null || {
        warn "No log files found. Start the services first: ./dev.sh start"
    }
}

# ---- Main ----

case "${1:-start}" in
    start)   cmd_start ;;
    stop)    cmd_stop ;;
    restart) cmd_restart ;;
    status)  cmd_status ;;
    seed)    cmd_seed ;;
    logs)    cmd_logs ;;
    *)
        echo "Usage: ./dev.sh {start|stop|restart|status|seed|logs}"
        echo ""
        echo "  start    Start hub + core + frontend (default)"
        echo "  stop     Stop all services"
        echo "  restart  Stop and start again"
        echo "  status   Show running status"
        echo "  seed     Seed 50 synthetic patients via the API"
        echo "  logs     Tail all log files"
        exit 1
        ;;
esac
