# Multi-stage build for patientspace-core.
# Stage 1: compile the Rust binary.
# Stage 2: minimal runtime image with the binary + SQLite.

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

# Build the core binary in release mode.
RUN cargo build --release -p patientspace-core

# ---- Stage 2: Runtime ----
FROM debian:bookworm-slim

# Install ca-certificates (for HTTPS to the hub) + libsqlite3 (for sqlx).
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user.
RUN useradd --create-home --uid 1000 patientspace
WORKDIR /home/patientspace

# Copy the binary from the builder stage.
COPY --from=builder /build/target/release/patientspace-core /usr/local/bin/patientspace-core

# Create a data directory for the SQLite database.
RUN mkdir -p /data && chown patientspace:patientspace /data
VOLUME /data

# Switch to the non-root user.
USER patientspace

# Expose the HTTP API port.
EXPOSE 8080

# Set default environment variables.
ENV PATIENTSPACE_FACILITY_ID=F-DOCKER1
ENV PATIENTSPACE_BIND=0.0.0.0:8080
ENV PATIENTSPACE_DATABASE_URL=sqlite:///data/patientspace.db?mode=rwc
ENV PATIENTSPACE_HUB_URL=http://hub:8081
ENV RUST_LOG=info,patientspace=debug

ENTRYPOINT ["patientspace-core"]
