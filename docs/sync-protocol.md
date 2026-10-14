# PatientSpace Sync Protocol (PSSP) — v0.1

**Status:** Draft  
**Owner:** PatientSpace Foundation  
**Last updated:** 2026-10-15

---

## 1. Goals & non-goals

### Goals
1. **True multi-master**: any facility can create or edit any patient record at any time, online or offline. The hub is *not* authoritative.
2. **Offline-first**: a facility operates indefinitely without internet. Sync runs whenever connectivity returns — minutes, hours, or days later.
3. **Transport-agnostic**: the same event envelope can travel over HTTPS, MQTT, or a USB stick carried by a clinician on a motorbike to the district office.
4. **End-to-end encrypted**: the hub cannot read patient payloads in transit or at rest; only facilities (and clinicians they delegate to) can decrypt.
5. **Forever-auditable**: every change is an immutable event kept for the legal retention period (forever, per the locked decision). Compaction is a query-side optimization only.
6. **Migration-friendly**: a facility moving from SmartCare can ingest its historical data through the same protocol endpoint as live edits, so the migration is "just another sync."
7. **Pitch-friendly**: from a facility operator's POV, migration is one button — *"Connect to SmartCare → Import → Go live."*

### Non-goals (v0.1)
- Real-time collaboration (no live cursors, no concurrent-edit notifications).
- Cross-patient transactions (one encounter touching 50 resources is sent as 50 events, not one atomic bundle).
- Cross-federation sync (syncing between two independent PatientSpace deployments).
- Streaming video / large binary blobs (radiology images) — those go through a separate object sync, out of scope here.

---

## 2. Core concepts

### 2.1 Facility
A single physical site (e.g., Petauke District Hospital) running one `patientspace-core` binary. Each facility has:

- **`facility_id`**: `F-XXXXXXXX` (8 random base32 chars, generated at first boot, never reused).
- **`facility_keypair`**: Ed25519 signing key + X25519 encryption keypair, generated at first boot, stored in OS keychain / TPM when available, else in an encrypted keystore on disk.
- **`hub_public_keys`**: the hub's Ed25519 verification key + X25519 public key, pinned at install time. Rotated only via signed rotation events.

### 2.2 Hub
A regional or national PatientSpace hub. Receives events from all facilities, stores them immutably, replays them to other facilities. The hub **cannot decrypt event payloads** — it only sees envelope metadata.

### 2.3 Resource
A FHIR R5 resource (Patient, Encounter, Observation, etc.). Each resource has:

- **`resource_id`**: a PatientSpace UUID (`U-UUIDv7`), globally unique, assigned at first creation. Never reused, never reassigned.
- **`resource_type`**: FHIR resource type (e.g., `Patient`, `Observation`).
- **`kind`**: one of `clinical`, `identity`, `audit`, `system`. Affects merge rules and retention.
- A **versioned history**: an ordered chain of resource versions (§2.4).

### 2.4 Resource version
A single immutable snapshot of a resource at a point in time. Each version carries:

- **`version_id`**: `V-UUIDv7`.
- **`resource_id`**: parent resource.
- **`author_facility_id`**: which facility created this version.
- **`author_clinician_id`**: which clinician (delegated key) created this version.
- **`vector_clock`**: a Lamport-style vector clock over facility IDs (§3).
- **`parent_version_ids`**: 0, 1, or 2 parent version IDs (§3.3 — merge versions have 2 parents).
- **`payload_ciphertext`**: the encrypted FHIR resource blob (§5).
- **`payload_hash`**: SHA-256 of the plaintext payload, for integrity verification post-decrypt.
- **`created_at`**: author's wall clock at creation (best-effort; causality is via vector clock, not this timestamp).
- **`kind`**: inherits from resource.
- **`signature`**: Ed25519 signature by the author facility over the entire version envelope (excluding the signature itself).

### 2.5 Event
An event is a resource version broadcast over the protocol. Events are the unit of sync. An event is **immutable once authored** — there is no "edit event," only "new version of the same resource."

### 2.6 Log
Each facility and the hub maintains:

- **`local_log`**: SQLite + WAL table `(facility_id, version_id, envelope_blob)`, primary key `(facility_id, version_id)`. Append-only. Never updated, never deleted (subject to §10.4 retention policy).
- **`vector_clock_state`**: per-resource, the latest vector clock the local node has observed. Used for sync handshake.
- **`causal_graph`**: in-memory + materialized view of `parent_version_ids` edges, for replay ordering.

---

## 3. Causality & vector clocks

### 3.1 Vector clock format
Each version carries a vector clock: a map `{facility_id -> counter}`. Counters are monotonically increasing per facility.

```json
{
  "F-PETAUKE1": 47,
  "F-LUSAKA2":  12,
  "F-HUB":      3
}
```

