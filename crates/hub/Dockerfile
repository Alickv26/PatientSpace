# Multi-stage build for patientspace-hub.
# Stage 1: compile the Rust binary.
# Stage 2: minimal runtime image.

# ---- Stage 1: Builder ----
FROM rust:1.99-slim AS builder

WORKDIR /build

# Install build deps for sqlx-sqlite (needs cc).
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy the workspace.
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY docs/ docs/

# Build the hub binary in release mode.
RUN cargo build --release -p patientspace-hub

# ---- Stage 2: Runtime ----
FROM debian:bookworm-slim

# Install ca-certificates + libsqlite3.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user.
RUN useradd --create-home --uid 1000 patientspace
WORKDIR /home/patientspace

# Copy the binary from the builder stage.
COPY --from=builder /build/target/release/patientspace-hub /usr/local/bin/patientspace-hub

# Create a data directory for the SQLite database.
RUN mkdir -p /data && chown patientspace:patientspace /data
VOLUME /data

# Switch to the non-root user.
USER patientspace

# Expose the PSEP hub port.
EXPOSE 8081

# Set default environment variables.
ENV PATIENTSPACE_HUB_BIND=0.0.0.0:8081
ENV PATIENTSPACE_HUB_DATABASE_URL=sqlite:///data/hub.db?mode=rwc
ENV RUST_LOG=info,patientspace=debug

ENTRYPOINT ["patientspace-hub"]
