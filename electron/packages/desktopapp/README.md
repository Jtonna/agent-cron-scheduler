# ACS Desktop App

Desktop UI for Agent Cron Scheduler. Built with Next.js 16 (App Router), React 19, TanStack Query 5, Tailwind v4, React Aria, Recharts, Lucide icons, and Storybook 10.

This README is the canonical reference for **how this app is built and maintained**. AI agents and humans should read it before adding new components, pages, hooks, or making structural changes — the conventions below are enforced by the build, the lint config, and reviewer expectations.

---

## Quick reference

```bash
# From the electron/ root
npm install                                           # install everything
npm run dev --workspace=packages/desktopapp           # Next.js dev server   → http://localhost:3000
npm run storybook --workspace=packages/desktopapp     # Storybook            → http://localhost:6006

# From packages/desktopapp/
npm test               # vitest watch mode
npm run test:run       # vitest single run
npm run test:coverage  # coverage report
npm run lint           # ESLint
npm run build          # next build
npm run build-storybook
```

Backend defaults to `http://127.0.0.1:8377`. Override with `NEXT_PUBLIC_API_URL` or set `window.__ACS_API_URL__` at runtime (used when packaged in Electron).

---

## Tech stack

| Layer | Library | Why |
|---|---|---|
| Framework | Next.js 16 (App Router, dev runs on `--webpack`) | App Router conventions; webpack avoids Turbopack's long-running dev memory leak |
| UI primitives | `react-aria-components` | Accessibility + keyboard nav out of the box |
| Data fetching | `@tanstack/react-query` | Cache, stale-while-revalidate, dedup, refetch-on-focus |
| Real-time | Native `EventSource` (SSE) | Single persistent connection at the provider level, drives query invalidation |
| Charts | `recharts` | Used by `HealthWidget` (donut) and `CostTrendWidget` (area) |
| Icons | `lucide-react` | One icon library across the app |
| Styling | `tailwindcss` v4 with `@theme` tokens | All visual values are CSS variables; no raw colors |
| Stories | `storybook` v10 + `@storybook/nextjs-vite` | Co-located `.stories.tsx` per component |
| Tests | `vitest` + `@testing-library/react` + `jsdom` | Smoke tests for utilities, hooks, key components |
| Lint | `eslint` v9 (flat config) + `eslint-config-next` + `eslint-plugin-storybook` | |
| Log viewer | `@melloware/react-logviewer` | The system logs page |

---

## Project structure

```
src/
├── apis/                          # data layer — survives across page navigation
│   ├── client.ts                  # fetch wrapper + ApiError + the api.* object
│   ├── types.ts                   # TypeScript types matching the backend (Job, JobRun, RecentRunEntry, etc.)
│   ├── format.ts                  # date / duration / cost formatting helpers
│   ├── jobStatus.ts               # isRunning, groupRunsByJob, AnyRun helper type
│   ├── providers.tsx              # <Providers> = RouterProvider + QueryClient + SSE + SSEQueryBridge
│   ├── sse.tsx                    # SSEProvider, useSSEEvents, useSSEConnected
│   ├── sseInvalidator.tsx         # SSEQueryBridge — single SSE→query invalidation point
│   ├── useHealth.ts               # GET /health
│   ├── useJobs.ts                 # GET /api/jobs
│   ├── useGlobalCostSummary.ts    # GET /api/costs/summary
│   ├── useJobRuns.ts              # GET /api/jobs/{id}/runs
│   ├── useRecentRuns.ts           # GET /api/runs/recent (with grow-limit pagination)
│   └── useSystemLogs.ts           # GET /api/logs (with SSE-driven append + 1MB cap)
│
├── app/                           # Next.js App Router
│   ├── globals.css                # @theme tokens + global resets
│   ├── layout.tsx                 # root layout; wraps in <Providers>
│   ├── not-found.tsx              # /_not-found page
│   ├── page.tsx                   # /  (dashboard)
│   ├── chat/page.tsx              # /chat (placeholder)
│   ├── create/page.tsx            # /create (placeholder for "Build a Job")
│   ├── jobs/page.tsx              # /jobs (operational hub: sidebar + widgets + jobs list)
│   └── systemlogs/page.tsx        # /systemlogs (LazyLog viewer)
│
└── components/
    ├── ui/                        # truly cross-cutting design primitives
    ├── navbar/                    # the top navbar + its dropdowns
    ├── sidebar/                   # the left sidebar shell + sections + JobsSidebar
    ├── jobs/                      # job-specific composites used in main content
    └── widgets/                   # dashboard tiles (StatWidget shell + concrete widgets)
```

