import { Geist, Geist_Mono } from "next/font/google";

const heading = Geist({
  variable: "--font-heading",
  subsets: ["latin"],
  display: "swap",
});

const body = Geist({
  variable: "--font-body",
  subsets: ["latin"],
  display: "swap",
});

const label = Geist({
  variable: "--font-label",
  subsets: ["latin"],
  display: "swap",
});

const code = Geist_Mono({
  variable: "--font-code",
  subsets: ["latin"],
  display: "swap",
});

const fonts = { heading, body, label, code };

const style = {
  theme: "dark" as const,
  neutral: "gray" as const,
  brand: "blue" as const,
  accent: "cyan" as const,
  solid: "contrast" as const,
  solidStyle: "flat" as const,
  border: "conservative" as const,
  surface: "filled" as const,
  transition: "all" as const,
  scaling: "100" as const,
};

export { fonts, style };
