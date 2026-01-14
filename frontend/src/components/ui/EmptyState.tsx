"use client";

import React from "react";
import { Column, Text, Icon } from "@once-ui-system/core";

interface EmptyStateProps {
  message: string;
  description?: string;
  icon?: React.ReactNode;
}

export function EmptyState({ message, description, icon }: EmptyStateProps) {
  return (
    <Column horizontal="center" vertical="center" paddingY="48" gap="12" fillWidth>
      {icon || (
        <Icon
          name="inbox"
          size="l"
          onBackground="neutral-weak"
        />
      )}
      <Text variant="body-default-s" onBackground="neutral-medium">
        {message}
      </Text>
      {description && (
        <Text variant="body-default-xs" onBackground="neutral-weak">
          {description}
        </Text>
      )}
    </Column>
  );
}
