"use client";

/**
 * WorkflowGraphEditor
 *
 * The interactive canvas at the heart of /create AND /workflows/[id]/edit.
 * Owns the `NewWorkflow.steps` mutation surface and renders the
 * whiteboard-style canvas:
 *
 *   - MiniMap (top-left, with a "Minimap" eyebrow label so users know
 *     what the aerial view is for)
 *   - Controls (zoom in/out/fit, top-right)
 *   - KindPaletteDock (bottom-centre horizontal dock; chip click appends
 *     a step in a scratch position on the canvas — visually disconnected
 *     until the user wires it into the chain)
 *   - ReactFlow canvas with custom StepNode + InsertEdge (mid-edge +
 *     button that opens a kind picker), dot-grid background on a
 *     muted surface that gives the white cursor real contrast
 *
 * Node interaction model (n8n-style, post-ACS-20):
 *   - Single click on a node → SELECTS it (reactflow built-in;
 *     StepNode renders a brand ring when `selected` is true).
 *   - Double click on a node → opens the step editor modal via
 *     `onNodeDoubleClick` → `stack.openAt(path)`. Previously this was
 *     a single-click trigger but that was swallowing reach-for-the-
 *     handle gestures (same trap n8n avoids in `CanvasNodeDefault.vue`
 *     with `@dblclick.stop="onActivate"`).
 *   - Pencil icon in `HoverActions` still opens the modal in one
 *     click as a fallback for mouse-only users.
 *   - StepEditorModal stack (palette-style modal; nested when drilling
 *     into match cases — each level pushes a frame onto the modal
 *     stack, owned by `useStepEditorStack`)
 *
 * State / position model (n8n-style, post-ACS-20 drag UX rework):
 *
 *   - Node positions are USER-CONTROLLED for the duration of the editor
 *     session. The first render runs dagre to lay out whatever the
 *     editor was seeded with; after that, positions belong to the user.
 *     They are never persisted to the backend.
 *   - Live drag is wired through reactflow's controlled-mode contract:
 *     `nodes` is editor-owned state, `onNodesChange={applyNodeChanges}`
 *     propagates every drag-frame's position delta back into state, and
 *     the node visually follows the cursor in real time. This replaces
 *     the previous "capture only on drag-stop" pattern that caused the
 *     teleport bug (the node appeared to jump on mouse-up because the
 *     external `nodes` prop only updated then).
 *   - Topology changes (insert / delete / nested edits) only mutate the
 *     specific node entry — surrounding positions are preserved. A new
 *     node added via the dock lands at a SCRATCH POSITION (offset from
 *     the most-recently-added node) and is visually flagged as
 *     "disconnected" until the user wires it into the chain.
 *
 * Edge model:
 *
 *   - Edges are derived from `steps[]` chain order (and match fan-out
 *     for branches) but reactflow stores them as editor-owned state so
 *     reactflow's controlled-mode contract is satisfied for selection /
 *     keyboard-delete behaviour. The derivation runs after every
 *     `steps[]` mutation; user-initiated edge changes (reconnect,
 *     connect, delete) feed back into `steps[]` first, then the
 *     edge-derive re-runs.
 *   - User can drag from a node's source handle to another node's
 *     target handle (`onConnect`) — if valid (same scope, no loops,
 *     no double-wiring) the underlying `steps[]` reorders via
 *     `reorderViaEdgeReconnect` so the dropped-on node becomes the next
 *     step after the source. Edges then re-derive. Handles protrude
 *     ~12px from the card edge (n8n-style) with a 24px transparent
 *     hit zone, so grabbing them is forgiving — see `StepNode.tsx`.
 *   - Edge head can be dragged onto another node (`onReconnect`) —
 *     same effect.
 *   - "Disconnected" nodes (added via the dock, not yet wired) are
 *     tracked in `disconnectedIds` and their auto-derived incoming
 *     edge is suppressed. As soon as the user wires something into
 *     them, they leave the set and re-join the auto-derived chain
 *     visually. They still SERIALISE at their position in `steps[]`
 *     (which is the tail) — known visual/runtime mismatch, mitigated
 *     by the dashed-border affordance + the sidebar "N step(s) not
 *     wired" hint. See option (a) in the ACS-20 task brief.
 *
 * Scope-aware drop + tail-+ affordance (ACS-20 follow-up):
 *
 *   - While a dock chip is being dragged over the canvas, each
 *     non-top-level scope (every match case + every match default)
 *     renders a faint tinted overlay around the bounding box of its
 *     nodes. The scope under the cursor gets a stronger highlight, so
 *     the user can see exactly which chain a drop will land in.
 *   - On drop: if the cursor falls within (or within `SCOPE_DROP_PAD`
 *     pixels of) a scope's bounding box, the new step is appended into
 *     that scope and AUTO-WIRED to the scope's prior tail. Otherwise
 *     the drop lands at top level — auto-wired to the top-level tail
 *     when nearby, otherwise disconnected (legacy behaviour).
 *   - Every chain's tail node grows a small floating `+` button
 *     (`TailPlusButton`) ~16px off its right edge. Clicking it opens
 *     the same kind picker the edge-+ uses, and the picked kind is
 *     appended into the tail's scope (auto-wired). Tail nodes that are
 *     themselves `match` steps are suppressed (a match has no "next"
 *     in the linear chain — it fans out into branches instead).
 *
 * The workflow identity (name, schedule, timezone, enabled) and the
 * Save / Create button NO LONGER live on the canvas. They live in the
 * `EditorSidebar` mounted by the page on the left. The editor exposes
 * controlled props so the page can keep both surfaces in sync without
 * the editor having to know about the sidebar.
 *
 * State / logic split:
 *   - The `NewWorkflow` state and its step-mutation callbacks (insert,
 *     delete, reorder, append) live in this file because they're tied
 *     to the canvas action surface.
 *   - The modal stack — drilling into match cases, breadcrumb
 *     derivation, live edit committer — lives in
 *     `useStepEditorStack`.
 *   - Server-shape `Job` ↔ `NewWorkflow` conversion lives in
 *     `workflowSerialization.ts`.
 *   - Submit wiring (create vs update, navigation, error state) lives
 *     in the page; the page reads `serialiseWorkflow(getWorkflow())`
 *     when the sidebar's submit button fires via the
 *     `onWorkflowChange` snapshot.
 *
 * The component runs in one of two modes via discriminated-union props:
 *   - `mode: "create"` — seeds with a fresh `NewWorkflow`.
 *   - `mode: "edit"` — seeds from an existing `Workflow` (the `Job` read
 *     shape from `apis/types.ts`) after stripping server-only fields.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Background,
  BackgroundVariant,
  ConnectionLineType,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  applyEdgeChanges,
  applyNodeChanges,
  useReactFlow,
  useViewport,
  type Connection,
  type Edge,
  type EdgeChange,
  type EdgeTypes,
  type IsValidConnection,
  type Node,
  type NodeChange,
  type NodeMouseHandler,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import type { Job } from "@/apis/types";
import { StepNode } from "./StepNode";
import { StepEditorModal } from "./StepEditorModal";
import { DOCK_DRAG_MIME, KindPaletteDock } from "./KindPaletteDock";
import { InsertEdge } from "./EdgePlusButton";
import { TailPlusButton } from "./TailPlusButton";
import { EDITOR_CURSOR_CSS } from "./cursors";
import {
  appendStepToScope,
  buildGraph,
  deleteStepAtPath,
  getChainForScope,
  getScopeForPath,
  getStepAtPath,
  insertStepAfter,
  layoutGraph,
  reorderStepAtPath,
  reorderViaEdgeReconnect,
  sameScope,
  type ChainScope,
  type StepNodeData,
} from "./graph";
import { makeDefaultStep } from "./types";
import type { NewStep, NewWorkflow, StepKind } from "./types";
import { useStepEditorStack } from "./useStepEditorStack";
import { jobToNewWorkflow } from "./workflowSerialization";

const nodeTypes = { step: StepNode };
const edgeTypes: EdgeTypes = { insert: InsertEdge };

interface BuildCallbacks {
  onEdit: (path: string) => void;
  onDelete: (path: string) => void;
  onSwitchKind: (path: string) => void;
  onReorder: (path: string, dir: "up" | "down") => void;
  onInsertAfter: (sourcePath: string, kind: StepKind) => void;
}

/** Pixel offset applied to each newly-added scratch node so successive
 * dock-clicks don't pile on top of each other. */