### 3.2 Clock comparison
For two versions `A` and `B` with clocks `VC(A)` and `VC(B)`:

- **`A < B`** (A happened-before B): `∀k: VC(A)[k] ≤ VC(B)[k]` and `∃k: VC(A)[k] < VC(B)[k]`.
- **`A > B`** (B happened-before A): symmetric.
- **`A || B`** (concurrent — conflict): neither `A < B` nor `A > B`. Both facilities edited without seeing each other's edit.

### 3.3 Parent linking
When a facility creates a new version, it sets `parent_version_ids` to the version(s) it had observed when authoring:

- **Linear edit**: 1 parent — the immediately prior version the facility had locally.
- **Merge version**: 2 parents — produced by §6 conflict resolution, after the facility has observed a concurrent version from elsewhere and merged it.

A version with 0 parents is a **genesis version** (resource creation).

### 3.4 Causal graph
The collection of all `(version_id → parent_version_ids)` edges forms a DAG per resource. The hub maintains the global DAG; each facility maintains its slice. Sync is the process of bringing the facility's slice closer to the hub's global DAG.

---

## 4. Event envelope

Every event on the wire (HTTPS or USB) is wrapped in a **PSEP envelope**. The envelope is **CBOR-encoded** (compact, schema-evolvable, deterministic byte ordering for signing).

### 4.1 Envelope schema

```cbor
PSEPEnvelope = {
  0: "psep",                    # magic, version tag
  1: 1,                          # protocol major version
  2: 0,                          # protocol minor version

  3: bstr .size 16,              # version_id (UUIDv7, 16 bytes)
  4: bstr .size 16,              # resource_id (UUIDv7, 16 bytes)
  5: tstr,                       # resource_type ("Patient", "Observation", ...)
  6: { + tstr => uint },         # vector_clock: {facility_id -> counter}
  7: [ * bstr .size 16 ],        # parent_version_ids (0, 1, or 2 UUIDs)
  8: tstr,                       # author_facility_id
  9: tstr,                       # author_clinician_id
 10: uint,                       # created_at_unix_ms (best-effort wall clock)
 11: tstr,                       # kind: "clinical" | "identity" | "audit" | "system"
 12: bstr,                       # payload_ciphertext (sealed-box, §5)
 13: bstr .size 32,              # payload_hash (SHA-256 of plaintext)
 14: bstr,                       # payload_nonce (24 bytes, libsodium nonce)
 15: bstr,                       # recipient_eph_pubkey (X25519 ephemeral pubkey, 32 bytes)
 16: bstr .size 64,              # signature (Ed25519 over fields 0-15, CBOR-canonical)
}
```

### 4.2 Why CBOR and not JSON
- Deterministic byte encoding (RFC 7049 canonical form) is required for stable signatures. JSON canonicalization is fragile.
- ~30–40% smaller than JSON for the same payload.
- Native binary handling for UUIDs, hashes, signatures (no base64 expansion).
- Schema-evolvable via CDDL; new optional fields don't break old parsers.

### 4.3 Why per-resource versions (not field-level patches)
- **Simplicity**: each event is one signed, immutable FHIR resource. No patch algebra.
- **Auditability**: a medico-legal reviewer can decrypt any historical version and see the complete resource as it stood at that moment.
- **Storage cost is acceptable**: with FHIR R5 + content-addressed payloads (§10.2), the typical Patient resource is ~2KB; Observation ~1KB. A busy 200-bed district hospital generates ~5,000 events/day = ~10MB/day. A $20 microSD holds ~5 years.
- **Conflict detection is exact**: vector clocks on full versions give us true concurrency detection, which field-patches obscure.

---

## 5. End-to-end encryption

### 5.1 Threat model
- **Hub is honest-but-curious**: we trust the hub to deliver events correctly (signature-verified), but we do not trust it to not read payloads. The hub operator, their cloud provider, and any lawful-access request against the hub must NOT be able to read patient data.
- **Facility-to-facility direct sync** (e.g., during a sneakernet exchange between two district hospitals) must also be encrypted; facilities do not share keys directly.

### 5.2 Key hierarchy

```
                                       ┌─── per-event AES-GCM key (random 32B)
                                       │     (encrypted with X25519 ECDH below)
                                       │
Facility X25519 static keypair ────────┤
                                       │
                                       └─── sealed box to:
                                            - author facility's X25519 pubkey
                                            - all currently-authorized reader
                                              facilities' X25519 pubkeys
                                            - hub DOES NOT have a private key
```