### `.storybook/` and tooling

```
.storybook/
├── main.ts                        # framework + addons + stories glob
└── preview.tsx                    # global decorator: QueryClientProvider for stories that use hooks

vitest.config.ts                   # test runner config (jsdom + react plugin + @ alias)
vitest.setup.ts                    # @testing-library/jest-dom matchers
eslint.config.mjs                  # flat config
postcss.config.mjs                 # Tailwind v4 postcss
next.config.ts                     # Next.js config
tsconfig.json                      # strict TypeScript
```

---

## Component organization — the four-folder system

Every component lives in **exactly one** of these four folders. The folder you pick is determined by a strict question:

```
ui/      — truly cross-cutting primitives. No domain knowledge.
           Could be lifted into another app unchanged.
           Examples: Button, Pill, JobStateIndicator, ChatBar, TabBar, RunTooltip.

navbar/  — anything tied to the top navbar.
           Examples: Navbar, ApiReferencesDropdown, InstanceDropdown.

sidebar/ — anything tied to the left sidebar (shell + sections + sidebar-specific items).
           Examples: Sidebar, SidebarSection, SidebarSectionHeading,
                     JobsSidebar, SidebarRecentJobs, JobSidebarItem.

jobs/    — job composites used in main content (NOT in the sidebar).
           Examples: JobsList, JobsListRow, JobRunCard, FilterTabs, FavoritedJobs.

widgets/ — full self-contained dashboard widgets (each composes StatWidget internally).
           Examples: StatWidget (shell), CostWidget, HealthWidget,
                     TopSpendersWidget, CostTrendWidget, SystemBanner.
```

**Layering rule:** dependencies flow inward → `widgets/`, `jobs/`, `sidebar/`, `navbar/` may import from `ui/`, never the other way. `ui/` has no domain imports.

**When in doubt:** if you'd render it in two unrelated screens, it's `ui/`. If it has a `Sidebar` prefix, it's `sidebar/`. If it owns a `<StatWidget>` wrapper, it's `widgets/`. Otherwise it's `jobs/`.

---

## Component conventions

### File structure
```
ComponentName.tsx           # the component
ComponentName.stories.tsx   # at least one default story + one variant
ComponentName.test.tsx      # optional, but encouraged for components with logic
```

### File top-of-file docblock (mandatory)

Every component starts with a one-paragraph docblock describing what it is, where it shows up, and any non-obvious behavior:

```tsx
/**
 * Button
 *
 * The app's primary action button. Polymorphic: renders a Link, a React
 * Aria Button, or a static span depending on which prop is provided.
 *
 * Intents (primary / secondary / ghost), sizes (sm / md / lg), and
 * shapes (pill / rounded) compose into the final visual.
 */
```

### Props interfaces

- Always named `<ComponentName>Props`
- Always exported alongside the component if a parent might need to type something against them
- Polymorphic components use **discriminated unions** for mutually exclusive prop sets (see `Button`, `Pill`, `JobStateIndicator`)
- Optional props always have explicit defaults in destructuring
- Non-obvious props get jsdoc

### `"use client"`

- Add it **only when needed**: components using hooks (`useState`, `useEffect`, `useRouter`, anything from `react-aria-components`, anything from `@tanstack/react-query`).
- Pages that compose only client components don't need it themselves unless they use hooks directly.
- Server components stay default — no directive.

### Styling

- **Design tokens only**, never raw Tailwind colors. `bg-brand`, not `bg-pink-500`. `text-fg-muted`, not `text-gray-500`.
- Sizes that recur are CSS variables under `@theme` (e.g. `--height-navbar`, `--size-status-dot`). Use them via `h-[var(--height-navbar)]` or extend the @theme.
- No `style={{}}` for things that could be classes. Inline styles are reserved for dynamic positioning (e.g., portal tooltips computing `left`/`top`).
- Status colors come from `JobStateIndicator` — don't reach for individual status tokens directly in feature components.

### Imports

- Cross-folder: absolute, `@/components/<folder>/<File>` and `@/apis/<file>`.
- Same folder: relative, `./SiblingFile`.
- Never `../` cross-folder — use `@/`.

### Polymorphic components

Three components in `ui/` follow this pattern. When in doubt, follow theirs:

- **`Button`** — `intent` × `size` × `shape` × (`href` | `onPress` | static)
- **`Pill`** — variation of Button for navigation/toggle pills
- **`JobStateIndicator`** — `state` × `variant` (`dot` | `badge` | `label`) × `size`

