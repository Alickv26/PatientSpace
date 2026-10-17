<script lang="ts">
  import { page } from '$app/state';
  import { getPatient, displayName, type Patient } from '$lib/api/patient';
  import { onMount } from 'svelte';

  let patientId = $derived(page.url.searchParams.get('patient') ?? '');
  let encounterId = $derived(page.url.searchParams.get('encounter') ?? '');
  let patient = $state<Patient | null>(null);
  let noPatient = $derived(!patientId);

  let medication = $state('');
  let atcCode = $state('');
  let dose = $state('');
  let frequency = $state('');
  let duration = $state('');
  let submitting = $state(false);
  let saved = $state(false);
  let error = $state('');
  let searchQuery = $state('');
  let searchResults = $state<Patient[]>([]);
  let searching = $state(false);

  onMount(async () => { if (patientId) { try { patient = await getPatient(patientId); } catch { patient = null; } } });

  async function searchPatients() {
    if (!searchQuery.trim()) { searchResults = []; return; }
    searching = true;
    try {
      const resp = await fetch(`/fhir/Patient?identifier=${encodeURIComponent(searchQuery)}&_count=20`);
      if (resp.ok) { const bundle = await resp.json(); searchResults = bundle.entry?.map((e: any) => e.resource) ?? []; }
    } catch { searchResults = []; } finally { searching = false; }
  }

  async function submit(e: Event) {
    e.preventDefault();
    if (!patientId) { error = 'No patient selected.'; return; }
    error = ''; submitting = true;
    try {
      const body: Record<string, unknown> = { resourceType: 'MedicationRequest', status: 'active', intent: 'order', authoredOn: new Date().toISOString(), subject: { reference: `Patient/${patientId}` } };
      if (encounterId) body.encounter = { reference: `Encounter/${encounterId}` };
      if (medication || atcCode) {
        const medCode: Record<string, unknown> = {};
        if (atcCode) medCode.coding = [{ system: 'http://www.whocc.no/atc', code: atcCode, display: medication || atcCode }];
        medCode.text = medication || atcCode;
        body.medicationCodeableConcept = medCode;
      }
      const dosageInstruction: Record<string, unknown> = {};
      if (dose) dosageInstruction.text = dose;
      if (frequency) dosageInstruction.timing = { repeat: { code: { coding: [{ system: 'http://terminology.hl7.org/CodeSystem/v3-GTSAbbreviation', code: frequency }] } } };
      if (duration) { dosageInstruction.timing = dosageInstruction.timing || {}; if (dosageInstruction.timing) { dosageInstruction.timing.repeat = dosageInstruction.timing.repeat || {}; dosageInstruction.timing.repeat.boundsDuration = { value: parseInt(duration), unit: 'd', code: 'd' }; } }
      if (Object.keys(dosageInstruction).length > 0) body.dosageInstruction = [dosageInstruction];

      const resp = await fetch('/fhir/MedicationRequest', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
      if (!resp.ok) throw new Error(`Failed: ${resp.status}`);
      saved = true;
      setTimeout(() => { window.location.href = `/patients/${patientId}`; }, 1500);
    } catch (err) { error = err instanceof Error ? err.message : 'Unknown error'; } finally { submitting = false; }
  }
</script>

<div class="flex items-center mb-4 gap-2">
  <a href="/patients"><button>← Back</button></a>
  <h1 style="font-size:20px;font-weight:700;margin:0;flex:1">Prescribe</h1>
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
      <div class="table-wrap"><table class="data"><thead><tr><th>Name</th><th>Smartcard</th><th>DOB</th></tr></thead><tbody>
        {#each searchResults as p (p.id)}
          <tr><td><a href="/encounters/prescribe?patient={p.id}">{displayName(p)}</a></td>
          <td>{p.identifier?.find((i: any) => i.system?.includes('smartcard'))?.value ?? '—'}</td>
          <td>{p.birthDate ?? '—'}</td></tr>
        {/each}
      </tbody></table></div>
    {:else if searchQuery && !searching}<p class="muted">No patients found.</p>{/if}
  </div>
{:else if patient}
  <div class="card" style="background:var(--ps-bg)"><strong>{displayName(patient)}</strong><span class="muted"> · {patient.gender ?? '—'} · {patient.birthDate ?? 'DOB unknown'}</span></div>
  {#if error}<div class="card" style="border-color:var(--ps-danger);color:var(--ps-danger)">{error}</div>{/if}
  <form onsubmit={submit} style="display:grid;gap:16px">
    <div class="card"><h2>Medication</h2>
      <div class="form-group"><label>Medication (name)</label><input bind:value={medication} placeholder="e.g., Amlodipine 5mg" /></div>
      <div class="form-group"><label>ATC code</label><input bind:value={atcCode} placeholder="e.g., C08CA01" style="font-family:monospace" /></div>
    </div>
    <div class="card"><h2>Dosage</h2>
      <div class="form-grid cols-3">
        <div class="form-group"><label>Dose</label><input bind:value={dose} placeholder="1 tablet" /></div>
        <div class="form-group"><label>Frequency</label><select bind:value={frequency}><option value="">—</option><option value="OD">Once daily (OD)</option><option value="BD">Twice daily (BD)</option><option value="TDS">Three times daily (TDS)</option><option value="QDS">Four times daily (QDS)</option><option value="PRN">As needed (PRN)</option></select></div>
        <div class="form-group"><label>Duration (days)</label><input type="number" bind:value={duration} placeholder="30" /></div>
      </div>
    </div>
    <button class="primary w-full" type="submit" disabled={submitting} style="min-height:48px">{submitting ? 'Prescribing...' : 'Prescribe'}</button>
    {#if saved}<p class="muted text-center" style="color:var(--ps-primary)">Prescribed! Redirecting...</p>{/if}
  </form>
{:else}
  <div class="card"><p class="muted">Loading patient...</p></div>
{/if}
