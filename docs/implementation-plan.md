# PatientSpace Implementation Plan

**Last updated**: 2026-10-15
**Current phase**: Phase 1 — MVP foundations

This document is the canonical task list for PatientSpace. Every agent working on this repo must read this file before starting work, and update task status here as work progresses.

## Status legend

| Symbol | Meaning |
|---|---|
| ✅ | Done — merged to `main`, acceptance criteria met |
| 🚧 | In Progress — actively being worked on by an agent |
| ⏳ | Next — ready to be picked up; all dependencies are ✅ |
| 🅿️ | Parked — explicitly deferred to a later phase |
| ⛔ | Blocked — waiting on an external dependency |

## How to use this plan

### Picking up work

1. Read [`direction.md`](../direction.md) and [`docs/sync-protocol.md`](./sync-protocol.md) first — they're the source of truth for product and protocol decisions.
2. Find a task marked ⏳ in the current phase whose dependencies are all ✅.
3. Update its status to 🚧 and add your agent name in the Owner column.
4. Create a branch named `p<phase>-t<NN>-<slug>` (e.g. `p1-t03-sync-agent-https`).
5. Read [`/home/z/my-project/worklog.md`](../../worklog.md) to see what previous agents have done.
6. Append your own worklog entry per the rules in the system prompt.

### Finishing work

1. Mark the task ✅ in this file.
2. If your work unblocks other tasks, flip them from ⏳ to ⏳ (no change needed unless they were 🅿️ or ⛔).
3. Open a PR with a description linking back to the task ID.
4. CI must pass before merge.

### When blocked

1. Mark the task ⛔ with a comment explaining the blocker.
2. Add an entry to the Risks register at the bottom of this file.
3. Don't sit on a ⛔ task — surface it in the next standup.

---

## Phase 0: Foundation ✅

**Goal**: establish the research, direction, protocol spec, and a compiling workspace.

All Phase 0 work is committed to `main`. No further work in this phase.

| ID | Task | Status | Owner | Acceptance criteria |
|---|---|---|---|---|
| P0-T01 | SmartCare Zambia research brief | ✅ | — | `research-smartcare.md` covers what SmartCare is, how it's used, limitations, and alternatives (OSS + commercial) |
| P0-T02 | Lock product + architecture direction | ✅ | — | `direction.md` has 10 locked decisions, including Svelte 5, Petauke pilot, AGPL v3 |
| P0-T03 | PSEP v0.1 sync protocol specification | ✅ | — | `docs/sync-protocol.md` covers envelope, vector clocks, sealed-box encryption, conflict resolution, HTTPS + sneakernet, SmartCare migration |
| P0-T04 | Cargo workspace scaffold (8 crates) | ✅ | — | `cargo build --workspace` clean; 16 unit tests pass; `core` and `hub` binaries boot and serve `/health` |
| P0-T05 | Svelte 5 frontend scaffold | ✅ | — | `bun run build` clean; 5 routes (`/`, `/patients`, `/patients/new`, `/encounters`, `/dashboards`); client bundle < 200KB |
| P0-T06 | CI workflow | ✅ | — | `.github/workflows/ci.yml` runs Rust build+test+clippy+fmt and Svelte build+check on every push/PR |
| P0-T07 | Repo meta (LICENSE, README, .env.example, .gitignore, .editorconfig) | ✅ | — | All present and consistent |

---

## Phase 1: MVP foundations 🚧

**Goal**: prove the protocol works end-to-end and the basic clinical workflow is usable on a single facility.

**Exit criteria** (all must be ✅):
- Two `patientspace-core` instances on separate laptops can sync Patient records over HTTPS, including during simulated 3G partitions.
- A clinician can register a patient, retrieve them, and view their detail screen.
- A mock SmartCare HIE can be ingested as genesis versions through the migration CLI.
- `docker compose up` brings up a working 1-facility + 1-hub dev environment.

### Epic P1-E1: PSEP sync end-to-end

