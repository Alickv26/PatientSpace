<script lang="ts">
  import { createPatient } from '$lib/api/patient';

  let smartcard = $state('');
  let nrc = $state('');
  let familyName = $state('');
  let givenName = $state('');
  let phone = $state('');
  let birthDate = $state('');
  let gender = $state<'female' | 'male' | 'other' | 'unknown'>('unknown');
  let submitting = $state(false);
  let error = $state('');

  async function submit(e: Event) {
    e.preventDefault();
    error = '';
    submitting = true;
    try {
      const patient = await createPatient({
        smartcardNo: smartcard || undefined,
        nrc: nrc || undefined,
        familyName: familyName || undefined,
        givenNames: givenName || undefined,
        phone: phone || undefined,
        gender,
        birthDate: birthDate || undefined,
      });
      window.location.href = `/patients/${patient.id}`;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Unknown error';
    } finally {
      submitting = false;
    }
  }
</script>

<div class="flex items-center mb-4 gap-2">
  <a href="/patients"><button>← Back</button></a>
  <h1 style="font-size:20px;font-weight:700;margin:0;flex:1">Register patient</h1>
</div>

{#if error}
  <div class="card" style="border-color:var(--ps-danger);color:var(--ps-danger)">
    {error}
  </div>
{/if}

<form onsubmit={submit} style="display:grid;gap:16px">
  <!-- Identification section -->
  <div class="card">
    <h2>Identification</h2>
    <div class="form-group">
      <label>Smartcard No.</label>
      <input bind:value={smartcard} placeholder="SC-12345678" />
    </div>
    <div class="form-group">
      <label>NRC (National Registration Card)</label>
      <input bind:value={nrc} placeholder="123456/78/1" />
    </div>
  </div>

  <!-- Personal section -->
  <div class="card">
    <h2>Personal</h2>
    <div class="form-grid cols-2">
      <div class="form-group">
        <label>Surname</label>
        <input bind:value={familyName} />
      </div>
      <div class="form-group">
        <label>Forename</label>
        <input bind:value={givenName} />
      </div>
    </div>
    <div class="form-grid cols-2">
      <div class="form-group">
        <label>Date of birth</label>
        <input type="date" bind:value={birthDate} />
      </div>
      <div class="form-group">
        <label>Gender</label>
        <select bind:value={gender}>
          <option value="unknown">unknown</option>
          <option value="female">female</option>
          <option value="male">male</option>
          <option value="other">other</option>
        </select>
      </div>
    </div>
  </div>

  <!-- Contact section -->
  <div class="card">
    <h2>Contact</h2>
    <div class="form-group">
      <label>Mobile phone</label>
      <input bind:value={phone} placeholder="+260 97 123 4567" />
    </div>
  </div>

  <button class="primary w-full" type="submit" disabled={submitting} style="min-height:48px">
    {submitting ? 'Registering...' : 'Register patient'}
  </button>
</form>