const SCRATCH_OFFSET = { x: 60, y: 80 };

/** Slack (in flow coordinates) added around each scope's node bounding
 * box for drop-zone detection AND highlight rendering. ~40px feels
 * snug enough to avoid spurious cross-scope drops yet generous enough
 * that a user dropping just outside the right edge of a case still
 * lands in the case. */
const SCOPE_DROP_PAD = 40;

/** Extra proximity radius (flow coords) for "did the drop land near the
 * top-level tail?" — if so the dropped node is auto-wired instead of
 * left disconnected. Matches the spec's ~120px heuristic. */
const TOP_LEVEL_PROXIMITY = 120;

/** Assumed StepNode dimensions, mirrored from `graph.ts`. Used for
 * scope-region bounding-box math. */
const NODE_WIDTH = 240;
const NODE_HEIGHT = 92;

/** Style applied to the in-flight connection line (drag from a handle). */
const CONNECTION_LINE_STYLE = {
  stroke: "var(--color-brand)",
  strokeWidth: 2,
  strokeDasharray: "5 4",
} as const;

interface WorkflowGraphEditorPropsCommon {
  /**
   * Controlled workflow name. Sourced from the page-level state shared
   * with `EditorSidebar`. Overrides whatever name the editor was
   * seeded with on every render.
   */
  name: string;
  /**
   * Controlled schedule. Sourced from the page-level state shared with
   * `EditorSidebar`.
   */
  schedule: string;
  /** Controlled timezone. */
  timezone: string;
  /** Controlled enabled flag. */
  enabled: boolean;
  /**
   * Fires whenever the editor mutates the workflow (step insert /
   * delete / reorder / nested change inside the modal). The page uses
   * this to keep its `NewWorkflow` snapshot in sync so the sidebar's
   * Save button can serialise the latest state.
   */
  onWorkflowChange?: (next: NewWorkflow) => void;
  /**
   * Fires whenever the set of visually-disconnected nodes (nodes added
   * via the dock and not yet wired) changes. The page forwards this to
   * the sidebar so the user gets a "N step(s) not wired" advisory next
   * to the Save button.
   */
  onDisconnectedCountChange?: (count: number) => void;
}

export type WorkflowGraphEditorProps =
  | ({ mode: "create"; initialWorkflow: NewWorkflow } & WorkflowGraphEditorPropsCommon)
  | ({
      mode: "edit";
      workflowId: string;
      initialWorkflow: Job;
    } & WorkflowGraphEditorPropsCommon);

export function WorkflowGraphEditor(props: WorkflowGraphEditorProps) {
  return (
    <ReactFlowProvider>
      <WorkflowGraphEditorInner {...props} />
    </ReactFlowProvider>
  );
}

