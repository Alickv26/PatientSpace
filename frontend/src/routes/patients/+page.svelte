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
  let error = $state<string | null>(null);

  let search = $state('');
  let count = $state(25);
  let offset = $state(0);

  // Debounce timer handle.
  let debounceHandle: ReturnType<typeof setTimeout> | null = null;
  // Tracks the latest in-flight request so we can ignore stale responses.
  let latestRequestId = 0;

  async function load() {
    loading = true;
    error = null;
    const myId = ++latestRequestId;
    try {
      const result = await listPatients({
        count,
        offset,
        identifier: search.trim() || undefined,
      });
      // Ignore responses from older requests (e.g. user kept typing).
      if (myId !== latestRequestId) return;
      patients = result.patients;
      total = result.total;
    } catch (e) {
      if (myId !== latestRequestId) return;
      error = e instanceof Error ? e.message : String(e);
      patients = [];
      total = 0;
    } finally {
      if (myId === latestRequestId) loading = false;
    }
  }

  function onSearchInput() {
    // Reset to first page when search changes.
    offset = 0;
    if (debounceHandle) clearTimeout(debounceHandle);
    debounceHandle = setTimeout(() => {
      debounceHandle = null;
      load();
    }, 300);
  }

  function nextPage() {
    if (offset + count >= total) return;
    offset += count;
    load();
  }

  function prevPage() {
    if (offset === 0) return;
    offset = Math.max(0, offset - count);
    load();
  }

  // Range shown — guard against empty / out-of-range.
  let rangeStart = $derived(total === 0 ? 0 : offset + 1);
  let rangeEnd = $derived(Math.min(offset + patients.length, total));

  onMount(() => {
    load();
  });
</script>

<h1 style="font-size:22px;font-weight:700;margin:0 0 16px">Patients</h1>

<div style="display:flex;align-items:center;gap:12px;margin-bottom:16px;flex-wrap:wrap">
  <input
    type="search"
    placeholder="Search by Smartcard No., NRC, or identifier value…"
    bind:value={search}
    oninput={onSearchInput}
    style="flex:1;min-width:280px;max-width:520px"
  />
  <a href="/patients/new">
    <button class="primary" type="button">+ Register new patient</button>
  </a>
</div>

<div class="card">
  {#if loading}
    <p class="muted">Loading…</p>
  {:else if error}
    <div class="error-banner">{error}</div>
  {:else if patients.length === 0}
    <p class="muted">
      No patients found.
      {#if search.trim()}No matches for “{search}”.{:else}Register the first patient.{/if}
    </p>
  {:else}
    <table class="data">
      <thead>
        <tr>
          <th>Smartcard</th>
          <th>NRC</th>
          <th>Name</th>
          <th>Gender</th>
          <th>DOB</th>
        </tr>
      </thead>
      <tbody>
        {#each patients as p (p.id)}
          <tr>
            <td>
              <a href={`/patients/${encodeURIComponent(p.id)}`}>
                {smartcardNo(p) ?? '—'}
              </a>
            </td>
            <td>{nrc(p) ?? '—'}</td>
            <td><a href={`/patients/${encodeURIComponent(p.id)}`}>{displayName(p)}</a></td>
            <td>{p.gender ?? '—'}</td>
            <td>{p.birthDate ?? '—'}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

{#if !loading && !error && total > 0}
  <div style="display:flex;justify-content:space-between;align-items:center;margin-top:12px;flex-wrap:wrap;gap:8px">
    <span class="muted" style="font-size:12px">
      Showing {rangeStart}–{rangeEnd} of {total}
    </span>
    <div style="display:flex;gap:8px">
      <button type="button" onclick={prevPage} disabled={offset === 0}>
        ← Previous
      </button>
      <button
        type="button"
        onclick={nextPage}
        disabled={offset + count >= total}
      >
        Next →
      </button>
    </div>
  </div>
{/if}

<style>
  .error-banner {
    background: #fef2f2;
    border: 1px solid var(--ps-danger);
    color: var(--ps-danger);
    padding: 8px 12px;
    border-radius: 4px;
    font-size: 13px;
  }
</style>
