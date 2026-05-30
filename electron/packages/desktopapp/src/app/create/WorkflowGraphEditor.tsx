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
 *     step after the source. Edges then re-derive.
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
import { KindPaletteDock } from "./KindPaletteDock";
import { InsertEdge } from "./EdgePlusButton";
import { EDITOR_CURSOR_CSS } from "./cursors";
import {
  buildGraph,
  deleteStepAtPath,
  getScopeForPath,
  insertStepAfter,
  layoutGraph,
  reorderStepAtPath,
  reorderViaEdgeReconnect,
  sameScope,
  type StepNodeData,
} from "./graph";
import { makeDefaultStep } from "./types";
import type { NewWorkflow, StepKind } from "./types";
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

  // Precompute the set of node ids that already have an incoming edge
  // and an outgoing edge, so `isValidConnection` can enforce the
  // single-in / single-out invariant. Match-fanout source ids are
  // intentionally allowed multiple outs at the model level — we only
  // gate user-initiated reconnects, which always target same-scope
  // reordering.
  const isValidConnection = useCallback<IsValidConnection<Edge>>(
    (conn) => {
      if (!conn.source || !conn.target) return false;
      if (conn.source === conn.target) return false;
      const sourceScope = getScopeForPath(conn.source);
      const targetScope = getScopeForPath(conn.target);
      if (!sourceScope || !targetScope) return false;
      // Strict linear-chain wiring: same scope only. No cross-branch reorders.
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

  /* ── Click handlers ───────────────────────────────────────────────── */

  const handleNodeClick: NodeMouseHandler = useCallback(
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
      <div className="flex-1 relative bg-surface-tertiary overflow-hidden">
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
          onNodeClick={handleNodeClick}
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
