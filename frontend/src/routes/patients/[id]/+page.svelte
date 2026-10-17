<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import {
    getPatient,
    smartcardNo,
    nrc,
    displayName,
    phone,
    hasConflict,
    type Patient,
  } from '$lib/api/patient';

  let patient = $state<Patient | null>(null);
  let loading = $state(true);
  let error = $state('');
  let activeTab = $state<'encounters' | 'observations' | 'medications'>('encounters');

  let encounters = $state<any[]>([]);
  let observations = $state<any[]>([]);
  let medications = $state<any[]>([]);

  let id = $derived(page.params.id);

  async function load() {
    loading = true;
    error = '';
    try {
      patient = await getPatient(id);

      // Fetch ALL encounters, observations, and medications
      // then filter client-side by patient reference.
      // (The backend doesn't yet support ?patient= filtering for generic resources.)
      const [encResp, obsResp, medResp] = await Promise.all([
        fetch(`/fhir/Encounter?_count=500`).catch(() => null),
        fetch(`/fhir/Observation?_count=500`).catch(() => null),
        fetch(`/fhir/MedicationRequest?_count=500`).catch(() => null),
      ]);

      const patientRef = `Patient/${patient!.id}`;

      if (encResp?.ok) {
        const bundle = await encResp.json();
        encounters = (bundle.entry ?? [])
          .map((e: any) => e.resource)
          .filter((e: any) => e.subject?.reference === patientRef);
      }
      if (obsResp?.ok) {
        const bundle = await obsResp.json();
        observations = (bundle.entry ?? [])
          .map((e: any) => e.resource)
          .filter((o: any) => o.subject?.reference === patientRef);
      }
      if (medResp?.ok) {
        const bundle = await medResp.json();
        medications = (bundle.entry ?? [])
          .map((e: any) => e.resource)
          .filter((m: any) => m.subject?.reference === patientRef);
      }
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load patient';
    } finally {
      loading = false;
    }
  }

  onMount(load);

  function encounterReason(e: any): string {
    return e.reasonCode?.[0]?.text ?? e.priority?.coding?.[0]?.display ?? 'General visit';
  }

  function encounterDate(e: any): string {
    const start = e.period?.start;
    if (!start) return '—';
    try { return new Date(start).toLocaleString(); } catch { return start; }
  }

  function observationDisplay(o: any): string {
    const code = o.code?.text ?? o.code?.coding?.[0]?.display ?? 'Unknown';
    // Handle valueQuantity (vitals) vs valueString (notes)
    if (o.valueQuantity) {
      return `${code}: ${o.valueQuantity.value} ${o.valueQuantity.unit ?? ''}`.trim();
    }
    if (o.valueString) {
      return `${code}: ${o.valueString}`;
    }
    // Handle component (clinical notes)
    if (o.component && o.component.length > 0) {
      return o.component.map((c: any) => {
        const label = c.code?.text ?? 'Note';
        const val = c.valueString ?? '—';
        return `${label}: ${val}`;
      }).join(' | ');
    }
    return code;
  }

  function observationDate(o: any): string {
    const dt = o.effectiveDateTime;
    if (!dt) return '—';
    try { return new Date(dt).toLocaleString(); } catch { return dt; }
  }

  function medicationDisplay(m: any): string {
    const med = m.medicationCodeableConcept?.text
      ?? m.medicationCodeableConcept?.coding?.[0]?.display
      ?? 'Unknown medication';
    const dose = m.dosageInstruction?.[0]?.text ?? '';
    return dose ? `${med} (${dose})` : med;
  }

  function medicationDate(m: any): string {
    const dt = m.authoredOn;
    if (!dt) return '—';
    try { return new Date(dt).toLocaleString(); } catch { return dt; }
  }
</script>

<div class="flex items-center mb-4 gap-2">
  <a href="/patients"><button>← Back</button></a>
</div>

