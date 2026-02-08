"use client";

import React from "react";
import { Card as OnceCard, Column, Row, Text } from "@once-ui-system/core";

interface HealthCardProps {
  label: string;
  value: string;
  icon?: React.ReactNode;
}

export function HealthCard({ label, value, icon }: HealthCardProps) {
  return (
    <OnceCard
      fillWidth
      padding="l"
      radius="l"
      border="neutral-alpha-medium"
    >
      <Row gap="12" vertical="start">
        {icon && (
          <Column
            horizontal="center"
            vertical="center"
            radius="m"
            padding="8"
            style={{
              background: "var(--neutral-alpha-weak)",
              flexShrink: 0,
              width: "36px",
              height: "36px",
              color: "var(--neutral-on-background-weak)",
            }}
          >
            {icon}
          </Column>
        )}
        <Column gap="4" style={{ minWidth: 0 }}>
          <Text
            variant="label-default-xs"
            onBackground="neutral-weak"
            style={{ textTransform: "uppercase", letterSpacing: "0.05em" }}
          >
            {label}
          </Text>
          <Text
            variant="heading-strong-m"
            style={{ overflow: "hidden", textOverflow: "ellipsis" }}
          >
            {value}
          </Text>
        </Column>
      </Row>
    </OnceCard>
  );
}
