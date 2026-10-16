<script lang="ts">
  import { goto } from '$app/navigation';

  let username = $state('');
  let password = $state('');
  let submitting = $state(false);
  let error = $state('');

  async function submit(e: Event) {
    e.preventDefault();
    error = '';
    submitting = true;

    try {
      const resp = await fetch('/api/auth/login', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username, password }),
      });

      if (resp.ok) {
        goto('/');
      } else {
        const data = await resp.json();
        error = data.error || 'Login failed';
      }
    } catch {
      error = 'Cannot connect to backend. Is the core running?';
    } finally {
      submitting = false;
    }
  }
</script>

<div style="display:flex;align-items:center;justify-content:center;min-height:80vh;padding:20px">
  <div style="width:100%;max-width:360px">
    <div style="text-align:center;margin-bottom:24px">
      <div style="font-size:28px;font-weight:700;color:var(--ps-primary)">PatientSpace</div>
      <p class="muted" style="margin:4px 0 0;font-size:14px">Sign in to continue</p>
    </div>

    {#if error}
      <div class="card" style="border-color:var(--ps-danger);color:var(--ps-danger);font-size:14px">
        {error}
      </div>
    {/if}

    <form onsubmit={submit} style="display:grid;gap:16px">
      <div class="form-group">
        <label>Username</label>
        <input bind:value={username} placeholder="admin" autocomplete="username" />
      </div>
      <div class="form-group">
        <label>Password</label>
        <input type="password" bind:value={password} placeholder="••••••••" autocomplete="current-password" />
      </div>
      <button class="primary w-full" type="submit" disabled={submitting} style="min-height:48px">
        {submitting ? 'Signing in...' : 'Sign in'}
      </button>
    </form>

    <p class="muted text-center" style="margin-top:16px;font-size:13px">
      Default: admin / admin123
    </p>
  </div>
</div>
