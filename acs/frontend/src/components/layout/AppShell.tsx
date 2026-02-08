"use client";

import React from "react";
import {
  Row,
  Column,
  LayoutProvider,
  ThemeProvider,
  ToastProvider,
  IconProvider,
} from "@once-ui-system/core";
import type {
  Theme,
  Schemes,
  NeutralColor,
  SolidType,
  SolidStyle,
  BorderStyle,
  SurfaceStyle,
  TransitionStyle,
  ScalingSize,
} from "@once-ui-system/core";
import { style } from "@/resources/once-ui.config";
import { iconLibrary } from "@/resources/icons";
import { Sidebar } from "./Sidebar";
import { SSEProvider } from "@/hooks/useSSE";

export function AppShell({ children }: { children: React.ReactNode }) {
  return (
    <LayoutProvider>
      <ThemeProvider
        theme={style.theme as Theme}
        brand={style.brand as Schemes}
        accent={style.accent as Schemes}
        neutral={style.neutral as NeutralColor}
        solid={style.solid as SolidType}
        solidStyle={style.solidStyle as SolidStyle}
        border={style.border as BorderStyle}
        surface={style.surface as SurfaceStyle}
        transition={style.transition as TransitionStyle}
        scaling={style.scaling as ScalingSize}
      >
        <ToastProvider>
          <IconProvider icons={iconLibrary}>
          <SSEProvider>
            <Row fillWidth style={{ minHeight: "100vh" }}>
              <Sidebar />
              <Column
                as="main"
                fillWidth
                padding="24"
                background="page"
                style={{ flex: 1, overflowY: "auto" }}
              >
                {children}
              </Column>
            </Row>
          </SSEProvider>
          </IconProvider>
        </ToastProvider>
      </ThemeProvider>
    </LayoutProvider>
  );
}