Discriminated union props prevent passing `href` and `onPress` together. Render branches inside the component pick the right element (Next.js `Link`, React Aria `Button`, plain `<span>`).

---

## Data layer (apis/)

### TanStack Query — the only fetcher

**Every** server interaction goes through a `useQuery` (or `useMutation`) hook in `src/apis/`. Components never call `fetch` directly.

Conventions:

- Query keys are arrays, sorted feature-first: `["jobs"]`, `["jobs", jobId, "runs"]`, `["runs/recent", limit]`, `["costs/summary", timeframe]`, `["health"]`, `["systemlogs", tail]`. The first segment names the resource.
- All hooks return a normalized shape: `{ data..., loading, error: string | null, refresh, ...domainExtras }`. Errors are converted from `Error` to `string`.
- Use `enabled: !!param` to guard hooks that depend on a runtime value.
- Use `placeholderData: keepPreviousData` for paginated queries (smooth transitions).
- The QueryClient lives in `src/apis/providers.tsx` with sensible defaults: `staleTime: 30s`, `gcTime: 5min`, `refetchOnWindowFocus: true`, `retry: 1`.

### SSE — the SSEQueryBridge pattern

Critical architecture decision: only **one** component subscribes to SSE events — `SSEQueryBridge` (`src/apis/sseInvalidator.tsx`), mounted at the provider level. It maps each SSE event type to the right `queryClient.invalidateQueries()` calls.

**Do not** subscribe to SSE inside individual hooks (an earlier version of this app did and produced a memory leak from multiple subscribers). Hooks fire when their cache is invalidated; that's the entire mechanism.

Exceptions: `useSystemLogs` uses SSE to **append** chunks (not invalidate) into the cache via `queryClient.setQueryData`, with a 1MB hard cap to prevent runaway growth.

### API client

`src/apis/client.ts` exposes a single `api` object with one method per endpoint, fully typed with request/response shapes from `types.ts`. Errors throw `ApiError` (with `status` + `code` + message). The base URL resolves from `window.__ACS_API_URL__` → `NEXT_PUBLIC_API_URL` → `http://127.0.0.1:8377`.

---

## Storybook

- Every component has `<ComponentName>.stories.tsx` co-located.
- Story file titles match folder structure: `Components/UI/Button`, `Components/Sidebar/JobsSidebar`, `Components/Widgets/StatWidget`, etc.
- The global decorator (`.storybook/preview.tsx`) wraps every story in a fresh `QueryClientProvider` with `retry: false, staleTime: Infinity` so hook-using components don't crash. Stories that use hooks render their loading/empty states by default; that's fine.
- At least two variants per story file (default + meaningful variant) — empty, loading, error, active state, etc.
- For inline data, define mock objects at the top of the story file. Don't import production constants.

### Adding a story

```tsx
import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { MyThing } from "./MyThing";

const meta: Meta<typeof MyThing> = {
  title: "Components/UI/MyThing",
  component: MyThing,
};
export default meta;
type Story = StoryObj<typeof MyThing>;

export const Default: Story = { args: { ... } };
export const Empty: Story = { args: { ... } };
```

---

## Testing

- Test files are co-located: `format.test.ts`, `jobStatus.test.ts`, `client.test.ts`, `useJobs.test.tsx`, `JobStateIndicator.test.tsx`.
- Run with `npm test` (watch) or `npm run test:run` (single).
- `vitest.config.ts` provides `jsdom` + the `@/` alias + the `vitest.setup.ts` setup file.
- Use `vi.useFakeTimers()` + `vi.setSystemTime()` for any time-relative assertion.
- Use `vi.mock("@/apis/client")` to mock the API client when testing hooks.
- For hooks, wrap rendering in a `QueryClientProvider`:

```tsx
function wrapper({ children }: { children: ReactNode }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}
const { result } = renderHook(() => useJobs(), { wrapper });
```

Coverage priorities (current and aspirational):
1. Pure utilities (`format`, `jobStatus`) — 100%
2. API client + each hook — at least one happy path + one error path
3. Polymorphic UI components (`Button`, `Pill`, `JobStateIndicator`) — verify each variant renders correctly
4. Page-level smoke tests — TBD

---

## Design tokens

All in `src/app/globals.css` under `@theme`. To re-theme, edit one file.

