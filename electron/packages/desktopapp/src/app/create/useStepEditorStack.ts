"use client";

/**
 * useStepEditorStack
 *
 * Encapsulates the modal-stack state used by `WorkflowGraphEditor` to
 * drive the (potentially nested) `StepEditorModal`. A "frame" is just a
 * path into the workflow's step tree; the topmost frame identifies the
 * step currently being edited, and intermediate frames feed the
 * breadcrumb shown in the modal header.
 *
 * The hook owns:
 *   - the stack of `ModalFrame`s,
 *   - the resolved `currentStep` for the topmost frame,
 *   - mutation helpers (`openAt`, `pop`, `closeAll`),
 *   - the live-edit committer (`handleStepChange`) which writes back
 *     into the workflow via `setWorkflow`,
 *   - the drill-in helper for match cases (`handleOpenNested`) which
 *     transparently seeds an empty case with a placeholder shell step
 *     so there is something to land on,
 *   - the derived `breadcrumb` array for the modal header.
 *
 * Splitting this out of `WorkflowGraphEditor` keeps the editor file
 * focused on canvas concerns (layout, callbacks, ReactFlow wiring) and
 * makes the modal-stack logic individually unit-testable.
 */

import { useCallback, useMemo, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import type { BreadcrumbCrumb } from "./StepEditorModal";
import {
  getStepAtPath,
  updateStepAtPath,
} from "./graph";
import { makeDefaultStep, type NewStep, type NewWorkflow } from "./types";

/**
 * One frame on the modal stack — identifies which step the user is
 * currently editing by its path. The breadcrumb is derived from the
 * stack when rendering.
 */
export interface ModalFrame {
  path: string;
}

export interface UseStepEditorStackResult {
  currentFrame: ModalFrame | undefined;
  currentStep: NewStep | null;
  breadcrumb: BreadcrumbCrumb[];
  openAt: (path: string) => void;
  pop: () => void;
  closeAll: () => void;
  handleStepChange: (next: NewStep) => void;
  handleOpenNested: (caseKey: string | null) => void;
  /** Drop a frame from the stack when its target step has been deleted. */
  forgetPath: (path: string) => void;
}

export function useStepEditorStack(
  workflow: NewWorkflow,
  setWorkflow: Dispatch<SetStateAction<NewWorkflow>>,
): UseStepEditorStackResult {
  const [modalStack, setModalStack] = useState<ModalFrame[]>([]);

  const currentFrame = modalStack[modalStack.length - 1];
  const currentStep = currentFrame
    ? getStepAtPath(workflow.steps, currentFrame.path)
    : null;

  const openAt = useCallback((path: string) => {
    setModalStack([{ path }]);
  }, []);

  const pop = useCallback(() => {
    setModalStack((prev) => prev.slice(0, -1));
  }, []);

  const closeAll = useCallback(() => {
    setModalStack([]);
  }, []);

  const forgetPath = useCallback((path: string) => {
    setModalStack((prev) => prev.filter((f) => f.path !== path));
  }, []);

  const handleStepChange = useCallback(
    (next: NewStep) => {
      if (!currentFrame) return;
      setWorkflow((prev) => ({
        ...prev,
        steps: updateStepAtPath(prev.steps, currentFrame.path, next),
      }));
    },
    [currentFrame, setWorkflow],
  );

  const handleOpenNested = useCallback(
    (caseKey: string | null) => {
      if (!currentFrame) return;
      const parentStep = getStepAtPath(workflow.steps, currentFrame.path);
      if (!parentStep || parentStep.kind !== "match") return;
      const childList =
        caseKey === null ? parentStep.default ?? [] : parentStep.cases[caseKey] ?? [];
      if (childList.length === 0) {
        // Drilling into an empty case — append a placeholder shell so the
        // user has something to edit. Otherwise getStepAtPath returns null.
        const placeholder = makeDefaultStep("shell");
        setWorkflow((prev) => ({
          ...prev,
          steps: updateStepAtPath(prev.steps, currentFrame.path, {
            ...parentStep,
            cases:
              caseKey !== null
                ? { ...parentStep.cases, [caseKey]: [placeholder] }
                : parentStep.cases,
            default: caseKey === null ? [placeholder] : parentStep.default,
          }),
        }));
      }
      const childPath =
        caseKey === null
          ? `${currentFrame.path}/default/0`
          : `${currentFrame.path}/cases/${caseKey}/0`;
      setModalStack((prev) => [...prev, { path: childPath }]);
    },
    [currentFrame, workflow.steps, setWorkflow],
  );

  const breadcrumb = useMemo<BreadcrumbCrumb[]>(() => {
    if (modalStack.length <= 1) return [];
    const crumbs: BreadcrumbCrumb[] = [];
    for (let i = 0; i < modalStack.length - 1; i++) {
      const frame = modalStack[i];
      const step = getStepAtPath(workflow.steps, frame.path);
      const label = step?.id ?? frame.path.split("/").pop() ?? "step";
      const targetIndex = i;
      crumbs.push({
        label,
        onClick: () => setModalStack((prev) => prev.slice(0, targetIndex + 1)),
      });
    }
    return crumbs;
  }, [modalStack, workflow.steps]);

  return {
    currentFrame,
    currentStep,
    breadcrumb,
    openAt,
    pop,
    closeAll,
    handleStepChange,
    handleOpenNested,
    forgetPath,
  };
}