| ID | Task | Status | Owner | Deps | Effort | Acceptance criteria |
|---|---|---|---|---|---|---|
| P1-T01 | SQLite event log storage layer | ✅ | PatientSpace Dev | — | 3d | New crate `crates/sync/src/log.rs`. SQLite + WAL. Tables: `envelopes(facility_id, version_id, envelope_blob, ingested_at)`, `vector_clocks(resource_id, facility_id, counter)`, `causal_edges(version_id, parent_version_id)`. Append-only — no UPDATE, no DELETE. Migration via sqlx. Unit tests cover insert + query by resource_id. |
| P1-T02 | Envelope encode/decode round-trip over SQLite | ✅ | Alickv26 | P1-T01 | 2d | Round-trip a sealed envelope through the SQLite log and verify the bytes match. Test: write 1000 random envelopes, read them all back, verify each `to_cbor()` matches what was stored. |
| P1-T03 | Sync agent: handshake + push (facility → hub) | ✅ | Alickv26 | P1-T02 | 4d | New module `crates/sync/src/agent.rs`. Tokio task. Performs `POST /psep/v1/handshake`, then `POST /psep/v1/push` with CBOR-seq body. Idempotent. Retries with additive backoff: 5s → 10s → 30s → 60s → 5min cap. Logs structured JSON to `~/.patientspace/logs/sync.jsonl`. |
| P1-T04 | Sync agent: pull (hub → facility) | ✅ | Alickv26 | P1-T03 | 3d | `POST /psep/v1/pull` + `POST /psep/v1/pull-ack`. Resumable from last acked `version_id`. Hub stub in `crates/hub` upgraded to actually serve from its own SQLite log. |
| P1-T05 | Hub: real handshake + push + pull endpoints | ✅ | Alickv26 | P1-T01 | 4d | `crates/hub` gets SQLite + the three endpoints per `docs/sync-protocol.md` §7. Signature verification on push. Idempotent dedup on `(version_id)`. Pull streams in causal-DAG order. |
| P1-T06 | Two-instance integration test | ✅ | Alickv26 | P1-T04, P1-T05 | 3d | `crates/sync/tests/two_instance.rs`. Spin up two `core` binaries + one `hub` on localhost. Create a Patient at Facility A, sync, verify Facility B can decrypt it. Simulate partition (kill hub), create at A, restore hub, sync, verify B receives. |
| P1-T07 | Conflict detection integration test | ✅ | Alickv26 | P1-T06 | 2d | Both facilities edit the same Patient concurrently (vector clocks diverge). Sync. Both facilities flag the conflict. Merge-policy.toml applied; auto-mergeable fields merged, `clinical_review` fields surface as conflict extension. |
| P1-T08 | Sneakernet export/import | ✅ | Alickv26 | P1-T02 | 3d | `patientspace export --to /media/usb` writes a `.psep` CBOR-seq file with header + payload + trailer per §8. `patientspace import --from /media/usb` ingests. Round-trip test with bit-flip injection verifies signature rejection. |

### Epic P1-E2: FHIR Patient persistence in `core`

| ID | Task | Status | Owner | Deps | Effort | Acceptance criteria |
|---|---|---|---|---|---|---|
| P1-T09 | Custom internal store schema | ✅ | Alickv26 | — | 3d | `docs/data-model.md` written. SQLite tables for `resources`, `versions`, `current_version_per_resource`. Decoupled from FHIR JSON shape — FHIR is the API, not the storage. Review with the team before implementing. |
| P1-T10 | `POST /fhir/Patient` end-to-end | ✅ | Alickv26 | P1-T09 | 3d | Svelte sends a Patient JSON → axum handler validates → wraps in PSEP `EnvelopeBuilder` → seals with author facility key → writes to SQLite log → returns 201 with the resource. Acceptance: `curl -X POST` works, the envelope is queryable in the log. |
| P1-T11 | `GET /fhir/Patient/{id}` end-to-end | ✅ | Alickv26 | P1-T10 | 2d | Reads the latest version for a resource from the log, decrypts, returns the FHIR JSON. 404 if not found. Handles conflict state by returning the merged shell + a `conflict` extension. |
| P1-T12 | `GET /fhir/Patient` (Bundle, list) | ✅ | Alickv26 | P1-T11 | 2d | Returns a FHIR Bundle of all Patients at this facility. Supports `_count` (default 50, max 200) and `_offset` for pagination. |
| P1-T13 | Patient search by Smartcard / NRC | ✅ | Alickv26 | P1-T12 | 1d | `GET /fhir/Patient?identifier=SC-12345678` works. Index on the `identifier` array. |

### Epic P1-E3: SmartCare migration adapter (mock)

