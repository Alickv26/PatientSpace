<script lang="ts">
  import '../app.css';
  let { children } = $props();
</script>

<div class="app-shell">
  <header class="app-header">
    <div class="brand">PatientSpace</div>
    <nav>
      <a href="/" class="active">Home</a>
      <a href="/patients">Patients</a>
      <a href="/encounters">Encounters</a>
      <a href="/dashboards">Dashboards</a>
    </nav>
  </header>
  <main class="app-main">
    {@render children()}
  </main>
</div>
