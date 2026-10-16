// Thin typed wrapper around the patientspace-core FHIR REST API.
//
// All endpoints are same-origin in dev (Vite proxies /fhir → 127.0.0.1:8080)
// and same-origin in production (SvelteKit adapter-node served by axum, or
// behind a reverse proxy).
//
// The server is authoritative for `id` — POST /fhir/Patient assigns a fresh
// PatientSpace resource_id (UUIDv7) and ignores any client-supplied id.

// -----------------------------------------------------------------
// FHIR resource shapes (subset used by PatientSpace v0.1)
// -----------------------------------------------------------------

export interface Identifier {
  system?: string;
  value: string;
  use?: string;
}

export interface HumanName {
  text?: string;
  given?: string[];
  family?: string;
  use?: string;
}

export interface ContactPoint {
  system?: string;
  value: string;
  use?: string;
}

export interface Address {
  line?: string[];
  city?: string;
  district?: string;
  state?: string;
  postalCode?: string;
  country?: string;
  use?: string;
}

export interface Extension {
  url: string;
  valueString?: string;
  valueBase64Binary?: string;
}

export interface Patient {
  resourceType: 'Patient';
  id: string;
  identifier?: Identifier[];
  active?: boolean;
  name?: HumanName[];
  telecom?: ContactPoint[];
  gender?: 'female' | 'male' | 'other' | 'unknown';
  birthDate?: string;
  address?: Address[];
  extension?: Extension[];
}

export interface Bundle<T> {
  resourceType: 'Bundle';
  type?: string;
  total: number;
  entry?: Array<{ resource: T }>;
}

// -----------------------------------------------------------------
// Identifier systems
// -----------------------------------------------------------------

export const SMARTCARD_SYSTEM = 'http://patientspace.org/sid/smartcard';
export const NRC_SYSTEM = 'http://patientspace.org/sid/nrc-zm';

// PatientSpace conflict extension — set by the Rust core when a Patient has
// two or more concurrent versions in the local event log.
const CONFLICT_EXTENSION_URL =
  'http://patientspace.org/fhir/StructureDefinition/conflict';

// -----------------------------------------------------------------
// Query params
// -----------------------------------------------------------------

export interface ListParams {
  count?: number;
  offset?: number;
  identifier?: string;
}

// -----------------------------------------------------------------
// Create-patient input
// -----------------------------------------------------------------

export interface NewPatientInput {
  smartcardNo?: string;
  nrc?: string;
  familyName?: string;
  givenNames?: string;
  phone?: string;
  gender?: 'female' | 'male' | 'other' | 'unknown';
  birthDate?: string;
}

// -----------------------------------------------------------------
// Helpers (pure)
// -----------------------------------------------------------------

export function smartcardNo(p: Patient): string | undefined {
  return p.identifier?.find((i) => i.system === SMARTCARD_SYSTEM)?.value;
}

export function nrc(p: Patient): string | undefined {
  return p.identifier?.find((i) => i.system === NRC_SYSTEM)?.value;
}

export function displayName(p: Patient): string {
  const n = p.name?.[0];
  if (!n) return '—';
  if (n.text) return n.text;
  const parts: string[] = [];
  if (n.given && n.given.length > 0) parts.push(n.given.join(' '));
  if (n.family) parts.push(n.family);
  return parts.length > 0 ? parts.join(' ') : '—';
}

export function phone(p: Patient): string | undefined {
  return p.telecom?.find((t) => t.system === 'phone' || t.system === 'sms' || t.system === undefined)?.value;
}

export function hasConflict(p: Patient): boolean {
  return Boolean(p.extension?.some((e) => e.url === CONFLICT_EXTENSION_URL));
}

// -----------------------------------------------------------------
// API functions
// -----------------------------------------------------------------

