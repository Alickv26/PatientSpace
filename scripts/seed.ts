#!/usr/bin/env bun
/**
 * Seed script for PatientSpace — generates synthetic patients via the FHIR API.
 *
 * Usage:
 *   bun run scripts/seed.ts                    # seeds 50 patients to localhost:8080
 *   bun run scripts/seed.ts --count 100        # seed 100 patients
 *   bun run scripts/seed.ts --url http://core:8080  # seed a different host
 *
 * Idempotent: running twice creates duplicate patients (different resource IDs).
 * This is intentional — the seed script is for demos, not production data.
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

// ---- FHIR Patient builder ----

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

// ---- Main ----

async function main() {
  console.log(`Seeding ${COUNT} patients to ${CORE_URL}...`);

  let created = 0;
  let failed = 0;

  for (let i = 0; i < COUNT; i++) {
    const patient = buildPatient();

    try {
      const resp = await fetch(`${CORE_URL}/fhir/Patient`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(patient),
      });

      if (resp.ok) {
        created++;
        if ((i + 1) % 10 === 0) {
          console.log(`  ${i + 1}/${COUNT}...`);
        }
      } else {
        failed++;
        const errText = await resp.text();
        console.error(`  Failed to create patient ${i + 1}: ${resp.status} ${errText}`);
      }
    } catch (err) {
      failed++;
      console.error(`  Error creating patient ${i + 1}: ${err}`);
    }
  }

  console.log(`\nDone: ${created} created, ${failed} failed.`);
  console.log(`Verify: curl ${CORE_URL}/health`);
  console.log(`List:   curl ${CORE_URL}/fhir/Patient`);
}

main();
