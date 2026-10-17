<script lang="ts">
  import { onMount } from 'svelte';

  let encounters = $state<any[]>([]);
  let loading = $state(true);
  let error = $state('');

  async function load() {
    loading = true;
    error = '';
    try {
      const resp = await fetch('/fhir/Encounter?_count=200');
      if (resp.ok) {
        const bundle = await resp.json();
        encounters = bundle.entry?.map((e: any) => e.resource) ?? [];
      }
    } catch {
      error = 'Failed to load encounters';
    } finally {
      loading = false;
    }
  }

  function getPatientId(e: any): string {
    const ref = e.subject?.reference ?? '';
    return ref.replace('Patient/', '') || '';
  }

  function getReason(e: any): string {
    return e.reasonCode?.[0]?.text ?? e.priority?.coding?.[0]?.code ?? '—';
  }

  function getDate(e: any): string {
    const start = e.period?.start;
    if (!start) return '—';
    try {
      return new Date(start).toLocaleString();
    } catch {
      return start;
    }
  }

  function getStatus(e: any): string {
    return e.status ?? '—';
  }

  onMount(load);
</script>

<div class="flex items-center justify-between mb-4 flex-wrap gap-2">
  <h1 style="font-size:20px;font-weight:700;margin:0">Encounters</h1>
  <button onclick={load}>↻ Refresh</button>
</div>

{#if loading}
  <div class="card"><p class="muted">Loading...</p></div>
{:else if error}
  <div class="card" style="border-color:var(--ps-danger);color:var(--ps-danger)">{error}</div>
{:else if encounters.length === 0}
  <div class="card">
    <p class="muted">No encounters recorded yet.</p>
    <p class="muted" style="font-size:13px;margin-top:8px">
      To create an encounter: open a patient → click "Start triage" or "New consultation".
    </p>
    <a href="/patients"><button class="primary mt-2">Find a patient</button></a>
  </div>
{:else}
  <div class="table-wrap">
    <table class="data">
      <thead>
        <tr>
          <th>Date</th>
          <th>Reason / Category</th>
          <th>Status</th>
          <th>Patient</th>
        </tr>
      </thead>
      <tbody>
        {#each encounters as e (e.id)}
          {@const pid = getPatientId(e)}
          <tr>
            <td>{getDate(e)}</td>
            <td>{getReason(e)}</td>
            <td>{getStatus(e)}</td>
            <td>
              {#if pid}
                <a href="/patients/{pid}">View patient</a>
              {:else}
                —
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}
