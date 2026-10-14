<script lang="ts">
  let smartcard = $state('');
  let nrc = $state('');
  let surname = $state('');
  let forenames = $state('');
  let phone = $state('');
  let yearOfBirth = $state('');
  let gender = $state<'female' | 'male' | 'other' | 'unknown'>('unknown');
  let message = $state('');

  async function submit(e: Event) {
    e.preventDefault();
    message = 'Patient creation is not yet wired up — see direction.md step 5.';
  }
</script>

<h1 style="font-size:22px;font-weight:700;margin:0 0 16px">Register new patient</h1>

<form onsubmit={submit} style="display:grid;gap:14px;max-width:640px">
  <div>
    <label>Smartcard No.</label>
    <input bind:value={smartcard} placeholder="SC-12345678" required />
  </div>
  <div>
    <label>NRC (National Registration Card)</label>
    <input bind:value={nrc} placeholder="123456/78/1" />
  </div>
  <div style="display:grid;grid-template-columns:1fr 1fr;gap:14px">
    <div>
      <label>Surname</label>
      <input bind:value={surname} required />
    </div>
    <div>
      <label>Forenames</label>
      <input bind:value={forenames} required />
    </div>
  </div>
  <div>
    <label>Mobile phone</label>
    <input bind:value={phone} placeholder="+260 97 123 4567" />
  </div>
  <div style="display:grid;grid-template-columns:1fr 1fr;gap:14px">
    <div>
      <label>Year of birth</label>
      <input type="number" bind:value={yearOfBirth} placeholder="1990" />
    </div>
    <div>
      <label>Gender</label>
      <select bind:value={gender}>
        <option value="unknown">unknown</option>
        <option value="female">female</option>
        <option value="male">male</option>
        <option value="other">other</option>
      </select>
    </div>
  </div>
  <div>
    <button class="primary" type="submit">Register patient</button>
    {#if message}<span class="muted" style="margin-left:12px">{message}</span>{/if}
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
</style>
