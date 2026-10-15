<script lang="ts">
  import { goto } from '$app/navigation';
  import { createPatient, type NewPatientInput } from '$lib/api/patient';

  let smartcardNo = $state('');
  let nrc = $state('');
  let familyName = $state('');
  let givenNames = $state('');
  let phone = $state('');
  let birthDate = $state('');
  let gender = $state<'female' | 'male' | 'other' | 'unknown'>('unknown');

  let submitting = $state(false);
  let error = $state<string | null>(null);

  async function submit(e: Event) {
    e.preventDefault();
    if (submitting) return;
    error = null;
    submitting = true;
    try {
      const input: NewPatientInput = {
        smartcardNo: smartcardNo.trim() || undefined,
        nrc: nrc.trim() || undefined,
        familyName: familyName.trim() || undefined,
        givenNames: givenNames.trim() || undefined,
        phone: phone.trim() || undefined,
        birthDate: birthDate || undefined,
        gender,
      };
      const patient = await createPatient(input);
      // Server returns the canonical Patient JSON with the assigned id.
      if (patient?.id) {
        await goto(`/patients/${encodeURIComponent(patient.id)}`);
      } else {
        error = 'Server returned a Patient without an id.';
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      submitting = false;
    }
  }
</script>

<h1 style="font-size:22px;font-weight:700;margin:0 0 16px">Register new patient</h1>
<p class="muted" style="margin:0 0 16px;max-width:640px">
  On submit, the Svelte client POSTs a FHIR R5 Patient to <code>/fhir/Patient</code>.
  The Rust core assigns a fresh PatientSpace resource_id (UUIDv7), seals the resource
  into a PSEP envelope addressed to the local facility, and ingests it into the
  append-only SQLite log.
</p>

<form onsubmit={submit} style="display:grid;gap:14px;max-width:640px">
  <div>
    <label for="smartcard">Smartcard No.</label>
    <input
      id="smartcard"
      bind:value={smartcardNo}
      placeholder="SC-12345678"
      autocomplete="off"
    />
  </div>
  <div>
    <label for="nrc">NRC (National Registration Card)</label>
    <input
      id="nrc"
      bind:value={nrc}
      placeholder="123456/78/1"
      autocomplete="off"
    />
  </div>
  <div style="display:grid;grid-template-columns:1fr 1fr;gap:14px">
    <div>
      <label for="family">Surname</label>
      <input id="family" bind:value={familyName} required autocomplete="off" />
    </div>
    <div>
      <label for="given">Forename(s)</label>
      <input
        id="given"
        bind:value={givenNames}
        placeholder="Chipo"
        required
        autocomplete="off"
      />
    </div>
  </div>
  <div>
    <label for="phone">Mobile phone</label>
    <input
      id="phone"
      bind:value={phone}
      placeholder="+260 97 123 4567"
      autocomplete="off"
    />
  </div>
  <div style="display:grid;grid-template-columns:1fr 1fr;gap:14px">
    <div>
      <label for="dob">Date of birth</label>
      <input id="dob" type="date" bind:value={birthDate} />
    </div>
    <div>
      <label for="gender">Gender</label>
      <select id="gender" bind:value={gender}>
        <option value="unknown">unknown</option>
        <option value="female">female</option>
        <option value="male">male</option>
        <option value="other">other</option>
      </select>
    </div>
  </div>
  {#if error}
    <div class="error-banner">{error}</div>
  {/if}
  <div style="display:flex;align-items:center;gap:12px">
    <button class="primary" type="submit" disabled={submitting}>
      {submitting ? 'Registering…' : 'Register patient'}
    </button>
    <a href="/patients" class="muted">Cancel</a>
  </div>
</form>

<style>
  label {
    display: block;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--ps-muted);
    margin-bottom: 4px;
  }
  .error-banner {
    background: #fef2f2;
    border: 1px solid var(--ps-danger);
    color: var(--ps-danger);
    padding: 8px 12px;
    border-radius: 4px;
    font-size: 13px;
  }
</style>
