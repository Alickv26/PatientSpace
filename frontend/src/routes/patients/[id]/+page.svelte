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

      // Fetch encounters, observations, and medications for this patient
      const [encResp, obsResp, medResp] = await Promise.all([
        fetch(`/fhir/Encounter?_count=100`).catch(() => null),
        fetch(`/fhir/Observation?_count=100`).catch(() => null),
        fetch(`/fhir/MedicationRequest?_count=100`).catch(() => null),
      ]);

      if (encResp?.ok) {
        const bundle = await encResp.json();
        encounters = (bundle.entry ?? []).map((e: any) => e.resource);
      }
      if (obsResp?.ok) {
        const bundle = await obsResp.json();
        observations = (bundle.entry ?? []).map((e: any) => e.resource);
      }
      if (medResp?.ok) {
        const bundle = await medResp.json();
        medications = (bundle.entry ?? []).map((e: any) => e.resource);
      }
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load patient';
    } finally {
      loading = false;
    }
  }

  onMount(load);

  function encounterReason(e: any): string {
    return e.reasonCode?.[0]?.text ?? '—';
  }

  function encounterDate(e: any): string {
    return e.period?.start ? new Date(e.period.start).toLocaleDateString() : '—';
  }

  function observationDisplay(o: any): string {
    const code = o.code?.text ?? o.code?.coding?.[0]?.display ?? '—';
    const value = o.valueQuantity?.value ?? o.valueString ?? '—';
    const unit = o.valueQuantity?.unit ?? '';
    return `${code}: ${value} ${unit}`.trim();
  }

  function observationDate(o: any): string {
    return o.effectiveDateTime ? new Date(o.effectiveDateTime).toLocaleDateString() : '—';
  }

  function medicationDisplay(m: any): string {
    return m.medicationCodeableConcept?.text ?? m.medicationCodeableConcept?.coding?.[0]?.display ?? '—';
  }
</script>

<div class="flex items-center mb-4 gap-2">
  <a href="/patients"><button>← Back</button></a>
</div>

{#if loading}
  <div class="card"><p class="muted">Loading...</p></div>
{:else if error}
  <div class="card" style="border-color:var(--ps-danger);color:var(--ps-danger)">
    {error}
  </div>
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
            <thead>
              <tr><th>Date</th><th>Reason</th><th>Status</th></tr>
            </thead>
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
      <a href="/encounters/triage?patient={patient.id}">
        <button class="primary mt-2">Start triage encounter</button>
      </a>
    {:else if activeTab === 'observations'}
      {#if observations.length > 0}
        <div class="table-wrap">
          <table class="data">
            <thead>
              <tr><th>Date</th><th>Observation</th></tr>
            </thead>
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
            <thead>
              <tr><th>Medication</th><th>Status</th></tr>
            </thead>
            <tbody>
              {#each medications as m (m.id)}
                <tr>
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
      <a href="/encounters/prescribe?encounter={patient.id}">
        <button class="primary mt-2">Prescribe medication</button>
      </a>
    {/if}
  </div>
{/if}