| ID | Task | Status | Owner | Deps | Effort | Acceptance criteria |
|---|---|---|---|---|---|---|
| P1-T14 | Mock SmartCare HIE server | ✅ | Alickv26 | — | 2d | `crates/smartcare-adapter/mock-server/` (separate binary). Serves synthetic FHIR R4 Patient/Encounter/Observation resources at `http://127.0.0.1:9090/fhir`. Seed with 100 patients, 500 encounters, 1000 observations. |
| P1-T15 | `SmartCareIngestAdapter` fleshed out | ✅ | Alickv26 | P1-T14 | 2d | Trait extended with `iter_encounters`, `iter_observations`. Concrete `SmartCareFhirAdapter` that reads from the mock server. |
| P1-T16 | `patientspace migrate` CLI | ✅ | Alickv26 | P1-T15, P1-T10 | 4d | `patientspace migrate --source fhir --config smartcare.yml --dry-run` produces a count report. Without `--dry-run`, ingests every SmartCare resource as a genesis version (vector_clock `{F-XXXX: 1}`, parent_version_ids `[]`, kind `clinical`, with a `provenance: migrated-from-smartcare` extension). |
| P1-T17 | Migration validation report | ✅ | Alickv26 | P1-T16 | 2d | `patientspace migrate --validate` cross-checks counts (SmartCare says N patients, PatientSpace has N patients). Detects duplicates via Smartcard + NRC + biometric dedup. Outputs JSON report. |

### Epic P1-E4: Petauke minimum clinical workflow (frontend)

| ID | Task | Status | Owner | Deps | Effort | Acceptance criteria |
|---|---|---|---|---|---|---|
| P1-T18 | Patient registration screen → `POST /Patient` | ✅ | Alickv26 | P1-T10 | 2d | `/patients/new` form (Smartcard, NRC, name, phone, DOB, gender) → POST to `/fhir/Patient` → redirect to `/patients/{id}` on success. Validation errors shown inline. |
| P1-T19 | Patient list screen → `GET /Patient` | ✅ | Alickv26 | P1-T12 | 1d | `/patients` shows a table: Smartcard, NRC, Name, Gender, DOB. Pagination. Click row → `/patients/{id}`. |
| P1-T20 | Patient detail screen → `GET /Patient/{id}` | ✅ | Alickv26 | P1-T11 | 2d | `/patients/{id}` shows demographics, identifiers, contact, address. Tabs for Encounters / Observations / Medications (empty in v0.1). Conflict banner if `conflict` extension present. |
| P1-T21 | Patient search box | ✅ | Alickv26 | P1-T13, P1-T19 | 1d | Top of `/patients` page: search by Smartcard or NRC. Debounced 300ms. |
| P1-T22 | Triage encounter screen (stub) | ✅ | Alickv26 | — | 2d | `/encounters/triage?patient={id}` form: vitals (BP, HR, temp, weight), presenting complaint, triage category. POST stubbed — saves to local state only for now. |
| P1-T23 | Consultation encounter screen (stub) | ✅ | Alickv26 | P1-T22 | 2d | `/encounters/consultation?patient={id}&encounter={id}` form: diagnosis (ICD-10 autocomplete), notes, plan. POST stubbed. |
| P1-T24 | Prescription screen (stub) | ✅ | Alickv26 | P1-T23 | 2d | `/encounters/prescribe?encounter={id}` form: medication (ATC autocomplete), dose, frequency, duration. POST stubbed. |
| P1-T25 | Pharmacy dispense screen (stub) | ✅ | Alickv26 | P1-T24 | 1d | `/pharmacy/dispense?request={id}` form: confirm dispense, quantity, batch. POST stubbed. |

### Epic P1-E5: DevOps

| ID | Task | Status | Owner | Deps | Effort | Acceptance criteria |
|---|---|---|---|---|---|---|
| P1-T26 | Dockerfile for `patientspace-core` | ✅ | Alickv26 | P1-T04 | 1d | Multi-stage Rust build, distroless final image, < 50MB. Runs as non-root. `EXPOSE 8080`. |
| P1-T27 | Dockerfile for `patientspace-hub` | ✅ | Alickv26 | P1-T05 | 1d | Same pattern as core. `EXPOSE 8081`. |
| P1-T28 | `docker-compose.yml` for local dev | ✅ | Alickv26 | P1-T26, P1-T27 | 1d | Brings up 1 hub + 1 core + the Svelte frontend on a single network. Volumes for SQLite. `docker compose up` → working dev environment in < 60s. |
| P1-T29 | Seed script with synthetic data | ✅ | Alickv26 | P1-T16 | 2d | `scripts/seed.ts` (or `.ts`) generates 50 synthetic patients, 200 encounters, 400 observations via the migration CLI. Idempotent. Useful for demos. |
| P1-T30 | Release artifact: single static binary | 🅿️ | — | — | 2d | `cargo build --release --target x86_64-unknown-linux-musl` produces a single static binary for `patientspace-core` < 30MB. Deferred to Phase 2 — not needed for dev. |

