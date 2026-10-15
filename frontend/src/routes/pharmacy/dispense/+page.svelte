<script lang="ts">
  import { page } from '$app/state';

  // patientId / prescription from query string (optional in v0.1).
  let patientId = $derived(page.url.searchParams.get('patient') ?? '');

  let quantity = $state('');
  let batchNumber = $state('');
  let confirmed = $state(false);

  let message = $state<string | null>(null);

  function submit(e: Event) {
    e.preventDefault();
    if (!confirmed) return;
    // v0.1 stub — no POST yet.
    message = 'Dispensed (stub) — dispense record captured client-side only for v0.1.';
  }
</script>

<svelte:head>
  <title>Pharmacy dispense · PatientSpace</title>
</svelte:head>

<h1 style="font-size:22px;font-weight:700;margin:0 0 4px">Pharmacy dispense</h1>
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
      <label for="qty">Quantity dispensed</label>
      <input id="qty" bind:value={quantity} placeholder="56 tablets" required />
    </div>
    <div>
      <label for="batch">Batch number</label>
      <input id="batch" bind:value={batchNumber} placeholder="RHZE-2025-A14" required />
    </div>
  </div>
  <div style="display:flex;gap:10px;align-items:center;padding:10px;border:1px solid var(--ps-border);border-radius:4px;background:#f9fafb">
    <input
      id="confirm"
      type="checkbox"
      bind:checked={confirmed}
      style="width:auto"
      required
    />
    <label for="confirm" style="margin:0;text-transform:none;letter-spacing:normal;font-size:13px;color:var(--ps-fg)">
      I confirm the patient has collected this medication and the batch number is correct.
    </label>
  </div>
  {#if message}
    <div class="ok-banner">{message}</div>
  {/if}
  <div style="display:flex;gap:12px;align-items:center">
    <button class="primary" type="submit" disabled={!confirmed}>
      Confirm dispense
    </button>
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
