"use client";

import React from "react";
import { Card as OnceCard, Column, Text } from "@once-ui-system/core";

interface CardProps {
  title?: string;
  padding?: boolean;
  className?: string;
  children: React.ReactNode;
}

export function Card({
  title,
  padding = true,
  className = "",
  children,
}: CardProps) {
  return (
    <OnceCard
      fillWidth
      padding={padding ? "l" : undefined}
      radius="l"
      border="neutral-alpha-medium"
      className={className}
    >
      <Column fillWidth gap="16">
        {title && (
          <Text
            variant="label-default-s"
            onBackground="neutral-weak"
            style={{ textTransform: "uppercase", letterSpacing: "0.05em" }}
          >
            {title}
          </Text>
        )}
        {children}
      </Column>
    </OnceCard>
  );
}
