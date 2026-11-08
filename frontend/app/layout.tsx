import type { Metadata } from "next";
import "./globals.css";
import { grotesk } from "../font";

export const metadata: Metadata = {
  title: "Hover MCP",
  description: "Interact with Solana Better",
};

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <body className={grotesk.variable}>{children}</body>
    </html>
  );
}
