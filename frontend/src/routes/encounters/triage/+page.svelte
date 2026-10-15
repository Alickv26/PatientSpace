<script lang="ts">
  import { page } from '$app/state';

  // patientId from the ?patient= query param (SvelteKit 3 reactive page state).
  let patientId = $derived(page.url.searchParams.get('patient') ?? '');

  let bpSystolic = $state('');
  let bpDiastolic = $state('');
  let heartRate = $state('');
  let temperature = $state('');
  let weight = $state('');
  let presentingComplaint = $state('');
  let triageCategory = $state<'emergency' | 'urgent' | 'standard' | 'low'>('standard');

  let message = $state<string | null>(null);

  function submit(e: Event) {
    e.preventDefault();
    // v0.1 stub — no POST yet. The Rust core doesn't expose a FHIR Encounter
    // endpoint. Wire this up in the next milestone.
    message = 'Saved (stub) — vitals captured client-side only for v0.1.';
  }
</script>

<svelte:head>
  <title>Triage · PatientSpace</title>
</svelte:head>

<h1 style="font-size:22px;font-weight:700;margin:0 0 4px">Triage encounter</h1>
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
      <label for="bp-sys">BP Systolic (mmHg)</label>
      <input id="bp-sys" type="number" min="0" max="300" bind:value={bpSystolic} placeholder="120" />
    </div>
    <div>
      <label for="bp-dia">BP Diastolic (mmHg)</label>
      <input id="bp-dia" type="number" min="0" max="200" bind:value={bpDiastolic} placeholder="80" />
    </div>
  </div>
  <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:14px">
    <div>
      <label for="hr">Heart rate (bpm)</label>
      <input id="hr" type="number" min="0" max="250" bind:value={heartRate} placeholder="72" />
    </div>
    <div>
      <label for="temp">Temperature (°C)</label>
      <input id="temp" type="number" step="0.1" min="30" max="45" bind:value={temperature} placeholder="36.8" />
    </div>
    <div>
      <label for="wt">Weight (kg)</label>
      <input id="wt" type="number" step="0.1" min="0" max="500" bind:value={weight} placeholder="62" />
    </div>
  </div>
  <div>
    <label for="complaint">Presenting complaint</label>
    <textarea id="complaint" bind:value={presentingComplaint} rows="3" placeholder="Cough × 2 weeks, night sweats, weight loss…"></textarea>
  </div>
  <div>
    <label for="cat">Triage category</label>
    <select id="cat" bind:value={triageCategory}>
      <option value="emergency">Emergency (red — immediate)</option>
      <option value="urgent">Urgent (orange — under 10 min)</option>
      <option value="standard">Standard (yellow — under 1 hr)</option>
      <option value="low">Low (green — ambulatory)</option>
    </select>
  </div>
  {#if message}
    <div class="ok-banner">{message}</div>
  {/if}
  <div style="display:flex;gap:12px;align-items:center">
    <button class="primary" type="submit">Save triage</button>
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
