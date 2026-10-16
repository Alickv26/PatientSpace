#!/usr/bin/env bun
/**
 * Seed script for PatientSpace — generates synthetic patients, encounters,
 * and observations via the FHIR API.
 *
 * Usage:
 *   bun run scripts/seed.ts                    # seeds 50 patients + encounters + observations
 *   bun run scripts/seed.ts --count 100        # seed 100 patients
 *   bun run scripts/seed.ts --url http://core:8080  # seed a different host
 */

const CORE_URL = process.argv.includes('--url')
  ? process.argv[process.argv.indexOf('--url') + 1]
  : 'http://localhost:8080';

const COUNT = process.argv.includes('--count')
  ? parseInt(process.argv[process.argv.indexOf('--count') + 1], 10)
  : 50;

// ---- Synthetic data pools ----

const SURNAMES = [
  'Banda', 'Phiri', 'Tembo', 'Mwale', 'Zulu', 'Nyirenda', 'Chirwa',
  'Lungu', 'Mumba', 'Sakala', 'Chisanga', 'Kabwe', 'Musonda', 'Mwiinga',
];

const GIVEN_NAMES = [
  'Chipo', 'John', 'Mary', 'Emmanuel', 'Grace', 'David', 'Ruth', 'Joseph',
  'Esther', 'Daniel', 'Sarah', 'Peter', 'Anna', 'Moses', 'Joyce', 'Andrew',
];

const DISTRICTS = [
  'Petauke', 'Nyimba', 'Mambwe', 'Katete', 'Sinda', 'Lusangazi', 'Vubwi',
];

const GENDERS: Array<'female' | 'male'> = ['female', 'male'];

const COMPLAINTS = [
  'Headache and fever for 3 days',
  'Cough with productive sputum',
  'Abdominal pain and diarrhea',
  'Hypertension follow-up',
  'Antenatal visit — 28 weeks',
  'HIV refill — ART',
  'Back pain — chronic',
  'Malaria suspected',
  'Child immunization',
  'Diabetes follow-up',
];

const DIAGNOSES = [
  { text: 'Essential hypertension', code: 'I10' },
  { text: 'Type 2 diabetes mellitus', code: 'E11' },
  { text: 'Acute upper respiratory infection', code: 'J06' },
  { text: 'Malaria', code: 'B54' },
  { text: 'HIV infection', code: 'B20' },
  { text: 'Normal pregnancy', code: 'Z34' },
];

const LOINC_CODES = [
  { code: '85354-9', display: 'Blood pressure' },
  { code: '8867-4', display: 'Heart rate' },
  { code: '8310-5', display: 'Body temperature' },
  { code: '29463-7', display: 'Body weight' },
  { code: '4544-3', display: 'Hematocrit' },
  { code: '2345-7', display: 'Glucose' },
];

// ---- Helpers ----

function randomChoice<T>(arr: T[]): T {
  return arr[Math.floor(Math.random() * arr.length)];
}

function randomDate(startYear: number, endYear: number): string {
  const year = startYear + Math.floor(Math.random() * (endYear - startYear + 1));
  const month = 1 + Math.floor(Math.random() * 12);
  const day = 1 + Math.floor(Math.random() * 28);
  return `${year}-${String(month).padStart(2, '0')}-${String(day).padStart(2, '0')}`;
}

function randomSmartcard(): string {
  const num = 10000000 + Math.floor(Math.random() * 89999999);
  return `SC-${num}`;
}

function randomNRC(): string {
  const part1 = 100000 + Math.floor(Math.random() * 899999);
  const part2 = 10 + Math.floor(Math.random() * 89);
  const part3 = Math.floor(Math.random() * 9) + 1;
  return `${part1}/${part2}/${part3}`;
}

function randomPhone(): string {
  const num = 970000000 + Math.floor(Math.random() * 29999999);
  return `+260 ${num}`;
}

// ---- FHIR builders ----

