#!/usr/bin/env bash
#
# Backup and restore for PatientSpace.
#
# Usage:
#   ./scripts/backup.sh                    # backup to /var/lib/patientspace/backups/
#   ./scripts/backup.sh /mnt/usb           # backup to USB stick
#   ./scripts/backup.sh /mnt/cloud         # backup to cloud mount
#
#   ./scripts/restore.sh /var/lib/patientspace/backups/patientspace-2026-10-16.tar.gz
#
# The backup is a simple tar.gz of the SQLite database + config.
# SQLite WAL mode ensures consistency — we use sqlite3 .backup for a safe copy.

set -euo pipefail

DATA_DIR="${PATIENTSPACE_DATA_DIR:-/var/lib/patientspace}"
BACKUP_DIR="${1:-$DATA_DIR/backups}"
TIMESTAMP=$(date +%Y-%m-%d_%H%M%S)
BACKUP_FILE="$BACKUP_DIR/patientspace-$TIMESTAMP.tar.gz"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

# Detect mode
MODE="${1:-backup}"
if [[ "$MODE" == "restore" ]]; then
    RESTORE_FILE="${2:?Usage: backup.sh restore <backup-file>}"
    
    info "=== Restoring from $RESTORE_FILE ==="
    
    if [[ ! -f "$RESTORE_FILE" ]]; then
        error "Backup file not found: $RESTORE_FILE"
    fi
    
    # Stop the service
    if systemctl is-active patientspace-core &>/dev/null; then
        info "Stopping patientspace-core..."
        systemctl stop patientspace-core
    fi
    
    # Backup current data (just in case)
    if [[ -f "$DATA_DIR/patientspace.db" ]]; then
        info "Backing up current database..."
        cp "$DATA_DIR/patientspace.db" "$DATA_DIR/patientspace.db.pre-restore"
    fi
    
    # Extract
    info "Restoring..."
    tar xzf "$RESTORE_FILE" -C "$DATA_DIR/"
    
    # Fix ownership
    chown patientspace:patientspace "$DATA_DIR"/patientspace.db*
    
    # Restart
    info "Starting patientspace-core..."
    systemctl start patientspace-core
    sleep 3
    
    if curl -sf http://localhost:8080/health &>/dev/null; then
        info "Restore complete — PatientSpace is running."
    else
        error "Restore complete but service didn't start. Check: journalctl -u patientspace-core"
    fi
    
    exit 0
fi

# ---- Backup mode ----

info "=== PatientSpace Backup ==="
info "Data dir:   $DATA_DIR"
info "Backup to:  $BACKUP_DIR"

mkdir -p "$BACKUP_DIR"

# Use sqlite3 .backup for a consistent snapshot (handles WAL correctly)
DB_FILE="$DATA_DIR/patientspace.db"
TMP_DIR=$(mktemp -d)

if [[ -f "$DB_FILE" ]]; then
    info "Creating consistent SQLite snapshot..."
    sqlite3 "$DB_FILE" ".backup '$TMP_DIR/patientspace.db'"
else
    error "Database not found: $DB_FILE"
fi

# Copy config
if [[ -d "/etc/patientspace" ]]; then
    cp -r /etc/patientspace "$TMP_DIR/config"
fi

# Create tarball
info "Creating archive..."
tar czf "$BACKUP_FILE" -C "$TMP_DIR" patientspace.db config

# Cleanup
rm -rf "$TMP_DIR"

# Show result
SIZE=$(du -h "$BACKUP_FILE" | cut -f1)
info "Backup complete: $BACKUP_FILE ($SIZE)"

# Keep last 7 backups, delete older ones
info "Cleaning old backups (keeping last 7)..."
ls -t "$BACKUP_DIR"/patientspace-*.tar.gz 2>/dev/null | tail -n +8 | xargs rm -f 2>/dev/null || true

info "Done."
echo ""
echo "To restore:"
echo "  sudo ./scripts/backup.sh restore $BACKUP_FILE"
