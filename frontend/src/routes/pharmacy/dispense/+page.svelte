<script lang="ts">
  import { page } from '$app/state';

  let requestId = $derived(page.url.searchParams.get('request') ?? '');

  let quantity = $state('');
  let batchNo = $state('');
  let confirmed = $state(false);
  let dispensed = $state(false);

  function submit(e: Event) {
    e.preventDefault();
    if (!confirmed) return;
    dispensed = true;
    setTimeout(() => (dispensed = false), 3000);
  }
</script>

<div class="flex items-center mb-4 gap-2">
  <a href="/patients"><button>← Back</button></a>
  <h1 style="font-size:20px;font-weight:700;margin:0;flex:1">Pharmacy dispense</h1>
</div>

<form onsubmit={submit} style="display:grid;gap:16px">
  <div class="card">
    <h2>Dispense details</h2>
    <div class="form-group">
      <label>Quantity dispensed</label>
      <input type="number" bind:value={quantity} placeholder="30" />
    </div>
    <div class="form-group">
      <label>Batch number</label>
      <input bind:value={batchNo} placeholder="e.g., BN-2026-001" style="font-family:monospace" />
    </div>
  </div>

  <div class="card">
    <label class="triage-radio" class:selected.low={confirmed}>
      <input
        type="checkbox"
        bind:checked={confirmed}
        style="width:auto;min-height:auto"
      />
      <span>I confirm the medication and quantity are correct.</span>
    </label>
  </div>

  <button class="primary w-full" type="submit" disabled={!confirmed} style="min-height:48px">
    Confirm dispense
  </button>
  {#if dispensed}
    <p class="muted text-center">Dispensed (stub)</p>
  {/if}
</form>
