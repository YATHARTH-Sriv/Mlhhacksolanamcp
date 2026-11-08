type PreviewKind = "chat" | "invoice" | "swap" | "treasury";

const features: {
  id: string;
  title: string;
  description: string;
  preview: PreviewKind;
}[] = [
  {
    id: "invoice-payments",
    title: "Turn an invoice into a transaction",
    description: "Give your agent a bill or PDF. Hover pulls out the payment details and prepares a reviewable draft.",
    preview: "invoice",
  },
  {
    id: "agent-payments",
    title: "Ask Hover to prepare a payment",
    description: "Describe a transfer in plain English. Get the recipient, token, and amount laid out before you sign.",
    preview: "chat",
  },
  {
    id: "swap-quotes",
    title: "Preview swaps with Jupiter",
    description: "Compare what you pay and receive with Jupiter quotes, slippage, and route details. Quote preview only; swap execution is coming later.",
    preview: "swap",
  },
  {
    id: "treasury",
    title: "See Your Portfolio Anytime",
    description: "Ask for balances across SOL, USDC, and the tokens your wallet holds, with estimated values from Jupiter prices.",
    preview: "treasury",
  },
];

function Preview({ kind }: { kind: PreviewKind }) {
  if (kind === "chat") {
    return (
      <div className="feature-preview preview-chat" aria-hidden="true">
        <div className="preview-topline"><span className="preview-brand-mark">✳</span><span>Agent workspace</span><span className="preview-topline-right">hover / MCP</span></div>
        <div className="chat-window">
          <div className="chat-rail"><span>✳</span><span>⌘</span><span>◫</span></div>
          <div className="chat-main">
            <div className="chat-header"># payments <span>···</span></div>
            <div className="chat-message"><span className="chat-avatar">Y</span><div><strong>You</strong><p>Prepare a 250 USDC payment for invoice #1042</p></div></div>
            <div className="chat-message"><span className="chat-avatar chat-avatar-hover">h</span><div><strong>hover</strong><p>Draft ready. Review the recipient and amount before signing.</p></div></div>
          </div>
        </div>
      </div>
    );
  }

  if (kind === "invoice") {
    return (
      <div className="feature-preview preview-invoice" aria-hidden="true">
        <div className="terminal-window">
          <div className="terminal-bar"><span className="terminal-dots"><i /><i /><i /></span><span>hover — payment draft</span><span /></div>
          <div className="terminal-body">
            <div className="terminal-command"><span>$</span> hover prepare invoice-1042.pdf</div>
            <div className="terminal-result"><div className="terminal-result-top"><span>PAYMENT DRAFT</span><span>Ready to review</span></div><div><span>Recipient</span><strong>7kQe...9mN4</strong></div><div><span>Amount</span><strong>250.00 USDC</strong></div><div><span>Network</span><strong>Solana</strong></div></div>
          </div>
        </div>
      </div>
    );
  }

  if (kind === "swap") {
    return (
      <div className="feature-preview preview-swap" aria-hidden="true">
        <div className="swap-window">
          <div className="swap-header"><span>⇄ &nbsp; Jupiter swap quote</span><span className="swap-badge">Preview only</span></div>
          <div className="swap-row"><span>You pay</span><strong>1 SOL</strong></div>
          <div className="swap-row"><span>Estimated receive</span><strong>≈ 90 USDC</strong></div>
          <div className="swap-row"><span>Requested slippage</span><strong>0.5%</strong></div>
          <div className="swap-footer">Illustrative quote · Mainnet pricing · No execution</div>
        </div>
      </div>
    );
  }

  return (
    <div className="feature-preview preview-treasury" aria-hidden="true">
      <div className="treasury-window">
        <div className="treasury-header"><span>✦ &nbsp; Treasury overview</span><span className="treasury-pill">Solana</span></div>
        <div className="treasury-balance"><small>Total balance</small><strong>$12,480<span>.20</span></strong><span className="treasury-change">↗ 2.4% this month</span></div>
        <div className="treasury-tokens"><div><span className="token-icon">◎</span><span>SOL</span><strong>$4,200</strong></div><div><span className="token-icon token-usdc">$</span><span>USDC</span><strong>$8,280</strong></div></div>
      </div>
    </div>
  );
}

export default function Features() {
  return (
    <section className="features-section" id="features" aria-labelledby="features-title">
      <div className="features-inner">
        <h2 className="features-title" id="features-title">
          <span>Everything your agent</span>
          <span>needs to be Finance Ready</span>
        </h2>
        <div className="features-grid">
          {features.map((feature) => (
            <article className="feature-card" id={feature.id} key={feature.id}>
              <Preview kind={feature.preview} />
              <div className="feature-copy">
                <h3>{feature.title}</h3>
                <p>{feature.description}</p>
              </div>
            </article>
          ))}
        </div>
      </div>
    </section>
  );
}
