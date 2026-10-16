#!/usr/bin/env bash
#
# NUC provisioning script — takes a fresh Ubuntu 24.04 NUC from first boot
# to a running PatientSpace installation.
#
# Usage:
#   scp scripts/provision-nuc.sh nuc:~/
#   ssh nuc
#   chmod +x provision-nuc.sh
#   sudo ./provision-nuc.sh --facility-id F-PETAUKE1 --hub-url http://hub.patientspace.org
#
# Or run locally to provision this machine:
#   sudo ./provision-nuc.sh --facility-id F-PETAUKE1

set -euo pipefail

FACILITY_ID=""
HUB_URL="https://hub.patientspace.org"
INSTALL_DIR="/opt/patientspace"
DATA_DIR="/var/lib/patientspace"
CONFIG_DIR="/etc/patientspace"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

# Parse args
while [[ $# -gt 0 ]]; do
    case $1 in
        --facility-id) FACILITY_ID="$2"; shift 2 ;;
        --hub-url) HUB_URL="$2"; shift 2 ;;
        *) error "Unknown option: $1" ;;
    esac
done

if [[ -z "$FACILITY_ID" ]]; then
    error "Usage: $0 --facility-id F-XXXXXXXX [--hub-url URL]"
fi

if [[ $EUID -ne 0 ]]; then
    error "This script must be run as root (use sudo)"
fi

info "=== PatientSpace NUC Provisioning ==="
info "Facility ID: $FACILITY_ID"
info "Hub URL:     $HUB_URL"
echo ""

# 1. System update
info "Updating system packages..."
apt-get update -qq
apt-get upgrade -y -qq
apt-get install -y -qq curl ca-certificates sqlite3

# 2. Create system user
if ! id patientspace &>/dev/null; then
    info "Creating system user: patientspace"
    useradd --system --no-create-home --shell /usr/sbin/nologin patientspace
fi

# 3. Create directories
info "Creating directories..."
mkdir -p "$INSTALL_DIR/bin" "$DATA_DIR" "$CONFIG_DIR" "$INSTALL_DIR/frontend"
chown patientspace:patientspace "$DATA_DIR" "$CONFIG_DIR"

# 4. Download or copy release tarball
TARBALL="patientspace-0.1.0-linux-x86_64.tar.gz"
if [[ -f "/tmp/$TARBALL" ]]; then
    info "Found release tarball at /tmp/$TARBALL"
    cp "/tmp/$TARBALL" /tmp/
elif [[ -f "$TARBALL" ]]; then
    info "Found release tarball in current directory"
else
    error "Release tarball not found. Run scripts/build-release.sh first and copy the .tar.gz to this machine."
fi

# 5. Extract and install
info "Extracting release..."
TMPDIR=$(mktemp -d)
tar xzf "$TARBALL" -C "$TMPDIR"
RELEASE_DIR=$(ls -d "$TMPDIR"/patientspace-*)

info "Installing binaries..."
cp "$RELEASE_DIR/bin/"* "$INSTALL_DIR/bin/"
chmod +x "$INSTALL_DIR/bin/"*

info "Installing frontend..."
cp -r "$RELEASE_DIR/frontend/"* "$INSTALL_DIR/frontend/"

# 6. Create config
info "Writing configuration..."
cat > "$CONFIG_DIR/core.env" << EOF
PATIENTSPACE_FACILITY_ID=$FACILITY_ID
PATIENTSPACE_BIND=0.0.0.0:8080
PATIENTSPACE_DATABASE_URL=sqlite://$DATA_DIR/patientspace.db?mode=rwc
PATIENTSPACE_HUB_URL=$HUB_URL
RUST_LOG=info,patientspace=debug
EOF

chown patientspace:patientspace "$CONFIG_DIR/core.env"
chmod 600 "$CONFIG_DIR/core.env"

# 7. Install systemd unit
info "Installing systemd service..."
cp "$RELEASE_DIR/systemd/patientspace-core.service" /etc/systemd/system/
systemctl daemon-reload
systemctl enable patientspace-core

# 8. Configure firewall (if ufw is active)
if command -v ufw &>/dev/null && ufw status | grep -q "active"; then
    info "Configuring firewall..."
    ufw allow 8080/tcp
fi

# 9. Start the service
info "Starting PatientSpace..."
systemctl start patientspace-core
sleep 3

# 10. Verify
if curl -sf http://localhost:8080/health &>/dev/null; then
    info "PatientSpace is running!"
    echo ""
    echo "  Health check:  curl http://localhost:8080/health"
    echo "  Web UI:         http://$(hostname -I | awk '{print $1}'):8080"
    echo "  Default login:  admin / admin123"
    echo ""
    echo "  Logs:           sudo journalctl -u patientspace-core -f"
    echo "  Stop:           sudo systemctl stop patientspace-core"
    echo "  Restart:        sudo systemctl restart patientspace-core"
else
    warn "PatientSpace didn't respond on port 8080. Check logs:"
    warn "  sudo journalctl -u patientspace-core -n 20"
fi

# Cleanup
rm -rf "$TMPDIR"

echo ""
info "=== Provisioning complete ==="
