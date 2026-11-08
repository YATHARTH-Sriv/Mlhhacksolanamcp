"use client";

import WalletFlow, { FlowArrow, flow } from "../../../../components/WalletFlow";
import { useParams } from "next/navigation";
import { useCallback, useEffect, useRef, useState } from "react";
import { getWallets } from "@wallet-standard/app";

type Transfer = {
  transfer_id: string; sender: string; recipient: string; asset: string; mint: string | null;
  amount: string; decimals: number; raw_amount: string; network: string; rpc_genesis_hash: string;
  fee_lamports: string; account_creation_lamports: string; creates_recipient_token_account: boolean;
  simulation: string; status: "awaiting_signature" | "submitted" | "confirmed" | "failed" | "expired";
  signature: string | null; message: string | null; transaction_base64?: string; expires_at_unix: number;
  bill_payment?: { merchant: string | null; confirmed_total: string; bill_currency: string; token_usd_price: string | null; estimated_bill_usd: string | null; quoted_at_unix: number };
};
type Account = { address: string; publicKey: Uint8Array; chains: readonly string[]; features: readonly string[] };
type Connect = { connect(): Promise<{ accounts: readonly Account[] }> };
type Sign = { signTransaction(input: { account: Account; transaction: Uint8Array }): Promise<readonly { signedTransaction: Uint8Array }[]> };

function sol(lamports: string) {
  const digits = lamports.padStart(10, "0");
  const fraction = digits.slice(-9).replace(/0+$/, "");
  return `${digits.slice(0, -9)}${fraction ? `.${fraction}` : ""} SOL`;
}
function encode(bytes: Uint8Array) { return btoa(Array.from(bytes, byte => String.fromCharCode(byte)).join("")); }
function decode(value: string) { return Uint8Array.from(atob(value), char => char.charCodeAt(0)); }

