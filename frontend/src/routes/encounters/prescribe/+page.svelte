<script lang="ts">
  import { page } from '$app/state';

  // patientId from the ?patient= query param (SvelteKit 3 reactive page state).
  let patientId = $derived(page.url.searchParams.get('patient') ?? '');

  let medicationName = $state('');
  let atcCode = $state('');
  let dose = $state('');
  let frequency = $state<'OD' | 'BD' | 'TDS' | 'QDS' | 'PRN'>('OD');
  let duration = $state('');

  let message = $state<string | null>(null);

  function submit(e: Event) {
    e.preventDefault();
    // v0.1 stub — no POST yet.
    message = 'Prescribed (stub) — prescription captured client-side only for v0.1.';
  }
</script>

<svelte:head>
  <title>Prescribe · PatientSpace</title>
</svelte:head>

<h1 style="font-size:22px;font-weight:700;margin:0 0 4px">Prescribe medication</h1>
<p class="muted" style="margin:0 0 16px">
  {#if patientId}
    Patient ID: <code>{patientId}</code>
  {:else}
    No patient selected —
    <a href="/patients">choose one from the patient list</a>.
  {/if}
</p>

<form onsubmit={submit} style="display:grid;gap:14px;max-width:640px">
  <div style="display:grid;grid-template-columns:2fr 1fr;gap:14px">
    <div>
      <label for="med">Medication name</label>
      <input id="med" bind:value={medicationName} placeholder="Rifampicin + Isoniazid + Pyrazinamide + Ethambutol (RHZE)" />
    </div>
    <div>
      <label for="atc">ATC code</label>
      <input id="atc" bind:value={atcCode} placeholder="J04AM02" />
    </div>
  </div>
  <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:14px">
    <div>
      <label for="dose">Dose</label>
      <input id="dose" bind:value={dose} placeholder="150/75/400/275 mg" />
    </div>
    <div>
      <label for="freq">Frequency</label>
      <select id="freq" bind:value={frequency}>
        <option value="OD">OD — once daily</option>
        <option value="BD">BD — twice daily</option>
        <option value="TDS">TDS — three times daily</option>
        <option value="QDS">QDS — four times daily</option>
        <option value="PRN">PRN — as needed</option>
      </select>
    </div>
    <div>
      <label for="dur">Duration</label>
      <input id="dur" bind:value={duration} placeholder="56 days" />
    </div>
  </div>
  {#if message}
    <div class="ok-banner">{message}</div>
  {/if}
  <div style="display:flex;gap:12px;align-items:center">
    <button class="primary" type="submit">Prescribe</button>
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
