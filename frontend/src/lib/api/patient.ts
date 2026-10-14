// Thin typed wrapper around the patientspace-core FHIR REST API.
// All endpoints are same-origin in dev (Vite proxies /fhir → 127.0.0.1:8080)
// and same-origin in production (SvelteKit adapter-node served by axum, or
// behind a reverse proxy).

export interface Patient {
  resourceType: 'Patient';
  id: string;
  identifier?: Array<{ system?: string; value: string; use?: string }>;
  active?: boolean;
  name?: Array<{ text?: string; given?: string[]; family?: string; use?: string }>;
  telecom?: Array<{ system?: string; value: string; use?: string }>;
  gender?: 'female' | 'male' | 'other' | 'unknown';
  birthDate?: string;
}

export interface Bundle<T> {
  resourceType: 'Bundle';
  total: number;
  entry?: Array<{ resource: T }>;
}

const SMARTCARD_SYSTEM = 'http://patientspace.org/sid/smartcard';
const NRC_SYSTEM = 'http://patientspace.org/sid/nrc-zm';

export function smartcardNo(p: Patient): string | undefined {
  return p.identifier?.find((i) => i.system === SMARTCARD_SYSTEM)?.value;
}

export function nrc(p: Patient): string | undefined {
  return p.identifier?.find((i) => i.system === NRC_SYSTEM)?.value;
}

export function displayName(p: Patient): string {
  return p.name?.[0]?.text ?? [p.name?.[0]?.given?.join(' '), p.name?.[0]?.family].filter(Boolean).join(' ') ?? '—';
}

export async function listPatients(): Promise<Patient[]> {
  const r = await fetch('/fhir/Patient');
  if (!r.ok) throw new Error(`listPatients failed: ${r.status}`);
  const body: Bundle<Patient> = await r.json();
  return body.entry?.map((e) => e.resource) ?? [];
}

export async function getPatient(id: string): Promise<Patient> {
  const r = await fetch(`/fhir/Patient/${id}`);
  if (!r.ok) throw new Error(`getPatient ${id} failed: ${r.status}`);
  return r.json();
}
