<script lang="ts">
  import { onMount } from 'svelte';

  let coreHealth = $state<{
    status: string;
    facility_id: string;
    version: string;
    envelope_count?: number;
  } | null>(null);
  let pingResult = $state<string>('');

  onMount(async () => {
    try {
      const r = await fetch('/health');
      coreHealth = await r.json();
    } catch {
      coreHealth = null;
    }
    try {
      const p = await fetch('/psep/v1/ping');
      pingResult = await p.text();
    } catch {
      pingResult = '(hub unreachable — expected in dev)';
    }
  });
</script>

<h1 style="font-size:22px;font-weight:700;margin:0 0 8px">PatientSpace</h1>
<p class="muted" style="margin:0 0 16px">
  Offline-first EHR for primary care. Pilot site: Petauke District Hospital, Eastern Province, Zambia.
</p>

<div class="kpi-grid">
  <div class="kpi">
    <div class="label">Facility</div>
    <div class="value" style="font-size:14px;font-family:monospace">
      {coreHealth?.facility_id ?? '—'}
    </div>
  </div>
  <div class="kpi">
    <div class="label">Status</div>
    <div class="value" style="font-size:14px">{coreHealth?.status ?? 'down'}</div>
  </div>
  <div class="kpi">
    <div class="label">Records</div>
    <div class="value" style="font-size:14px">{coreHealth?.envelope_count ?? '—'}</div>
  </div>
  <div class="kpi">
    <div class="label">Hub</div>
    <div class="value" style="font-size:14px">
      {pingResult === 'pong' ? '🟢 reachable' : '🔴 offline'}
    </div>
  </div>
</div>

<div class="card">
  <h2>Quick actions</h2>
  <div class="form-grid cols-2">
    <a href="/patients/new"><button class="primary">Register patient</button></a>
    <a href="/patients"><button>Patient list</button></a>
    <a href="/import"><button class="primary">Import from SmartCare</button></a>
    <a href="/encounters/triage"><button>Start triage</button></a>
    <a href="/dashboards"><button>Dashboards</button></a>
    <a href="/conflicts"><button>Conflict queue</button></a>
  </div>
</div>

<div class="card">
  <h2>What works in v0.1</h2>
  <ul style="margin:0;padding-left:20px;line-height:1.8;font-size:14px">
    <li><b>PSEP sync</b> — two facilities sync through a hub with E2E encryption, conflict detection, sneakernet</li>
    <li><b>FHIR API</b> — POST/GET/list/search patients</li>
    <li><b>Clinical workflow</b> — registration, list, detail, triage/consult/prescribe/dispense</li>
    <li><b>Mobile-first UI</b> — hamburger drawer, dark mode, offline banner, 44px tap targets</li>
  </ul>
</div>
