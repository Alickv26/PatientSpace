<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';

  let sourceType = $state<'fhir' | 'db' | 'csv' | 'auto'>('auto');
  let sourceUrl = $state('http://127.0.0.1:9090/fhir');
  let mockServerUrl = $state('http://127.0.0.1:9090/fhir');
  let mockCount = $state(50);
  let importing = $state(false);
  let result = $state<any>(null);
  let error = $state('');
  let importHistory = $state<any[]>([]);

  async function runImport() {
    importing = true;
    error = '';
    result = null;

    try {
      let patients: any[] = [];

      if (sourceType === 'fhir' || sourceType === 'auto') {
        const resp = await fetch(`${sourceUrl}/Patient?_count=1000`);
        if (resp.ok) {
          const bundle = await resp.json();
          patients = (bundle.entry ?? []).map((e: any) => e.resource);
        }
      }

      if (patients.length === 0 && (sourceType === 'db' || sourceType === 'csv' || sourceType === 'auto')) {
        error = 'CSV/DB import requires the CLI tool. Run:\n\n' +
          'cargo run -p patientspace-smartcare-adapter -- run \\\n' +
          `  --source ${sourceType} \\\n` +
          `  --source-url <path-to-file> \\\n` +
          '  --target http://localhost:8080';
        importing = false;
        return;
      }

      if (patients.length === 0) {
        error = `No patients found at ${sourceUrl}. Make sure the mock SmartCare HIE is running.\n\nStart it with:\n  cargo run -p patientspace-smartcare-adapter -- mock-server --count ${mockCount}`;
        importing = false;
        return;
      }

      let ingested = 0;
      let failed = 0;

      for (const patient of patients) {
        const patientData: Record<string, unknown> = { resourceType: 'Patient' };
        if (patient.identifier) patientData.identifier = patient.identifier;
        if (patient.name) patientData.name = patient.name;
        if (patient.telecom) patientData.telecom = patient.telecom;
        if (patient.gender) patientData.gender = patient.gender;
        if (patient.birthDate) patientData.birthDate = patient.birthDate;
        if (patient.address) patientData.address = patient.address;
        patientData.extension = [{
          url: 'http://patientspace.org/fhir/StructureDefinition/migrated-from-smartcare',
          valueString: sourceType,
        }];

        try {
          const resp = await fetch('/fhir/Patient', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(patientData),
          });
          if (resp.ok) { ingested++; } else { failed++; }
        } catch { failed++; }
      }

      result = {
        source_type: sourceType,
        source_url: sourceUrl,
        source_count: patients.length,
        ingested,
        failed,
        status: failed === 0 ? 'pass' : failed < patients.length ? 'warn' : 'fail',
      };
      importHistory = [result, ...importHistory];

      if (ingested > 0) {
        setTimeout(() => goto('/patients'), 3000);
      }
    } catch (err) {
      error = err instanceof Error ? err.message : 'Import failed';
    } finally {
      importing = false;
    }
  }

  async function startMockServer() {
    error = '';
    try {
      const resp = await fetch(`${mockServerUrl}/Patient?_count=1`);
      if (resp.ok) {
        sourceUrl = mockServerUrl;
        error = `Mock server is running at ${mockServerUrl} ✓`;
      } else {
        error = `Mock server not reachable at ${mockServerUrl}.\n\nStart it in a terminal:\n  cargo run -p patientspace-smartcare-adapter -- mock-server --count ${mockCount}`;
      }
    } catch {
      error = `Mock server not reachable at ${mockServerUrl}.\n\nStart it in a terminal:\n  cargo run -p patientspace-smartcare-adapter -- mock-server --count ${mockCount}`;
    }
  }

  function checkMockServer() {
    fetch(`${mockServerUrl}/Patient?_count=1`)
      .then(r => { if (r.ok) sourceUrl = mockServerUrl; })
      .catch(() => {});
  }

  onMount(checkMockServer);
</script>

<div class="flex items-center mb-4 gap-2">
  <a href="/"><button>← Back</button></a>
  <h1 style="font-size:20px;font-weight:700;margin:0;flex:1">Import from SmartCare</h1>
</div>

<p class="muted mb-4" style="font-size:14px">
  Migrate patient data from SmartCare into PatientSpace. Supports FHIR, database dumps, and CSV exports.
</p>

