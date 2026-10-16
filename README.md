# PatientSpace

Offline-first EHR for primary care, designed for the Zambian public health context. Co-exists with SmartCare (the national EHR) via FHIR/HIE, with a planned migration path. Headline differentiator: real-time clinician analytics at the point of care via embedded DuckDB.

**Pilot site**: Petauke District Hospital, Eastern Province, Zambia.

## Status

Pre-alpha. v0.1 scaffold. See [`direction.md`](./direction.md) for the locked-in product and architecture decisions, and [`docs/sync-protocol.md`](./docs/sync-protocol.md) for the PSEP v0.1 sync protocol specification.

## Stack

- **Backend**: Rust (axum + SQLite + DuckDB + Ed25519/X25519 + AES-GCM).
- **Frontend**: Svelte 5 + SvelteKit + Tailwind v4.
- **Sync protocol**: PSEP — append-only log replication with vector clocks, multi-master, end-to-end encrypted envelopes (CBOR wire format).
- **Data model**: FHIR R5 API over a custom optimized internal store.
- **License**: AGPL-3.0-or-later.

## Repo layout

```
PatientSpace/
├── Cargo.toml                  # Rust workspace
├── crates/
│   ├── core/                   # Facility binary (axum + SQLite + sync agent)
│   ├── hub/                    # Regional/national hub binary
│   ├── fhir/                   # FHIR R5 resource types
│   ├── sync/                   # PSEP envelopes, vector clocks, keys
│   ├── analytics/              # DuckDB dashboard definitions
│   ├── auth/                   # RBAC
│   ├── smartcare-adapter/      # Migration adapters (FHIR / DB / CSV)
│   └── shared/                 # Common types, error handling
├── frontend/                   # Svelte 5 + SvelteKit PWA
├── docs/
│   ├── sync-protocol.md        # PSEP v0.1 spec
│   └── ... (more to come)
├── .github/workflows/ci.yml    # Rust + Svelte CI
├── direction.md                # Locked-in architecture decisions
└── research-smartcare.md       # SmartCare Zambia research brief
```

## Quick start

### Prerequisites
- Rust 1.99+ (`rustup`)
- Bun 1.3+ (or npm/pnpm equivalent)

### Build & run the Rust core (facility binary)
```bash
cargo build --workspace
PATIENTSPACE_FACILITY_ID=F-PETAUKE1 \
PATIENTSPACE_BIND=127.0.0.1:8080 \
cargo run -p patientspace-core
```
Check: `curl http://127.0.0.1:8080/health`

### Build & run the Rust hub (national binary)
```bash
PATIENTSPACE_HUB_BIND=127.0.0.1:8081 cargo run -p patientspace-hub
```
Check: `curl http://127.0.0.1:8081/psep/v1/ping` → `pong`

### Build & run the Svelte frontend
```bash
cd frontend
bun install
bun run dev
```
Open: http://localhost:5173 (Vite proxies `/fhir`, `/health`, `/psep` to the core on 8080).

### Run Rust unit tests
```bash
cargo test --workspace
```

## Documentation

- [`research-smartcare.md`](./research-smartcare.md) — what SmartCare is, how it's used, its limitations, and the landscape of open + closed source alternatives.
- [`direction.md`](./direction.md) — the 10 locked-in product & architecture decisions, including the Svelte-vs-React rationale.
- [`docs/sync-protocol.md`](./docs/sync-protocol.md) — PSEP v0.1 sync protocol: envelopes, vector clocks, sealed-box encryption, conflict resolution, HTTPS + sneakernet transports, and SmartCare migration.
- [`docs/implementation-plan.md`](./docs/implementation-plan.md) — the canonical task list. Agents pick up ⏳ tasks from here and update status as work progresses. Phase 0 is complete; Phase 1 is in progress.

## License

AGPL-3.0-or-later. See [`LICENSE`](./LICENSE).

Copyright (c) 2026 PatientSpace Foundation and contributors.
