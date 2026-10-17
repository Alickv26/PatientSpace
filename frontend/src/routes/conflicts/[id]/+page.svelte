<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';

  let id = $derived(page.params.id);
  let conflict = $state<any>(null);
  let loading = $state(true);
  let error = $state('');
  let selectedVersion = $state<string>('');

  async function load() {
    loading = true;
    error = '';
    try {
      const resp = await fetch(`/api/conflicts/${id}`);
      if (resp.ok) {
        conflict = await resp.json();
        if (conflict.versions?.length > 0) {
          selectedVersion = conflict.versions[0].version_id;
        }
      } else {
        error = 'Failed to load conflict';
      }
    } catch {
      error = 'Cannot connect to backend';
    } finally {
      loading = false;
    }
  }

  function formatDate(ms: number): string {
    return new Date(ms).toLocaleString();
  }

  function getField(resource: any, path: string): string {
    const parts = path.split('.');
    let val: any = resource;
    for (const p of parts) {
      if (val == null) return '—';
      val = val[p];
    }
    if (val == null) return '—';
    if (Array.isArray(val)) {
      if (val.length === 0) return '—';
      if (typeof val[0] === 'object') {
        return val.map((v: any) => v.text ?? v.value ?? v.code ?? JSON.stringify(v)).join(', ');
      }
      return val.join(', ');
    }
    if (typeof val === 'object') return JSON.stringify(val);
    return String(val);
  }

  // Get all unique top-level field keys across all versions.
  let allFields = $derived.by(() => {
    if (!conflict?.versions) return [];
    const keys = new Set<string>();
    for (const v of conflict.versions) {
      if (v.resource && typeof v.resource === 'object') {
        for (const k of Object.keys(v.resource)) {
          if (k !== 'id' && k !== 'resourceType') {
            keys.add(k);
          }
        }
      }
    }
    return Array.from(keys).sort();
  });

  // Check if a field differs across versions.
  function fieldDiffers(field: string): boolean {
    if (!conflict?.versions || conflict.versions.length < 2) return false;
    const values = conflict.versions.map((v: any) => getField(v.resource, field));
    return !values.every((v: string) => v === values[0]);
  }

  function selectVersion(versionId: string) {
    selectedVersion = versionId;
  }

  function resolve() {
    // In v0.1, "resolving" just marks it as reviewed by navigating back.
    // In v0.2, this would POST a merge version with 2 parents.
    window.location.href = '/conflicts';
  }

  onMount(load);
</script>

<div class="flex items-center mb-4 gap-2">
  <a href="/conflicts"><button>← Back to queue</button></a>
  <h1 style="font-size:20px;font-weight:700;margin:0;flex:1">Conflict Review</h1>
</div>

{#if loading}
  <div class="card"><p class="muted">Loading...</p></div>
{:else if error}
  <div class="card" style="border-color:var(--ps-danger);color:var(--ps-danger)">
    {error}
  </div>
{:else if conflict}
  {@const versions = conflict.versions ?? []}

  <div class="card" style="border-left:4px solid var(--ps-accent);background:var(--ps-accent-bg)">
    <strong>⚠️ Conflict detected</strong> — this {conflict.resource_id ? 'resource' : 'record'} has
    {versions.length} concurrent versions from multiple facilities.
    Review the differences below and select the version to keep.
  </div>

  <!-- Version selector -->
  <div class="card">
    <h2>Conflicting Versions</h2>
    <div style="display:grid;gap:8px">
      {#each versions as v (v.version_id)}
        <label
          class="triage-radio"
          class:selected.standard={selectedVersion === v.version_id}
          style="cursor:pointer"
        >
          <input
            type="radio"
            name="version"
            value={v.version_id}
            bind:group={selectedVersion}
            style="width:auto;min-height:auto"
          />
          <div>
            <div style="font-weight:600">{v.author_facility_id}</div>
            <div class="muted" style="font-size:13px">
              {formatDate(v.created_at_unix_ms)} · {v.version_id.slice(0, 12)}...
            </div>
          </div>
        </label>
      {/each}
    </div>
  </div>

  <!-- Field comparison table -->
  <div class="card">
    <h2>Field Comparison</h2>
    {#if allFields.length > 0}
      <div class="table-wrap">
        <table class="data">
          <thead>
            <tr>
              <th>Field</th>
              {#each versions as v}
                <th>{v.author_facility_id}<br><span class="muted" style="font-size:11px;font-weight:400">{formatDate(v.created_at_unix_ms)}</span></th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each allFields as field}
              <tr class={fieldDiffers(field) ? 'diff-row' : ''}>
                <td style="font-weight:600">{field}</td>
                {#each versions as v}
                  <td style={fieldDiffers(field) ? 'color:var(--ps-danger);font-weight:500' : ''}>
                    {getField(v.resource, field)}
                  </td>
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <p class="muted">No fields to compare.</p>
    {/if}
  </div>

  <!-- Selected version preview -->
  {#if selectedVersion}
    {@const selected = versions.find((v: any) => v.version_id === selectedVersion)}
    <div class="card">
      <h2>Selected Version Preview</h2>
      <pre style="background:var(--ps-bg);padding:12px;border-radius:6px;overflow-x:auto;font-size:13px;line-height:1.5">{JSON.stringify(selected?.resource, null, 2)}</pre>
    </div>
  {/if}

  <!-- Actions -->
  <div class="card">
    <button class="primary w-full" onclick={resolve} style="min-height:48px">
      Keep selected version & resolve
    </button>
    <p class="muted text-center" style="font-size:13px;margin-top:8px">
      This will create a merge version closing the conflict. (v0.1: marks as reviewed)
    </p>
  </div>
{:else}
  <div class="card"><p class="muted">No conflict found.</p></div>
{/if}

<style>
  .diff-row {
    background: rgba(185, 28, 28, 0.05);
  }
  .diff-row td {
    border-bottom: 1px solid var(--ps-border) !important;
  }
  pre {
    white-space: pre-wrap;
    word-wrap: break-word;
  }
</style>