function WorkflowGraphEditorInner(props: WorkflowGraphEditorProps) {
  const seed = useMemo<NewWorkflow>(
    () =>
      props.mode === "edit"
        ? jobToNewWorkflow(props.initialWorkflow)
        : props.initialWorkflow,
    // Seed once — see WorkflowGraphEditor history for the rationale.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  const [internalWorkflow, setWorkflow] = useState<NewWorkflow>(seed);

  // Controlled overlays. The page owns name/schedule/timezone/enabled;
  // the editor's internal `setWorkflow` calls below only ever touch
  // `steps`, so the overlay stays authoritative for the controlled
  // fields without an effect or setState loop.
  const { name, schedule, timezone, enabled, onWorkflowChange, onDisconnectedCountChange } = props;
  const workflow: NewWorkflow = useMemo(
    () => ({
      ...internalWorkflow,
      name,
      schedule,
      timezone: timezone.length > 0 ? timezone : undefined,
      enabled,
    }),
    [internalWorkflow, name, schedule, timezone, enabled],
  );

  // Tell the page about the latest snapshot whenever it changes so the
  // submit handler always has fresh `steps`.
  useEffect(() => {
    onWorkflowChange?.(workflow);
  }, [workflow, onWorkflowChange]);

  const stack = useStepEditorStack(workflow, setWorkflow);

  /* ── Mutations (forward-declared; wired with stable refs below) ──── */

  const handleEdit = useCallback(
    (path: string) => stack.openAt(path),
    [stack],
  );

  // The set of node ids that the user has not yet wired into the chain
  // (added via the dock or left disconnected after edge deletion). We
  // suppress their auto-derived incoming edge so the user sees them as
  // floating until they manually connect them. They still serialise at
  // their position in `steps[]` (which is the tail for dock-adds).
  const [disconnectedIds, setDisconnectedIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  );

  // Notify the page (and ultimately the sidebar) whenever the count
  // changes so the "N step(s) not wired" advisory stays in sync.
  useEffect(() => {
    onDisconnectedCountChange?.(disconnectedIds.size);
  }, [disconnectedIds, onDisconnectedCountChange]);

  const handleDelete = useCallback(
    (path: string) => {
      setWorkflow((prev) => ({
        ...prev,
        steps: deleteStepAtPath(prev.steps, path),
      }));
      stack.forgetPath(path);
      setDisconnectedIds((prev) => {
        if (!prev.has(path)) return prev;
        const next = new Set(prev);
        next.delete(path);
        return next;
      });
    },
    [stack],
  );

  const handleReorder = useCallback((path: string, dir: "up" | "down") => {
    setWorkflow((prev) => ({
      ...prev,
      steps: reorderStepAtPath(prev.steps, path, dir),
    }));
  }, []);

  const handleSwitchKind = useCallback(
    // Same surface as Edit — open the modal; the kind switcher lives in its header.
    (path: string) => stack.openAt(path),
    [stack],
  );

  const handleInsertAfter = useCallback(
    (sourcePath: string, kind: StepKind) => {
      const newStep = makeDefaultStep(kind);
      setWorkflow((prev) => ({
        ...prev,
        steps: insertStepAfter(prev.steps, sourcePath, newStep),
      }));
    },
    [],
  );

  /* ── Initial graph build + position store ─────────────────────────── */

  // Build callbacks bundle. We pass the latest callbacks via a ref so
  // every node's `data` always sees the current closures without
  // forcing the whole graph to rebuild on every mutation. The ref is
  // refreshed inside an effect (not during render) to satisfy React's
  // strict ref-access rules.
  const callbacksRef = useRef<BuildCallbacks>({
    onEdit: handleEdit,
    onDelete: handleDelete,
    onSwitchKind: handleSwitchKind,
    onReorder: handleReorder,
    onInsertAfter: handleInsertAfter,
  });
  useEffect(() => {
    callbacksRef.current = {
      onEdit: handleEdit,
      onDelete: handleDelete,
      onSwitchKind: handleSwitchKind,
      onReorder: handleReorder,
      onInsertAfter: handleInsertAfter,
    };
  }, [
    handleEdit,
    handleDelete,
    handleSwitchKind,
    handleReorder,
    handleInsertAfter,
  ]);

  /**
   * Compute a fresh node + edge set from the current `steps[]`. Pure —
   * no state writes — so it's safe to call inside effect bodies that
   * diff against the prior state. Reads callbacks via the ref so the
   * function identity stays stable across renders.
   */
  const computeGraph = useCallback((steps: NewWorkflow["steps"]) => {
    return buildGraph(steps, callbacksRef.current);
  }, []);

  // Position store: nodeId → {x, y}. Survives across topology changes;
  // only mutated by reactflow's live drag (via applyNodeChanges) and by
  // the scratch-position assignment for new dock-added nodes. Seeded
  // once during the lazy `useState` initialiser below so the very first
  // render already has dagre-laid-out positions, then handed off to
  // reactflow's controlled-mode contract.
  const positionsRef = useRef<Map<string, { x: number; y: number }>>(new Map());

  /* ── Editor-owned nodes + edges state (reactflow controlled mode) ── */

  // Lazy initialisers — build the seeded graph (with no callbacks; the
  // mount-effect below re-derives via `computeGraph` to wire the real
  // handlers in). Position seed is computed once via dagre and stashed
  // outside the ref so the strict ref-access lint stays satisfied.
  const seedPositions = useMemo(() => {
    const seedBuilt = buildGraph(seed.steps);
    const positioned = layoutGraph(seedBuilt.nodes, seedBuilt.edges);
    const map = new Map<string, { x: number; y: number }>();
    for (const n of positioned) {
      map.set(n.id, n.position);
    }
    return { nodes: positioned, edges: seedBuilt.edges, map };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // Hydrate the persistent position map on first mount. Safe — runs
  // once before any user interaction.
  useEffect(() => {
    for (const [id, pos] of seedPositions.map) {
      positionsRef.current.set(id, pos);
    }
    // Mount-only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const [nodes, setNodes] = useState<Node<StepNodeData>[]>(() => seedPositions.nodes);
  const [edges, setEdges] = useState<Edge[]>(() => seedPositions.edges);

  /**
   * Reconcile `nodes` + `edges` with the latest `workflow.steps`. Runs
   * after every topology / data change. Preserves user-controlled
   * positions and the disconnectedIds suppression.
   */
  useEffect(() => {
    const built = computeGraph(workflow.steps);
    const prevPositions = positionsRef.current;
    const builtIds = new Set(built.nodes.map((n) => n.id));

    // Drop position entries for nodes that no longer exist (deleted).
    for (const id of Array.from(prevPositions.keys())) {
      if (!builtIds.has(id)) prevPositions.delete(id);
    }

    // Assign scratch positions for any newly-introduced node ids that
    // don't yet have a position. New ids most commonly come from the
    // dock-append; this also handles match-case sub-step inserts.
    setNodes((prev) => {
      const prevById = new Map(prev.map((n) => [n.id, n] as const));
      const scratchAnchor = computeScratchAnchor(prevPositions);
      let scratchCursor = 0;
      return built.nodes.map((n) => {
        const existingPos = prevPositions.get(n.id);
        if (existingPos) {
          const prior = prevById.get(n.id);
          return {
            ...n,
            position: existingPos,
            // Preserve any drag-related transient flags reactflow added
            // (selected, dragging) so a topology change mid-interaction
            // doesn't cancel selection.
            selected: prior?.selected,
            dragging: prior?.dragging,
          };
        }
        const offsetMul = ++scratchCursor;
        const pos = {
          x: scratchAnchor.x + SCRATCH_OFFSET.x * offsetMul,
          y: scratchAnchor.y + SCRATCH_OFFSET.y * offsetMul,
        };
        prevPositions.set(n.id, pos);
        return { ...n, position: pos };
      });
    });

    // Edges always re-derive from current steps[], minus any incoming
    // edges to nodes the user has marked as disconnected. Drop stale
    // disconnectedIds whose node no longer exists.
    setDisconnectedIds((prev) => {
      let changed = false;
      const next = new Set<string>();
      for (const id of prev) {
        if (builtIds.has(id)) next.add(id);
        else changed = true;
      }
      return changed ? next : prev;
    });

    setEdges(() => {
      const filtered = built.edges.filter(
        (e) => !disconnectedIds.has(e.target),
      );
      return filtered;
    });
    // disconnectedIds intentionally referenced; the setEdges above
    // reads the current value to suppress incoming edges. This effect
    // re-runs when it changes so the visual reflects the latest state.
  }, [workflow.steps, computeGraph, disconnectedIds]);

  /* ── Live drag / reactflow controlled handlers ────────────────────── */

  /**
   * Reactflow's controlled-mode contract: apply incoming changes
   * (position deltas during drag, selection toggles, dimension
   * reports, etc.) to our `nodes` state. This is what makes drag
   * smooth — every frame, reactflow emits a `position` change with the
   * new cursor-aligned position; without applying them, the rendered
   * node never moves until drag stop.
   *
   * We also mirror position changes into `positionsRef` so subsequent
   * topology reconciles preserve the user's drag.
   */
  const handleNodesChange = useCallback((changes: NodeChange<Node<StepNodeData>>[]) => {
    setNodes((prev) => {
      const next = applyNodeChanges(changes, prev);
      // Mirror any position changes into the persistent position map.
      for (const change of changes) {
        if (change.type === "position" && change.position) {
          positionsRef.current.set(change.id, change.position);
        } else if (change.type === "remove") {
          positionsRef.current.delete(change.id);
        }
      }
      return next;
    });
  }, []);

  const handleEdgesChange = useCallback((changes: EdgeChange<Edge>[]) => {
    setEdges((prev) => applyEdgeChanges(changes, prev));
  }, []);

  /* ── Connection wiring (manual user-drag from handle to handle) ───── */

  // Single source of truth for "is this drag a legal wire?". Used both
  // by reactflow during the in-flight drag (to color the line valid /
  // invalid and to accept the drop) AND by `handleConnect` /
  // `handleReconnect` as a guard before mutating `steps[]`. The clauses
  // below enumerate every reason a connection can be refused — each
  // failure path is documented so future maintenance can tell at a
  // glance whether a real wire is being rejected by accident.
  const isValidConnection = useCallback<IsValidConnection<Edge>>(
    (conn) => {
      // 1. Missing source/target ids — reactflow occasionally hands us
      //    half-formed Connection objects during cancelation; ignore.
      if (!conn.source || !conn.target) return false;
      // 2. Self-loop — a node cannot wire to itself in a linear chain.
      if (conn.source === conn.target) return false;
      // 3. Either path is malformed (couldn't be parsed as a chain
      //    location). Shouldn't happen for nodes we rendered, but the
      //    guard keeps the helper safe to call.
      const sourceScope = getScopeForPath(conn.source);
      const targetScope = getScopeForPath(conn.target);
      if (!sourceScope || !targetScope) return false;
      // 4. Strict linear-chain topology: a wire may only reorder within
      //    a single chain (top-level OR a single match-case branch).
      //    Cross-branch reconnects have no clean linear-chain semantic
      //    so we reject them. This DOES allow wiring to a disconnected
      //    node in the same scope — the disconnected flag is purely
      //    visual; the underlying path is still part of `steps[]`.
      if (!sameScope(sourceScope, targetScope)) return false;
      return true;
    },
    [],
  );

  /**
   * User dragged from a source handle and dropped on a target handle —
   * create a new connection. We translate this into a reorder of the
   * underlying `steps[]` array (so that target immediately follows
   * source in the linear chain). The edge then re-derives via the
   * reconcile effect, and we clear the target from `disconnectedIds`
   * so its incoming edge stops being suppressed.
   */
  const handleConnect = useCallback(
    (conn: Connection) => {
      if (!conn.source || !conn.target) return;
      if (conn.source === conn.target) return;
      const sourceScope = getScopeForPath(conn.source);
      const targetScope = getScopeForPath(conn.target);
      if (!sourceScope || !targetScope) return;
      if (!sameScope(sourceScope, targetScope)) return;
      setWorkflow((prev) =>
        reorderViaEdgeReconnect(prev, conn.source!, conn.target!, sourceScope),
      );
      setDisconnectedIds((prev) => {
        if (!prev.has(conn.target!)) return prev;
        const next = new Set(prev);
        next.delete(conn.target!);
        return next;
      });
    },
    [],
  );

  const handleReconnect = useCallback(
    (oldEdge: Edge, newConnection: Connection) => {
      const newTarget = newConnection.target;
      const source = newConnection.source ?? oldEdge.source;
      if (!source || !newTarget) return;
      // No-op cases: same node, same target as before.
      if (source === newTarget) return;
      if (newTarget === oldEdge.target) return;
      const sourceScope = getScopeForPath(source);
      const targetScope = getScopeForPath(newTarget);
      if (!sourceScope || !targetScope) return;
      if (!sameScope(sourceScope, targetScope)) return;
      setWorkflow((prev) =>
        reorderViaEdgeReconnect(prev, source, newTarget, sourceScope),
      );
      setDisconnectedIds((prev) => {
        if (!prev.has(newTarget)) return prev;
        const next = new Set(prev);
        next.delete(newTarget);
        return next;
      });
    },
    [],
  );

  /* ── Dock-append: new node lands at a scratch position, disconnected */

  const handleAppend = useCallback((kind: StepKind) => {
    const newStep = makeDefaultStep(kind);
    const newPath = computeAppendedTopLevelPath(internalWorkflow.steps.length);
    setWorkflow((prev) => ({
      ...prev,
      steps: insertStepAfter(prev.steps, null, newStep),
    }));
    setDisconnectedIds((prev) => {
      const next = new Set(prev);
      next.add(newPath);
      return next;
    });
  }, [internalWorkflow.steps.length]);

  /* ── Dock-drop: drag a chip onto the canvas to place at the cursor ── */

  // Reactflow's instance for translating client coordinates to canvas
  // coordinates. Available because we render inside ReactFlowProvider
  // (see `WorkflowGraphEditor`, the outer wrapper component).
  const { screenToFlowPosition } = useReactFlow();

  const canvasWrapperRef = useRef<HTMLDivElement | null>(null);

  // Snapshot the live node positions as a plain Map derived from the
  // editor-owned `nodes` state. We deliberately avoid reading
  // `positionsRef.current` during render — that ref is a write-only
  // mirror used by the reconcile effect to preserve positions across
  // topology changes. Render-time reads come from `nodes`.
  const positionsByNode = useMemo(() => {
    const map = new Map<string, { x: number; y: number }>();
    for (const n of nodes) map.set(n.id, n.position);
    return map;
  }, [nodes]);

  const scopeRegions = useMemo(
    () => collectScopeRegions(workflow, positionsByNode),
    [workflow, positionsByNode],
  );

  const tails = useMemo(
    () => collectTails(workflow, positionsByNode),
    [workflow, positionsByNode],
  );

  /**
   * While dragging a dock chip over the canvas, which scope is the
   * cursor over? Drives both the highlight overlay and the drop
   * routing. `null` means "no scope" — drop falls back to top-level.
   */
  const [hoveredScope, setHoveredScope] = useState<ChainScope | null>(null);

  /**
   * Whether ANY dock-chip drag is currently in flight over the
   * canvas. Toggles on `dragenter`/`dragleave`. Drives the overlay
   * visibility — we don't want scope regions visible during normal
   * canvas interaction.
   */
  const [dockDragActive, setDockDragActive] = useState(false);

  /**
   * Required: without preventing the default `dragover` the browser
   * refuses the drop entirely (no `drop` event will fire). Setting
   * `dropEffect = "move"` keeps the OS-level cursor consistent with
   * the dock's "move" effectAllowed.
   */
  const handleDragOver = useCallback(
    (e: React.DragEvent<HTMLDivElement>) => {
      if (!e.dataTransfer.types.includes(DOCK_DRAG_MIME)) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      const flowPos = screenToFlowPosition({ x: e.clientX, y: e.clientY });
      const scopeAt = findScopeAt(scopeRegions, flowPos);
      setHoveredScope((prev) => {
        if (prev === null && scopeAt === null) return prev;
        if (prev && scopeAt && sameScope(prev, scopeAt)) return prev;
        return scopeAt;
      });
      if (!dockDragActive) setDockDragActive(true);
    },
    [screenToFlowPosition, scopeRegions, dockDragActive],
  );

  const handleDragLeave = useCallback((e: React.DragEvent<HTMLDivElement>) => {
    // Only treat leaves of the canvas wrapper itself as "left"; child
    // dragenter/leave events from the reactflow inner DOM would
    // otherwise toggle us off mid-drag. `relatedTarget` is the node
    // the cursor moved to; if it's still inside our wrapper, ignore.
    const wrapper = canvasWrapperRef.current;
    if (wrapper && e.relatedTarget instanceof Node && wrapper.contains(e.relatedTarget)) {
      return;
    }
    setDockDragActive(false);
    setHoveredScope(null);
  }, []);

  /**
   * Drop handler: read the kind from the data-transfer payload. If the
   * drop landed within a non-top-level scope (case or default), append
   * into that scope and auto-wire. Otherwise drop at top level — if
   * the drop is near the top-level tail, append + auto-wire; if not,
   * insert as a disconnected node at the cursor (legacy behaviour).
   */
  const handleDrop = useCallback(
    (e: React.DragEvent<HTMLDivElement>) => {
      const kind = e.dataTransfer.getData(DOCK_DRAG_MIME) as StepKind | "";
      // Always clear drag UI state, even on a no-op drop.
      setDockDragActive(false);
      setHoveredScope(null);
      if (!kind) return;
      e.preventDefault();

      const flowPos = screenToFlowPosition({ x: e.clientX, y: e.clientY });
      // Centre the dropped node on the cursor — StepNode is 240×~92.
      const position = { x: flowPos.x - 120, y: flowPos.y - 46 };

      // Re-resolve scope from the drop coordinate (the cached
      // `hoveredScope` may be stale by a frame).
      const dropScope = findScopeAt(scopeRegions, flowPos);

      if (dropScope) {
        // Scope-aware drop: append into the case/default chain and
        // auto-wire to the prior tail. The result of `appendStepToScope`
        // gives us the canonical path so we can seed the position
        // synchronously, before the reconcile effect runs.
        const newStep = makeDefaultStep(kind);
        setWorkflow((prev) => {
          const result = appendStepToScope(prev, dropScope, newStep);
          if (!result) return prev;
          positionsRef.current.set(result.newPath, position);
          return result.workflow;
        });
        return;
      }

      // Top-level drop. If the cursor is within proximity of the
      // top-level tail, auto-wire by appending without flagging
      // disconnected. Otherwise stay with the disconnected-on-drop
      // behaviour so the user can wire it themselves.
      const topChain = workflow.steps;
      const topTailPath =
        topChain.length > 0 ? `s/${topChain.length - 1}` : null;
      const topTailStep = topTailPath
        ? getStepAtPath(workflow.steps, topTailPath)
        : null;
      const topTailPos = topTailPath ? positionsRef.current.get(topTailPath) : null;
      const nearTopTail =
        topTailPos &&
        topTailStep &&
        topTailStep.kind !== "match" &&
        Math.hypot(
          flowPos.x - (topTailPos.x + NODE_WIDTH / 2),
          flowPos.y - (topTailPos.y + NODE_HEIGHT / 2),
        ) < TOP_LEVEL_PROXIMITY;

      const newPath = computeAppendedTopLevelPath(workflow.steps.length);
      positionsRef.current.set(newPath, position);
      const newStep = makeDefaultStep(kind);
      setWorkflow((prev) => ({
        ...prev,
        steps: insertStepAfter(prev.steps, null, newStep),
      }));
      if (!nearTopTail) {
        setDisconnectedIds((prev) => {
          const next = new Set(prev);
          next.add(newPath);
          return next;
        });
      }
    },
    [scopeRegions, screenToFlowPosition, workflow.steps],
  );

  /**
   * Tail-+ click handler: append a step into the given scope using the
   * shared `appendStepToScope` helper. New node is auto-wired (not
   * disconnected) because the click is an explicit "add after this
   * tail" gesture. Position is seeded just past the tail's right edge
   * so the new node lands in a sensible spot before the user nudges
   * it (otherwise the reconcile effect's scratch fallback can drop it
   * far from the parent chain).
   */
  const handleTailAppend = useCallback(
    (scope: ChainScope, kind: StepKind) => {
      const chain = getChainForScope(workflow, scope);
      if (!chain || chain.length === 0) return;
      const tailIdx = chain.length - 1;
      const tailBase =
        scope.kind === "top-level"
          ? "s"
          : scope.kind === "case"
            ? `${scope.matchPath}/cases/${scope.caseKey}`
            : `${scope.matchPath}/default`;
      const tailPath = `${tailBase}/${tailIdx}`;
      const tailPos = positionsRef.current.get(tailPath);
      const seedPos = tailPos
        ? { x: tailPos.x + NODE_WIDTH + 80, y: tailPos.y }
        : null;
      const newStep = makeDefaultStep(kind);
      setWorkflow((prev) => {
        const result = appendStepToScope(prev, scope, newStep);
        if (!result) return prev;
        if (seedPos) positionsRef.current.set(result.newPath, seedPos);
        return result.workflow;
      });
    },
    [workflow],
  );

  /* ── Click handlers ───────────────────────────────────────────────── */

  // n8n-style: SINGLE click selects (reactflow built-in via the
  // `selected` prop + our brand ring in StepNode), DOUBLE click opens
  // the step editor modal. Single-click previously opened the modal,
  // which kept swallowing reach-for-the-handle gestures — same trap
  // n8n's editor explicitly avoids in `CanvasNodeDefault.vue`
  // (`@dblclick.stop="onActivate"`). Reactflow exposes both
  // `onNodeClick` and `onNodeDoubleClick` independently — they
  // compose cleanly (the dblclick does NOT suppress the click), so
  // selection still fires on the leading click of a double-click, and
  // the user sees the ring snap in just before the modal opens.
  const handleNodeDoubleClick: NodeMouseHandler = useCallback(
    (_event, node: Node) => {
      const data = node.data as StepNodeData;
      stack.openAt(data.path);
    },
    [stack],
  );

  // Inject the `disconnected` flag into each node's `data` so StepNode
  // can render the dashed-border affordance. Cheap memo over the
  // shallow node list — most renders keep the same identities.
  const nodesWithDisconnectedFlag = useMemo(
    () =>
      nodes.map((n) =>
        disconnectedIds.has(n.id)
          ? { ...n, data: { ...n.data, disconnected: true } }
          : n,
      ),
    [nodes, disconnectedIds],
  );

  const { currentStep, currentFrame, breadcrumb, handleStepChange, handleOpenNested, pop, closeAll } =
    stack;

  return (
    <div
      data-acs-editor
      className="flex-1 flex flex-col min-h-0"
    >
      {/* Accessibility: force high-contrast SVG cursors throughout the
          editor surface so OSX-style white cursors stay visible against
          the near-white canvas. Scoped to [data-acs-editor]. */}
      <style>{EDITOR_CURSOR_CSS}</style>
      <div
        ref={canvasWrapperRef}
        className="flex-1 relative bg-surface-tertiary overflow-hidden"
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
      >
        <ReactFlow
          nodes={nodesWithDisconnectedFlag}
          edges={edges}
          onNodesChange={handleNodesChange}
          onEdgesChange={handleEdgesChange}
          onConnect={handleConnect}
          onReconnect={handleReconnect}
          isValidConnection={isValidConnection}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          onNodeDoubleClick={handleNodeDoubleClick}
          fitView
          fitViewOptions={{ padding: 0.25 }}
          proOptions={{ hideAttribution: true }}
          nodesDraggable
          nodesConnectable
          elementsSelectable
          connectionLineType={ConnectionLineType.SmoothStep}
          connectionLineStyle={CONNECTION_LINE_STYLE}
          defaultEdgeOptions={{
            type: "smoothstep",
            style: { stroke: "var(--color-border-strong)", strokeWidth: 1.5 },
          }}
        >
          {/* Dot-grid canvas background — makes the whiteboard feel like
              a real surface and gives the white cursor something to land
              against. Color uses an existing faint foreground token. */}
          <Background
            variant={BackgroundVariant.Dots}
            gap={20}
            size={1.25}
            color="var(--color-fg-faint)"
          />
          <Controls
            position="top-right"
            showInteractive={false}
            className="!bg-surface !border !border-border !rounded-card !shadow-sm"
          />
          {/* Scope overlays + tail-+ buttons live in a viewport-
              transformed layer so they pan/zoom with the canvas. The
              overlay is only visible while a dock chip is being dragged
              (`dockDragActive`); tail buttons are always visible. */}
          <ViewportOverlay
            regions={dockDragActive ? scopeRegions : EMPTY_REGIONS}
            hoveredScope={hoveredScope}
            tails={tails}
            onTailAppend={handleTailAppend}
          />
        </ReactFlow>

        {/* ── Top-left: labeled minimap stack ───────────────────────── */}
        <div className="absolute top-4 left-4 z-20 flex flex-col gap-1">
          <span className="text-eyebrow pl-0.5">Minimap</span>
          <MiniMap
            pannable
            zoomable
            position="top-left"
            maskColor="rgba(243,244,246,0.6)"
            className="!relative !top-0 !left-0 !m-0 !bg-surface !border !border-border !rounded-card !shadow-sm"
          />
        </div>

        {/* ── Bottom-centre dock: kind palette ──────────────────────── */}
        <KindPaletteDock onAdd={handleAppend} />

        {/* ── Empty state hint ───────────────────────────────────────── */}
        {workflow.steps.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
            <div className="text-fg-muted text-sm">
              No steps yet — pick a kind from the dock below. Drag from a node&apos;s
              right-edge handle to another node&apos;s left-edge handle to wire them.
            </div>
          </div>
        )}
      </div>

      {/* ── Modal stack (palette-style step editor + nested frames) ─── */}
      {currentStep && currentFrame && (
        <StepEditorModal
          key={currentFrame.path}
          step={currentStep}
          breadcrumb={breadcrumb}
          onChange={handleStepChange}
          onClose={() => (breadcrumb.length > 0 ? pop() : closeAll())}
          onOpenNested={handleOpenNested}
        />
      )}
    </div>
  );
}

/* ── Helpers ──────────────────────────────────────────────────────── */

/**
 * Predicts the path of a step that's about to be appended to the
 * top-level chain. Used so we can flag the new node as `disconnected`
 * synchronously with the `setWorkflow` call, before the topology-
 * reconcile effect has read the new `steps[]`.
 */
function computeAppendedTopLevelPath(currentLength: number): string {
  return `s/${currentLength}`;
}

/**
 * Computes a sensible anchor point for new scratch-positioned nodes —
 * the right edge of the current cluster, so freshly-added nodes don't
 * land on top of existing ones. Falls back to origin when the canvas
 * is empty.
 */
function computeScratchAnchor(
  positions: ReadonlyMap<string, { x: number; y: number }>,
): { x: number; y: number } {
  if (positions.size === 0) return { x: 80, y: 80 };
  let maxX = -Infinity;
  let avgY = 0;
  let n = 0;
  for (const pos of positions.values()) {
    if (pos.x > maxX) maxX = pos.x;
    avgY += pos.y;
    n++;
  }
  return { x: maxX + 240, y: avgY / Math.max(n, 1) };
}

/** Shared empty array so the `ViewportOverlay`'s `regions` prop stays
 * referentially stable when there's no in-flight dock drag. */
const EMPTY_REGIONS: ScopeRegion[] = [];

/**
 * Mounts inside `<ReactFlow>` as a child — gives us access to
 * `useViewport` (translation + zoom) so we can render scope-region
 * overlays and per-chain tail-+ buttons in canvas coordinates.
 *
 * The wrapping div uses the same matrix transform reactflow applies
 * to its `.react-flow__viewport`, so all positioning math is in flow
 * coordinates. `pointer-events` is `none` on the layer; individual
 * tail buttons re-enable pointer events on themselves.
 */
function ViewportOverlay({
  regions,
  hoveredScope,
  tails,
  onTailAppend,
}: {
  regions: ScopeRegion[];
  hoveredScope: ChainScope | null;
  tails: TailInfo[];
  onTailAppend: (scope: ChainScope, kind: StepKind) => void;
}) {
  const { x, y, zoom } = useViewport();
  return (
    <div
      // z-10 keeps overlays above the canvas grid but below the
      // controls/minimap (which sit at z-20 in their own absolutely-
      // positioned wrappers). Tail buttons re-enable pointer events.
      className="absolute inset-0 z-10 pointer-events-none"
      style={{
        transform: `translate(${x}px, ${y}px) scale(${zoom})`,
        transformOrigin: "0 0",
      }}
    >
      {regions.map((r) => {
        const isHovered = hoveredScope && sameScope(hoveredScope, r.scope);
        return (
          <div
            key={r.key}
            className={[
              "absolute rounded-card transition-colors",
              isHovered
                ? "bg-brand/10 border border-dashed border-brand/60"
                : "bg-fg/[0.04] border border-dashed border-border-strong/40",
            ].join(" ")}
            style={{
              left: r.x,
              top: r.y,
              width: r.width,
              height: r.height,
            }}
          />
        );
      })}
      {tails.map((t) => (
        <TailPlusButton
          key={t.tailNodeId}
          tailNodeId={t.tailNodeId}
          scope={t.scope}
          position={t.position}
          onAppend={onTailAppend}
        />
      ))}
    </div>
  );
}

/* ── Scope-region & tail helpers ──────────────────────────────────── */

interface ScopeRegion {
  /** Stable id used as a React key. */
  key: string;
  scope: ChainScope;
  /** Bounding box in flow coordinates, padded by `SCOPE_DROP_PAD`. */
  x: number;
  y: number;
  width: number;
  height: number;
}

interface TailInfo {
  tailNodeId: string;
  scope: ChainScope;
  position: { x: number; y: number };
}

/**
 * Walks all non-top-level chains in the workflow (every match case and
 * every match default, recursively through nested matches) and yields
 * a `ScopeRegion` per chain whose bounding box can be computed from
 * the live position map. Scopes whose nodes have no measured position
 * yet are skipped.
 */
function collectScopeRegions(
  workflow: NewWorkflow,
  positions: ReadonlyMap<string, { x: number; y: number }>,
): ScopeRegion[] {
  const regions: ScopeRegion[] = [];

  function visit(steps: NewStep[], basePath: string) {
    for (let i = 0; i < steps.length; i++) {
      const step = steps[i];
      const path = `${basePath}/${i}`;
      if (step.kind !== "match") continue;

      for (const caseKey of Object.keys(step.cases ?? {})) {
        const scope: ChainScope = { kind: "case", matchPath: path, caseKey };
        const chain = step.cases[caseKey] ?? [];
        const region = chainRegion(
          chain,
          `${path}/cases/${caseKey}`,
          scope,
          `case:${path}:${caseKey}`,
          positions,
        );
        if (region) regions.push(region);
        visit(chain, `${path}/cases/${caseKey}`);
      }
      if (step.default && step.default.length > 0) {
        const scope: ChainScope = { kind: "default", matchPath: path };
        const region = chainRegion(
          step.default,
          `${path}/default`,
          scope,
          `default:${path}`,
          positions,
        );
        if (region) regions.push(region);
        visit(step.default, `${path}/default`);
      }
    }
  }
  visit(workflow.steps, "s");
  return regions;
}

function chainRegion(
  chain: NewStep[],
  basePath: string,
  scope: ChainScope,
  key: string,
  positions: ReadonlyMap<string, { x: number; y: number }>,
): ScopeRegion | null {
  if (chain.length === 0) return null;
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  let hadAny = false;
  for (let i = 0; i < chain.length; i++) {
    const pos = positions.get(`${basePath}/${i}`);
    if (!pos) continue;
    hadAny = true;
    if (pos.x < minX) minX = pos.x;
    if (pos.y < minY) minY = pos.y;
    if (pos.x + NODE_WIDTH > maxX) maxX = pos.x + NODE_WIDTH;
    if (pos.y + NODE_HEIGHT > maxY) maxY = pos.y + NODE_HEIGHT;
  }
  if (!hadAny) return null;
  return {
    key,
    scope,
    x: minX - SCOPE_DROP_PAD,
    y: minY - SCOPE_DROP_PAD,
    width: maxX - minX + SCOPE_DROP_PAD * 2,
    height: maxY - minY + SCOPE_DROP_PAD * 2,
  };
}

/**
 * Returns the scope whose region contains the given flow coordinate,
 * or `null` if the cursor isn't over any scope region. When regions
 * overlap (e.g. nested matches), the deepest scope wins — we prefer
 * the most specific match by picking the smallest-area region.
 */
function findScopeAt(
  regions: ScopeRegion[],
  pos: { x: number; y: number },
): ChainScope | null {
  let best: ScopeRegion | null = null;
  for (const r of regions) {
    if (pos.x < r.x || pos.x > r.x + r.width) continue;
    if (pos.y < r.y || pos.y > r.y + r.height) continue;
    if (!best || r.width * r.height < best.width * best.height) {
      best = r;
    }
  }
  return best ? best.scope : null;
}

/**
 * Walks every chain in the workflow and returns its tail node info
 * (id + position + scope), filtering out tails whose underlying step
 * is a `match` (those are fan-out points, not "next step" hosts).
 */
function collectTails(
  workflow: NewWorkflow,
  positions: ReadonlyMap<string, { x: number; y: number }>,
): TailInfo[] {
  const tails: TailInfo[] = [];

  function tailOf(scope: ChainScope, chainBasePath: string) {
    const chain = getChainForScope(workflow, scope);
    if (!chain || chain.length === 0) return;
    const tailIdx = chain.length - 1;
    const tailPath = `${chainBasePath}/${tailIdx}`;
    const tailStep = getStepAtPath(workflow.steps, tailPath);
    if (!tailStep) return;
    if (tailStep.kind === "match") return; // suppressed
    const pos = positions.get(tailPath);
    if (!pos) return;
    tails.push({ tailNodeId: tailPath, scope, position: pos });
  }

  tailOf({ kind: "top-level" }, "s");

  function visit(steps: NewStep[], basePath: string) {
    for (let i = 0; i < steps.length; i++) {
      const step = steps[i];
      const path = `${basePath}/${i}`;
      if (step.kind !== "match") continue;
      for (const caseKey of Object.keys(step.cases ?? {})) {
        tailOf(
          { kind: "case", matchPath: path, caseKey },
          `${path}/cases/${caseKey}`,
        );
        visit(step.cases[caseKey] ?? [], `${path}/cases/${caseKey}`);
      }
      if (step.default && step.default.length > 0) {
        tailOf({ kind: "default", matchPath: path }, `${path}/default`);
        visit(step.default, `${path}/default`);
      }
    }
  }
  visit(workflow.steps, "s");
  return tails;
}
