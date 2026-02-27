"use client";

import { Montserrat, Source_Code_Pro } from "next/font/google";
import "./globals.css";
import dynamic from "next/dynamic";
import { ThemeProvider } from "@/components/theme-provider";

const AppShell = dynamic(() => import("@/components/layout/AppShell").then(mod => ({ default: mod.AppShell })), {
  ssr: false,
});

const montserrat = Montserrat({
  subsets: ["latin"],
  variable: "--font-sans",
});
const sourceCodePro = Source_Code_Pro({
  subsets: ["latin"],
  variable: "--font-mono",
});

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <title>Agent Cron Scheduler</title>
        <meta name="description" content="Cron job scheduler with web dashboard" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
      </head>
      <body className={`${montserrat.variable} ${sourceCodePro.variable} font-sans antialiased`}>
        <ThemeProvider
          attribute="class"
          defaultTheme="light"
          disableTransitionOnChange
        >
          <AppShell />
        </ThemeProvider>
      </body>
    </html>
  );
}