### 5.3 Encryption procedure (per event)
1. Author facility generates a random 32-byte AES-GCM key `K` and a 12-byte nonce `N`.
2. Author encrypts the FHIR resource (CBOR-serialized) with AES-GCM-256: `ciphertext, tag = AESGCM(K, N, plaintext)`.
3. For each **authorized reader facility** (including the author itself, so it can read its own event back from the hub), the author generates an ephemeral X25519 keypair `(eph_priv, eph_pub)`, performs ECDH with the reader's static X25519 pubkey, and encrypts `K` with the resulting shared secret using libsodium `crypto_box_seal`.
4. The envelope carries: `payload_ciphertext` (the AES-GCM blob), `payload_nonce` (N), `payload_hash` (SHA-256 of plaintext, for integrity), and a list of **sealed key boxes** — one per reader. The envelope's `recipient_eph_pubkey` is the ephemeral pubkey the author used for itself; the multi-recipient list is in a sidecar structure (§5.4).

### 5.4 Multi-recipient keying
To avoid re-encrypting the entire payload N times for N readers, we use a single AES-GCM payload and a sidecar `recipient_keys` array:

```cbor
Envelope.recipient_keys = [
  {
    reader_facility_id: "F-LUSAKA2",
    eph_pubkey: bstr .size 32,        # ephemeral X25519 pubkey
    sealed_key:  bstr .size 80,       # libsodium sealed box containing K
  },
  ...
]
```

Each reader finds its `reader_facility_id` in the list, opens the sealed box with its static X25519 private key, recovers `K`, and decrypts the payload.

### 5.5 Authorization policy
The set of "authorized reader facilities" for a given event is determined by an **access policy**, evaluated at author time:

- **Identity resources** (Patient, RelatedPerson): readable by all facilities in the same district + the provincial hub's audit role (read-only, no decrypt — for header metadata only).
- **Clinical resources** (Encounter, Observation, MedicationRequest, etc.): readable by the author facility + any facility that has an active Encounter with this patient in the last 90 days.
- **Audit resources**: readable only by the author facility + the foundation's audit key (revocable).
- **System resources** (key rotations, schema migrations): readable by all facilities.

The policy is evaluated locally on the author's machine; the recipient list is embedded in the envelope. The hub **does not know** the policy; it only sees "this envelope has N sealed key boxes for N recipients."

### 5.6 Key rotation
- Facility keypairs rotate via a `system/key-rotation` event signed by the old key and the new key together (cross-signed).
- Recipients who lose access to their old private key cannot decrypt events encrypted to the old key. This is by design — we do not support post-hoc re-encryption in v0.1. Facilities must maintain secure backups of their keystore.
- Hub public keys rotate via a `system/hub-key-rotation` event signed by the foundation's root key (offline, hardware-backed).

---

## 6. Conflict detection & merge

### 6.1 Detection
When a facility receives a version from the hub whose vector clock is concurrent (`A || B`) with a version the facility already has, a **conflict** is flagged. The facility stores both versions; neither is discarded.

### 6.2 Resolution policy
For each FHIR resource type, a per-field merge policy is defined in `crates/fhir/merge-policy.toml`. The policy classifies each FHIR path into one of four merge classes:

| Class | Merge rule | Example fields |
|---|---|---|
| **`lww_scalar`** | Last-writer-wins, by `created_at_unix_ms` (tie-break by `version_id` lex order) | `Patient.name.text`, `Patient.telecom.value`, `Observation.value[x]` |
| **`union_set`** | Union of all values across concurrent versions, dedup by natural key | `Patient.identifier` (system + value), `Patient.address`, `Encounter.diagnosis` |
| **`append_only`** | Never merge — append all concurrent values; never delete | `Patient.photo`, `DocumentReference.content`, audit logs |
| **`clinical_review`** | Never auto-merge; surface to clinician as "conflicting edit" | `Patient.extension:allergyIntolerance`, `MedicationRequest.medication[x]`, `Observation.interpretation` |

### 6.3 Common-ancestor 3-way merge
For `lww_scalar` and `union_set` fields, the merge uses the **most recent common ancestor** version (lowest node on the DAG that both concurrent versions descend from) as the baseline. The merge is deterministic given the same set of input versions; any facility computing it arrives at the same result.

### 6.4 Conflict surfacing
For `clinical_review` fields, the protocol does **not** auto-merge. Instead:

1. The facility computes a "merged shell" — the resource with all `lww_scalar`/`union_set`/`append_only` fields resolved, and all `clinical_review` fields left as `null` with a `conflict` extension listing the conflicting values.
2. The clinician UI shows a banner: *"This record has conflicting edits from Petauke (Dr. Banda, 2026-10-14) and Lusaka (Dr. Phiri, 2026-10-15). Review required."*
3. The clinician resolves the conflict by creating a new version with the chosen value. This new version has **2 parents** (a merge version) and closes the conflict.

