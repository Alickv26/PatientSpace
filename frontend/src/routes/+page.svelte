<script lang="ts">
  import { onMount } from 'svelte';

  let coreHealth = $state<{ status: string; facility_id: string; version: string } | null>(null);
  let pingResult = $state<string>('');

  onMount(async () => {
    try {
      const r = await fetch('/health');
      coreHealth = await r.json();
    } catch (e) {
      coreHealth = null;
    }
    try {
      const p = await fetch('https://hub.patientspace.org/psep/v1/ping');
      pingResult = await p.text();
    } catch {
      pingResult = '(hub unreachable — expected in dev)';
    }
  });
</script>

<h1 style="font-size:22px;font-weight:700;margin:0 0 8px">Welcome to PatientSpace</h1>
<p class="muted" style="margin:0 0 20px;max-width:640px">
  Offline-first EHR for primary care. Co-exists with SmartCare via FHIR/HIE. Headline
  differentiator: real-time clinician analytics at the point of care. Pilot site: Petauke
  District Hospital, Eastern Province, Zambia.
</p>

<div class="kpi-grid">
  <div class="kpi">
    <div class="label">Local facility</div>
    <div class="value" style="font-size:14px;font-family:monospace">
      {coreHealth?.facility_id ?? '—'}
    </div>
  </div>
  <div class="kpi">
    <div class="label">Core version</div>
    <div class="value" style="font-size:14px;font-family:monospace">
      {coreHealth?.version ?? '—'}
    </div>
  </div>
  <div class="kpi">
    <div class="label">Core status</div>
    <div class="value" style="font-size:14px">{coreHealth?.status ?? 'down'}</div>
  </div>
  <div class="kpi">
    <div class="label">Hub reachability</div>
    <div class="value" style="font-size:14px">{pingResult === 'pong' ? 'reachable' : 'offline'}</div>
  </div>
</div>

<div class="card">
  <h2>What works in this scaffold</h2>
  <ul style="margin:0;padding-left:20px;line-height:1.8">
    <li><b>Rust core binary</b> — <code>patientspace-core</code> serves <code>/health</code> and stub FHIR endpoints on <code>127.0.0.1:8080</code>.</li>
    <li><b>Rust hub binary</b> — <code>patientspace-hub</code> serves <code>/psep/v1/ping</code> and a stub handshake.</li>
    <li><b>PSEP envelope</b> — full crypto round-trip (Ed25519 sign + X25519 ECDH + AES-GCM sealed-box) covered by unit tests in <code>crates/sync</code>.</li>
    <li><b>FHIR Patient</b> — R5 resource type with JSON round-trip tests in <code>crates/fhir</code>.</li>
    <li><b>RBAC</b> — role/permission matrix in <code>crates/auth</code>.</li>
    <li><b>Dashboards</b> — three built-in SQL dashboards defined in <code>crates/analytics</code>.</li>
    <li><b>Svelte 5 UI shell</b> — this page. Vite proxies <code>/fhir</code>, <code>/health</code>, <code>/psep</code> to the Rust core.</li>
  </ul>
</div>

<div class="card">
  <h2>What's next (per direction.md §"Immediate next steps")</h2>
  <ol style="margin:0;padding-left:20px;line-height:1.8">
    <li>Write <code>docs/architecture.md</code> + <code>docs/data-model.md</code>.</li>
    <li>Prototype <code>crates/sync</code> end-to-end: PSEP envelope + SQLite log + sync agent over HTTPS.</li>
    <li>Prototype <code>crates/fhir</code> Patient round-trip through the custom store.</li>
    <li>Build <code>crates/smartcare-adapter</code> stub against a mock SmartCare HIE.</li>
    <li>Build Petauke minimum workflow: registration → triage → consultation → prescription → pharmacy.</li>
    <li>MoH + Petauke IT engagement — pin SmartCare access mode for migration.</li>
  </ol>
</div>
