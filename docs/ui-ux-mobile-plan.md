# PatientSpace Mobile-First UI/UX Plan

**Status:** v0.2 — mobile-first responsive redesign
**Last updated:** 2026-10-16

## Design decisions (locked)

| Dimension | Decision |
|---|---|
| Primary devices | Phone (375px) + tablet (768px) |
| Navigation | Hamburger drawer on mobile/tablet, top bar on desktop |
| Patient list | Scrollable table with sticky first column (Name) |
| Forms | Stacked sections — all fields visible, grouped, vertically scrollable |
| Color scheme | Light (default) + dark mode toggle. Respects `prefers-color-scheme` |
| Offline UX | Offline banner + action queue indicator |

## Breakpoints

| Breakpoint | Width | Target |
|---|---|---|
| `sm` | >= 640px | Large phones (landscape), small tablets |
| `md` | >= 768px | Tablets (portrait) |
| `lg` | >= 1024px | Tablets (landscape), small desktops |
| `xl` | >= 1280px | Desktops |

Default (no media query): phone portrait (< 640px).

## Navigation

### Mobile/tablet (< 1024px): hamburger drawer
- Top bar: hamburger icon (☰) left, "PatientSpace" brand center, dark mode toggle (🌙/☀️) right
- Tap hamburger → drawer slides in from left, pushes content right
- Drawer items: Home, Patients, Encounters, Dashboards, Triage, Prescribe, Dispense
- Drawer has facility ID at bottom + sync status indicator
- Tap outside drawer or any nav item → drawer closes
- Swipe-left-to-close gesture (future)

### Desktop (>= 1024px): top nav bar
- Current horizontal nav bar (Home, Patients, Encounters, Dashboards)
- Dark mode toggle at far right
- No hamburger — full nav visible

## Color scheme

### Light mode (default)
```
--ps-bg: #fafafa
--ps-surface: #ffffff
--ps-fg: #1a1a1a
--ps-muted: #6b7280
--ps-border: #e5e7eb
--ps-primary: #0f6b3f
--ps-primary-fg: #ffffff
--ps-accent: #c9a227
--ps-danger: #b91c1c
```

### Dark mode
```
--ps-bg: #0f1614
--ps-surface: #1a2420
--ps-fg: #e5e7eb
--ps-muted: #9ca3af
--ps-border: #2d3a35
--ps-primary: #16a34a
--ps-primary-fg: #ffffff
--ps-accent: #eab308
--ps-danger: #ef4444
```

### Toggle
- 🌙/☀️ icon button in the top bar
- Stored in `localStorage('patientspace-theme')`
- On load: check localStorage, fall back to `prefers-color-scheme`
- Toggling sets `data-theme="dark"` on `<html>` element

## Tap targets
- All buttons, nav items, table rows: `min-height: 44px`
- Inputs: `min-height: 44px`, `font-size: 16px` (prevents iOS zoom)
- Checkbox/radio: `min-width: 44px`, `min-height: 44px` with visible label area

## Patient list — scrollable table

### Mobile/tablet
- Table fills viewport width, horizontal scroll if needed
- First column (Name) is sticky: `position: sticky; left: 0; z-index: 1` with solid background
- Other columns scroll horizontally underneath
- Column widths: Name 140px, Smartcard 120px, NRC 120px, Gender 80px, DOB 100px
- Search bar stays pinned at top (sticky), table scrolls below
- Pagination at bottom: large Previous/Next buttons (44px height)

### Desktop (>= 1024px)
- Table fits without horizontal scroll
- Same column layout but more padding

## Forms — stacked sections

### Registration form
- All fields stacked vertically on mobile
- Two-column grids (Surname/Forename, DOB/Gender) collapse to single column on mobile, two columns on tablet+
- Section headers: "Identification" (Smartcard, NRC), "Personal" (Name, DOB, Gender), "Contact" (Phone)
- Each section in a card with a subtle header
- Submit button: full-width, 48px height, primary color

### Triage form
- Vitals grid: 4 columns on desktop → 2 columns on tablet → 1 column on phone
- Vitals: BP Systolic, BP Diastolic, Heart rate, Temperature, Weight
- Each vital in its own card with label + input + unit suffix
- Complaint: full-width textarea
- Triage category: large radio buttons (color-coded: red/orange/yellow/green), 44px height each

### Consultation form
- Diagnosis + ICD-10: stacked, 1 column
- Notes + Plan: full-width textareas
- Submit: full-width

### Prescription form
- Medication + ATC: stacked
- Dose/Frequency/Duration: 3 columns on desktop → 1 column on mobile

## Offline banner + queue

### Offline banner
- Appears below the top bar when the hub is unreachable
- Yellow background (`--ps-accent`), dark text
- "⚠️ Offline — changes will sync when connected"
- Dismissible (×) but reappears if still offline on next check
- Checks `/health` every 30s; if unreachable, shows banner

### Action queue indicator
- Small badge in the top bar next to the hamburger: shows count of unsynced envelopes
- Query: `GET /health` → `envelope_count` minus last known synced count
- Badge color: green (0 unsynced), yellow (1-10), red (>10)
- Tap badge → toast: "N records pending sync"

## Patient detail — mobile layout

### Mobile
- Patient name + demographics in a compact card at top (no sidebar)
- Tabs: horizontal scrollable tabs (Encounters, Observations, Medications)
- Tab content fills remaining viewport
- Conflict banner: full-width yellow strip below the header

### Desktop
- Same layout but wider, with more breathing room
- Tabs don't need horizontal scroll

## Implementation order

1. **`app.css`** — design tokens (light + dark), breakpoints, tap targets, drawer styles, table sticky column, form responsive grids
2. **`+layout.svelte`** — hamburger drawer nav, dark mode toggle, offline banner, sync badge
3. **`+page.svelte`** (home) — responsive KPI grid, quick actions (2-column on mobile, 4-column on desktop)
4. **`patients/+page.svelte`** — scrollable table with sticky first column, sticky search bar
5. **`patients/new/+page.svelte`** — stacked sections with responsive grids
6. **`patients/[id]/+page.svelte`** — compact mobile detail layout, scrollable tabs
7. **`encounters/triage/+page.svelte`** — responsive vitals grid, color-coded triage category
8. **`encounters/consultation/+page.svelte`** — stacked form
9. **`encounters/prescribe/+page.svelte`** — responsive dose grid
10. **`pharmacy/dispense/+page.svelte`** — stacked form with large checkbox
