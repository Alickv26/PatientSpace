# SmartCare Zambia — Research Brief

## 1. What SmartCare Is

**SmartCare** is the **national Electronic Health Record (EHR) system of Zambia**, owned and led by the Ministry of Health (MoH). It is the dominant health information system in the country and one of the largest deployed EHRs in sub-Saharan Africa.

### Origin & ownership
- Originally developed in **2004–2005** with support from **CDC Zambia** (US President's Emergency Plan for AIDS Relief, PEPFAR) to capture individual-level HIV patient data.
- Adopted as **Zambia's national EHR in 2017** by the MoH.
- Currently deployed in **1,600+ healthcare facilities** across all 10 provinces.
- Operated under the Smart Zambia Institute (e-Government Division) and MoH.
- **Three product generations exist**: SmartCare Legacy → SmartCare Plus → **SmartCare Pro** (current).

### Architecture (SmartCare Pro — current)
- **Web-based**, n-tier architecture: Application layer (front-end), Business layer (rules), Data layer (DB). Decoupled, OOP-based.
- Centralized national server at MoH headquarters; facilities push/pull data.
- **SmartCare Reporting Service (SRS)**: separate reporting subsystem. Nightly ETL moves data from OLTP → OLAP database so reports don't slow down the live system.
- **Health Information Exchange (HIE) platform** — integrates with other national systems using **FHIR, HL7, JSON, XML** over multiple protocols.
- **Cybersecurity**: AES-256 at rest/in transit, firewalls, IDS, MFA, VAPT, claims HIPAA + GDPR compliance, incident response plan, continuous monitoring.
- **Biometrics**: fingerprint capture at registration; used to identify patients at reception, pharmacy, etc., via a biometric driver.
- **Zambia 360 Data Access and Sharing Portal** — national health data warehouse + visualization layer for policymakers.

### Modules / Features
SmartCare organizes data into "modules" across the patient journey:
1. **Patient registration & demographics** (with Smartcard issued at first visit; patient records portable across facilities).
2. **Clinical encounter documentation** (diagnoses, vitals, treatment plans, allergies, immunizations, radiology, lab results).
3. **Pharmacy / prescription management** — integrated with eLMIS for stock visibility.
4. **Laboratory orders & results** — integrated with **DISA** (national Laboratory Information System).
5. **HIV/ART module** — the original use case; ART enrollment, CD4, viral load tracking.
6. **PMTCT** (Prevention of Mother-to-Child Transmission) — maternal + infant HIV cascade.
7. **TB module** — case-based surveillance, expanded from HIV.
8. **Maternal & Child Health (MCH)** — ANC, deliveries, immunizations.
9. **Clinical Decision Support Systems (CDSS)** — embedded clinical guidelines.
10. **Reporting & analytics** — routine + ad-hoc, fed into DHIS2 and the Zambia 360 portal.
11. **Inventory & supply chain** — via eLMIS integration.
12. **Case-Based Surveillance (CBS)** — feeds national surveillance.
13. **Helpdesk / ITSM** — "TUSO" service desk, toll-free 8080.

### Key integrations (HIE)
| System | Purpose |
|---|---|
| DISA | Laboratory Information System |
| eLMIS | Logistics / commodities |
| DHIS2 | National aggregate reporting |
| CBS | Case-based surveillance |

---

## 2. How It's Used (in practice)

### Patient journey
1. Patient registers at the facility → biometric + Smartcard issued.
2. At each service point (triage, consultation, lab, pharmacy, MCH, ART), clinicians enter data into SmartCare instead of (or alongside) paper forms.
3. Lab orders flow to DISA, results flow back electronically.
4. Pharmacy dispenses with stock deducted in eLMIS.
5. Data syncs to the national MoH server; aggregates flow to DHIS2 and the Zambia 360 portal.

### Deployment process (per facility)
1. **Readiness assessment** — site visit, infrastructure check.
2. **Installation + configuration** (1–5 days typical).
3. **Training** — classroom + practical + go-live support.
4. **Ongoing support** — via TUSO service desk + on-site visits.

### Users
- Clinicians (doctors, nurses, clinical officers), data entry clerks, pharmacy, lab, M&E officers, facility managers, district/provincial health officers, MoH HQ analysts, implementing partners (PEPFAR/CDC-funded IPs).

---

## 3. Limitations & Known Pain Points

These come from peer-reviewed studies (Gumede-Moyo et al. 2019, BMJ Open; Kaumba et al. 2023; Mutale 2017; UNZA theses) and direct user reports.

### A. Structural / design
- **Database grew organically** — modules kept being added since 2004; now "tries to address multiple issues simultaneously," making it hard to query and slow.
- **Slow boot times** — system is "very slow" in the mornings; long reboot. Frequent hangs during data entry.
- **Reporting performance** — was a major pain until SRS/OLAP was added; even now, complex reports are slow.
- **SmartCare Legacy + Plus + Pro coexist** — fragmented data, migration overhead.
- **Closed source, vendor-lock-in** — only MoH/CDC teams can modify; external researchers cannot audit.

### B. Infrastructure
- **Power outages / load shedding** — single biggest operational challenge. When power is out, data isn't collected; clinicians reluctant to fall back to paper; massive backlogs when power returns.
- **Poor/no internet at many facilities** — sync to national server fails; offline buffer overflows; data loss reported.
- **Functioning equipment shortages** — many sites lack PCs, UPS, or printers; toner shortages for printing forms.
- **Computer viruses** — PCs used for SmartCare also used for other tasks; infections take systems down.
- **No dedicated IT staff** at most facilities.

### C. Human / process
- **Inadequate training** — many staff "don't understand how and why they are collecting this data"; turnover means skills leak.
- **Funding cycles** — implementing partners hire data clerks on grants; when grants end, clerks are laid off → data entry stops. "Data are lost as data entry staff are laid off between funding cycles."
- **Incomplete paper forms** — back-to-paper workflows have >50% incomplete forms; data quality issues cascade.
- **Resistance / low computer literacy** — especially among older clinicians.
- **Data discrepancies** — paper files vs SmartCare often don't match (e.g., 200 paper files vs 300 SmartCare records due to "VIP" file storage).
- **No buy-in / feedback loop** — facility staff don't see their own data back; SmartCare feels like an extractive reporting tool rather than a care tool.

### D. Functional / product
- **Limited offline-first design** — sync breaks, no graceful degradation.
- **No mobile-native client** — primarily a desktop web app.
- **Limited interoperability in practice** — HIE exists on paper but integrations with DISA/eLMIS/DHIS2 are recent and partial.
- **No patient portal / PHR** — patients cannot access their own records.
- **Weak analytics** — relies on separate DHIS2 + Zambia 360 rather than built-in dashboards for clinicians.
- **Single-tenant deployment model** — hard to extend beyond Zambia without re-engineering.

### E. Legal / governance
- New laws (Data Protection Act 2021, Cyber Security Act 2021, eGovernment Act 2021) — full impact not yet realized; compliance gaps likely.
- No clear data-sharing governance with third parties (researchers, NGOs).

---

## 4. Existing Alternatives

### A. Open-source EHR / HIS

| System | Language / Stack | Strengths | Weaknesses |
|---|---|---|---|
| **OpenMRS** | Java + Hibernate + MySQL; REST APIs; FHIR support | Built for low-resource settings; multitenant; strong community; used in 40+ countries; Mozambique, Rwanda, Kenya national deployments | No built-in billing; UI dated; complex to customize; JVM heavy |
| **Bahmni** (built on OpenMRS + OpenELIS + Odoo/ERPNext) | Java + JS + Python | End-to-end hospital system: EMR + lab + billing + inventory; designed for low-resource hospitals; Thoughtworks-backed | Heavy composite stack; deployment complex; not Rust |
| **OpenEMR** | PHP + MySQL | Most popular OSS EHR globally; integrated practice management + billing; FHIR APIs; Docker-friendly | US-centric; not designed for offline/low-resource; PHP legacy |
| **GNU Health** | Python + PostgreSQL + Tryton | Strong in public health, lab, socioeconomics; WHO-endorsed | Smaller community; less polished UI; not focused on offline |
| **Open Hospital** | Java + Web (newer versions) | Simple, hospital-focused, Italian origin | Limited features; smaller community |
| **HospitalRun** | JavaScript (Ember) + PouchDB/CouchDB | Offline-first, modern UI; designed for developing world | Largely abandoned (low activity); feature-light |
| **OpenELIS** | Java | Lab-specific; integrates with OpenMRS/Bahmni | Lab only, not full HIS |
| **VistA** (US VA) | MUMPS / Cache | Battle-tested, full-featured, large installed base | MUMPS is legacy; very US-flavored; hard to localize |
| **EHRServer** | Ruby on Rails | FHIR-native | Small project, limited traction |
| **OystEHR** | TypeScript (Node) | Modern, FHIR-native, serverless-first | New, narrow community; proprietary-leaning open core |

### B. Closed-source / commercial relevant in Africa
- **Meditech, Epic, Cerner (Oracle Health)** — enterprise Western; too expensive/complex for low-resource settings.
- **Helium Health (Nigeria)** — SaaS hospital management for West Africa; proprietary.
- **Medicai, CareCloud, Fidelity Health** — regional SaaS, varying footprint.
- **Edoce (Kenya), Slade Health, Geniotech EHR (Tanzania)** — local SaaS, limited reach.
- **Jamii Med, Medbook** — regional PHR/EMR startups.

### C. Key infrastructure building blocks
- **DHIS2** (Norway/HISP) — aggregate health reporting; Java; de-facto standard in 80+ countries.
- **OpenCRVS** — civil registration (JS/Node).
- **OpenSRP/CommCare** — community health worker mobile apps (Java/Android).
- **HAPI FHIR** — reference FHIR server (Java).

### D. Rust-specific landscape (important for your strategy)
- **No production Rust EHR exists** as of late 2025. This is genuinely open territory.
- Rust building blocks that would matter: `axum`/`actix-web` for HTTP, `sqlx`/`sea-orm`/`diesel` for DB, `tokio` for async, `serde` for (de)serialization, `rustls` for TLS, `ring`/`openssl` for crypto, FHIR via generated structs from `fhir-model` crate.
- Comparable Rust projects in adjacent space: **RudderStack** (data), **Meilisearch** (search), **Qdrant** (vector DB), **TiKV** (KV store) — show Rust can ship healthcare-grade infra, but none are EHRs.

---

## 5. Strategic Gaps That a Rust Successor Could Exploit

1. **Offline-first by design** — local-first P2P sync (CRDTs / Merkle trees), graceful degradation on power/internet loss, conflict resolution. SmartCare fails here badly.
2. **Single binary, low-footprint deployment** — Rust compiles to one binary; can run on a Raspberry Pi or NUC at a clinic; no JVM, no separate DB server required (embed SQLite/LibreOffice-style storage with optional Postgres at provincial hub).
3. **Type-safe FHIR-native core** — model everything as FHIR R5 resources; expose FHIR REST API from day 1; integrate with any HIE without custom adapters.
4. **Patient-centric, not reporting-centric** — give facility staff their own dashboards + patient portal; close the feedback loop that SmartCare lacks.
5. **Mobile-first** — Rust core + thin mobile (React Native / Flutter) or PWA clients; supports CHWs at point of care, not just facility desks.
6. **Audit-grade security & privacy** — Rust's memory safety + signed audit log + per-field encryption + ZK-friendly pseudonymization for research extracts.
7. **Modular plugin architecture** — HIV, TB, MCH, lab, billing, supply chain as separately installable modules; don't repeat SmartCare's monolithic bloat.
8. **Open source, Africa-led governance** — forkable, auditable, community-driven; can be deployed by any country without licensing MoU.
9. **First-class analytics** — embedded columnar store (DuckDB) for OLAP inside the same binary; no separate OLAP server.
10. **Lightweight biometrics** — open-source fingerprint matching (e.g., SourceAFIS) embedded; no proprietary driver.

---

## 6. Suggested Next Conversations

Before any architecture is drawn, we need decisions on:
1. **Scope** — full hospital HIS, or focused EHR for primary care + clinics, or vertical (HIV/TB/MCH) first?
2. **Deployment model** — facility-embedded server, district hub, cloud-only, or hybrid?
3. **Interoperability strategy** — be a SmartCare/DHIS2 companion, or a full replacement?
4. **Audience** — Zambia-only first, or pan-African from day 1?
5. **Front-end stack** — Rust-only (Leptos/Dioxus), or Rust backend + TypeScript/React frontend?
6. **Offline sync strategy** — CRDTs, log replication, or simple last-writer-wins?
7. **Data model** — FHIR-native, OpenMRS dictionary, or custom?
8. **Licensing & governance** — Apache 2.0 vs AGPL vs MPL; foundation vs company-led.
9. **Funding model** — grants (PEPFAR/GF/WHO), SaaS, government procurement, mixed?
10. **Initial pilot scope** — one facility, one disease program, or one district?

---

*Research compiled from smartcarezambia.io, MoH Zambia, SMART Zambia Institute, CDC Zambia, BMJ Open (Gumede-Moyo 2019), ScienceDirect (Kaumba 2023), UNZA dissertations, PMC comparisons of OSS EHRs, and project pages of OpenMRS, Bahmni, OpenEMR, GNU Health, HospitalRun.*