/**
 * List Patients with pagination + optional identifier search.
 *
 * The backend accepts both `value` (matches any identifier whose value
 * equals the filter) and `system|value` (matches both). Pass the raw
 * search string the user typed — it's URL-encoded before being sent.
 *
 * Returns `{ patients, total }`. `total` is the count of matches BEFORE
 * pagination (FHIR `Bundle.total`).
 */
export async function listPatients(
  params: ListParams = {},
): Promise<{ patients: Patient[]; total: number }> {
  const qs = new URLSearchParams();
  qs.set('_count', String(params.count ?? 50));
  qs.set('_offset', String(params.offset ?? 0));
  if (params.identifier && params.identifier.trim() !== '') {
    qs.set('identifier', params.identifier.trim());
  }
  const r = await fetch(`/fhir/Patient?${qs.toString()}`);
  if (!r.ok) {
    const text = await r.text().catch(() => '');
    throw new Error(`listPatients failed: ${r.status} ${text}`);
  }
  const body: Bundle<Patient> = await r.json();
  return {
    patients: body.entry?.map((e) => e.resource) ?? [],
    total: body.total ?? 0,
  };
}

/**
 * Fetch a single Patient by id. The backend accepts both `U-{uuid}` and
 * bare `{uuid}`.
 */
export async function getPatient(id: string): Promise<Patient> {
  const r = await fetch(`/fhir/Patient/${encodeURIComponent(id)}`);
  if (!r.ok) {
    const text = await r.text().catch(() => '');
    throw new Error(`getPatient ${id} failed: ${r.status} ${text}`);
  }
  return r.json();
}

/**
 * Build a FHIR Patient JSON object from structured input.
 *
 * The server assigns the `id` — we never set it client-side. We do set
 * `active: true` and assemble identifiers / name / telecom in the
 * canonical PatientSpace shape.
 */
export function buildPatientJson(input: NewPatientInput): Record<string, unknown> {
  const patient: Record<string, unknown> = {
    resourceType: 'Patient',
    active: true,
  };

  const identifiers: Identifier[] = [];
  if (input.smartcardNo && input.smartcardNo.trim() !== '') {
    identifiers.push({ system: SMARTCARD_SYSTEM, value: input.smartcardNo.trim() });
  }
  if (input.nrc && input.nrc.trim() !== '') {
    identifiers.push({ system: NRC_SYSTEM, value: input.nrc.trim() });
  }
  if (identifiers.length > 0) {
    patient.identifier = identifiers;
  }

  if (input.familyName || input.givenNames) {
    const name: HumanName = { use: 'official' };
    const given = input.givenNames?.trim();
    if (given) name.given = given.split(/\s+/);
    if (input.familyName && input.familyName.trim() !== '') {
      name.family = input.familyName.trim();
    }
    // Canonical text form: "Family, Given1 Given2"
    const fam = name.family ?? '';
    const giv = name.given?.join(' ') ?? '';
    if (fam && giv) name.text = `${fam}, ${giv}`;
    else if (fam) name.text = fam;
    else name.text = giv;
    patient.name = [name];
  }

  if (input.phone && input.phone.trim() !== '') {
    patient.telecom = [
      { system: 'phone', use: 'mobile', value: input.phone.trim() },
    ];
  }

  if (input.gender) {
    patient.gender = input.gender;
  }

  if (input.birthDate && input.birthDate.trim() !== '') {
    patient.birthDate = input.birthDate.trim();
  }

  return patient;
}

/**
 * Create a new Patient. POSTs FHIR Patient JSON to /fhir/Patient, expects
 * 201 Created with the canonical Patient JSON (server-assigned id) in the
 * response body.
 */
export async function createPatient(input: NewPatientInput): Promise<Patient> {
  const json = buildPatientJson(input);
  const r = await fetch('/fhir/Patient', {
    method: 'POST',
    headers: { 'Content-Type': 'application/fhir+json' },
    body: JSON.stringify(json),
  });
  if (!r.ok && r.status !== 201) {
    const text = await r.text().catch(() => '');
    throw new Error(`createPatient failed: ${r.status} ${text}`);
  }
  return r.json();
}
