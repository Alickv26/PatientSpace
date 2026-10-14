# PatientSpace — Direction & Architecture Decisions

*Locked in: 2026-10-15*  
*Updated: 2026-10-15 — frontend → Svelte 5; pilot facility confirmed as Petauke District Hospital.*

## Product positioning
**PatientSpace** is a **primary care EHR** for clinics and first-level hospitals in Zambia, designed to **co-exist with SmartCare** and integrate via FHIR/HIE rather than replace it outright. Headline differentiator: **real-time clinician analytics at the point of care** — closing the gap that makes SmartCare feel like "MoH's tool, not ours."

## Ten locked-in decisions

| # | Dimension | Decision |
|---|---|---|
| 1 | Scope | Primary care EHR (outpatient + MCH + chronic care) |
| 2 | SmartCare stance | Co-exist + integrate via FHIR; absorb functions module-by-module |
| 3 | Deployment topology | Hybrid: embedded Rust binary at facility (offline-first) + cloud at provincial/national (aggregation + patient portal) |
| 4 | Frontend | Rust backend (axum) + **Svelte 5 / SvelteKit** frontend (was React/TS; see §"Why Svelte" below) |
| 5 | Offline sync | Append-only log replication (event-sourced, immutable events, deterministic reconciliation) |
| 6 | Data model | Hybrid: FHIR R5 API externally, custom optimized internal store |
| 7 | License | AGPL v3, foundation-led governance |
| 8 | Funding | Self-funded/bootstrapped + anchor Zambian MoH contract |
| 9 | Pilot | **Petauke District Hospital** (Eastern Province, ~200 beds, existing SmartCare Pro deployment). Full primary-care EHR, ~3–6 months to go-live. Migration is a first-class feature, not an afterthought — see `docs/sync-protocol.md` §9. |
| 10 | Differentiator | Real-time clinician analytics (embedded DuckDB dashboards at point of care) |

## Resulting architecture sketch

```
┌────────────────────────────────────────────────────────────────┐
│  FACILITY (clinic / first-level hospital)                      │
│                                                                │
│  ┌─────────────────────┐    ┌──────────────────────────────┐   │
│  │ Svelte 5 / SvelteKit│◄──►│ patientspace-core (Rust)     │   │
│  │  PWA (offline-first)│    │  axum HTTP API               │   │
│  │  - Clinician UI     │    │  PSEP sync agent             │   │
│  │  - Service worker   │    │    (see docs/sync-protocol)  │   │
│  │    w/ IndexedDB     │    │  Event log (SQLite + WAL)    │   │
│  │    cache            │    │  Materialized views (SQLite) │   │
│  │  - DuckDB-Wasm for  │    │  DuckDB embedded (analytics) │   │
│  │    client-side      │    │  FHIR R5 facade              │   │
│  │    dashboards       │    │  SmartCare ingest adapters   │   │
│  └─────────────────────┘    └────────────┬─────────────────┘   │
│                                          │                     │
│  Devices: NUC + UPS + biometric scanner (Petauke Dell microtower) │
└──────────────────────────────────────────┼─────────────────────┘
                                           │ TLS + PSEP envelope (CBOR,
                                           │  end-to-end sealed-box)
                                           │ over HTTPS or USB sneakernet
                                           ▼
┌────────────────────────────────────────────────────────────────┐
│  PROVINCIAL / NATIONAL CLOUD                                   │
│                                                                │
│  ┌────────────────────────────────────────────────────────┐    │
│  │ patientspace-hub (Rust, horizontally scalable)         │    │
│  │  - Log ingestion + merge                               │    │
│  │  - Postgres (canonical store)                          │    │
│  │  - DuckDB (national analytics)                         │    │
│  │  - FHIR REST API for HIE                               │    │
│  │  - Patient portal backend                              │    │
│  │  - SmartCare HIE adapter (FHIR ↔ SmartCare XML)        │    │
│  └────────────────────────────────────────────────────────┘    │
│  Integrations: DISA (labs), eLMIS (supply), DHIS2 (reporting)  │
└────────────────────────────────────────────────────────────────┘
```

## Repo layout (proposed)

