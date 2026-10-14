<script lang="ts">
  // Stub: in v0.1 these dashboard definitions live in `crates/analytics/src/lib.rs`.
  // The Rust core will expose them at /api/dashboards, and a future endpoint
  // /api/dashboards/:id/run will execute them against embedded DuckDB.
  const dashboards = [
    {
      id: 'clinic_today',
      title: 'Clinic Today',
      description: 'Queue length, avg wait, suspected TB cases flagged, defaulters on ART walked in today.',
    },
    {
      id: 'art_defaulters',
      title: 'ART Defaulters (30+ days overdue)',
      description: 'Patients on ART who have missed their expected pickup date by 30+ days.',
    },
    {
      id: 'hiv_viral_suppression',
      title: 'HIV Viral Suppression Rate (24 months)',
      description: 'Percentage of patients with viral load < 1000 copies/mL in the last 24 months.',
    },
  ];
</script>

<h1 style="font-size:22px;font-weight:700;margin:0 0 16px">Dashboards</h1>
<p class="muted" style="margin:0 0 16px;max-width:640px">
  These three dashboards are defined in <code>crates/analytics/src/lib.rs</code>. In v0.1 they
  run against embedded DuckDB inside the Rust core binary — the headline differentiator vs
  SmartCare. Wiring is the next milestone.
</p>

<div class="kpi-grid">
  {#each dashboards as d}
    <div class="card" style="margin:0">
      <h2 style="font-size:13px">{d.title}</h2>
      <p class="muted" style="font-size:12px;line-height:1.5;margin:0">{d.description}</p>
      <p style="margin:8px 0 0;font-size:11px"><code>{d.id}</code></p>
    </div>
  {/each}
</div>
