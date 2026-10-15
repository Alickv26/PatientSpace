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
  let error = $state<string | null>(null);
  let activeTab = $state<'encounters' | 'observations' | 'medications'>(
    'encounters',
  );

  // SvelteKit 3: page state is reactive. page.params.id updates on navigation.
  let id = $derived(page.params.id);

  async function load(currentId: string) {
    if (!currentId) {
      error = 'No patient id in URL.';
      loading = false;
      return;
    }
    loading = true;
    error = null;
    try {
      patient = await getPatient(currentId);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      patient = null;
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    load(id);
  });

  // Re-load when the route param changes (e.g. nav between patients).
  $effect(() => {
    if (id) load(id);
  });
</script>

<svelte:head>
  <title>{patient ? displayName(patient) : 'Patient'} · PatientSpace</title>
</svelte:head>

{#if loading}
  <p class="muted">Loading patient…</p>
{:else if error}
  <div class="error-banner">{error}</div>
  <p style="margin-top:12px"><a href="/patients">← Back to patient list</a></p>
{:else if patient}
  <h1 style="font-size:22px;font-weight:700;margin:0 0 4px">{displayName(patient)}</h1>
  <p class="muted" style="margin:0 0 12px">
    {patient.gender ?? 'unknown'}
    {#if patient.birthDate}· DOB {patient.birthDate}{/if}
    · ID <code>{patient.id}</code>
  </p>

  {#if hasConflict(patient)}
    <div class="conflict-banner">
      <strong>Conflict detected.</strong>
      This patient has concurrent versions in the local log and needs reconciliation.
    </div>
  {/if}

  <div class="card">
    <h2>Demographics</h2>
    <dl class="demo-grid">
      <dt>Smartcard No.</dt>
      <dd>{smartcardNo(patient) ?? '—'}</dd>
      <dt>NRC</dt>
      <dd>{nrc(patient) ?? '—'}</dd>
      <dt>Mobile phone</dt>
      <dd>{phone(patient) ?? '—'}</dd>
      <dt>Date of birth</dt>
      <dd>{patient.birthDate ?? '—'}</dd>
      <dt>Gender</dt>
      <dd>{patient.gender ?? '—'}</dd>
      <dt>Patient ID</dt>
      <dd><code>{patient.id}</code></dd>
    </dl>
  </div>

  <div class="card">
    <div class="tabs">
      <button
        type="button"
        class:active={activeTab === 'encounters'}
        onclick={() => (activeTab = 'encounters')}
      >
        Encounters
      </button>
      <button
        type="button"
        class:active={activeTab === 'observations'}
        onclick={() => (activeTab = 'observations')}
      >
        Observations
      </button>
      <button
        type="button"
        class:active={activeTab === 'medications'}
        onclick={() => (activeTab = 'medications')}
      >
        Medications
      </button>
    </div>

    {#if activeTab === 'encounters'}
      <div style="padding-top:12px">
        <p class="muted" style="margin:0 0 12px">
          No encounters recorded yet for this patient.
        </p>
        <a href={`/encounters/triage?patient=${encodeURIComponent(patient.id)}`}>
          <button class="primary" type="button">+ Start triage encounter</button>
        </a>
      </div>
    {:else if activeTab === 'observations'}
      <div style="padding-top:12px">
        <p class="muted">No observations recorded yet. Vitals captured during triage will appear here.</p>
      </div>
    {:else if activeTab === 'medications'}
      <div style="padding-top:12px">
        <p class="muted">No medications prescribed yet.</p>
        <a href={`/encounters/prescribe?patient=${encodeURIComponent(patient.id)}`}>
          <button type="button">+ Prescribe medication</button>
        </a>
      </div>
    {/if}
  </div>

  <p style="margin-top:12px">
    <a href="/patients">← Back to patient list</a>
  </p>
{/if}

<style>
  .error-banner {
    background: #fef2f2;
    border: 1px solid var(--ps-danger);
    color: var(--ps-danger);
    padding: 8px 12px;
    border-radius: 4px;
    font-size: 13px;
  }
  .conflict-banner {
    background: #fef3c7;
    border: 1px solid var(--ps-accent);
    color: #78350f;
    padding: 10px 14px;
    border-radius: 4px;
    font-size: 13px;
    margin-bottom: 16px;
  }
  .demo-grid {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 8px 16px;
    margin: 0;
    font-size: 13px;
  }
  .demo-grid dt {
    color: var(--ps-muted);
    text-transform: uppercase;
    font-size: 11px;
    letter-spacing: 0.06em;
    align-self: center;
  }
  .demo-grid dd {
    margin: 0;
    align-self: center;
  }
  .tabs {
    display: flex;
    gap: 0;
    border-bottom: 1px solid var(--ps-border);
    margin: -16px -16px 0;
    padding: 0 8px;
  }
  .tabs button {
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    padding: 10px 14px;
    font-size: 13px;
    color: var(--ps-muted);
    cursor: pointer;
  }
  .tabs button:hover {
    color: var(--ps-primary);
    border-color: var(--ps-border);
  }
  .tabs button.active {
    color: var(--ps-primary);
    border-bottom-color: var(--ps-primary);
    font-weight: 600;
  }
</style>