### 6.5 Conflict never blocks care
If a clinician is in the middle of an emergency and the conflict is unresolved, they can still see the last-known-good version (the common ancestor). The merged shell is a **non-blocking read view**; clinical operations continue.

### 6.6 Audit of conflicts
Every conflict, every auto-merge, and every clinician resolution is itself recorded as a `system/merge-resolution` event in the log. The medico-legal trail shows: who edited what, when, what was concurrent, what was auto-merged, what was manually resolved, and by whom.

---

## 7. Sync protocol (HTTPS transport)

### 7.1 Endpoint
All sync traffic is HTTPS to the hub: `https://hub.patientspace.org/psep/v1`. The hub is behind a CDN (Cloudflare) for DDoS protection and edge caching of envelope metadata. Payloads are end-to-end encrypted, so the CDN is not a trust boundary.

### 7.2 Handshake
Before pushing or pulling, the facility performs a handshake to negotiate the diff:

```
POST /psep/v1/handshake
Authorization: PSEP-Sig v1 facility_id=F-PETAUKE1,signature=...

Request body (CBOR):
{
  facility_id: "F-PETAUKE1",
  facility_clock: {                   # facility's view of the global vector clock
    "F-PETAUKE1": 1247,               #   - per facility, the highest counter it has
    "F-LUSAKA2":  842,                #     observed in any version of any resource
    "F-NKOKA1":   901,
    ...
  },
  resource_watermarks: {              # per resource, the highest version_id seen
    "U-0190ab...": "V-0190cd...",
    "U-0190ef...": "V-0190gh...",
    ...
  },
  capabilities: ["psep-v1", "cbor", "sealed-box-v1"],
}

Response (CBOR):
{
  hub_clock: { ... },                 # hub's view of the global vector clock
  push_size_hint: 14,                 # number of versions hub has that facility doesn't
  pull_size_hint: 3,                  # number of versions facility has that hub doesn't
  pull_required: true,                # hub needs facility's missing versions
  push_required: true,                # facility needs hub's missing versions
  session_id: "S-...",                # for resumable transfer
  session_expiry_unix_ms: ...,
}
```

The handshake is **O(small)** — proportional to the number of *facilities* and *active resources*, not the number of versions. The `resource_watermarks` is sent only for resources that have changed since the last successful sync.

### 7.3 Push (facility → hub)

```
POST /psep/v1/push?session_id=S-...
Content-Type: application/cbor-seq    # concatenated CBOR envelopes

[envelope1][envelope2]...[envelopeN]
```

The body is a **CBOR sequence** (RFC 8742): concatenated CBOR items, no length prefix. Each item is one PSEP envelope. The hub acknowledges each envelope by `version_id` in the response:

```
Response (CBOR):
{
  accepted: ["V-...", "V-...", ...],
  rejected: [
    { version_id: "V-...", reason: "duplicate" },
    { version_id: "V-...", reason: "bad_signature" },
    { version_id: "V-...", reason: "bad_vector_clock" },
  ],
  bytes_received: 284732,
}
```

The hub is **idempotent** — pushing the same envelope twice is a no-op (`accepted: []` on the second push).

### 7.4 Pull (hub → facility)

```
POST /psep/v1/pull?session_id=S-...
Content-Type: application/cbor-seq

Response body: [envelope1][envelope2]...[envelopeN]
```

The hub streams envelopes the facility is missing, in causal-DAG order (topological sort by `parent_version_ids`). The facility acknowledges:

```
POST /psep/v1/pull-ack?session_id=S-...
{ received: ["V-...", "V-...", ...] }
```

If the connection drops mid-stream, the facility resumes from the last acknowledged `version_id` in a new session.

### 7.5 Chunking and backpressure
- Each HTTP request body is capped at 8 MB (about 4,000 typical envelopes). Larger pushes split into multiple requests.
- The hub may send `HTTP 429` with a `Retry-After` header if the facility is pushing faster than it can verify signatures (rate-limited per `facility_id`).
- The facility applies **additive backoff**: 5s → 10s → 30s → 60s → 5min cap.

### 7.6 Sync agent lifecycle
The `patientspace-core` sync agent runs as a background tokio task:

1. **Idle state**: sleep 60s, then attempt handshake.
2. **Handshake fails** (network down): exponential backoff to 30 min, then idle.
3. **Handshake succeeds, both push and pull required**: do pull first (so the facility has the latest global state before pushing its own edits — reduces conflict rate), then push.
4. **Pull produces conflicts**: enqueue to clinician UI's "Review" queue, do not block sync.
5. **Sync completes**: log `system/sync-completed` audit event, return to idle.

### 7.7 Online-ness detection
The agent does **not** rely on OS "online" events (unreliable on Linux). It does a lightweight HEAD request to `https://hub.patientspace.org/psep/v1/ping` every 60s. A 200 OK within 5s = online; otherwise = offline.

