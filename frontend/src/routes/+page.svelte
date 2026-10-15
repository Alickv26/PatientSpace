<script lang="ts">
  import { onMount } from 'svelte';

  interface CoreHealth {
    status: string;
    facility_id: string;
    version: string;
    envelope_count: number;
  }

  let coreHealth = $state<CoreHealth | null>(null);
  let hubReachable = $state<boolean | null>(null);
  let hubDetail = $state<string>('');

  onMount(async () => {
    try {
      const r = await fetch('/health');
      if (r.ok) {
        coreHealth = await r.json();
      } else {
        coreHealth = null;
      }
    } catch {
      coreHealth = null;
    }
    try {
      const p = await fetch('/psep/v1/ping');
      if (p.ok) {
        const t = await p.text();
        hubReachable = t.trim() === 'pong';
        hubDetail = t.trim();
      } else {
        hubReachable = false;
        hubDetail = `HTTP ${p.status}`;
      }
    } catch {
      hubReachable = false;
      hubDetail = '(hub unreachable — expected in dev)';
    }
  });
</script>

<h1 style="font-size:22px;font-weight:700;margin:0 0 8px">Welcome to PatientSpace</h1>
<p class="muted" style="margin:0 0 20px;max-width:640px">
  Offline-first EHR for primary care. Co-exists with SmartCare via FHIR/HIE. Headline
  differentiator: real-time clinician analytics at the point of care. Pilot site: Petauke
  District Hospital, Eastern Province, Zambia.
</p>

<div class="quick-actions">
  <a class="qa" href="/patients/new">
    <div class="qa-icon">+</div>
    <div class="qa-label">Register patient</div>
  </a>
  <a class="qa" href="/patients">
    <div class="qa-icon">☰</div>
    <div class="qa-label">Patient list</div>
  </a>
  <a class="qa" href="/encounters">
    <div class="qa-icon">♥</div>
    <div class="qa-label">Start triage</div>
  </a>
  <a class="qa" href="/dashboards">
    <div class="qa-icon">▦</div>
    <div class="qa-label">View dashboards</div>
  </a>
</div>

<div class="kpi-grid">
  <div class="kpi">
    <div class="label">Local facility</div>
    <div class="value" style="font-size:14px;font-family:monospace">
      {coreHealth?.facility_id ?? '—'}
    </div>
  </div>
  <div class="kpi">
    <div class="label">Core status</div>
    <div class="value" style="font-size:14px">
      {coreHealth?.status ?? 'down'}
    </div>
  </div>
  <div class="kpi">
    <div class="label">Core version</div>
    <div class="value" style="font-size:14px;font-family:monospace">
      {coreHealth?.version ?? '—'}
    </div>
  </div>
  <div class="kpi">
    <div class="label">Envelope count</div>
    <div class="value" style="font-size:14px;font-family:monospace">
      {coreHealth?.envelope_count?.toString() ?? '—'}
    </div>
  </div>
  <div class="kpi">
    <div class="label">Hub reachability</div>
    <div class="value" style="font-size:14px">
      {hubReachable === null
        ? '…'
        : hubReachable
          ? 'reachable'
          : 'offline'}
    </div>
    <div class="muted" style="font-size:11px;margin-top:2px">{hubDetail}</div>
  </div>
</div>

<div class="card">
  <h2>What works in v0.1</h2>
  <ul style="margin:0;padding-left:20px;line-height:1.8">
    <li><b>Rust core binary</b> — <code>patientspace-core</code> serves <code>/health</code>, FHIR Patient <code>POST</code> / <code>GET</code> / list / search on <code>127.0.0.1:8080</code>. All writes sealed into PSEP envelopes and ingested into the local append-only SQLite log.</li>
    <li><b>PSEP envelope</b> — Ed25519 sign + X25519 ECDH + AES-GCM sealed-box round-trip, covered by unit tests in <code>crates/sync</code>.</li>
    <li><b>FHIR Patient</b> — R5 resource type with JSON round-trip tests in <code>crates/fhir</code>; PatientSpace <code>conflict</code> extension surfaced on reads when concurrent versions exist.</li>
    <li><b>Svelte 5 UI</b> — patient registration, list with search + pagination, detail view, triage / consultation / prescription / pharmacy dispense stubs. Vite proxies <code>/fhir</code>, <code>/health</code>, <code>/psep</code> to the Rust core.</li>
    <li><b>Dashboards</b> — three built-in SQL dashboards defined in <code>crates/analytics</code> (wiring is the next milestone).</li>
  </ul>
</div>

<style>
  .quick-actions {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 12px;
    margin-bottom: 16px;
  }
  .qa {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 16px 12px;
    background: white;
    border: 1px solid var(--ps-border);
    border-radius: 6px;
    color: var(--ps-fg);
    text-decoration: none;
    transition: all 0.15s ease;
  }
  .qa:hover {
    border-color: var(--ps-primary);
    text-decoration: none;
    transform: translateY(-1px);
  }
  .qa-icon {
    font-size: 22px;
    color: var(--ps-primary);
    font-weight: 700;
  }
  .qa-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--ps-fg);
  }
</style>
