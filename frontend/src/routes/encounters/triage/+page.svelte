<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
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
  let submitting = $state(false);
  let saved = $state(false);
  let error = $state('');

  onMount(async () => {
    if (patientId) {
      try {
        patient = await getPatient(patientId);
      } catch {
        patient = null;
      }
    }
  });

  async function postFhir(type: string, body: Record<string, unknown>): Promise<string | null> {
    const resp = await fetch(`/fhir/${type}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (resp.ok) {
      const json = await resp.json();
      return json.id;
    }
    return null;
  }

  async function submit(e: Event) {
    e.preventDefault();
    error = '';
    submitting = true;

    try {
      // 1. Create an Encounter
      const encounterId = await postFhir('Encounter', {
        resourceType: 'Encounter',
        status: 'finished',
        class: { system: 'http://terminology.hl7.org/CodeSystem/v3-ActCode', code: 'AMB', display: 'ambulatory' },
        subject: { reference: `Patient/${patientId}` },
        period: { start: new Date().toISOString() },
        reasonCode: [{ text: complaint || 'Not specified' }],
        priority: { coding: [{ code: triageCategory, display: triageCategory }] },
      });

      if (!encounterId) throw new Error('Failed to create encounter');

      // 2. Create vitals as Observations linked to the encounter
      const now = new Date().toISOString();
      const observations: Array<[string, string, string, string]> = [];

      if (bpSystolic && bpDiastolic) {
        observations.push(['85354-9', 'Blood pressure', `${bpSystolic}/${bpDiastolic}`, 'mmHg']);
      }
      if (heartRate) {
        observations.push(['8867-4', 'Heart rate', heartRate, 'bpm']);
      }
      if (temperature) {
        observations.push(['8310-5', 'Body temperature', temperature, '°C']);
      }
      if (weight) {
        observations.push(['29463-7', 'Body weight', weight, 'kg']);
      }

      for (const [loinc, display, value, unit] of observations) {
        await postFhir('Observation', {
          resourceType: 'Observation',
          status: 'final',
          code: {
            coding: [{ system: 'http://loinc.org', code: loinc, display }],
            text: display,
          },
          subject: { reference: `Patient/${patientId}` },
          encounter: { reference: `Encounter/${encounterId}` },
          effectiveDateTime: now,
          valueQuantity: { value: parseFloat(value), unit, system: 'http://unitsofmeasure.org' },
        });
      }

      saved = true;
      setTimeout(() => {
        if (patientId) goto(`/patients/${patientId}`);
      }, 1500);
    } catch (err) {
      error = err instanceof Error ? err.message : 'Unknown error';
    } finally {
      submitting = false;
    }
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

{#if error}
  <div class="card" style="border-color:var(--ps-danger);color:var(--ps-danger)">
    {error}
  </div>
{/if}

<form onsubmit={submit} style="display:grid;gap:16px">
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

  <div class="card">
    <h2>Presenting complaint</h2>
    <div class="form-group">
      <textarea bind:value={complaint} rows="3" placeholder="Patient's main complaint..."></textarea>
    </div>
  </div>

  <div class="card">
    <h2>Triage category</h2>
    <div class="triage-radios">
      {#each categories as cat}
        <label class="triage-radio {triageCategory === cat.value ? 'selected ' + cat.cls : ''}">
          <input type="radio" name="triage" value={cat.value} bind:group={triageCategory} style="width:auto;min-height:auto" />
          <div>
            <div style="font-weight:600">{cat.label}</div>
            <div class="muted" style="font-size:13px">{cat.desc}</div>
          </div>
        </label>
      {/each}
    </div>
  </div>

  <button class="primary w-full" type="submit" disabled={submitting} style="min-height:48px">
    {submitting ? 'Saving...' : 'Save triage'}
  </button>
  {#if saved}
    <p class="muted text-center" style="color:var(--ps-primary)">Saved! Redirecting to patient...</p>
  {/if}
</form>