---

## 8. Sneakernet protocol (USB transport)

### 8.1 Use case
A facility has had no internet for >24h. A clinician or driver is making a routine trip to the district health office (which has fiber). The facility operator inserts a USB stick, runs `patientspace export --to /media/usb`, and hands the stick to the driver. At the district office, the operator runs `patientspace import --from /media/usb` on a machine that has both the USB and internet; the import pushes to the hub and pulls back any pending updates, then writes the response back to the USB; the driver returns; the original facility runs `patientspace import --from /media/usb` to ingest the response.

### 8.2 USB file format
A single file: `petauke1_2026-10-15_001.psep`. The file is a **CBOR sequence** with a header envelope and payload envelopes:

```
[HeaderEnvelope]
[PayloadEnvelope1]
[PayloadEnvelope2]
...
[TrailerEnvelope]
```

**HeaderEnvelope**:
```cbor
{
  type: "psep-usb-header",
  protocol_version: 1,
  origin_facility_id: "F-PETAUKE1",
  exported_at_unix_ms: ...,
  exporter_signature: bstr,            # Ed25519 over the file (header + payload + trailer)
  payload_envelope_count: 1247,
  payload_blake3_hash: bstr .size 32,  # integrity check
}
```

**TrailerEnvelope**:
```cbor
{
  type: "psep-usb-trailer",
  payload_blake3_hash: bstr .size 32,
  exporter_signature: bstr,
}
```

### 8.3 Round-trip
1. **Facility A** exports → USB stick contains A's outbound envelopes.
2. **District office** imports USB → pushes to hub → pulls hub's response (any versions hub has that A doesn't) → writes the response back to the USB in a `petauke1_2026-10-15_001.response.psep` file.
3. **Facility A** imports the `.response.psep` file → ingests the hub's response.

The district office acts as a **relay**, not a trusted intermediary — it cannot decrypt any payloads (they are sealed-box encrypted for facility A and other authorized readers, not for the district office).

### 8.4 Hygiene
- USB sticks are pre-formatted ext4 (not FAT32 — we need file permissions and large files).
- Each export uses a new filename with a sequence number; the facility keeps the last 7 exports locally for forensic recovery.
- The import path is the same code path as HTTPS sync — same envelope validation, same vector clock check, same conflict detection. **No separate code path for sneakernet.**

---

## 9. Migration: SmartCare → PatientSpace

### 9.1 Pluggable ingest adapter
SmartCare data is ingested as a **stream of resource versions** through the same PSEP pipeline. There is no "migration mode" — the migration is just an author that produces many genesis versions.

```rust
trait SmartCareIngestAdapter {
    fn iter_patient_versions(&mut self) -> impl Iterator<Item = Patient>;
    fn iter_encounter_versions(&mut self) -> impl Iterator<Item = Encounter>;
    fn iter_observation_versions(&mut self) -> impl Iterator<Item = Observation>;
    // ... etc
}
```

Three adapters ship with v0.1:

| Adapter | Source | When to use |
|---|---|---|
| `SmartCareFhirAdapter` | SmartCare Pro HIE (FHIR R4 endpoint) | Preferred — cleanest, no DB access required. Petauke IT just gives us an OAuth client. |
| `SmartCareDbAdapter` | Direct read-only connection to SmartCare Pro's MySQL | When HIE is unavailable or batch export is too slow. Requires DB credentials. |
| `SmartCareCsvAdapter` | SmartCare's CSV/XML export files | Fallback for Legacy / Plus installs that predate HIE. Manual file delivery. |

The adapter is selected at install time via `patientspace migrate --source <adapter> --config <path>`.

### 9.2 Migration workflow (Petauke)
1. **Pre-flight (week before go-live)**: Site visit to Petauke. Install `patientspace-core` on a fresh NUC. Generate facility keypair. Pin hub public keys.
2. **Source assessment**: Run `patientspace migrate --source fhir --dry-run --config smartcare.yml` to enumerate counts (N patients, M encounters, K observations) and detect data quality issues (duplicate Smartcard numbers, missing NRCs, orphaned encounters).
3. **Identity crosswalk build**: For each SmartCare patient, generate a PatientSpace `resource_id`. Persist the mapping `(SmartCare patient UUID, Smartcard No., NRC) → PatientSpace resource_id` in the local `identity_crosswalk` table. This table itself is a synced resource (`IdentityCrosswalk`), so other facilities can look up "do I already know this patient?" without re-ingesting.
4. **Bulk ingest**: Author genesis versions for every historical patient, encounter, observation, etc. Each version has:
   - `author_facility_id = F-PETAUKE1` (Petauke "owns" its history).
   - `vector_clock = { "F-PETAUKE1": 1 }` (or incrementing counter).
   - `parent_version_ids = []` (genesis).
   - `created_at_unix_ms` = the original SmartCare timestamp (preserved for medico-legal continuity).
   - `kind = clinical`.
   - An extension `provenance: migrated-from-smartcare` with the source adapter name, source record ID, and migration timestamp.
