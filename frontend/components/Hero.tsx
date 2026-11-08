"use client";

import { useEffect, useRef, useState } from "react";

const agentPrompt = "Help me prepare a Solana payment with Hover. Extract the recipient, token, and amount from my invoice, show me the exact transaction details, and wait for my explicit wallet approval before submitting anything.";

function CopyIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <rect x="8" y="8" width="11" height="11" rx="1.5" stroke="currentColor" strokeWidth="1.8" />
      <path d="M16 8V5.5A1.5 1.5 0 0 0 14.5 4h-9A1.5 1.5 0 0 0 4 5.5v9A1.5 1.5 0 0 0 5.5 16H8" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  );
}

function CheckIcon() {
  return <svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="m5 12 4.5 4.5L19 7" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" /></svg>;
}

function ArrowIcon() {
  return <svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M5 19 19 5M8 5h11v11" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" /></svg>;
}

export default function Hero() {
  const [copyState, setCopyState] = useState<"idle" | "copied" | "error">("idle");
  const resetTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => () => {
    if (resetTimer.current) clearTimeout(resetTimer.current);
  }, []);

  async function copyPrompt() {
    try {
      await navigator.clipboard.writeText(agentPrompt);
      setCopyState("copied");
    } catch {
      setCopyState("error");
    }
    if (resetTimer.current) clearTimeout(resetTimer.current);
    resetTimer.current = setTimeout(() => setCopyState("idle"), 2400);
  }

  return (
    <section className="hero" id="manifesto" aria-labelledby="hero-title">
      <div className="hero-content">
        <h1 className="hero-title" id="hero-title">
          <span className="hero-title-muted">Your agent is ready to pay.</span>
          <span className="hero-title-strong">Give it the Power of Solana.</span>
        </h1>
        <p className="hero-description">
          Hover gives your agents tools to Manage your onchain operations .
        </p>
        <div className="hero-actions" id="get-started">
          <button className="prompt-button" type="button" onClick={copyPrompt} aria-label={copyState === "copied" ? "Agent prompt copied" : "Copy MCP url"}>
            <span className="prompt-symbols" aria-hidden="true"><span>✳</span><span>✺</span><span>⬡</span></span>
            <span className="prompt-label" aria-live="polite">
              {copyState === "copied" ? "Copied to clipboard" : copyState === "error" ? "Copy failed — try again" : "Copy agent prompt"}
            </span>
            <span className="prompt-icon">{copyState === "copied" ? <CheckIcon /> : <CopyIcon />}</span>
          </button>
          <button className="hero-secondary" type="button" onClick={copyPrompt} aria-label="Start free by copying the agent prompt">
            How To Connect <span className="hero-arrow"><ArrowIcon /></span>
          </button>
        </div>
      </div>
    </section>
  );
}
