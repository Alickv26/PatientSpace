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

  let id = $derived(page.params.id);

  async function load() {
    loading = true;
    error = '';
    try {
      patient = await getPatient(id);
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load patient';
    } finally {
      loading = false;
    }
  }

  onMount(load);
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
        Encounters
      </button>
      <button class="tab" class:active={activeTab === 'observations'} onclick={() => (activeTab = 'observations')}>
        Observations
      </button>
      <button class="tab" class:active={activeTab === 'medications'} onclick={() => (activeTab = 'medications')}>
        Medications
      </button>
    </div>

    {#if activeTab === 'encounters'}
      <p class="muted">No encounters recorded yet.</p>
      <a href="/encounters/triage?patient={patient.id}">
        <button class="primary mt-2">Start triage encounter</button>
      </a>
    {:else if activeTab === 'observations'}
      <p class="muted">No observations recorded yet.</p>
    {:else if activeTab === 'medications'}
      <p class="muted">No medications prescribed yet.</p>
    {/if}
  </div>
{/if}
