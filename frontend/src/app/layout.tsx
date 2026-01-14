import type { Metadata } from "next";
import "@once-ui-system/core/css/styles.css";
import "@once-ui-system/core/css/tokens.css";
import "./globals.css";
import { Flex, Column } from "@once-ui-system/core";
import { AppShell } from "@/components/layout/AppShell";
import { fonts, style } from "@/resources/once-ui.config";

export const metadata: Metadata = {
  title: "Agent Cron Scheduler",
  description: "Cron job scheduler with web dashboard",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <Flex
      suppressHydrationWarning
      as="html"
      lang="en"
      fillWidth
      className={`${fonts.heading.variable} ${fonts.body.variable} ${fonts.label.variable} ${fonts.code.variable}`}
    >
      <head>
        <script
          id="theme-init"
          dangerouslySetInnerHTML={{
            __html: `
              (function() {
                try {
                  const root = document.documentElement;
                  const config = ${JSON.stringify({
                    theme: style.theme,
                    brand: style.brand,
                    accent: style.accent,
                    neutral: style.neutral,
                    solid: style.solid,
                    "solid-style": style.solidStyle,
                    border: style.border,
                    surface: style.surface,
                    transition: style.transition,
                    scaling: style.scaling,
                  })};
                  Object.entries(config).forEach(([key, value]) => {
                    root.setAttribute('data-' + key, value);
                  });
                  const resolveTheme = (themeValue) => {
                    if (!themeValue || themeValue === 'system') {
                      return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
                    }
                    return themeValue;
                  };
                  const savedTheme = localStorage.getItem('data-theme');
                  const resolvedTheme = savedTheme ? resolveTheme(savedTheme) : config.theme === 'system' ? resolveTheme('system') : config.theme;
                  root.setAttribute('data-theme', resolvedTheme);
                  const styleKeys = Object.keys(config);
                  styleKeys.forEach(key => {
                    const value = localStorage.getItem('data-' + key);
                    if (value) {
                      root.setAttribute('data-' + key, value);
                    }
                  });
                } catch (e) {
                  console.error('Failed to initialize theme:', e);
                  document.documentElement.setAttribute('data-theme', 'dark');
                }
              })();
            `,
          }}
        />
      </head>
      <Column as="body" background="page" fillWidth margin="0" padding="0">
        <AppShell>{children}</AppShell>
      </Column>
    </Flex>
  );
}