function buildPatient(): Record<string, unknown> {
  const given = randomChoice(GIVEN_NAMES);
  const family = randomChoice(SURNAMES);
  const gender = randomChoice(GENDERS);
  const birthDate = randomDate(1940, 2020);
  const smartcard = randomSmartcard();
  const nrc = randomNRC();
  const phone = randomPhone();
  const district = randomChoice(DISTRICTS);

  return {
    resourceType: 'Patient',
    identifier: [
      { system: 'http://patientspace.org/sid/smartcard', value: smartcard },
      { system: 'http://patientspace.org/sid/nrc-zm', value: nrc },
    ],
    name: [
      { text: `${family}, ${given}`, given: [given], family, use: 'official' },
    ],
    telecom: [
      { system: 'phone', value: phone, use: 'mobile' },
    ],
    gender,
    birthDate,
    address: [
      { district, city: district, country: 'Zambia' },
    ],
  };
}

function buildEncounter(patientId: string): Record<string, unknown> {
  const complaint = randomChoice(COMPLAINTS);
  const diagnosis = randomChoice(DIAGNOSES);
  const now = new Date().toISOString();

  return {
    resourceType: 'Encounter',
    status: 'finished',
    class: { system: 'http://terminology.hl7.org/CodeSystem/v3-ActCode', code: 'AMB', display: 'ambulatory' },
    subject: { reference: `Patient/${patientId}` },
    period: { start: now },
    reasonCode: [{ text: complaint }],
    diagnosis: [{
      condition: { display: diagnosis.text },
      use: { coding: [{ system: 'http://terminology.hl7.org/CodeSystem/diagnosis-role', code: 'AD' }] },
    }],
  };
}

function buildObservation(patientId: string, encounterId: string): Record<string, unknown> {
  const loinc = randomChoice(LOINC_CODES);
  const value = 30 + Math.floor(Math.random() * 170);
  const unit = loinc.code === '85354-9' ? 'mmHg' : loinc.code === '8867-4' ? 'bpm' : loinc.code === '8310-5' ? '°C' : loinc.code === '29463-7' ? 'kg' : '%';

  return {
    resourceType: 'Observation',
    status: 'final',
    code: {
      coding: [{ system: 'http://loinc.org', code: loinc.code, display: loinc.display }],
      text: loinc.display,
    },
    subject: { reference: `Patient/${patientId}` },
    encounter: { reference: `Encounter/${encounterId}` },
    effectiveDateTime: new Date().toISOString(),
    valueQuantity: { value, unit, system: 'http://unitsofmeasure.org' },
  };
}

// ---- API helpers ----

async function postFhir(resourceType: string, body: Record<string, unknown>): Promise<string | null> {
  try {
    const resp = await fetch(`${CORE_URL}/fhir/${resourceType}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (resp.ok) {
      const json = await resp.json();
      return json.id;
    }
    return null;
  } catch {
    return null;
  }
}

// ---- Main ----

async function main() {
  console.log(`Seeding ${COUNT} patients + encounters + observations to ${CORE_URL}...`);

  let patientsCreated = 0;
  let encountersCreated = 0;
  let observationsCreated = 0;
  let failed = 0;

  for (let i = 0; i < COUNT; i++) {
    // 1. Create patient
    const patientId = await postFhir('Patient', buildPatient());
    if (!patientId) {
      failed++;
      continue;
    }
    patientsCreated++;

    // 2. Create encounter for this patient
    const encounterId = await postFhir('Encounter', buildEncounter(patientId));
    if (encounterId) {
      encountersCreated++;

      // 3. Create 2-4 observations for this encounter
      const obsCount = 2 + Math.floor(Math.random() * 3);
      for (let j = 0; j < obsCount; j++) {
        const obsId = await postFhir('Observation', buildObservation(patientId, encounterId));
        if (obsId) {
          observationsCreated++;
        }
      }
    }

    if ((i + 1) % 10 === 0) {
      console.log(`  ${i + 1}/${COUNT}... (${patientsCreated} patients, ${encountersCreated} encounters, ${observationsCreated} observations)`);
    }
  }

  console.log(`\nDone!`);
  console.log(`  Patients:      ${patientsCreated} created, ${failed} failed`);
  console.log(`  Encounters:    ${encountersCreated} created`);
  console.log(`  Observations:  ${observationsCreated} created`);
  console.log(`\nVerify:`);
  console.log(`  curl ${CORE_URL}/health`);
  console.log(`  curl ${CORE_URL}/fhir/Patient`);
  console.log(`  curl ${CORE_URL}/fhir/Encounter`);
  console.log(`  curl ${CORE_URL}/fhir/Observation`);
}

main();