{#if loading}
  <div class="card"><p class="muted">Loading...</p></div>
{:else if error}
  <div class="card" style="border-color:var(--ps-danger);color:var(--ps-danger)">{error}</div>
{:else if patient}
  <div class="card">
    <h1 style="font-size:22px;font-weight:700;margin:0 0 4px">{displayName(patient)}</h1>
    <p class="muted" style="margin:0">
      {patient.gender ?? '—'} · {patient.birthDate ?? 'DOB unknown'}
    </p>
  </div>

  {#if hasConflict(patient)}
    <div class="card" style="border-color:var(--ps-accent);background:var(--ps-accent-bg)">
      <strong>⚠️ Conflict detected</strong> — this patient record has concurrent
      edits from multiple facilities. Review in the
      <a href="/conflicts">conflict queue</a>.
    </div>
  {/if}

  <!-- Clinical actions -->
  <div class="card">
    <h2>Clinical actions</h2>
    <div class="form-grid cols-2">
      <a href="/encounters/triage?patient={patient.id}"><button class="primary">🩺 Start triage</button></a>
      <a href="/encounters/consultation?patient={patient.id}"><button class="primary">📋 New consultation</button></a>
      <a href="/encounters/prescribe?patient={patient.id}"><button>✍️ Prescribe medication</button></a>
      <a href="/pharmacy/dispense?patient={patient.id}"><button>📦 Pharmacy dispense</button></a>
    </div>
  </div>

  <div class="card">
    <h2>Demographics</h2>
    <div class="form-grid cols-2">
      <div>
        <div class="muted" style="font-size:11px;text-transform:uppercase">Smartcard</div>
        <div style="font-size:15px">{smartcardNo(patient) ?? '—'}</div>
      </div>
      <div>
        <div class="muted" style="font-size:11px;text-transform:uppercase">NRC</div>
        <div style="font-size:15px">{nrc(patient) ?? '—'}</div>
      </div>
      <div>
        <div class="muted" style="font-size:11px;text-transform:uppercase">Phone</div>
        <div style="font-size:15px">{phone(patient) ?? '—'}</div>
      </div>
      <div>
        <div class="muted" style="font-size:11px;text-transform:uppercase">Date of birth</div>
        <div style="font-size:15px">{patient.birthDate ?? '—'}</div>
      </div>
      <div>
        <div class="muted" style="font-size:11px;text-transform:uppercase">Gender</div>
        <div style="font-size:15px">{patient.gender ?? '—'}</div>
      </div>
      <div>
        <div class="muted" style="font-size:11px;text-transform:uppercase">Patient ID</div>
        <div style="font-family:monospace;font-size:12px;word-break:break-all">{patient.id}</div>
      </div>
    </div>
  </div>

  <div class="card">
    <div class="tabs">
      <button class="tab" class:active={activeTab === 'encounters'} onclick={() => (activeTab = 'encounters')}>
        Encounters ({encounters.length})
      </button>
      <button class="tab" class:active={activeTab === 'observations'} onclick={() => (activeTab = 'observations')}>
        Observations ({observations.length})
      </button>
      <button class="tab" class:active={activeTab === 'medications'} onclick={() => (activeTab = 'medications')}>
        Medications ({medications.length})
      </button>
    </div>

    {#if activeTab === 'encounters'}
      {#if encounters.length > 0}
        <div class="table-wrap">
          <table class="data">
            <thead><tr><th>Date</th><th>Reason / Category</th><th>Status</th></tr></thead>
            <tbody>
              {#each encounters as e (e.id)}
                <tr>
                  <td>{encounterDate(e)}</td>
                  <td>{encounterReason(e)}</td>
                  <td>{e.status ?? '—'}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <p class="muted">No encounters recorded yet.</p>
      {/if}
    {:else if activeTab === 'observations'}
      {#if observations.length > 0}
        <div class="table-wrap">
          <table class="data">
            <thead><tr><th>Date</th><th>Observation</th></tr></thead>
            <tbody>
              {#each observations as o (o.id)}
                <tr>
                  <td>{observationDate(o)}</td>
                  <td>{observationDisplay(o)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <p class="muted">No observations recorded yet.</p>
      {/if}
    {:else if activeTab === 'medications'}
      {#if medications.length > 0}
        <div class="table-wrap">
          <table class="data">
            <thead><tr><th>Date</th><th>Medication</th><th>Status</th></tr></thead>
            <tbody>
              {#each medications as m (m.id)}
                <tr>
                  <td>{medicationDate(m)}</td>
                  <td>{medicationDisplay(m)}</td>
                  <td>{m.status ?? '—'}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <p class="muted">No medications prescribed yet.</p>
      {/if}
    {/if}
  </div>
{/if}
