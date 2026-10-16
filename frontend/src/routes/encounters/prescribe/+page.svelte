<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';

  let encounterId = $derived(page.url.searchParams.get('encounter') ?? '');
  let patientId = $derived(page.url.searchParams.get('patient') ?? '');

  let medication = $state('');
  let atcCode = $state('');
  let dose = $state('');
  let frequency = $state('');
  let duration = $state('');
  let submitting = $state(false);
  let saved = $state(false);
  let error = $state('');

  async function submit(e: Event) {
    e.preventDefault();
    error = '';
    submitting = true;

    try {
      const body: Record<string, unknown> = {
        resourceType: 'MedicationRequest',
        status: 'active',
        intent: 'order',
        authoredOn: new Date().toISOString(),
      };

      if (patientId) {
        body.subject = { reference: `Patient/${patientId}` };
      }
      if (encounterId) {
        body.encounter = { reference: `Encounter/${encounterId}` };
      }

      // Medication coding
      if (medication || atcCode) {
        const medCode: Record<string, unknown> = {};
        if (atcCode) {
          medCode.coding = [{ system: 'http://www.whocc.no/atc', code: atcCode, display: medication || atcCode }];
        }
        medCode.text = medication || atcCode;
        body.medicationCodeableConcept = medCode;
      }

      // Dosage instruction
      const dosageInstruction: Record<string, unknown> = {};
      if (dose) dosageInstruction.text = dose;
      if (frequency) {
        dosageInstruction.timing = {
          repeat: {
            code: { coding: [{ system: 'http://terminology.hl7.org/CodeSystem/v3-GTSAbbreviation', code: frequency }] },
          },
        };
      }
      if (duration) {
        dosageInstruction.timing = dosageInstruction.timing || {};
        if (dosageInstruction.timing) {
          dosageInstruction.timing.repeat = dosageInstruction.timing.repeat || {};
          dosageInstruction.timing.repeat.boundsDuration = { value: parseInt(duration), unit: 'd', code: 'd' };
        }
      }
      if (Object.keys(dosageInstruction).length > 0) {
        body.dosageInstruction = [dosageInstruction];
      }

      const resp = await fetch('/fhir/MedicationRequest', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });

      if (!resp.ok) throw new Error(`Failed: ${resp.status}`);

      saved = true;
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
  <h1 style="font-size:20px;font-weight:700;margin:0;flex:1">Prescribe</h1>
</div>

{#if error}
  <div class="card" style="border-color:var(--ps-danger);color:var(--ps-danger)">
    {error}
  </div>
{/if}

<form onsubmit={submit} style="display:grid;gap:16px">
  <div class="card">
    <h2>Medication</h2>
    <div class="form-group">
      <label>Medication (name)</label>
      <input bind:value={medication} placeholder="e.g., Amlodipine 5mg" />
    </div>
    <div class="form-group">
      <label>ATC code (WHO Anatomical Therapeutic Chemical)</label>
      <input bind:value={atcCode} placeholder="e.g., C08CA01" style="font-family:monospace" />
    </div>
  </div>

  <div class="card">
    <h2>Dosage</h2>
    <div class="form-grid cols-3">
      <div class="form-group">
        <label>Dose</label>
        <input bind:value={dose} placeholder="1 tablet" />
      </div>
      <div class="form-group">
        <label>Frequency</label>
        <select bind:value={frequency}>
          <option value="">—</option>
          <option value="OD">Once daily (OD)</option>
          <option value="BD">Twice daily (BD)</option>
          <option value="TDS">Three times daily (TDS)</option>
          <option value="QDS">Four times daily (QDS)</option>
          <option value="PRN">As needed (PRN)</option>
        </select>
      </div>
      <div class="form-group">
        <label>Duration (days)</label>
        <input type="number" bind:value={duration} placeholder="30" />
      </div>
    </div>
  </div>

  <button class="primary w-full" type="submit" disabled={submitting} style="min-height:48px">
    {submitting ? 'Prescribing...' : 'Prescribe'}
  </button>
  {#if saved}
    <p class="muted text-center" style="color:var(--ps-primary)">Prescribed! Redirecting...</p>
  {/if}
</form>
