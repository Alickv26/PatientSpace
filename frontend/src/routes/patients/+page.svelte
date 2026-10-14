<script lang="ts">
  import { onMount } from 'svelte';

  let patients = $state<any[]>([]);
  let loading = $state(true);

  onMount(async () => {
    try {
      const r = await fetch('/fhir/Patient');
      const body = await r.json();
      patients = body.entry ?? [];
    } catch (e) {
      patients = [];
    } finally {
      loading = false;
    }
  });
</script>

<h1 style="font-size:22px;font-weight:700;margin:0 0 16px">Patients</h1>

<p class="muted" style="margin:0 0 16px">
  <a href="/patients/new">+ Register new patient</a>
</p>

<div class="card">
  {#if loading}
    <p class="muted">Loading…</p>
  {:else if patients.length === 0}
    <p class="muted">No patients yet. The Rust core returns an empty bundle for v0.1.</p>
  {:else}
    <table class="data">
      <thead>
        <tr>
          <th>Smartcard</th><th>NRC</th><th>Name</th><th>Gender</th><th>DOB</th>
        </tr>
      </thead>
      <tbody>
        {#each patients as p}
          <tr>
            <td>{p.resource?.identifier?.find((i: any) => i.system?.endsWith('smartcard'))?.value ?? '—'}</td>
            <td>{p.resource?.identifier?.find((i: any) => i.system?.endsWith('nrc-zm'))?.value ?? '—'}</td>
            <td>{p.resource?.name?.[0]?.text ?? '—'}</td>
            <td>{p.resource?.gender ?? '—'}</td>
            <td>{p.resource?.birthDate ?? '—'}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