async function request(path: string, body?: unknown): Promise<Transfer> {
  const response = await fetch(path, {
    method: body ? "POST" : "GET", cache: "no-store", signal: AbortSignal.timeout(20_000),
    headers: body ? { "Content-Type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  const text = await response.text();
  let value;
  try { value = JSON.parse(text); } catch { throw new Error(`Transfer server returned HTTP ${response.status}. Retry checking this transfer.`); }
  if (!response.ok) throw new Error(value.message ?? "Transfer request failed.");
  return value as Transfer;
}

export default function ApprovalRoute() {
  const { id } = useParams<{ id: string }>();
  return <Approval key={id} id={id} />;
}

function Approval({ id }: { id: string }) {
  const [transfer, setTransfer] = useState<Transfer | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [checked, setChecked] = useState(false);
  const [signed, setSigned] = useState<string | null>(null);
  const working = useRef(false);
  const base = `/api/transfers/${encodeURIComponent(id)}`;

  const refresh = useCallback(async () => {
    try { setTransfer(await request(base)); setError(""); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "Could not check transfer status."); }
  }, [base]);

  useEffect(() => {
    getWallets(); // Start Wallet Standard discovery before the user's click.
    let active = true;
    request(base).then(value => { if (active) { setTransfer(value); setError(""); } })
      .catch(cause => { if (active) setError(cause instanceof Error ? cause.message : "Could not load transfer."); });
    return () => { active = false; };
  }, [base]);

  useEffect(() => {
    if (transfer?.status !== "submitted") return;
    const timer = setInterval(refresh, 2500);
    return () => clearInterval(timer);
  }, [transfer?.status, refresh]);

  async function approve() {
    if (working.current || !transfer || !checked) return;
    working.current = true; setBusy(true); setError("");
    try {
      let signedBytes = signed;
      if (!signedBytes) {
        if (!transfer.transaction_base64 || transfer.status !== "awaiting_signature") throw new Error("Check status or ask your agent to prepare a fresh transfer.");
        if (Date.now() / 1000 >= transfer.expires_at_unix) throw new Error("This review expired. Ask your agent to prepare a fresh transfer.");
        const wallet = getWallets().get().find(wallet => wallet.name.toLowerCase().includes("phantom"));
        if (!wallet) throw new Error("Open this page in a browser with Phantom installed, then retry.");
        const connect = wallet.features["standard:connect"] as Connect | undefined;
        const signing = wallet.features["solana:signTransaction"] as Sign | undefined;
        if (!connect || !signing) throw new Error("This Phantom wallet does not support signing transaction bytes. Use the Phantom browser extension.");
        const connected = await connect.connect();
        const account = connected.accounts.find(account => account.address === transfer.sender);
        if (!account) throw new Error(`Select the sender wallet ${transfer.sender} in Phantom, then retry.`);
        const [result] = await signing.signTransaction({ account, transaction: decode(transfer.transaction_base64) });
        if (!result) throw new Error("Phantom did not return a signed transaction.");
        signedBytes = encode(result.signedTransaction);
        setSigned(signedBytes);
      }
      // Keep the exact signed bytes in memory for safe retries after a timeout.
      setTransfer(await request(`${base}/submit`, { signed_transaction_base64: signedBytes }));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Signing or submission failed. Check this transfer's status before starting another payment.");
    } finally { working.current = false; setBusy(false); }
  }

  const statusLabels = { awaiting_signature: "Ready to review", submitted: "Confirming", confirmed: "Confirmed", failed: "Failed", expired: "Expired" };
  const row = (label: string, value: string) => <div className={flow.row}><dt>{label}</dt><dd>{value}</dd></div>;
  return (
    <WalletFlow section="Payment approval" lead={transfer?.status === "confirmed" ? "Payment sent." : "Review it."} title={transfer?.status === "confirmed" ? "You’re all set." : "Then sign."}
      description="A clear view of what you’re sending, where it’s going, and what it costs. The final approval is yours."
      steps={[
        { label: "Review the details", detail: "Check the amount, recipient, and network.", active: transfer?.status === "awaiting_signature" },
        { label: "Sign in Phantom", detail: "Approve this exact transfer in your wallet.", active: busy },
        { label: "Wait for confirmation", detail: "Return to your agent once it’s confirmed.", active: transfer?.status === "submitted" || transfer?.status === "confirmed" },
      ]}>
      <div className={flow.panelHeader}><span>02 / Transfer</span><span className={flow.badge} data-state={transfer?.status}>{transfer ? statusLabels[transfer.status] : error ? "Unable to load" : "Loading details"}</span></div>
      <section className={flow.body} aria-busy={busy || (!transfer && !error)} aria-label="Transfer review">
        {!transfer && !error && <div role="status"><h2>Loading your transfer</h2><p className={flow.subtitle}>Fetching the details and simulation result…</p><div className={flow.skeleton} /><div className={flow.skeleton} /><div className={flow.skeleton} /></div>}
        {transfer && <>
          <p className={flow.label}>{transfer.status === "confirmed" ? "Amount sent" : "You’re sending"}</p>
          <h2 className={flow.amount}>{transfer.amount} <span className={flow.asset}>{transfer.asset}</span></h2>
          <p className={flow.footnote}>{transfer.network === "surfpool" ? "Surfpool · Local test assets" : transfer.network}</p>
          <dl className={flow.rows}>
            {row("From / Fee payer", transfer.sender)}
            {row("To / Recipient", transfer.recipient)}
            {row("Token mint", transfer.mint ?? "Native SOL · Not wrapped SOL")}
            {row("Network fee", sol(transfer.fee_lamports))}
            {row("Account creation", transfer.creates_recipient_token_account ? sol(transfer.account_creation_lamports) : "No additional cost")}
            {row("Simulation", transfer.simulation)}
            {transfer.signature && row("Transaction signature", transfer.signature)}
          </dl>
          {transfer.bill_payment && <>
            <p className={flow.groupTitle}>Bill details</p>
            <dl className={flow.rows}>
              {transfer.bill_payment.merchant && row("Merchant", transfer.bill_payment.merchant)}
              {row("Confirmed bill total", `${transfer.bill_payment.confirmed_total} ${transfer.bill_payment.bill_currency}`)}
              {transfer.bill_payment.estimated_bill_usd && row("Estimated bill value", `$${transfer.bill_payment.estimated_bill_usd} USD`)}
              {transfer.bill_payment.token_usd_price && row("Jupiter price / Token", `$${transfer.bill_payment.token_usd_price}`)}
              {row("Price quoted at", new Date(transfer.bill_payment.quoted_at_unix * 1000).toISOString())}
            </dl>
            <p className={flow.footnote}>The token amount is fixed. Its USD value may change. Confirm the recipient accepts this token.</p>
          </>}
          {transfer.message && <p role="status" className={flow.notice}>{transfer.message}</p>}
          {transfer.status === "awaiting_signature" && <label className={flow.confirm}><input type="checkbox" checked={checked} onChange={event => setChecked(event.target.checked)} disabled={busy} /><span>I’ve checked the recipient, amount, token, fees, and {transfer.network} network.</span></label>}
          <div className={flow.actions}>
            {(transfer.status === "awaiting_signature" || (transfer.status === "submitted" && signed)) && <button type="button" onClick={approve} disabled={busy || !checked} className={flow.primary}>{busy ? "Waiting for wallet / RPC…" : signed ? "Retry signed transaction" : "Sign in Phantom"}{!busy && <FlowArrow />}</button>}
            <button type="button" onClick={refresh} disabled={busy} className={flow.secondary}>Check status</button>
          </div>
          {transfer.status === "confirmed" && <p role="status" className={flow.notice}>Transfer confirmed on {transfer.network}. Return to your agent to refresh your portfolio.</p>}
          {transfer.status === "expired" && <p className={flow.footnote}>Ask your agent for a new transfer, then review its new signing link.</p>}
        </>}
        {error && <div role="alert" className={flow.error}><p>{error}</p>{!transfer && <button type="button" onClick={refresh} className={flow.secondary}>Retry loading</button>}</div>}
      </section>
      <div className={flow.panelFooter}>Phantom signs locally. Hover submits to {transfer?.network ?? "your configured network"}. Keep this page open until the outcome is known.</div>
    </WalletFlow>
  );
}
