<script lang="ts">
  import { page } from '$app/state';

  // patientId from the ?patient= query param (SvelteKit 3 reactive page state).
  let patientId = $derived(page.url.searchParams.get('patient') ?? '');

  let diagnosis = $state('');
  let icd10 = $state('');
  let notes = $state('');
  let plan = $state('');

  let message = $state<string | null>(null);

  function submit(e: Event) {
    e.preventDefault();
    // v0.1 stub — no POST yet.
    message = 'Saved (stub) — consultation captured client-side only for v0.1.';
  }
</script>

<svelte:head>
  <title>Consultation · PatientSpace</title>
</svelte:head>

<h1 style="font-size:22px;font-weight:700;margin:0 0 4px">Consultation</h1>
<p class="muted" style="margin:0 0 16px">
  {#if patientId}
    Patient ID: <code>{patientId}</code>
  {:else}
    No patient selected —
    <a href="/patients">choose one from the patient list</a>.
  {/if}
</p>

<form onsubmit={submit} style="display:grid;gap:14px;max-width:640px">
  <div style="display:grid;grid-template-columns:1fr 1fr;gap:14px">
    <div>
      <label for="dx">Diagnosis</label>
      <input id="dx" bind:value={diagnosis} placeholder="Pulmonary tuberculosis" />
    </div>
    <div>
      <label for="icd">ICD-10 code</label>
      <input id="icd" bind:value={icd10} placeholder="A15.0" />
    </div>
  </div>
  <div>
    <label for="notes">Clinical notes</label>
    <textarea id="notes" bind:value={notes} rows="5" placeholder="History, examination, differential reasoning…"></textarea>
  </div>
  <div>
    <label for="plan">Plan</label>
    <textarea id="plan" bind:value={plan} rows="4" placeholder="Investigations, treatment, follow-up…"></textarea>
  </div>
  {#if message}
    <div class="ok-banner">{message}</div>
  {/if}
  <div style="display:flex;gap:12px;align-items:center">
    <button class="primary" type="submit">Save consultation</button>
    {#if patientId}
      <a href={`/patients/${encodeURIComponent(patientId)}`}>← Back to patient</a>
    {/if}
  </div>
</form>

<style>
  label {
    display: block;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--ps-muted);
    margin-bottom: 4px;
  }
  .ok-banner {
    background: #ecfdf5;
    border: 1px solid var(--ps-primary);
    color: #064e3b;
    padding: 8px 12px;
    border-radius: 4px;
    font-size: 13px;
  }
</style>