```
patientspace/
├── Cargo.toml                    # workspace
├── crates/
│   ├── core/                     # facility binary (axum + sqlite + duckdb)
│   ├── hub/                      # provincial/national binary (axum + postgres)
│   ├── fhir/                     # FHIR R5 resource types + serializer
│   ├── sync/                     # append-only log replication
│   ├── analytics/                # DuckDB queries + dashboard definitions
│   ├── auth/                     # RBAC, MFA, biometric matching
│   ├── smartcare-adapter/        # FHIR ↔ SmartCare XML bridge
│   └── shared/                   # common types, error handling
├── frontend/                     # Svelte 5 + SvelteKit PWA (Vite, shadcn-svelte)
├── migrations/                   # SQL migrations for SQLite + Postgres
├── docs/
│   ├── architecture.md           # (TODO)
│   ├── data-model.md             # (TODO)
│   ├── sync-protocol.md          # ✅ PSEP v0.1 — written
│   ├── fhir-conformance.md       # (TODO)
│   ├── smartcare-migration.md    # (TODO; expand on sync-protocol §9)
│   └── deployment.md             # (TODO)
├── scripts/                      # ops, deploy, seed data
└── psep/                         # CDDL schemas + conformance tests for the protocol
```

## Why Svelte 5 (not React)

For PatientSpace's constraints — offline-first, 3G networks in Eastern Province, low-end clinic hardware — Svelte 5 is the better choice:

| Factor | Svelte 5 | React |
|---|---|---|
| Bundle size shipped to browser | ~10–20KB runtime (compiled) | ~45KB+ (React + ReactDOM + scheduler) |
| Runtime perf on old tablets / NUCs | Better (no VDOM, fine-grained via runes) | Worse (VDOM diffing) |
| Onboarding curve for African dev community | Lower | Higher (hooks mental model) |
| FHIR / healthcare ecosystem | Smaller, but most libs (fhirclient.js, @medplum/core, fhir-kit-client) are framework-agnostic JS | Larger (fhir-react, Medplum React, SMART-on-FHIR React helpers) |
| Talent pool for hiring | Smaller but growing | Largest |
| Component library | shadcn-svelte, Flowbite-svelte, Skeleton | shadcn/ui, Mantine, MUI |

**The honest trade-off**: React has more healthcare-specific libraries, but most FHIR tooling is plain JS that works fine in Svelte. We'll build ~30 Svelte FHIR resource renderers ourselves (a week of work). The bundle size and runtime perf win matters more at Petauke-type sites than the ecosystem size.

**Decision**: Svelte 5 + SvelteKit + shadcn-svelte.

## Headline feature: Point-of-care analytics

Every clinician's screen has a "ward round" / "clinic today" dashboard backed by DuckDB running inside the same Rust binary as the EHR. Examples:

- **Clinic today**: queue length, avg wait, suspected TB cases flagged, defaulters on ART walked in today, vaccine due-list.
- **My panel**: hypertensive patients uncontrolled, diabetics overdue for HbA1c, pregnant women due for ANC visit.
- **Trend**: my facility's HIV viral suppression rate over 24 months, side-by-side with district average.

All queries run against the same event log, materialized as Parquet/Arrow. No external BI tool. No "go ask M&E." This is the **single feature** we will demo in every sales pitch.

## What's NOT in v1 (explicitly deferred)

- Inpatient / ward management (defer to SmartCare or v2)
- Full billing / insurance claims (defer to v2)
- Advanced imaging / PACS
- Telemedicine
- AI clinical decision support beyond simple rule-based alerts
- Multi-country localization (Zambia-first; pan-African expansion post-pilot)

## Immediate next steps (recommended order)

1. **Scaffold the workspace** — Cargo workspace + SvelteKit frontend skeleton + CI. (Next concrete task.)
2. ~~Write `docs/sync-protocol.md`~~ ✅ Done — PSEP v0.1 drafted.
3. **Write `docs/architecture.md` and `docs/data-model.md`** — flesh out the crate-level architecture and the custom internal store schema that sits behind the FHIR facade.
4. **Prototype `crates/sync`** — PSEP envelope encode/decode + vector clock + sealed-box encryption + SQLite log. Prove offline + reconciliation on two laptops before scaling.
5. **Prototype `crates/fhir` + a single resource (Patient)** — get one FHIR R5 resource round-tripping through the custom store.
6. **Build `crates/smartcare-adapter` stub** — implement `SmartCareFhirAdapter` against a mock SmartCare HIE endpoint. Used for migration dry-runs.
7. **Build the Petauke minimum workflow** — registration → triage → consultation → prescription → pharmacy dispense. Five Svelte screens, one event log, one DuckDB dashboard.
8. **MoH + Petauke IT engagement** — confirm SmartCare access mode (FHIR vs DB vs CSV) for the migration adapter; negotiate OAuth client for PSEP hub; agree on go-live date.