5. **Push to hub**: The migration produces thousands of genesis versions; the sync agent pushes them in batches. A typical district hospital with 50,000 patients × 5 years of history = ~2M events = ~4GB on the wire. Over a 3G connection at 1MB/s, that's ~70 minutes total, but the agent throttles to avoid saturating the uplink.
6. **Validation**: Cross-check counts. `patientspace migrate --validate` produces a report: "SmartCare says 51,234 patients; PatientSpace has 51,234 patients; 0 missing; 12 duplicates resolved via biometric dedup."
7. **Cutover**: At a chosen go-live moment, Petauke stops entering data into SmartCare and starts entering into PatientSpace. SmartCare is left read-only for one month as a safety net, then decommissioned.

### 9.3 SmartCare → FHIR mapping
The trickiest mappings:

| SmartCare field | FHIR R5 path | Notes |
|---|---|---|
| Patient Smartcard No. | `Patient.identifier[system=smartcard].value` | Primary identifier in PatientSpace crosswalk |
| Patient NRC | `Patient.identifier[system=nrc-zm].value` | Secondary, used for dedup |
| Patient fingerprint template | `Patient.extension:biometric-template` (Base64) | Encrypted at field level; never leaves facility |
| Encounter visit date | `Encounter.period.start` | |
| Diagnosis (ICD-10) | `Encounter.diagnosis.condition` (Reference to Condition) | SmartCare stores text; we map via ICD-10 lookup |
| ART regimen | `MedicationRequest.medicationCodeableConcept` | Uses WHO ATC codes |
| Lab result (CD4, viral load) | `Observation.code` (LOINC) + `Observation.value[x]` | SmartCare DISA integration provides LOINC already |
| PMTCT visit | `Encounter.serviceType` = `pmtct` | |

Mappings live in `crates/smartcare-adapter/mapping.toml`, auditable and version-controlled.

### 9.4 Rollback safety
If PatientSpace has a critical bug in the first month, Petauke can fall back to SmartCare (read-only mode was preserved). The protocol's append-only log means PatientSpace can re-export its records back to SmartCare's CSV/XML format via a `patientspace export --to smartcare-csv` command — same adapter, in reverse. We are never locked in.

---

## 10. Operational concerns

### 10.1 Storage budget
- **Per event on disk**: ~2.5KB (envelope + ciphertext + indexes). 5,000 events/day = ~12.5MB/day = ~4.5GB/year.
- **Storage target**: 64GB SSD on the facility NUC. Comfortable for ~10 years of operation without compaction.
- **Hub storage**: linear in total events across all facilities. A 100-facility deployment at 5,000 events/day each = 500K events/day = ~1.5GB/day = ~550GB/year. Cloud storage is cheap; this is not a constraint.

### 10.2 Content-addressed payloads
Identical payloads (e.g., the same Observation posted twice due to a sync retry) hash to the same `payload_hash`. Both envelopes reference the same blob. The hub and facility deduplicate on `payload_hash` in a side-table `(payload_hash -> blob)`. This typically saves 10–20% storage in steady state.

### 10.3 Compaction (query-side only)
The append-only log is never rewritten. But the **materialized view** layer (the FHIR REST API's read path) compacts: for each resource, only the latest non-conflicted version is served by default. History endpoints page through the version chain. This is invisible to the protocol.

### 10.4 Retention
- **Clinical and identity versions**: retained forever, immutable. No deletion. Patient death does not trigger deletion; it triggers a `Patient.active = false` and a `system/death-record` audit event.
- **System versions** (key rotations, sync-completed events): compacted after 7 years to summary form (count per day per facility). Audit trail still queries "did facility X sync on date Y?" — yes/no, not the full event stream.
- **Right-to-be-forgotten**: Zambia's Data Protection Act 2021 does not grant a general right to erasure of medical records (medical records are exempted). If a court orders erasure, the protocol supports a `system/redaction-order` event that tombstones the resource versions; the original ciphertext blobs remain in the log but are not retrievable without the redaction key (held offline by the foundation's legal counsel).

### 10.5 Repair
- `patientspace repair --facility F-PETAUKE1`: re-pulls every version from the hub and verifies Blake3 hashes. Detects local corruption.
- `patientspace repair --hub`: re-verifies every signature on the hub. Detects hub-side corruption.
- Both commands are safe to run on a live system; they are read-only operations.