<div class="card">
  <h2>Source</h2>

  <div class="form-group">
    <label>Source type</label>
    <select bind:value={sourceType}>
      <option value="auto">Auto-detect (recommended)</option>
      <option value="fhir">FHIR endpoint (SmartCare Pro HIE)</option>
      <option value="db">Database dump (SmartCare Pro mysqldump)</option>
      <option value="csv">CSV export (SmartCare Legacy/Plus)</option>
    </select>
  </div>

  {#if sourceType === 'fhir' || sourceType === 'auto'}
    <div class="form-group">
      <label>Source URL</label>
      <input bind:value={sourceUrl} placeholder="http://127.0.0.1:9090/fhir" />
    </div>

    <div class="card" style="background:var(--ps-bg);margin-top:12px">
      <h2 style="font-size:14px">Mock SmartCare HIE (for testing)</h2>
      <p class="muted" style="font-size:13px;margin:0 0 12px">
        Start a mock SmartCare server with synthetic patients for testing the import.
      </p>
      <div class="form-grid cols-2">
        <div class="form-group">
          <label>Mock server URL</label>
          <input bind:value={mockServerUrl} />
        </div>
        <div class="form-group">
          <label>Patient count</label>
          <input type="number" bind:value={mockCount} />
        </div>
      </div>
      <button onclick={startMockServer}>Check if mock server is running</button>
    </div>
  {:else if sourceType === 'db'}
    <div class="card" style="background:var(--ps-accent-bg);border-color:var(--ps-accent)">
      <p style="margin:0;font-size:14px">
        <strong>DB import requires the CLI tool.</strong><br><br>
        Export the SmartCare Pro database:
        <br><code>mysqldump --tab=/tmp/smartcare_dump smartcare</code>
        <br><br>Then run:
        <br><code>cargo run -p patientspace-smartcare-adapter -- run \<br>
        &nbsp;&nbsp;--source db --source-url /tmp/smartcare_dump/ \<br>
        &nbsp;&nbsp;--target http://localhost:8080</code>
      </p>
    </div>
  {:else if sourceType === 'csv'}
    <div class="card" style="background:var(--ps-accent-bg);border-color:var(--ps-accent)">
      <p style="margin:0;font-size:14px">
        <strong>CSV import requires the CLI tool.</strong><br><br>
        Export patients from SmartCare as CSV, then run:
        <br><code>cargo run -p patientspace-smartcare-adapter -- run \<br>
        &nbsp;&nbsp;--source csv --source-url /path/to/patients.csv \<br>
        &nbsp;&nbsp;--target http://localhost:8080</code>
        <br><br>Expected columns: smartcard_no, nrc, surname, firstname, othernames, gender, birthdate, phone, district, address
      </p>
    </div>
  {/if}
</div>

{#if error}
  <div class="card" style="border-color:var(--ps-danger);color:var(--ps-danger);white-space:pre-wrap;font-size:13px">
    {error}
  </div>
{/if}

{#if sourceType === 'fhir' || sourceType === 'auto'}
  <button class="primary w-full" onclick={runImport} disabled={importing} style="min-height:48px">
    {importing ? 'Importing...' : 'Start import'}
  </button>
{/if}

{#if result}
  <div class="card" style="border-left:4px solid {result.status === 'pass' ? 'var(--ps-success)' : result.status === 'warn' ? 'var(--ps-accent)' : 'var(--ps-danger)'}">
    <h2>Import Result</h2>
    <div class="kpi-grid">
      <div class="kpi">
        <div class="label">Source patients</div>
        <div class="value">{result.source_count}</div>
      </div>
      <div class="kpi">
        <div class="label">Imported</div>
        <div class="value" style="color:var(--ps-success)">{result.ingested}</div>
      </div>
      <div class="kpi">
        <div class="label">Failed</div>
        <div class="value" style="color:var(--ps-danger)">{result.failed}</div>
      </div>
      <div class="kpi">
        <div class="label">Status</div>
        <div class="value" style="font-size:16px;text-transform:uppercase">{result.status}</div>
      </div>
    </div>
    {#if result.ingested > 0}
      <p class="muted text-center mt-2" style="color:var(--ps-primary)">
        Import successful! Redirecting to patient list...
      </p>
    {/if}
  </div>
{/if}

{#if importHistory.length > 0}
  <div class="card">
    <h2>Import History</h2>
    <div class="table-wrap">
      <table class="data">
        <thead>
          <tr><th>Source</th><th>URL</th><th>Count</th><th>Imported</th><th>Failed</th><th>Status</th></tr>
        </thead>
        <tbody>
          {#each importHistory as h}
            <tr>
              <td>{h.source_type}</td>
              <td style="font-size:12px;font-family:monospace">{h.source_url}</td>
              <td>{h.source_count}</td>
              <td style="color:var(--ps-success)">{h.ingested}</td>
              <td style="color:var(--ps-danger)">{h.failed}</td>
              <td>{h.status}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
{/if}

<style>
  code {
    background: var(--ps-bg);
    padding: 2px 6px;
    border-radius: 4px;
    font-family: monospace;
    font-size: 13px;
    display: inline-block;
    margin: 2px 0;
  }
</style>
