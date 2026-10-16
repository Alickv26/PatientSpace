<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';

  let requestId = $derived(page.url.searchParams.get('request') ?? '');
  let patientId = $derived(page.url.searchParams.get('patient') ?? '');

  let quantity = $state('');
  let batchNo = $state('');
  let confirmed = $state(false);
  let submitting = $state(false);
  let dispensed = $state(false);
  let error = $state('');

  async function submit(e: Event) {
    e.preventDefault();
    if (!confirmed) return;
    error = '';
    submitting = true;

    try {
      const body: Record<string, unknown> = {
        resourceType: 'MedicationDispense',
        status: 'completed',
        whenHandedOver: new Date().toISOString(),
      };

      if (patientId) {
        body.subject = { reference: `Patient/${patientId}` };
      }
      if (requestId) {
        body.authorizingPrescription = { reference: `MedicationRequest/${requestId}` };
      }
      if (quantity) {
        body.quantity = { value: parseInt(quantity), unit: 'tablets', system: 'http://unitsofmeasure.org' };
      }
      if (batchNo) {
        body.batch = { lotNumber: batchNo };
      }

      const resp = await fetch('/fhir/MedicationDispense', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });

      if (!resp.ok) throw new Error(`Failed: ${resp.status}`);

      dispensed = true;
      setTimeout(() => {
        if (patientId) goto(`/patients/${patientId}`);
        else goto('/patients');
      }, 1500);
    } catch (err) {
      error = err instanceof Error ? err.message : 'Unknown error';
    } finally {
      submitting = false;
    }
  }
</script>

<div class="flex items-center mb-4 gap-2">
  <a href="/patients"><button>← Back</button></a>
  <h1 style="font-size:20px;font-weight:700;margin:0;flex:1">Pharmacy dispense</h1>
</div>

{#if error}
  <div class="card" style="border-color:var(--ps-danger);color:var(--ps-danger)">
    {error}
  </div>
{/if}

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
      <input type="checkbox" bind:checked={confirmed} style="width:auto;min-height:auto" />
      <span>I confirm the medication and quantity are correct.</span>
    </label>
  </div>

  <button class="primary w-full" type="submit" disabled={!confirmed || submitting} style="min-height:48px">
    {submitting ? 'Dispensing...' : 'Confirm dispense'}
  </button>
  {#if dispensed}
    <p class="muted text-center" style="color:var(--ps-primary)">Dispensed! Redirecting...</p>
  {/if}
</form>