### 10.6 Observability
Each sync session logs:
- `sync.session.started` (facility_id, session_id, push_size_hint, pull_size_hint)
- `sync.envelope.pushed` (version_id, bytes)
- `sync.envelope.pulled` (version_id, bytes, decrypted: bool)
- `sync.conflict.detected` (resource_id, version_a, version_b, fields_in_conflict)
- `sync.session.completed` (duration_ms, envelopes_pushed, envelopes_pulled, conflicts_detected)

Logs are structured JSON, written to `~/.patientspace/logs/sync.jsonl`, and never contain plaintext patient data (only version IDs and metadata).

---

## 11. Authorization & access control

### 11.1 Facility identity
Each facility has an Ed25519 signing key. Every envelope is signed by the author facility. The hub verifies signatures before accepting an envelope; facilities verify signatures before ingesting.

### 11.2 Clinician identity
Within a facility, clinicians authenticate via local password + (optionally) YubiKey. The facility issues **delegated clinician keypairs** (Ed25519) signed by the facility's key. Clinician signatures are recorded in `author_clinician_id` for audit. If a clinician leaves, the facility revokes their key via a `system/clinician-revocation` event.

### 11.3 Read access
The hub does not enforce read access — it serves any envelope to any facility that asks. **End-to-end encryption** is the access control: if a facility cannot decrypt an envelope (because it is not in the recipient list), the envelope is opaque ciphertext to that facility. This is by design — the hub is untrusted.

A misbehaving hub cannot grant a facility access it shouldn't have; it can only fail to deliver.

### 11.4 Cross-facility lookup
When Facility B (Lusaka) wants to look up a patient registered at Facility A (Petauke), B sends:

```
POST /psep/v1/lookup
{ identifier: { system: "smartcard", value: "SC-12345678" } }
```

The hub responds with the **envelope metadata** for matching resources (version IDs, author facility, created_at) but **not the decrypted payload**. B then requests the specific version, and the hub delivers the ciphertext. B can decrypt only if A's author put B in the recipient list — which happens automatically for resources where B has an active Encounter with this patient (§5.5).

If B does not have access, B sees "patient exists at Petauke, but you do not have access to their records." B can request access via a `system/access-request` event, which Petauke's clinician sees in their UI and can grant or deny.

---

## 12. Wire format & framing

- All HTTP bodies are `application/cbor-seq` (RFC 8742): concatenated CBOR items.
- For sneakernet, the file format is the same CBOR sequence with a header and trailer envelope.
- Each envelope is self-delimiting in CBOR (the parser knows where one ends and the next begins).
- Maximum envelope size: 1MB. Resources larger than this (e.g., DiagnosticReport with embedded PDFs) are split — the FHIR resource references a `Binary` resource, which is synced via a separate `psep/binary` endpoint with chunked transfer. Out of scope for v0.1; Binary resources will be added in v0.2.

---

## 13. Versioning & upgrades

### 13.1 Protocol version
The protocol version is in the envelope (fields 1 and 2). v0.1 = major 1, minor 0. Minor versions are forward/backward compatible (new optional fields). Major versions require a coordinated upgrade.

### 13.2 Schema evolution
FHIR R5 schemas evolve via the standard FHIR versioning. The `payload_hash` is computed over the **canonical CBOR serialization** of the FHIR resource, which is stable across FHIR patch versions. Major FHIR version bumps (R5 → R6) are handled by carrying a `fhir_version` field in the envelope (defaults to R5) and versioning the merge policy accordingly.

