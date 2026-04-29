# ACS Desktop App

New desktop UI for Agent Cron Scheduler. Built with Next.js 16, React 19, Tailwind CSS v4, React Aria, and Lucide icons.

## Getting started

```bash
# From the electron/ root
npm install
npm run dev --workspace=packages/desktopapp    # http://localhost:3000
npm run storybook --workspace=packages/desktopapp  # http://localhost:6006
```

The app connects to the ACS backend at `http://127.0.0.1:8377` by default. Override via the `NEXT_PUBLIC_API_URL` environment variable.

## Project structure

```
src/
├── apis/                   # API layer (persists across navigation)
│   ├── client.ts           # Fetch wrapper, ApiError, api.* methods
│   ├── sse.tsx             # SSEProvider context + useSSEEvents / useSSEConnected hooks
│   └── useSystemLogs.ts    # System logs data hook
├── app/                    # Next.js App Router pages
│   ├── globals.css         # Tailwind config + design tokens (@theme block)
│   ├── layout.tsx          # Root layout (wraps app in SSEProvider)
│   ├── not-found.tsx       # Custom 404 page
│   ├── page.tsx            # Dashboard (/)
│   ├── chat/page.tsx       # Chat (/chat)
│   ├── create/page.tsx     # Build a job (/create)
│   └── systemlogs/page.tsx # System logs viewer (/systemlogs)
└── components/             # Reusable UI components (each has a .stories.tsx)
    ├── ChatBar.tsx          # Message input with send button
    ├── FavoritedJobs.tsx    # Quick-access job links with empty state
    ├── FilterTabs.tsx       # Route-aware navigation pills (Dashboard/Jobs/Backups/Logs)
    ├── JobRunCard.tsx       # Job execution card with status, cost, slide-in hover
    ├── Navbar.tsx           # Top navigation bar with dropdowns and instance selector
    ├── Pill.tsx             # Generic toggle pill
    ├── SystemBanner.tsx     # Version, uptime, backups, update notification
    └── TabBar.tsx           # Tab strip with expandable filter panel
```

## Design tokens

All colors, borders, radii, shadows, and sizing are defined as semantic tokens in `src/app/globals.css` via Tailwind v4's `@theme` block. No hardcoded color classes — change the theme by editing one file.

| Token group | Example classes | Purpose |
|---|---|---|
| Brand | `bg-brand`, `hover:bg-brand-hover`, `ring-brand-ring` | Primary accent (pink) |
| Foreground | `text-fg`, `text-fg-secondary`, `text-fg-muted`, `text-fg-subtle` | Text hierarchy |
| Surface | `bg-surface`, `bg-surface-secondary`, `bg-surface-hover` | Background layers |
| Border | `border-border`, `border-border-subtle`, `border-border-strong` | Border levels |
| Status | `text-status-success`, `bg-status-failed-bg`, `border-status-running-border` | Job state colors |
| Radius | `rounded-pill`, `rounded-card`, `rounded-menu`, `rounded-input` | Shape tokens |
| Sizing | `h-[var(--height-navbar)]`, `w-[var(--size-status-dot)]` | Fixed dimensions |

## Key libraries

| Library | Purpose |
|---|---|
| `react-aria-components` | Accessible UI primitives (Button, Menu, Popover, TextField, ToggleButton) |
| `lucide-react` | Icon library |
| `@melloware/react-logviewer` | Log viewer (LazyLog) for system logs page |
| `tailwindcss` v4 | Styling via utility classes + `@theme` tokens |
| `storybook` v10 | Component development and documentation |

## SSE connection

The `SSEProvider` in `src/apis/sse.tsx` maintains a single persistent `EventSource` connection to the backend's `/api/events` endpoint. It lives in `layout.tsx` so the connection survives page navigation. Components subscribe to events via `useSSEEvents()`.

## Storybook

Every component in `src/components/` has a co-located `.stories.tsx` file. Run Storybook to develop and preview components in isolation:

```bash
npm run storybook --workspace=packages/desktopapp
```

## Scripts

| Command | Description |
|---|---|
| `npm run dev` | Start Next.js dev server |
| `npm run build` | Production build |
| `npm run storybook` | Start Storybook dev server |
| `npm run build-storybook` | Build static Storybook |
| `npm run lint` | Run ESLint |
