<script lang="ts">
  import { page } from '$app/state';
  import { getPatient, displayName, type Patient } from '$lib/api/patient';
  import { onMount } from 'svelte';

  let patient = $state<Patient | null>(null);
  let patientId = $derived(page.url.searchParams.get('patient') ?? '');

  let bpSystolic = $state('');
  let bpDiastolic = $state('');
  let heartRate = $state('');
  let temperature = $state('');
  let weight = $state('');
  let complaint = $state('');
  let triageCategory = $state<'emergency' | 'urgent' | 'standard' | 'low'>('standard');
  let saved = $state(false);

  onMount(async () => {
    if (patientId) {
      try {
        patient = await getPatient(patientId);
      } catch {
        patient = null;
      }
    }
  });

  function submit(e: Event) {
    e.preventDefault();
    saved = true;
    setTimeout(() => (saved = false), 3000);
  }

  const categories = [
    { value: 'emergency', label: 'Emergency', desc: 'Red — immediate', cls: 'emergency' },
    { value: 'urgent', label: 'Urgent', desc: 'Orange — under 10 min', cls: 'urgent' },
    { value: 'standard', label: 'Standard', desc: 'Yellow — under 1 hr', cls: 'standard' },
    { value: 'low', label: 'Low', desc: 'Green — walk-in', cls: 'low' },
  ] as const;
</script>

<div class="flex items-center mb-4 gap-2">
  <a href="/patients"><button>← Back</button></a>
  <h1 style="font-size:20px;font-weight:700;margin:0;flex:1">Triage</h1>
</div>

{#if patient}
  <div class="card" style="background:var(--ps-bg)">
    <strong>{displayName(patient)}</strong>
    <span class="muted"> · {patient.gender ?? '—'} · {patient.birthDate ?? 'DOB unknown'}</span>
  </div>
{/if}

<form onsubmit={submit} style="display:grid;gap:16px">
  <!-- Vitals -->
  <div class="card">
    <h2>Vitals</h2>
    <div class="form-grid cols-4">
      <div class="form-group">
        <label>BP Systolic (mmHg)</label>
        <input type="number" bind:value={bpSystolic} placeholder="120" />
      </div>
      <div class="form-group">
        <label>BP Diastolic (mmHg)</label>
        <input type="number" bind:value={bpDiastolic} placeholder="80" />
      </div>
      <div class="form-group">
        <label>Heart rate (bpm)</label>
        <input type="number" bind:value={heartRate} placeholder="72" />
      </div>
      <div class="form-group">
        <label>Temperature (°C)</label>
        <input type="number" step="0.1" bind:value={temperature} placeholder="36.8" />
      </div>
    </div>
    <div class="form-group">
      <label>Weight (kg)</label>
      <input type="number" step="0.1" bind:value={weight} placeholder="65" />
    </div>
  </div>

  <!-- Complaint -->
  <div class="card">
    <h2>Presenting complaint</h2>
    <div class="form-group">
      <textarea bind:value={complaint} rows="3" placeholder="Patient's main complaint..."></textarea>
    </div>
  </div>

  <!-- Triage category -->
  <div class="card">
    <h2>Triage category</h2>
    <div class="triage-radios">
      {#each categories as cat}
        <label
          class="triage-radio {triageCategory === cat.value ? 'selected ' + cat.cls : ''}"
        >
          <input
            type="radio"
            name="triage"
            value={cat.value}
            bind:group={triageCategory}
            style="width:auto;min-height:auto"
          />
          <div>
            <div style="font-weight:600">{cat.label}</div>
            <div class="muted" style="font-size:13px">{cat.desc}</div>
          </div>
        </label>
      {/each}
    </div>
  </div>

  <button class="primary w-full" type="submit" style="min-height:48px">
    Save triage
  </button>
  {#if saved}
    <p class="muted text-center">Saved (stub — no data persisted in v0.1)</p>
  {/if}
</form>
