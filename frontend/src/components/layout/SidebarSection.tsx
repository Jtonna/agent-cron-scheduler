"use client";

import React, { useState } from "react";
import { Column, Row, Text, Icon } from "@once-ui-system/core";

interface SidebarSectionProps {
  title: string;
  defaultOpen?: boolean;
  children: React.ReactNode;
}

export function SidebarSection({
  title,
  defaultOpen = true,
  children,
}: SidebarSectionProps) {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <Column gap="2">
      <Row
        paddingX="12"
        paddingY="4"
        horizontal="between"
        vertical="center"
        onClick={() => setOpen(!open)}
        style={{ cursor: "pointer" }}
      >
        <Text
          variant="label-default-xs"
          onBackground="neutral-weak"
          style={{ textTransform: "uppercase", letterSpacing: "0.05em" }}
        >
          {title}
        </Text>
        <Icon
          name="chevronDown"
          size="xs"
          onBackground="neutral-weak"
          style={{
            transition:
              "transform var(--transition-duration) var(--transition-timing)",
            transform: open ? "rotate(0deg)" : "rotate(-90deg)",
          }}
        />
      </Row>
      {open && <Column gap="2">{children}</Column>}
    </Column>
  );
}
