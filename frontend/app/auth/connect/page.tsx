"use client";

import WalletFlow, { FlowArrow, flow } from "../../../components/WalletFlow";
import { useRef, useState } from "react";

type PublicKey = { toString(): string };
type Phantom = {
  isPhantom?: boolean;
  publicKey: PublicKey | null;
  connect(): Promise<{ publicKey: PublicKey }>;
  signMessage(message: Uint8Array, display: "utf8"): Promise<{ signature: Uint8Array }>;
};
type Challenge = { challenge_id: string; wallet_address: string; message: string; expires_in: number };

async function post<T>(action: string, body: unknown): Promise<T> {
  const response = await fetch(`/oauth/wallet/${action}`, {
    method: "POST", credentials: "same-origin",
    headers: { "Content-Type": "application/json" }, body: JSON.stringify(body),
    signal: AbortSignal.timeout(20_000),
  });
  const text = await response.text();
  let result;
  try { result = JSON.parse(text); }
  catch { throw new Error(`Login server returned HTTP ${response.status}. Please retry.`); }
  if (!response.ok) throw new Error(result.error_description ?? result.error ?? "Login failed.");
  return result as T;
}

export default function ConnectPage() {
  const provider = useRef<Phantom | null>(null);
  const inFlight = useRef(false);
  const [challenge, setChallenge] = useState<Challenge | null>(null);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");

  async function act(action: () => Promise<void>) {
    if (inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    setError("");
    try { await action(); }
    catch (cause) {
      setError(cause instanceof Error ? cause.message : "Login failed. Please reconnect.");
      setChallenge(null);
      setStatus("");
    } finally { inFlight.current = false; setBusy(false); }
  }

  function connect() {
    return act(async () => {
      const params = new URLSearchParams(window.location.search);
      if (["client_id", "redirect_uri", "state", "resource", "scope", "code_challenge", "code_challenge_method", "response_type"].some(key => !params.get(key))) {
        throw new Error("This login link is incomplete. Start the connection from your MCP client.");
      }
      const wallet = (window as Window & { phantom?: { solana?: Phantom } }).phantom?.solana;
      if (!wallet?.isPhantom) throw new Error("Open this page in a browser with the Phantom extension installed.");
      provider.current = wallet;
      setStatus("Connecting to Phantom…");
      const connected = await wallet.connect();
      const result = await post<Challenge>("challenge", {
        wallet_address: connected.publicKey.toString(), authorization: Object.fromEntries(params),
      });
      setChallenge(result);
      setStatus("Review the permissions and message before signing.");
    });
  }

  function approve() {
    return act(async () => {
      const wallet = provider.current;
      if (!wallet || !challenge) return;
      if (wallet.publicKey?.toString() !== challenge.wallet_address) throw new Error("Wallet changed. Connect again.");
      setStatus("Approve the login message in Phantom…");
      const signed = await wallet.signMessage(new TextEncoder().encode(challenge.message), "utf8");
      const result = await post<{ redirect_to: string }>("verify", {
        challenge_id: challenge.challenge_id, signature: Array.from(signed.signature),
      });
      const expected = new URLSearchParams(window.location.search).get("redirect_uri");
      if (!expected) throw new Error("Missing callback address.");
      const callback = new URL(expected);
      const destination = new URL(result.redirect_to);
      if (callback.origin !== destination.origin || callback.pathname !== destination.pathname) throw new Error("Unexpected callback address.");
      setStatus("Connected. Returning to your client…");
      window.location.assign(destination.toString());
    });
  }

  const permissions = challenge?.message.split("\n").find(line => line.startsWith("Permissions:"))?.replace("Permissions:", "").trim().split(/\s+/) ?? [];
  const permissionLabels: Record<string, string> = { "identity:read": "Read your wallet identity", "portfolio:read": "Read balances and portfolio", "transactions:write": "Prepare transfers for your approval" };
  return (
    <WalletFlow section="Wallet access" lead="Your wallet." title="Your call."
      description="Connect your wallet to give your agent a place to start. You stay in control of every signature."
      steps={[
        { label: "Connect your wallet", detail: "Choose your account in Phantom.", active: !challenge },
        { label: "Review access", detail: "Check the permissions before you sign.", active: !!challenge },
        { label: "Back to your agent", detail: "Continue the conversation, now connected." },
      ]}>
      <div className={flow.panelHeader}><span>01 / Connect</span><span className={flow.badge}>{challenge ? "Wallet connected" : "Awaiting connection"}</span></div>
      <section className={flow.body} aria-labelledby="login-title" aria-busy={busy}>
        <div className={flow.walletIcon} aria-hidden="true"><svg viewBox="0 0 24 24" fill="none"><rect x="3" y="5" width="18" height="15" rx="3" stroke="currentColor" strokeWidth="1.5" /><path d="M3 8h18M16 12h5v5h-5a2.5 2.5 0 0 1 0-5Z" stroke="currentColor" strokeWidth="1.5" /></svg></div>
        <h2 id="login-title">{challenge ? "Review your access" : "Connect with Phantom"}</h2>
        <p className={flow.subtitle}>{challenge ? "Confirm this is your wallet and review what your agent can access." : "Choose your wallet, then sign a message to confirm it belongs to you."}</p>
        {challenge ? <>
          <p className={flow.label}>Connected wallet</p><p className={flow.address}>{challenge.wallet_address}</p>
          <ul className={flow.permissions}>{permissions.map(permission => <li key={permission}><span aria-hidden="true">↗</span><span>{permissionLabels[permission] ?? permission}</span></li>)}</ul>
          <details className={flow.details}><summary>View the exact sign-in message</summary><pre>{challenge.message}</pre></details>
          <div className={flow.actions}>
            <button type="button" onClick={approve} disabled={busy} className={flow.primary}>{busy ? "Waiting for approval…" : "Sign and allow access"}{!busy && <FlowArrow />}</button>
            <button type="button" disabled={busy} className={flow.secondary} onClick={() => { setChallenge(null); setStatus(""); }}>Start again</button>
          </div>
        </> : <>
          <ul className={flow.permissions}>
            <li><span aria-hidden="true">01</span><span>Connect the wallet you want your agent to use.</span></li>
            <li><span aria-hidden="true">02</span><span>Review the requested permissions before granting access.</span></li>
          </ul>
          <div className={flow.actions}><button type="button" onClick={connect} disabled={busy} className={flow.primary}>{busy ? "Connecting…" : "Connect Phantom"}{!busy && <FlowArrow />}</button></div>
        </>}
        {status && <p role="status" className={flow.notice}>{status}</p>}
        {error && <p role="alert" className={flow.error}>{error}</p>}
        <p className={flow.footnote}>This is a sign-in message. Every transfer still needs a separate signature in Phantom.</p>
      </section>
      <div className={flow.panelFooter}>Your private keys stay in your wallet.</div>
    </WalletFlow>
  );
}
