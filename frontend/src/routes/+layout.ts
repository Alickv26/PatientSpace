// SvelteKit: this file is rendered on the server for the initial load.
// Keep it minimal — the patient-facing shell is a SPA after first paint.
//
// prerender = false: query-param routes (search, pagination) don't
// prerender well — SvelteKit would have to crawl every search term.
// SSR stays on so first paint is fast even on slow links.
export const prerender = false;
export const ssr = true;
