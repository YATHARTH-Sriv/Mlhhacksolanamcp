function ArrowIcon() {
  return <svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M5 19 19 5M8 5h11v11" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" /></svg>;
}

export default function Hero() {
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
          <a className="hero-secondary" href="https://github.com/YATHARTH-Sriv/Mlhhacksolanamcp">
            How to Connect <span className="hero-arrow"><ArrowIcon /></span>
          </a>
        </div>
      </div>
    </section>
  );
}