---

## Phase 2: Pilot readiness ⏳

**Goal**: Petauke can run on PatientSpace for routine primary care, with SmartCare kept read-only as a 30-day fallback.

**Entry criteria**: Phase 1 fully ✅.
**Exit criteria**:
- Auth works: clinicians log in, sign actions with delegated keys.
- Real SmartCare data can be migrated end-to-end (mock is replaced with the real Petauke HIE).
- Conflict resolution UI works.
- DuckDB dashboards render at the point of care, < 500ms query latency on a 5-year dataset.
- Encounter / Observation / MedicationRequest / MedicationDispense / Condition FHIR resources all round-trip.
- Single-binary installer runs on a fresh NUC.

### Epic P2-E1: Auth & identity

| ID | Task | Status | Owner | Deps | Effort | Acceptance criteria |
|---|---|---|---|---|---|---|
| P2-T01 | Clinician login (local password) | ⏳ | — | — | 3d | `POST /api/auth/login` with username + password (argon2 hash). Sets HttpOnly session cookie. Session in SQLite. Logout works. |
| P2-T02 | Delegated clinician signing keys | ⏳ | — | P2-T01 | 4d | On login, facility issues the clinician an Ed25519 delegated keypair (signed by facility's key). Stored in session. Every authored envelope's `author_clinician_id` references this key. Revocation via `system/clinician-revocation` event. |
| P2-T03 | Biometric scanner integration | ⏳ | — | P2-T01 | 5d | Fingerprint scanner (DigitalPersona U.are.U 4500) on the registration desk. SourceAFIS-Rust embedded. Patient lookup by fingerprint. Per §9.3 of sync-protocol.md. |
| P2-T04 | Session management + idle timeout | ⏳ | — | P2-T01 | 2d | 15-min idle timeout. Re-auth on sensitive operations (delete, key rotation). |

### Epic P2-E2: SmartCare migration (real)

| ID | Task | Status | Owner | Deps | Effort | Acceptance criteria |
|---|---|---|---|---|---|---|
| P2-T05 | Confirm SmartCare access mode with Petauke IT | ⛔ | — | — | 1w | External dependency. Decide between FHIR/HIE, direct DB, or CSV export. Triggers adapter choice in P2-T06. |
| P2-T06 | Real `SmartCareFhirAdapter` or `SmartCareDbAdapter` | ⏳ | — | P2-T05 | 1w | Implement whichever adapter P2-T05 selects. Tested against real Petauke data (read-only copy in dev). |
| P2-T07 | Identity crosswalk builder | ⏳ | — | P2-T06 | 4d | For each SmartCare patient, generate PatientSpace `resource_id`. Persist `(SmartCare UUID, Smartcard No., NRC) → PatientSpace resource_id` in `identity_crosswalk` table. This table is itself a synced resource. |
| P2-T08 | Bulk ingest pipeline | ⏳ | — | P2-T07 | 5d | Streams genesis versions through PSEP. Throttled to avoid saturating 3G uplink. Resume on interruption. ~50K patients × 5yr history = ~2M events expected for Petauke. |
| P2-T09 | Validation report (real) | ⏳ | — | P2-T08 | 2d | Cross-check counts. Detect duplicates via biometric dedup. Output JSON + human-readable PDF. Reviewed and signed off by Petauke IT before cutover. |
| P2-T10 | Rollback / re-export tooling | ⏳ | — | P2-T08 | 3d | `patientspace export --to smartcare-csv` re-exports PatientSpace records back to SmartCare's CSV/XML format. Same adapter, reversed. Tested by round-tripping a sample. |

### Epic P2-E3: Conflict resolution UI

| ID | Task | Status | Owner | Deps | Effort | Acceptance criteria |
|---|---|---|---|---|---|---|
| P2-T11 | Conflict review queue in Svelte | ⏳ | — | P1-T20 | 3d | `/conflicts` page lists all `clinical_review` conflicts at this facility. Each item shows patient name, conflicting fields, both authors, timestamps. |
| P2-T12 | 3-way merge display | ⏳ | — | P2-T11 | 3d | Detail view shows common ancestor + both concurrent versions side-by-side. Diff highlighting per field. |
| P2-T13 | Clinician resolution flow | ⏳ | — | P2-T12 | 4d | Clinician picks one value (or types a new one) → POST creates a new version with 2 parents (merge version). Conflict cleared. All actions audited. |

### Epic P2-E4: DuckDB analytics

| ID | Task | Status | Owner | Deps | Effort | Acceptance criteria |
|---|---|---|---|---|---|---|
| P2-T14 | DuckDB wiring in `core` | ⏳ | — | P1-T09 | 4d | Add `duckdb` back to `crates/core/Cargo.toml`. Materialize the event log into a DuckDB table on every sync. Query API: `POST /api/dashboards/{id}/run` accepts parameters, returns Arrow JSON. |
| P2-T15 | Dashboard query execution endpoint | ⏳ | — | P2-T14 | 2d | Wired to the 3 built-in dashboards in `crates/analytics`. Validates SQL against SQL injection (parameterized queries only). |
| P2-T16 | Frontend dashboard rendering | ⏳ | — | P2-T15 | 4d | `/dashboards/{id}` page. Use Chart.js (lightweight) or Apache ECharts (heavier). KPI tiles + table + chart components. |
| P2-T17 | Real-time refresh | ⏳ | — | P2-T16 | 2d | Dashboards auto-refresh every 60s when the tab is visible. Manual refresh button. |

### Epic P2-E5: Encounter + clinical resources

| ID | Task | Status | Owner | Deps | Effort | Acceptance criteria |
|---|---|---|---|---|---|---|
| P2-T18 | `Encounter` resource + persistence | ⏳ | — | P1-T10 | 4d | FHIR R5 Encounter modeled. `POST /fhir/Encounter`, `GET /fhir/Encounter/{id}`, `GET /fhir/Encounter?patient={id}`. Triage / consultation / pharmacy dispense are Encounter serviceTypes. |
| P2-T19 | `Observation` resource | ⏳ | — | P2-T18 | 3d | Vitals, lab results. LOINC codes. `GET /fhir/Observation?patient={id}&code={loinc}`. |
| P2-T20 | `MedicationRequest` resource | ⏳ | — | P2-T18 | 3d | WHO ATC codes. Prescription screen wired up. |
| P2-T21 | `MedicationDispense` resource | ⏳ | — | P2-T20 | 2d | Pharmacy dispense screen wired up. Stock deduction integration stubbed. |
| P2-T22 | `Condition` resource | ⏳ | — | P2-T18 | 2d | ICD-10 coded diagnoses. Encounter.diagnosis references these. |
| P2-T23 | Wire frontend stub screens to real endpoints | ⏳ | — | P2-T18–P2-T22 | 4d | Replace the local-state stubs from P1-T22 through P1-T25 with real `POST` calls. |

### Epic P2-E6: Field-deployment readiness

| ID | Task | Status | Owner | Deps | Effort | Acceptance criteria |
|---|---|---|---|---|---|---|
| P2-T24 | Single-binary release artifact | ⏳ | — | P1-T30 | 2d | `patientspace-core` musl static binary < 30MB. Bundled frontend assets. Ships as a single tarball + a systemd unit file. |
| P2-T25 | NUC provisioning script | ⏳ | — | P2-T24 | 3d | `scripts/provision-nuc.sh` — takes a fresh Ubuntu 24.04 NUC from PXE boot to running PatientSpace. Generates facility keypair on first boot. Pins hub public keys. |
| P2-T26 | Backup + restore | ⏳ | — | P1-T01 | 3d | Nightly SQLite backup to local USB + cloud (encrypted). `patientspace restore --from backup.db` works. Tested by restoring a backup to a fresh NUC. |
| P2-T27 | Offline installer | ⏳ | — | P2-T24 | 2d | `patientspace-install.tar.gz` contains the binary, systemd unit, default config, and the hub's public keys. No internet required at install time. |

---

## Phase 3: Petauke pilot go-live ⏳

**Goal**: Petauke District Hospital runs PatientSpace for primary care. SmartCare is kept read-only for 30 days as fallback, then decommissioned.

**Entry criteria**: Phase 2 fully ✅, MoH contract signed.
**Exit criteria**:
- 30 days of routine primary care on PatientSpace at Petauke.
- Zero data loss events.
- SmartCare decommissioned.
- Lessons-learned report published.

### Epic P3-E1: Pre-go-live

| ID | Task | Status | Owner | Deps | Effort | Acceptance criteria |
|---|---|---|---|---|---|---|
| P3-T01 | Site readiness assessment | ⏳ | — | — | 1w | Petauke IT confirms: power (UPS), 3G/cellular signal, NUC placement, biometric scanner desk, clinician training room. |
| P3-T02 | Hardware procurement + shipping | ⏳ | — | P3-T01 | 3w | NUC (Dell OptiPlex Micro or equivalent), UPS (1000VA), biometric scanner × 2 (registration + pharmacy), USB sticks × 10 for sneakernet, 4G router with backup SIM. |
| P3-T03 | Network install at Petauke | ⏳ | — | P3-T02 | 1w | 3G/4G router installed and tested. Local WiFi for clinicians. NUC + scanner wired to LAN. |
| P3-T04 | MoH SLE / contract signed | ⛔ | — | — | 4w | External dependency. Service-level expectations, data ownership, incident response, support hours. |
| P3-T05 | Data Protection Impact Assessment | ⏳ | — | — | 2w | DPIA per Zambia Data Protection Act 2021. Reviewed by MoH legal. |

### Epic P3-E2: Migration week

| ID | Task | Status | Owner | Deps | Effort | Acceptance criteria |
|---|---|---|---|---|---|---|
| P3-T06 | SmartCare data export | ⏳ | — | P2-T05 | 1d | Petauke IT produces the SmartCare export (FHIR / DB dump / CSV per access mode). |
| P3-T07 | PatientSpace migration dry-run | ⏳ | — | P2-T08, P3-T06 | 2d | Run `patientspace migrate --dry-run` against the real export. Review validation report with Petauke IT. Resolve duplicates. |
| P3-T08 | Validation meeting | ⏳ | — | P3-T07 | 1d | Petauke clinical lead + IT + PatientSpace team review the dry-run report. Sign-off to proceed. |
| P3-T09 | Cutover | ⏳ | — | P3-T08 | 1d | Friday evening. Run real migration. Saturday morning clinicians see PatientSpace at triage. SmartCare enters read-only mode. |
| P3-T10 | SmartCare → read-only | ⏳ | — | P3-T09 | 1d | SmartCare accounts converted to read-only. Signage at clinic: "PatientSpace is the system of record." |

### Epic P3-E3: Training

| ID | Task | Status | Owner | Deps | Effort | Acceptance criteria |
|---|---|---|---|---|---|---|
| P3-T11 | Clinician training materials | ⏳ | — | P2-T23 | 2w | Quick-reference cards (laminated) for: registration, triage, consultation, prescribe, dispense, dashboards. Video walkthroughs (5min each) for the same. Chewa + English. |
| P3-T12 | Train-the-trainer | ⏳ | — | P3-T11 | 1w | 3 Petauke clinicians trained as super-users. They run in-clinic training for the rest. |
| P3-T13 | On-site support week 1 | ⏳ | — | P3-T09 | 1w | PatientSpace engineer on-site at Petauke Mon-Fri 7am-7pm. Daily debrief. |
| P3-T14 | On-site support week 2 | ⏳ | — | P3-T13 | 1w | Reduced hours (8am-5pm). Daily check-in via TUSO-style helpdesk. |

### Epic P3-E4: 30-day stabilization

| ID | Task | Status | Owner | Deps | Effort | Acceptance criteria |
|---|---|---|---|---|---|---|
| P3-T15 | Daily standup with Petauke IT | ⏳ | — | P3-T09 | 30d | 15min video call every weekday. Issues logged in TUSO-style helpdesk. |
| P3-T16 | Bug triage | ⏳ | — | P3-T15 | 30d | Bugs classified P0/P1/P2/P3. P0 = data loss or care blocked → fix within 4h. P1 = workaround exists → 1 week. P2 = 1 month. P3 = backlog. |
| P3-T17 | Performance monitoring | ⏳ | — | P3-T09 | 30d | Sync latency, dashboard query latency, page load times tracked. Alert if any exceed baseline by 2x. |
| P3-T18 | SmartCare decommission | ⏳ | — | P3-T16 | 1d | Day 30. SmartCare DB backed up to encrypted archive. Application uninstalled. PatientSpace is the sole system of record. |
| P3-T19 | Lessons-learned report | ⏳ | — | P3-T18 | 1w | Published openly. Feeds into Phase 4 planning. |

---

## Phase 4: Post-pilot 🅿️

Things we know we'll need but aren't planning in detail yet. Re-prioritize after the Petauke pilot completes (P3-T19).

| ID | Task | Status | Notes |
|---|---|---|---|
| P4-T01 | Inpatient / ward management | 🅿️ | Defer to SmartCare or build in v2. Decision after pilot. |
| P4-T02 | Full billing / insurance claims | 🅿️ | Not needed for public primary care; revisit if private clinics adopt. |
| P4-T03 | PACS / advanced imaging | 🅿️ | Out of scope for primary care. |
| P4-T04 | Telemedicine | 🅿️ | Likely a separate companion app, not core EHR. |
| P4-T05 | AI clinical decision support | 🅿️ | Beyond rule-based alerts. Watch OSS LLM space. |
| P4-T06 | Multi-country localization | 🅿️ | Zambia-first; expansion to Malawi / Mozambique / Tanzania post-pilot. |
| P4-T07 | Hub federation (DR) | 🅿️ | Lusaka + Ndola hubs sync as peers. Per sync-protocol §15.5. |
| P4-T08 | Patient portal / PHR | 🅿️ | Patient-issued X25519 keys on smartcards. Per sync-protocol §15.3. |
| P4-T09 | Post-quantum crypto | 🅿️ | Hybridize X25519 with ML-KEM around 2027. Per sync-protocol §15.4. |
| P4-T10 | Selective disclosure | 🅿️ | Redacted patient views for referring clinicians. Per sync-protocol §15.6. |
| P4-T11 | Binary resources (radiology, signed PDFs) | 🅿️ | Chunked transfer protocol. Per sync-protocol §15.1. |
| P4-T12 | Cross-federation sync | 🅿️ | If Zimbabwe deploys PatientSpace. Per sync-protocol §15.2. |

---

## Cross-cutting concerns

These don't fit a single phase — they apply throughout.

| ID | Concern | Cadence | Notes |
|---|---|---|---|
| CC-01 | Security review | Every release | External reviewer pre-pilot. Pen-test pre-go-live. |
| CC-02 | FHIR conformance testing | Every release | Run against HL7 FHIR validator. Patient, Encounter, Observation, MedicationRequest, MedicationDispense, Condition. |
| CC-03 | Documentation updates | Per epic | Keep `docs/` in sync with code. Each epic closes with a docs PR. |
| CC-04 | Performance benchmarks | Every release | Sync throughput, dashboard latency, page load. Compared to baseline. |
| CC-05 | Chaos testing | Pre-pilot | 7-day chaos test: 10 simulated facilities, random partitions, random restarts, one facility lost. Per sync-protocol §14.3. |
| CC-06 | Backup restore drill | Quarterly | Restore from backup to fresh hardware. Time-to-restore tracked. |

---

## Risks register

Add a row here whenever a task flips to ⛔ or whenever a new risk is identified. Review at every standup.

| ID | Risk | Likelihood | Impact | Mitigation | Owner | Status |
|---|---|---|---|---|---|---|
| R-01 | Petauke IT cannot give us SmartCare DB or FHIR access; only CSV export | Medium | High (forces P1-T15 + P2-T06 down the CSV path; harder dedup) | Start MoH conversation early; build CSV adapter in parallel; push for at least FHIR | — | Open |
| R-02 | DuckDB bundled C++ build continues to time out in CI | Medium | Medium (delays P2-T14) | Use DuckDB pre-built binary in CI; or fall back to in-process SQLite for analytics in v0.1 | — | Open |
| R-03 | Petauke 3G uplink insufficient for initial bulk migration (~4GB over the wire) | High | Medium (cutover delayed) | Sneakernet the initial migration via USB; only delta-sync over 3G afterward | — | Open |
| R-04 | Clinician resistance to switching from SmartCare | Medium | High (pilot fails) | Involve Petauke clinical lead from day 1; training materials co-designed with clinicians; super-user program | — | Open |
| R-05 | MoH contract negotiations drag past Phase 2 completion | Medium | High (pilot slips) | Run MoH engagement in parallel with engineering from Phase 1; don't wait for Phase 2 to start | — | Open |
| R-06 | Biometric scanner hardware availability in Zambia | Low | Medium (P2-T03 slips) | Source 2+ scanners from South Africa as backup; plan for fingerprint-only-initial fallback | — | Open |
| R-07 | Loss of facility private key = loss of all historical data access | Low | Critical | Mandatory encrypted backup of keystore to USB + cloud on first boot; documented key-recovery procedure using peer-facility quorum (per sync-protocol §14.3) | — | Open |

---

## Open questions

Decisions that need to be made before relevant tasks can start. Each question has an owner and a deadline.

| ID | Question | Owner | Deadline | Decision | Decided on |
|---|---|---|---|---|---|
| Q-01 | Should we use shadcn-svelte or build components from scratch? | — | Before P1-T18 | TBD | — |
| Q-02 | Which chart library for dashboards (Chart.js vs ECharts vs uPlot)? | — | Before P2-T16 | TBD | — |
| Q-03 | Do we ship the frontend as a separate SvelteKit adapter-node process, or have axum serve the static build? | — | Before P2-T24 | TBD | — |
| Q-04 | For the biometric scanner, do we standardize on DigitalPersona or SecuGen? | — | Before P2-T03 | TBD | — |
| Q-05 | Where is the hub hosted? (Cloudflare Workers, Hetzner, AWS, self-hosted at Smart Zambia?) | — | Before P3-T01 | TBD | — |
| Q-06 | Does Petauke need Chewa-language UI, or is English acceptable for v0.1? | — | Before P3-T11 | TBD | — |
| Q-07 | What is the formal medico-legal retention period for medical records in Zambia? (We assumed forever in P0-T03.) | — | Before P3-T05 | TBD | — |
| Q-08 | Do we need to support ZAMACE (Zambia Medical Association) reporting outputs from v0.1? | — | Before P3-T09 | TBD | — |

---

## Glossary

Quick reference for terms used throughout this plan. Cross-reference with `docs/sync-protocol.md` Appendix B.

| Term | Meaning |
|---|---|
| **PSEP** | PatientSpace Sync Protocol — the wire protocol defined in `docs/sync-protocol.md` |
| **Envelope** | One immutable, signed, encrypted resource version on the wire |
| **Vector clock** | Per-failure monotonic counter map used to detect causality and concurrency |
| **Genesis version** | A version with 0 parents — the first version of a resource |
| **Merge version** | A version with 2 parents — produced by conflict resolution |
| **Facility** | A single physical site running one `patientspace-core` binary (e.g., Petauke District Hospital) |
| **Hub** | The regional/national server that relays envelopes between facilities |
| **Sneakernet** | Carrying data on a USB stick between sites when the internet is down |
| **Smartcard** | The physical ID card Zambian patients carry; its number is a Patient identifier |
| **NRC** | National Registration Card — Zambian national ID |
| **IP** | Implementing Partner — PEPFAR/CDC-funded org that supports MoH facilities (e.g., CIDRZ, CHAI) |
| **MoH** | Ministry of Health (Zambia) |
| **DISA** | District Health Information System (lab) — SmartCare's lab integration partner |
| **eLMIS** | Electronic Logistics Management Information System — Zambia's supply chain system |
| **DHIS2** | District Health Information System 2 — aggregate reporting used across 80+ countries |
| **PMTCT** | Prevention of Mother-to-Child Transmission (of HIV) |
| **ART** | Antiretroviral Therapy |
| **TUSO** | SmartCare's helpdesk / IT service management brand. PatientSpace will have an equivalent. |
| **DPIA** | Data Protection Impact Assessment |
| **SLE** | Service-Level Expectation (contract) |

---

## Change log

| Date | Change | Author |
|---|---|---|
| 2026-10-15 | Initial plan created. Phase 0 fully ✅. Phase 1 task list defined (30 tasks across 5 epics). | PatientSpace Dev |
| 2026-10-15 | P1-T01 (SQLite event log) ✅. Added `crates/sync/src/log.rs` + `crates/sync/migrations/20261015000001_init.sql`. 14 new unit tests (26 total in sync crate, 30 across workspace). | PatientSpace Dev |
