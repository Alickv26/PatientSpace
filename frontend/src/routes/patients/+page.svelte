<script lang="ts">
  import { onMount } from 'svelte';
  import {
    listPatients,
    smartcardNo,
    nrc,
    displayName,
    type Patient,
  } from '$lib/api/patient';

  let patients = $state<Patient[]>([]);
  let total = $state(0);
  let loading = $state(true);
  let error = $state('');

  let search = $state('');
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  let count = $state(50);
  let offset = $state(0);

  let rangeStart = $derived(Math.min(offset + 1, total));
  let rangeEnd = $derived(Math.min(offset + patients.length, total));

  async function load() {
    loading = true;
    error = '';
    try {
      const result = await listPatients({
        count,
        offset,
        identifier: search || undefined,
      });
      patients = result.patients;
      total = result.total;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load patients';
      patients = [];
    } finally {
      loading = false;
    }
  }

  function onSearchInput() {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => {
      offset = 0;
      load();
    }, 300);
  }

  function nextPage() {
    offset += count;
    load();
  }

  function prevPage() {
    offset = Math.max(0, offset - count);
    load();
  }

  onMount(load);
</script>

<div class="flex items-center justify-between mb-4 flex-wrap gap-2">
  <h1 style="font-size:20px;font-weight:700;margin:0">Patients</h1>
  <a href="/patients/new"><button class="primary">+ Register</button></a>
</div>

<div class="mb-4">
  <input
    bind:value={search}
    oninput={onSearchInput}
    placeholder="Search by Smartcard or NRC..."
  />
</div>

{#if error}
  <div class="card" style="border-color:var(--ps-danger);color:var(--ps-danger)">
    {error}
  </div>
{:else if loading}
  <div class="card"><p class="muted">Loading...</p></div>
{:else if patients.length === 0}
  <div class="card">
    <p class="muted">
      {search ? 'No patients match your search.' : 'No patients yet.'}
      {!search ? '<br><a href="/patients/new">Register the first patient</a>' : ''}
    </p>
  </div>
{:else}
  <div class="table-wrap">
    <table class="data">
      <thead>
        <tr>
          <th>Name</th>
          <th>Smartcard</th>
          <th>NRC</th>
          <th>Gender</th>
          <th>DOB</th>
        </tr>
      </thead>
      <tbody>
        {#each patients as p (p.id)}
          <tr>
            <td><a href="/patients/{p.id}">{displayName(p)}</a></td>
            <td>{smartcardNo(p) ?? '—'}</td>
            <td>{nrc(p) ?? '—'}</td>
            <td>{p.gender ?? '—'}</td>
            <td>{p.birthDate ?? '—'}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>

  <div class="flex justify-between items-center mt-4 flex-wrap gap-2">
    <span class="muted" style="font-size:14px">
      Showing {rangeStart}–{rangeEnd} of {total}
    </span>
    <div class="flex gap-2">
      <button onclick={prevPage} disabled={offset === 0}>Previous</button>
      <button onclick={nextPage} disabled={offset + count >= total}>Next</button>
    </div>
  </div>
{/if}