### 13.3 Upgrade procedure
- The foundation publishes a new `patientspace-core` release.
- Each facility upgrades at its own pace (no coordinated downtime).
- Old envelopes remain readable forever; new envelopes may use new fields, which old parsers ignore (CBOR's forward compatibility).
- A facility running an old version cannot *produce* envelopes with new fields, but it can *ingest* them (ignoring the new fields). This is by design — rural facilities with intermittent maintenance windows are not blocked.

---

## 14. Test plan & invariants

### 14.1 Invariants
The protocol is correct if and only if the following invariants hold:

1. **Determinism**: Given the same set of input versions, every facility computes the same merged view.
2. **Causality preservation**: If event A happened-before event B (per vector clocks), every facility observes A before B in its causal graph.
3. **Integrity**: Any envelope ingested by a facility has a valid signature from a known facility and a valid `payload_hash` matching the decrypted plaintext.
4. **Confidentiality**: A facility cannot decrypt an envelope for which it is not in the recipient list.
5. **Convergence**: After all facilities have synced with the hub, every facility has the same set of versions in its log (eventual consistency).
6. **Auditability**: For any current state of any resource, the complete version chain back to genesis is reconstructable from the log.

### 14.2 Test suite
`crates/sync/tests/` contains:

- **`vector_clock_tests.rs`**: unit tests for clock comparison, parent linking, conflict detection.
- **`envelope_roundtrip_tests.rs`**: encode/decode/sign/verify/encrypt/decrypt round-trips for every FHIR resource type.
- **`merge_policy_tests.rs`**: per-field merge policy, with synthetic concurrent edits covering every class.
- **`sync_simulation_tests.rs`**: N simulated facilities, each on a tokio task, with a simulated network partition matrix. Verifies convergence after partitions heal.
- **`smartcare_migration_tests.rs`**: ingests a synthetic SmartCare dataset (10K patients, 100K encounters) and verifies counts, identity crosswalk, and FHIR mapping correctness.
- **`sneakernet_roundtrip_tests.rs`**: export → relay → import, with bit-flip injection on the USB file to verify signature rejection.

### 14.3 Chaos testing
Before each release, the foundation runs a 7-day chaos test: 10 simulated facilities, random network partitions, random facility restarts, random clock skew, one facility permanently lost (key destruction — verifies the "lost facility" recovery procedure, which involves a quorum of peer facilities signing a `system/failure-declaration` event).

---

## 15. Open questions for v0.2

1. **Binary resources** (radiology, signed PDFs): chunked transfer protocol design.
2. **Cross-federation sync**: if Zimbabwe deploys PatientSpace independently, can a border facility share records with a Zambian facility?
3. **Patient-issued keys**: can a patient hold their own X25519 key (on a Smartcard-style card) and decrypt their records on their phone? This is the long-term PHR vision.
4. **Post-quantum**: when do we hybridize the X25519 ECDH with ML-KEM (Kyber)? Probably 2027, when NIST PQC standards are widely deployed.
5. **Hub federation**: can the Lusaka hub and the Ndola hub sync with each other as peers, for disaster recovery?
6. **Selective disclosure**: can a facility share a redacted view of a patient (e.g., hide HIV status from a referring dentist) without losing the audit trail?

---

## Appendix A: Why this design over alternatives

### A.1 Why not CRDTs (automerge/yrs)?
- CRDTs require defining every data type as a CRDT. FHIR resources are deeply nested and don't map cleanly.
- CRDT resolution is opaque — hard to produce a medico-legal audit trail showing "what was the merged value and why."
- CRDT libraries are heavy (automerge-wasm is ~150KB) and don't play well with Rust's type system yet.
- Our per-resource-version + vector-clock + policy-driven merge gives us CRDT-like convergence for the cases that matter, with a fully auditable resolution.

### A.2 Why not just FHIRcast / FHIR Subscriptions?
- FHIRcast is for real-time UI synchronization, not offline sync.
- FHIR Subscriptions require the source to be online and reachable; doesn't survive multi-day partitions.
- Both assume a single authoritative server; we are explicitly multi-master.

### A.3 Why not CouchDB/PouchDB replication?
- CouchDB's replication model is close to what we want (multi-master, offline-first), but:
  - It's a full database, not an event log. No version chain, no causality graph.
  - Conflict resolution is "deterministic but arbitrary" (revision tree + deterministic winning rev). Not suitable for clinical fields where the merge policy matters per-field.
  - No end-to-end encryption — CouchDB encrypts at rest, but the server can read.
  - JVM-style ops story; we want a single Rust binary.
- We borrow CouchDB's mental model (every node replicates with every other node, eventual consistency) but rebuild the internals for clinical-grade audit and confidentiality.

### A.4 Why not just use Git internally?
- Git's DAG is conceptually similar to our version chain, but:
  - Git is line-based text; FHIR resources are structured.
  - Git's merge model is human-driven; we need deterministic auto-merge for 99% of cases.
  - Git's object store doesn't support content-addressed encryption.
  - Git tooling is designed for human-scale repos, not 2M-resource-per-facility scale.
- We borrow Git's DAG + content addressing; we don't use Git.

---

## Appendix B: Glossary

- **CBOR**: Concise Binary Object Representation (RFC 8949). Compact binary JSON-like format.
- **CBOR sequence**: Concatenated CBOR items (RFC 8742). Used for streaming multiple envelopes in one body.
- **Ed25519**: EdDSA over Curve25519, used for signatures.
- **X25519**: ECDH over Curve25519, used for key agreement.
- **AES-GCM-256**: Authenticated encryption used for payload confidentiality.
- **libsodium sealed box**: Anonymous sender, authenticated recipient public-key encryption. Built on X25519 + XSalsa20-Poly1305.
- **Vector clock**: Per-node monotonic counter map, used for causality tracking.
- **FHIR R5**: Fast Healthcare Interoperability Resources, Release 5 (HL7, 2023).
- **Sneakernet**: Carrying data on physical media (USB stick) between sites.
