<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  let dashboards = $state<any[]>([]);
  let results = $state<Record<string, any>>({});
  let loading = $state<Record<string, boolean>>({});
  let error = $state('');
  let refreshInterval: ReturnType<typeof setInterval> | null = null;

  async function loadDashboards() {
    try {
      const resp = await fetch('/api/dashboards');
      if (resp.ok) {
        const data = await resp.json();
        dashboards = data.dashboards ?? [];
        // Run all dashboards on load
        for (const d of dashboards) {
          runDashboard(d.id);
        }
      }
    } catch {
      error = 'Failed to load dashboards. Is the backend running?';
    }
  }

  async function runDashboard(id: string) {
    loading[id] = true;
    try {
      const resp = await fetch(`/api/dashboards/${id}/run`);
      if (resp.ok) {
        results[id] = await resp.json();
      }
    } catch {
      // ignore
    } finally {
      loading[id] = false;
    }
  }

  function refreshAll() {
    for (const d of dashboards) {
      runDashboard(d.id);
    }
  }

  onMount(() => {
    loadDashboards();
    refreshInterval = setInterval(refreshAll, 60_000);
  });

  onDestroy(() => {
    if (refreshInterval) clearInterval(refreshInterval);
  });
</script>

<div class="flex items-center justify-between mb-4 flex-wrap gap-2">
  <h1 style="font-size:20px;font-weight:700;margin:0">Dashboards</h1>
  <button class="primary" onclick={refreshAll}>↻ Refresh</button>
</div>

<p class="muted mb-4" style="font-size:14px">
  Real-time analytics computed from the local event log. Auto-refreshes every 60 seconds.
</p>

{#if error}
  <div class="card" style="border-color:var(--ps-danger);color:var(--ps-danger)">
    {error}
  </div>
{/if}

{#if dashboards.length === 0 && !error}
  <div class="card"><p class="muted">Loading dashboards...</p></div>
{/if}

{#each dashboards as d (d.id)}
  <div class="card">
    <h2>{d.title}</h2>
    <p class="muted" style="font-size:13px;margin:0 0 12px">{d.description}</p>

    {#if loading[d.id]}
      <p class="muted">Loading...</p>
    {:else if results[d.id]}
      {@const result = results[d.id]}

      {#if result.chart_type === 'kpi'}
        <div class="kpi-grid">
          {#each result.kpis as kpi}
            <div class="kpi">
              <div class="label">{kpi.label}</div>
              <div class="value">{kpi.value}</div>
            </div>
          {/each}
        </div>
      {:else if result.chart_type === 'bar'}
        {#each result.datasets as dataset}
          <div style="margin-bottom:16px">
            <h3 style="font-size:14px;font-weight:600;margin:0 0 8px">{dataset.title}</h3>
            {#if dataset.data.length > 0}
              <div class="bar-chart">
                {#each dataset.data as item}
                  {@const maxVal = Math.max(...dataset.data.map((d: any) => d.value), 1)}
                  {@const pct = (item.value / maxVal) * 100}
                  <div class="bar-row">
                    <span class="bar-label">{item.label}</span>
                    <div class="bar-track">
                      <div class="bar-fill" style="width:{pct}%"></div>
                    </div>
                    <span class="bar-value">{item.value}</span>
                  </div>
                {/each}
              </div>
            {:else}
              <p class="muted" style="font-size:13px">No data</p>
            {/if}
          </div>
        {/each}
      {:else if result.chart_type === 'table'}
        {#if result.rows.length > 0}
          <div class="table-wrap">
            <table class="data">
              <thead>
                <tr>
                  {#each result.columns as col}
                    <th>{col}</th>
                  {/each}
                </tr>
              </thead>
              <tbody>
                {#each result.rows as row}
                  <tr>
                    <td>{row.date ?? '—'}</td>
                    <td>{row.patient ?? '—'}</td>
                    <td>{row.reason ?? '—'}</td>
                    <td>{row.status ?? '—'}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {:else}
          <p class="muted" style="font-size:13px">No data</p>
        {/if}
      {/if}
    {:else}
      <p class="muted">No data</p>
    {/if}
  </div>
{/each}

<style>
  .bar-chart {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .bar-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
  }
  .bar-label {
    width: 80px;
    font-size: 13px;
    text-align: right;
    color: var(--ps-muted);
    flex-shrink: 0;
  }
  .bar-track {
    flex: 1;
    height: 24px;
    background: var(--ps-bg);
    border-radius: 4px;
    overflow: hidden;
  }
  .bar-fill {
    height: 100%;
    background: var(--ps-primary);
    border-radius: 4px;
    transition: width 0.3s ease;
    min-width: 2px;
  }
  .bar-value {
    width: 40px;
    font-size: 13px;
    font-weight: 600;
    color: var(--ps-fg);
    flex-shrink: 0;
  }
  h3 {
    color: var(--ps-fg);
  }
</style>
