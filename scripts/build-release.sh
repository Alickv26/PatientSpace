#!/usr/bin/env bash
#
# Build a single distributable release artifact for PatientSpace.
#
# Produces: patientspace-{version}-linux-x86_64.tar.gz containing:
#   - patientspace-core binary (release mode)
#   - patientspace-hub binary (release mode)
#   - frontend static build
#   - default config
#   - systemd unit files
#   - install.sh
#
# Usage: ./scripts/build-release.sh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

VERSION=$(grep "^version" Cargo.toml | head -1 | sed 's/.*= "//;s/"//')
ARCH=$(uname -m)
RELEASE_NAME="patientspace-${VERSION}-linux-${ARCH}"
RELEASE_DIR="/tmp/${RELEASE_NAME}"

echo "=== Building PatientSpace release ${VERSION} ==="

# Clean previous build
rm -rf "$RELEASE_DIR"
mkdir -p "$RELEASE_DIR/bin" "$RELEASE_DIR/frontend" "$RELEASE_DIR/config" "$RELEASE_DIR/systemd"

# 1. Build Rust binaries in release mode
echo "[1/5] Building Rust binaries (release mode)..."
cargo build --release -p patientspace-core -p patientspace-hub
cp target/release/patientspace-core "$RELEASE_DIR/bin/"
cp target/release/patientspace-hub "$RELEASE_DIR/bin/"
echo "  Done. Binary sizes:"
ls -lh "$RELEASE_DIR/bin/"

# 2. Build frontend
echo "[2/5] Building frontend..."
cd frontend
bun install --frozen-lockfile 2>/dev/null || bun install
bun run build
cp -r build/* "$RELEASE_DIR/frontend/"
cd "$ROOT_DIR"
echo "  Done."

# 3. Create config files
echo "[3/5] Creating config..."
cat > "$RELEASE_DIR/config/patientspace-core.env" << 'EOF'
PATIENTSPACE_FACILITY_ID=F-CHANGE-ME
PATIENTSPACE_BIND=0.0.0.0:8080
PATIENTSPACE_DATABASE_URL=sqlite:///var/lib/patientspace/patientspace.db?mode=rwc
PATIENTSPACE_HUB_URL=http://hub.patientspace.org
RUST_LOG=info,patientspace=debug
EOF

cat > "$RELEASE_DIR/config/patientspace-hub.env" << 'EOF'
PATIENTSPACE_HUB_BIND=0.0.0.0:8081
PATIENTSPACE_HUB_DATABASE_URL=sqlite:///var/lib/patientspace/hub.db?mode=rwc
RUST_LOG=info,patientspace=debug
EOF

# 4. Create systemd unit files
echo "[4/5] Creating systemd units..."
cat > "$RELEASE_DIR/systemd/patientspace-core.service" << 'EOF'
[Unit]
Description=PatientSpace Core (facility EHR)
After=network.target

[Service]
Type=simple
User=patientspace
Group=patientspace
EnvironmentFile=/etc/patientspace/core.env
ExecStart=/opt/patientspace/bin/patientspace-core
WorkingDirectory=/var/lib/patientspace
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

cat > "$RELEASE_DIR/systemd/patientspace-hub.service" << 'EOF'
[Unit]
Description=PatientSpace Hub (sync relay)
After=network.target

[Service]
Type=simple
User=patientspace
Group=patientspace
EnvironmentFile=/etc/patientspace/hub.env
ExecStart=/opt/patientspace/bin/patientspace-hub
WorkingDirectory=/var/lib/patientspace
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

# 5. Create install script
echo "[5/5] Creating install script..."
cat > "$RELEASE_DIR/install.sh" << 'INSTALL_EOF'
#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="/opt/patientspace"
DATA_DIR="/var/lib/patientspace"
CONFIG_DIR="/etc/patientspace"

echo "=== PatientSpace Installer ==="

# Create user
if ! id patientspace &>/dev/null; then
    useradd --system --no-create-home --shell /usr/sbin/nologin patientspace
    echo "  Created system user: patientspace"
fi

# Create directories
mkdir -p "$INSTALL_DIR/bin" "$DATA_DIR" "$CONFIG_DIR" /opt/patientspace/frontend

# Copy binaries
cp bin/patientspace-core bin/patientspace-hub "$INSTALL_DIR/bin/"
chmod +x "$INSTALL_DIR/bin/"*

# Copy frontend
cp -r frontend/* "$INSTALL_DIR/frontend/"

# Copy config (don't overwrite if exists)
for f in core.env hub.env; do
    if [[ ! -f "$CONFIG_DIR/$f" ]]; then
        cp "config/patientspace-$f" "$CONFIG_DIR/$f"
    fi
done

# Set ownership
chown -R patientspace:patientspace "$DATA_DIR" "$CONFIG_DIR"

# Install systemd units
cp systemd/*.service /etc/systemd/system/
systemctl daemon-reload

echo ""
echo "=== Installation complete ==="
echo ""
echo "Next steps:"
echo "  1. Edit /etc/patientspace/core.env — set your facility ID"
echo "  2. Start:  sudo systemctl enable --now patientspace-core"
echo "  3. Check:  curl http://localhost:8080/health"
echo "  4. Open:   http://localhost:8080 (frontend served by core)"
echo ""
INSTALL_EOF
chmod +x "$RELEASE_DIR/install.sh"

# Create tarball
echo ""
echo "Creating tarball..."
tar -czf "${RELEASE_NAME}.tar.gz" -C /tmp "$RELEASE_NAME"
rm -rf "$RELEASE_DIR"

echo ""
echo "=== Release built: ${RELEASE_NAME}.tar.gz ==="
ls -lh "${RELEASE_NAME}.tar.gz"
echo ""
echo "Install on a NUC:"
echo "  scp ${RELEASE_NAME}.tar.gz nuc:~/"
echo "  ssh nuc"
echo "  tar xzf ${RELEASE_NAME}.tar.gz"
echo "  cd ${RELEASE_NAME} && sudo ./install.sh"
