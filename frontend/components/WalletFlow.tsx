import Link from "next/link";
import type { ReactNode } from "react";
import "./WalletFlow.css";

export const flow = {
  shell: "wallet-flow-shell",
  header: "wallet-flow-header",
  headerLabel: "wallet-flow-headerLabel",
  back: "wallet-flow-back",
  primary: "wallet-flow-primary",
  layout: "wallet-flow-layout",
  intro: "wallet-flow-intro",
  eyebrow: "wallet-flow-eyebrow",
  title: "wallet-flow-title",
  description: "wallet-flow-description",
  steps: "wallet-flow-steps",
  stepNumber: "wallet-flow-stepNumber",
  panel: "wallet-flow-panel",
  panelHeader: "wallet-flow-panelHeader",
  body: "wallet-flow-body",
  subtitle: "wallet-flow-subtitle",
  badge: "wallet-flow-badge",
  walletIcon: "wallet-flow-walletIcon",
  permissions: "wallet-flow-permissions",
  actions: "wallet-flow-actions",
  secondary: "wallet-flow-secondary",
  footnote: "wallet-flow-footnote",
  panelFooter: "wallet-flow-panelFooter",
  address: "wallet-flow-address",
  label: "wallet-flow-label",
  details: "wallet-flow-details",
  notice: "wallet-flow-notice",
  error: "wallet-flow-error",
  amount: "wallet-flow-amount",
  asset: "wallet-flow-asset",
  rows: "wallet-flow-rows",
  row: "wallet-flow-row",
  groupTitle: "wallet-flow-groupTitle",
  confirm: "wallet-flow-confirm",
  footer: "wallet-flow-footer",
  skeleton: "wallet-flow-skeleton",
} as const;

export function FlowArrow() {
  return <svg viewBox="0 0 20 20" fill="none" aria-hidden="true"><path d="M4 10h12m-5-5 5 5-5 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" /></svg>;
}

export default function WalletFlow({ section, lead, title, description, steps, children }: {
  section: string; lead: string; title: string; description: string;
  steps: { label: string; detail: string; active?: boolean }[]; children: ReactNode;
}) {
  return <div className={flow.shell}>
    <header className={flow.header}>
      <Link href="/" className="wordmark" aria-label="Hover home">hover<span className="wordmark-dot">.</span></Link>
      <span className={flow.headerLabel}>{section}</span>
      <Link href="/" className={flow.back}>Back to home <FlowArrow /></Link>
    </header>
    <main className={flow.layout}>
      <div className={flow.intro}>
        <p className={flow.eyebrow}><span className="active-dot" aria-hidden="true" /> {section}</p>
        <h1 className={flow.title}><span>{lead}</span>{title}</h1>
        <p className={flow.description}>{description}</p>
        <ol className={flow.steps}>{steps.map((step, index) => <li key={step.label} data-active={step.active || undefined}>
          <span className={flow.stepNumber}>0{index + 1}</span>
          <div><p>{step.label}</p><span>{step.detail}</span></div>
        </li>)}</ol>
      </div>
      <div className={flow.panel}>{children}</div>
    </main>
    <footer className={flow.footer}><span>Hover / Onchain operations, in your hands.</span><span>Powered by Solana</span></footer>
  </div>;
}
