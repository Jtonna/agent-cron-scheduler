# Agent Cron Scheduler - Frontend

Frontend for the Agent Cron Scheduler (ACS) Electron desktop application.

## Stack

- **Next.js 16** - build tool only (static export)
- **React 19** - UI framework
- **React Router v7** - client-side routing (NOT Next.js App Router)
- **Tailwind CSS v4** - styling

## Architecture

- **Page components**: `src/routes/` (DashboardPage, AllJobsPage, CreateJobPage, JobDetailPage, EditJobPage, RunLogPage, SystemLogsPage)
- **Router setup**: AppShell.tsx wraps all routes with BrowserRouter and persistent providers (SSE, Sidebar, ConnectionBanner)
- **Build output**: `out/` directory (static export) served by Electron's in-process HTTP server

## Development

Run the dev server:

```bash
npm run dev
```

Opens [http://localhost:3000](http://localhost:3000). API requests are proxied to the ACS daemon on port 8377.

## Build

```bash
npm run build
```

Produces `out/` for Electron packaging.
