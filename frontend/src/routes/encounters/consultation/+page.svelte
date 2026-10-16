<script lang="ts">
  import { page } from '$app/state';
  import { getPatient, displayName, type Patient } from '$lib/api/patient';
  import { onMount } from 'svelte';

  let patient = $state<Patient | null>(null);
  let patientId = $derived(page.url.searchParams.get('patient') ?? '');

  let diagnosisText = $state('');
  let icd10Code = $state('');
  let notes = $state('');
  let plan = $state('');
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
</script>

<div class="flex items-center mb-4 gap-2">
  <a href="/patients"><button>← Back</button></a>
  <h1 style="font-size:20px;font-weight:700;margin:0;flex:1">Consultation</h1>
</div>

{#if patient}
  <div class="card" style="background:var(--ps-bg)">
    <strong>{displayName(patient)}</strong>
    <span class="muted"> · {patient.gender ?? '—'} · {patient.birthDate ?? 'DOB unknown'}</span>
  </div>
{/if}

<form onsubmit={submit} style="display:grid;gap:16px">
  <div class="card">
    <h2>Diagnosis</h2>
    <div class="form-group">
      <label>Diagnosis (text)</label>
      <input bind:value={diagnosisText} placeholder="e.g., Essential hypertension" />
    </div>
    <div class="form-group">
      <label>ICD-10 code</label>
      <input bind:value={icd10Code} placeholder="e.g., I10" style="font-family:monospace" />
    </div>
  </div>

  <div class="card">
    <h2>Notes</h2>
    <div class="form-group">
      <label>Clinical notes</label>
      <textarea bind:value={notes} rows="4" placeholder="Examination findings, history..."></textarea>
    </div>
    <div class="form-group">
      <label>Plan</label>
      <textarea bind:value={plan} rows="3" placeholder="Treatment plan, follow-up..."></textarea>
    </div>
  </div>

  <button class="primary w-full" type="submit" style="min-height:48px">
    Save consultation
  </button>
  {#if saved}
    <p class="muted text-center">Saved (stub)</p>
  {/if}
</form>
