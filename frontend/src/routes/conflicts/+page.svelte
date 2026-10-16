<script lang="ts">
  import { onMount } from 'svelte';

  let conflicts = $state<any[]>([]);
  let loading = $state(true);
  let error = $state('');

  async function load() {
    loading = true;
    error = '';
    try {
      const resp = await fetch('/api/conflicts');
      if (resp.ok) {
        const data = await resp.json();
        conflicts = data.conflicts ?? [];
      } else {
        error = 'Failed to load conflicts';
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

  function getDisplayName(resource: any): string {
    if (resource.name?.[0]?.text) return resource.name[0].text;
    if (resource.name?.[0]?.given) return resource.name[0].given.join(' ');
    if (resource.reasonCode?.[0]?.text) return resource.reasonCode[0].text;
    if (resource.code?.text) return resource.code.text;
    if (resource.medicationCodeableConcept?.text) return resource.medicationCodeableConcept.text;
    return resource.id ?? 'Unknown';
  }

  onMount(load);
</script>

<div class="flex items-center justify-between mb-4 flex-wrap gap-2">
  <h1 style="font-size:20px;font-weight:700;margin:0">Conflict Queue</h1>
  <button onclick={load}>↻ Refresh</button>
</div>

<p class="muted mb-4" style="font-size:14px">
  Resources with concurrent edits from multiple facilities. Review and resolve each conflict.
</p>

{#if loading}
  <div class="card"><p class="muted">Loading...</p></div>
{:else if error}
  <div class="card" style="border-color:var(--ps-danger);color:var(--ps-danger)">
    {error}
  </div>
{:else if conflicts.length === 0}
  <div class="card">
    <p class="muted" style="text-align:center;padding:20px 0">
      ✅ No conflicts detected. All records are in sync.
    </p>
  </div>
{:else}
  {#each conflicts as c (c.resource_id)}
    <div class="card" style="border-left:4px solid var(--ps-accent)">
      <div class="flex items-center justify-between flex-wrap gap-2">
        <div>
          <h2 style="margin:0 0 4px;font-size:16px">
            {c.resource_type} — {getDisplayName(c.conflicting_versions[0]?.resource)}
          </h2>
          <p class="muted" style="margin:0;font-size:13px">
            {c.conflicting_versions.length} conflicting versions from:
            {c.conflicting_versions.map((v: any) => v.author_facility_id).join(', ')}
          </p>
        </div>
        <a href="/conflicts/{c.resource_id}">
          <button class="primary">Review & resolve</button>
        </a>
      </div>

      <div style="margin-top:12px;display:grid;gap:8px">
        {#each c.conflicting_versions as v (v.version_id)}
          <div style="display:flex;gap:8px;align-items:center;padding:8px;background:var(--ps-bg);border-radius:6px;font-size:13px">
            <span class="muted" style="width:120px">{v.author_facility_id}</span>
            <span class="muted" style="width:180px">{formatDate(v.created_at_unix_ms)}</span>
            <span style="font-family:monospace;font-size:11px;color:var(--ps-muted)">{v.version_id.slice(0, 12)}...</span>
          </div>
        {/each}
      </div>
    </div>
  {/each}
{/if}
