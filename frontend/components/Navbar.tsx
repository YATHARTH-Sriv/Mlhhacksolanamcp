"use client";

import { useEffect, useRef, useState } from "react";
import Link from "next/link";

const productLinks = [
  { label: "Agent payments", href: "#agent-payments" },
  { label: "Treasury", href: "#treasury" },
  { label: "Jupiter swap quotes", href: "#swap-quotes" },
];

const navLinks = [
  { label: "Docs", href: "#careers" },
  { label: "Future Plans", href: "#pricing" },
//   { label: "About Me", href: "#blog" },
];

function Chevron() {
  return <svg aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="m4.5 6 3.5 3.5L11.5 6" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" /></svg>;
}

function Arrow() {
  return <svg aria-hidden="true" viewBox="0 0 16 16" fill="none"><path d="M4 12 12 4M5.5 4H12v6.5" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" /></svg>;
}

export default function Navbar() {
  const [productOpen, setProductOpen] = useState(false);
  const [mobileOpen, setMobileOpen] = useState(false);
  const [dark, setDark] = useState(false);
  const productRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function onPointerDown(event: PointerEvent) {
      if (productRef.current && !productRef.current.contains(event.target as Node)) setProductOpen(false);
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setProductOpen(false);
        setMobileOpen(false);
      }
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, []);

  useEffect(() => {
    function syncTheme() {
      const features = document.getElementById("features");
      if (!features) return;
      const bounds = features.getBoundingClientRect();
      setDark(bounds.top <= -140 && bounds.bottom > 0);
    }

    syncTheme();
    window.addEventListener("scroll", syncTheme, { passive: true });
    window.addEventListener("resize", syncTheme);
    return () => {
      window.removeEventListener("scroll", syncTheme);
      window.removeEventListener("resize", syncTheme);
    };
  }, []);

  return (
    <header className={`site-header${dark ? " is-dark" : ""}`}>
      <Link className="wordmark" href="/" aria-label="Hover home">hover<span className="wordmark-dot">.</span></Link>

      <nav className="desktop-nav" aria-label="Main navigation">
        <div className="product-menu" ref={productRef}>
          <button className="nav-link product-trigger" type="button" aria-expanded={productOpen} aria-controls="product-dropdown" onClick={() => setProductOpen((open) => !open)}>
            Product <Chevron />
          </button>
          {productOpen && (
            <div className="product-dropdown" id="product-dropdown">
              {productLinks.map((link) => <a key={link.label} href={link.href} onClick={() => setProductOpen(false)}>{link.label}</a>)}
            </div>
          )}
        </div>
        {navLinks.map((link) => <a className="nav-link" key={link.label} href={link.href}>{link.label}</a>)}
      </nav>

      <div className="desktop-actions">
        <span className="nav-divider" aria-hidden="true" />
        <a className="button button-outline" href="#book-demo">Github</a>
        <a className="button button-dark" href="#get-started">Get Started <Arrow /></a>
      </div>

      <button className="mobile-toggle" type="button" aria-label={mobileOpen ? "Close menu" : "Open menu"} aria-controls="mobile-navigation" aria-expanded={mobileOpen} onClick={() => setMobileOpen((open) => !open)}>
        <span /><span />
      </button>

      {mobileOpen && (
        <nav className="mobile-nav" id="mobile-navigation" aria-label="Mobile navigation">
          <a className="mobile-link mobile-link-active" href="#manifesto" onClick={() => setMobileOpen(false)}><span className="active-dot" aria-hidden="true" />Manifesto</a>
          <span className="mobile-section-label">Product</span>
          {productLinks.map((link) => <a className="mobile-link mobile-sublink" key={link.label} href={link.href} onClick={() => setMobileOpen(false)}>{link.label}</a>)}
          {navLinks.map((link) => <a className="mobile-link" key={link.label} href={link.href} onClick={() => setMobileOpen(false)}>{link.label}</a>)}
          <div className="mobile-actions">
            <a className="button button-outline" href="#book-demo" onClick={() => setMobileOpen(false)}>Book a demo</a>
            <a className="button button-dark" href="#get-started" onClick={() => setMobileOpen(false)}>Start free <Arrow /></a>
          </div>
        </nav>
      )}
    </header>
  );
}
