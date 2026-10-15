<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';

  let { children } = $props();

  let drawerOpen = $state(false);
  let theme = $state<'light' | 'dark'>('light');
  let online = $state(true);
  let pendingCount = $state(0);

  // Current path for active nav highlighting
  let currentPath = $state('/');

  function toggleTheme() {
    theme = theme === 'light' ? 'dark' : 'light';
    document.documentElement.setAttribute('data-theme', theme);
    localStorage.setItem('patientspace-theme', theme);
  }

  function closeDrawer() {
    drawerOpen = false;
  }

  function isActive(path: string): boolean {
    if (path === '/') return currentPath === '/';
    return currentPath.startsWith(path);
  }

  async function checkOnline() {
    try {
      const resp = await fetch('/health');
      if (resp.ok) {
        online = true;
        const health = await resp.json();
        // pending = total envelopes - synced count (simplified: just show count)
        pendingCount = health.envelope_count ?? 0;
      } else {
        online = false;
      }
    } catch {
      online = false;
    }
  }

  onMount(() => {
    // Load theme
    const saved = localStorage.getItem('patientspace-theme') as 'light' | 'dark' | null;
    if (saved) {
      theme = saved;
    } else if (window.matchMedia('(prefers-color-scheme: dark)').matches) {
      theme = 'dark';
    }
    document.documentElement.setAttribute('data-theme', theme);

    // Track current path
    currentPath = window.location.pathname;
    // Update on navigation
    const originalPushState = history.pushState;
    history.pushState = function (...args) {
      originalPushState.apply(history, args);
      currentPath = window.location.pathname;
    };
    window.addEventListener('popstate', () => {
      currentPath = window.location.pathname;
    });

    // Check online status every 30s
    checkOnline();
    const interval = setInterval(checkOnline, 30_000);
    return () => clearInterval(interval);
  });

  const navItems = [
    { href: '/', label: 'Home', icon: '🏠' },
    { href: '/patients', label: 'Patients', icon: '👥' },
    { href: '/encounters', label: 'Encounters', icon: '📋' },
    { href: '/encounters/triage', label: 'Triage', icon: '🩺' },
    { href: '/encounters/consultation', label: 'Consultation', icon: '💊' },
    { href: '/encounters/prescribe', label: 'Prescribe', icon: '✍️' },
    { href: '/pharmacy/dispense', label: 'Dispense', icon: '📦' },
    { href: '/dashboards', label: 'Dashboards', icon: '📊' },
  ];
</script>

<div class="app-shell">
  <!-- Top bar -->
  <header class="app-header">
    <button class="icon hamburger" onclick={() => (drawerOpen = true)} aria-label="Menu">
      ☰
    </button>
    <div class="brand">PatientSpace</div>
    <!-- Desktop nav -->
    <nav>
      {#each navItems.slice(0, 4) as item}
        <a href={item.href} class={isActive(item.href) ? 'active' : ''}>{item.label}</a>
      {/each}
    </nav>
    <div style="display:flex;align-items:center;gap:8px">
      {#if !online}
        <span class="sync-badge critical" title="Offline">⚠️</span>
      {:else if pendingCount > 0}
        <span class="sync-badge pending" title="{pendingCount} records pending sync">
          {pendingCount}
        </span>
      {/if}
      <button class="icon" onclick={toggleTheme} aria-label="Toggle theme">
        {theme === 'light' ? '🌙' : '☀️'}
      </button>
    </div>
  </header>

  <!-- Offline banner -->
  {#if !online}
    <div class="offline-banner">
      ⚠️ <strong>Offline</strong> — changes will sync when connected
    </div>
  {/if}

  <!-- Drawer overlay -->
  {#if drawerOpen}
    <div class="drawer-overlay open" onclick={closeDrawer}></div>
  {/if}

  <!-- Drawer -->
  <div class="drawer" class:open={drawerOpen}>
    <div class="drawer-header">
      <span>PatientSpace</span>
    </div>
    <nav class="drawer-nav">
      {#each navItems as item}
        <a
          href={item.href}
          class={isActive(item.href) ? 'active' : ''}
          onclick={closeDrawer}
        >
          <span style="font-size:20px">{item.icon}</span>
          {item.label}
        </a>
      {/each}
    </nav>
    <div class="drawer-footer">
      {#if online}
        🟢 Online · {pendingCount} records
      {:else}
        🔴 Offline
      {/if}
    </div>
  </div>

  <!-- Main content -->
  <main class="app-main">
    {@render children()}
  </main>
</div>
