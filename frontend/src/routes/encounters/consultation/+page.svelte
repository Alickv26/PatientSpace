<script lang="ts">
  import { page } from '$app/state';
  import { getPatient, displayName, type Patient } from '$lib/api/patient';
  import { onMount } from 'svelte';

  let patient = $state<Patient | null>(null);
  let patientId = $derived(page.url.searchParams.get('patient') ?? '');
  let noPatient = $derived(!patientId);

  let diagnosisText = $state('');
  let icd10Code = $state('');
  let notes = $state('');
  let plan = $state('');
  let submitting = $state(false);
  let saved = $state(false);
  let error = $state('');

  let searchQuery = $state('');
  let searchResults = $state<Patient[]>([]);
  let searching = $state(false);

  onMount(async () => {
    if (patientId) {
      try { patient = await getPatient(patientId); } catch { patient = null; }
    }
  });

  async function searchPatients() {
    if (!searchQuery.trim()) { searchResults = []; return; }
    searching = true;
    try {
      const resp = await fetch(`/fhir/Patient?identifier=${encodeURIComponent(searchQuery)}&_count=20`);
      if (resp.ok) {
        const bundle = await resp.json();
        searchResults = bundle.entry?.map((e: any) => e.resource) ?? [];
      }
    } catch { searchResults = []; } finally { searching = false; }
  }

  async function postFhir(type: string, body: Record<string, unknown>): Promise<string | null> {
    const resp = await fetch(`/fhir/${type}`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body),
    });
    if (resp.ok) { const json = await resp.json(); return json.id; }
    return null;
  }

  async function submit(e: Event) {
    e.preventDefault();
    if (!patient || !patientId) { error = 'No patient selected.'; return; }
    error = ''; submitting = true;
    try {
      const encounterId = await postFhir('Encounter', {
        resourceType: 'Encounter', status: 'finished',
        class: { system: 'http://terminology.hl7.org/CodeSystem/v3-ActCode', code: 'AMB', display: 'ambulatory' },
        subject: { reference: `Patient/${patientId}` },
        period: { start: new Date().toISOString() },
      });
      if (!encounterId) throw new Error('Failed to create encounter');

      if (diagnosisText || icd10Code) {
        const condition: Record<string, unknown> = {
          resourceType: 'Condition',
          clinicalStatus: { coding: [{ system: 'http://terminology.hl7.org/CodeSystem/condition-clinical', code: 'active' }] },
          subject: { reference: `Patient/${patientId}` },
          encounter: { reference: `Encounter/${encounterId}` },
        };
        if (icd10Code) {
          condition.code = { coding: [{ system: 'http://hl7.org/fhir/sid/icd-10', code: icd10Code, display: diagnosisText || icd10Code }], text: diagnosisText || icd10Code };
        } else { condition.code = { text: diagnosisText }; }
        await postFhir('Condition', condition);
      }

      if (notes || plan) {
        await postFhir('Observation', {
          resourceType: 'Observation', status: 'final',
          code: { coding: [{ system: 'http://patientspace.org/loinc', code: 'clinical-notes', display: 'Clinical notes' }], text: 'Clinical notes' },
          subject: { reference: `Patient/${patientId}` }, encounter: { reference: `Encounter/${encounterId}` },
          effectiveDateTime: new Date().toISOString(),
          component: [notes ? { code: { text: 'Notes' }, valueString: notes } : null, plan ? { code: { text: 'Plan' }, valueString: plan } : null].filter(Boolean),
        });
      }

      saved = true;
      setTimeout(() => { window.location.href = `/patients/${patientId}`; }, 1500);
    } catch (err) { error = err instanceof Error ? err.message : 'Unknown error'; }
    finally { submitting = false; }
  }
</script>

<div class="flex items-center mb-4 gap-2">
  <a href="/patients"><button>← Back</button></a>
  <h1 style="font-size:20px;font-weight:700;margin:0;flex:1">Consultation</h1>
</div>

{#if noPatient}
  <div class="card">
    <h2>Select a patient</h2>
    <p class="muted mb-4" style="font-size:14px">Search by Smartcard number or NRC.</p>
    <div class="flex gap-2 mb-4">
      <input bind:value={searchQuery} oninput={() => setTimeout(searchPatients, 300)} placeholder="Search..." style="flex:1" />
      <button onclick={searchPatients} disabled={searching}>{searching ? '...' : 'Search'}</button>
    </div>
    {#if searchResults.length > 0}
      <div class="table-wrap">
        <table class="data"><thead><tr><th>Name</th><th>Smartcard</th><th>Gender</th><th>DOB</th></tr></thead>
        <tbody>
          {#each searchResults as p (p.id)}
            <tr><td><a href="/encounters/consultation?patient={p.id}">{displayName(p)}</a></td>
            <td>{p.identifier?.find((i: any) => i.system?.includes('smartcard'))?.value ?? '—'}</td>
            <td>{p.gender ?? '—'}</td><td>{p.birthDate ?? '—'}</td></tr>
          {/each}
        </tbody></table>
      </div>
    {:else if searchQuery && !searching}<p class="muted">No patients found.</p>{/if}
  </div>
{:else if patient}
  <div class="card" style="background:var(--ps-bg)">
    <strong>{displayName(patient)}</strong>
    <span class="muted"> · {patient.gender ?? '—'} · {patient.birthDate ?? 'DOB unknown'}</span>
  </div>

  {#if error}<div class="card" style="border-color:var(--ps-danger);color:var(--ps-danger)">{error}</div>{/if}

  <form onsubmit={submit} style="display:grid;gap:16px">
    <div class="card">
      <h2>Diagnosis</h2>
      <div class="form-group"><label>Diagnosis (text)</label><input bind:value={diagnosisText} placeholder="e.g., Essential hypertension" /></div>
      <div class="form-group"><label>ICD-10 code</label><input bind:value={icd10Code} placeholder="e.g., I10" style="font-family:monospace" /></div>
    </div>
    <div class="card">
      <h2>Notes</h2>
      <div class="form-group"><label>Clinical notes</label><textarea bind:value={notes} rows="4" placeholder="Examination findings, history..."></textarea></div>
      <div class="form-group"><label>Plan</label><textarea bind:value={plan} rows="3" placeholder="Treatment plan, follow-up..."></textarea></div>
    </div>
    <button class="primary w-full" type="submit" disabled={submitting} style="min-height:48px">
      {submitting ? 'Saving...' : 'Save consultation'}
    </button>
    {#if saved}<p class="muted text-center" style="color:var(--ps-primary)">Saved! Redirecting to patient...</p>{/if}
  </form>
{:else}
  <div class="card"><p class="muted">Loading patient...</p></div>
{/if}
