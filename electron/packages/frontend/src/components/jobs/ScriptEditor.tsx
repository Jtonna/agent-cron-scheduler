"use client";

import React, { useRef, useEffect } from "react";
import { EditorView, basicSetup } from "codemirror";
import { EditorState, Compartment } from "@codemirror/state";
import { oneDark } from "@codemirror/theme-one-dark";
import { python } from "@codemirror/lang-python";
import { StreamLanguage } from "@codemirror/language";
import { shell } from "@codemirror/legacy-modes/mode/shell";
import { powerShell } from "@codemirror/legacy-modes/mode/powershell";
import { cn } from "@/lib/utils";

interface ScriptEditorProps {
  value: string;
  onChange: (value: string) => void;
  language: "shell" | "batch" | "python" | "powershell";
  className?: string;
}

function getLanguageExtension(language: ScriptEditorProps["language"]) {
  switch (language) {
    case "python":
      return python();
    case "shell":
      return StreamLanguage.define(shell);
    case "powershell":
      return StreamLanguage.define(powerShell);
    case "batch":
      // No highlighting available for batch — use plain text (no extension)
      return [];
  }
}

const editorTheme = EditorView.theme({
  "&": {
    height: "300px",
    fontSize: "13px",
  },
  ".cm-scroller": {
    overflow: "auto",
  },
  "&.cm-focused": {
    outline: "none",
  },
});

export function ScriptEditor({ value, onChange, language, className }: ScriptEditorProps) {
  const editorRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const languageCompartment = useRef(new Compartment());
  const isInternalChange = useRef(false);

  // Initialize editor on mount
  useEffect(() => {
    if (!editorRef.current) return;

    const view = new EditorView({
      state: EditorState.create({
        doc: value,
        extensions: [
          basicSetup,
          oneDark,
          editorTheme,
          languageCompartment.current.of(getLanguageExtension(language)),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) {
              isInternalChange.current = true;
              onChange(update.state.doc.toString());
            }
          }),
        ],
      }),
      parent: editorRef.current,
    });

    viewRef.current = view;
    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []); // Mount only

  // Reconfigure language compartment when language prop changes
  useEffect(() => {
    if (viewRef.current) {
      viewRef.current.dispatch({
        effects: languageCompartment.current.reconfigure(getLanguageExtension(language)),
      });
    }
  }, [language]);

  // Sync external value changes into the editor
  useEffect(() => {
    if (viewRef.current && !isInternalChange.current) {
      const currentContent = viewRef.current.state.doc.toString();
      if (currentContent !== value) {
        viewRef.current.dispatch({
          changes: { from: 0, to: currentContent.length, insert: value },
        });
      }
    }
    isInternalChange.current = false;
  }, [value]);

  return (
    <div
      ref={editorRef}
      className={cn(
        "overflow-hidden rounded-md border border-input resize-y",
        className
      )}
    />
  );
}