| Token group | Example tokens / classes | Notes |
|---|---|---|
| **Brand** | `--color-brand`, `--color-brand-hover`, `--color-brand-ring`, `--color-brand-muted` | Primary accent (pink) |
| **Foreground (text)** | `text-fg`, `text-fg-secondary`, `text-fg-tertiary`, `text-fg-muted`, `text-fg-subtle`, `text-fg-faint`, `text-fg-ghost` | 7-level text hierarchy |
| **Surface (backgrounds)** | `bg-surface`, `bg-surface-secondary`, `bg-surface-tertiary`, `bg-surface-hover` | |
| **Border** | `border-border`, `border-border-subtle`, `border-border-strong`, `border-border-active` | |
| **Status** | `--color-status-{running,success,failed,killed,warning}` + `-dot`, `-bg`, `-border` for each | Always reach for these via `JobStateIndicator` |
| **Decorative** | `--color-gradient-bot-*`, `--color-gradient-hero-*` | |
| **Radius** | `rounded-pill` (full), `rounded-card`, `rounded-menu`, `rounded-input`, `rounded-badge` | |
| **Sizing** | `--height-navbar`, `--height-banner`, `--height-tab-bar`, `--height-input`, `--height-btn`, `--height-avatar`, `--width-sidebar`, `--size-status-dot`, `--size-icon-{sm,md,lg}` | |
| **Shadow** | `--shadow-menu` | Only menus/popovers get shadows; cards use borders |

---

## How to add things

### A new component
1. Decide which folder by the rules above (`ui/` / `navbar/` / `sidebar/` / `jobs/` / `widgets/`).
2. Create `<Name>.tsx` with a docblock + props interface + named export.
3. Create `<Name>.stories.tsx` with `Default` + at least one variant. Title is `Components/<Folder>/<Name>`.
4. If the component has logic (formatters, conditional rendering with multiple branches), add `<Name>.test.tsx`.
5. Use design tokens only; reach for `JobStateIndicator` / `Button` / `Pill` before rolling your own.
6. Add `"use client"` only if the component uses client-only features.

### A new page
1. Create `src/app/<route>/page.tsx` (or nested route per Next.js App Router).
2. Add `"use client"` if the page uses hooks.
3. Compose existing components — don't define new components inside page files.
4. The `<Providers>` wrap is already in `layout.tsx`; queries and SSE work out of the box.

### A new API call
1. Add the request/response types to `src/apis/types.ts`.
2. Add a method to the `api` object in `src/apis/client.ts`.
3. Create `src/apis/use<Thing>.ts` exporting a hook that wraps `useQuery`.
4. If the data should refresh on a backend event, add a switch case in `SSEQueryBridge` (`src/apis/sseInvalidator.tsx`) — never subscribe to SSE in the hook itself.
5. Add a smoke test for the hook.

### A new design token
1. Add the CSS variable to the `@theme` block in `globals.css`.
2. Reference it via Tailwind utility (`bg-foo`, `text-foo`) or `var(--foo)` in dynamic styles.
3. Update this README's tokens table.

---

## Code style enforcement

- **ESLint** via `npm run lint`. Flat config in `eslint.config.mjs`. Storybook plugin enabled.
- TypeScript strict mode catches type errors at compile time.
- No automated formatter or pre-commit hook — keep code style consistent by following existing files.

---

## Known issues / open work

- Pre-existing rules-of-hooks lint warnings (5) need a focused pass — not blocking the build but real.
- The dev script intentionally uses `next dev --webpack` (not Turbopack) due to a Turbopack memory leak in long-running sessions. Revisit when the Next.js team ships a fix.
- Several routes referenced in nav/links don't exist yet (`/jobs/[id]`, `/jobs/[id]/runs/[run_id]`, `/docs/*`, `/settings`). They render the 404 page when clicked.
- `FAVORITED_JOBS` in `src/app/page.tsx` is mocked until backend support lands (tracked as `ACS-17`).
- Backend pagination on `/api/runs/recent` is not implemented — the dashboard fakes it by growing the `limit` query param (tracked as `ACS-15`). Same for status filtering (`ACS-16`).

---

## Things AI agents should not do

- Don't subscribe to SSE inside individual hooks. Use the `SSEQueryBridge` pattern.
- Don't reach for raw Tailwind colors (`bg-pink-500`, `text-gray-700`). Always tokens.
- Don't `style={{}}` for things that should be className.
- Don't add a component without a story.
- Don't edit `.git/` config or set up GitHub things without explicit human direction.
- Don't run dev with Turbopack (`next dev` without `--webpack`) — it leaks memory.
- Don't write `: any` — TypeScript strict mode catches it; we treat it as a code smell anyway.
- Don't import from `@/components` without the folder segment (`@/components/ui/Button`, never `@/components/Button`).
