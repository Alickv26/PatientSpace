<script lang="ts">
  import { page } from '$app/state';
  import { getPatient, displayName, type Patient } from '$lib/api/patient';
  import { onMount } from 'svelte';

  let requestId = $derived(page.url.searchParams.get('request') ?? '');
  let patientId = $derived(page.url.searchParams.get('patient') ?? '');
  let patient = $state<Patient | null>(null);
  let noPatient = $derived(!patientId);

  let quantity = $state('');
  let batchNo = $state('');
  let confirmed = $state(false);
  let submitting = $state(false);
  let dispensed = $state(false);
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
    if (!confirmed || !patientId) return;
    error = ''; submitting = true;
    try {
      const body: Record<string, unknown> = { resourceType: 'MedicationDispense', status: 'completed', whenHandedOver: new Date().toISOString(), subject: { reference: `Patient/${patientId}` } };
      if (requestId) body.authorizingPrescription = { reference: `MedicationRequest/${requestId}` };
      if (quantity) body.quantity = { value: parseInt(quantity), unit: 'tablets', system: 'http://unitsofmeasure.org' };
      if (batchNo) body.batch = { lotNumber: batchNo };
      const resp = await fetch('/fhir/MedicationDispense', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
      if (!resp.ok) throw new Error(`Failed: ${resp.status}`);
      dispensed = true;
      setTimeout(() => { window.location.href = `/patients/${patientId}`; }, 1500);
    } catch (err) { error = err instanceof Error ? err.message : 'Unknown error'; } finally { submitting = false; }
  }
</script>

<div class="flex items-center mb-4 gap-2">
  <a href="/patients"><button>← Back</button></a>
  <h1 style="font-size:20px;font-weight:700;margin:0;flex:1">Pharmacy dispense</h1>
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
          <tr><td><a href="/pharmacy/dispense?patient={p.id}">{displayName(p)}</a></td>
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
    <div class="card"><h2>Dispense details</h2>
      <div class="form-group"><label>Quantity dispensed</label><input type="number" bind:value={quantity} placeholder="30" /></div>
      <div class="form-group"><label>Batch number</label><input bind:value={batchNo} placeholder="e.g., BN-2026-001" style="font-family:monospace" /></div>
    </div>
    <div class="card"><label class="triage-radio" class:selected.low={confirmed}>
      <input type="checkbox" bind:checked={confirmed} style="width:auto;min-height:auto" />
      <span>I confirm the medication and quantity are correct.</span>
    </label></div>
    <button class="primary w-full" type="submit" disabled={!confirmed || submitting} style="min-height:48px">{submitting ? 'Dispensing...' : 'Confirm dispense'}</button>
    {#if dispensed}<p class="muted text-center" style="color:var(--ps-primary)">Dispensed! Redirecting...</p>{/if}
  </form>
{:else}
  <div class="card"><p class="muted">Loading patient...</p></div>
{/if}
