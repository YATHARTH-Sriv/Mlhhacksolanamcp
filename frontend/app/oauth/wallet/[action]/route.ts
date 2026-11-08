// Only these browser-facing operations need cookie forwarding. OAuth token
// exchange and MCP requests are handled by the backend rewrites.
export async function POST(request: Request, context: { params: Promise<{ action: string }> }) {
  const { action } = await context.params;
  if (action !== "challenge" && action !== "verify") return new Response(null, { status: 404 });
  const frontend = process.env.FRONTEND_ORIGIN;
  const backend = process.env.BACKEND_INTERNAL_URL;
  if (!frontend || !backend) return Response.json({ error_description: "Login is not configured." }, { status: 503 });
  const origin = request.headers.get("origin");
  if (origin !== new URL(frontend).origin) return Response.json({ error_description: "Invalid request origin." }, { status: 403 });
  if (!request.headers.get("content-type")?.toLowerCase().startsWith("application/json")) return new Response(null, { status: 415 });

  // Bound the body while reading, rather than buffering an arbitrary upload.
  const reader = request.body?.getReader();
  if (!reader) return new Response(null, { status: 400 });
  const chunks: Uint8Array[] = [];
  let size = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.length;
    if (size > 16_384) { await reader.cancel(); return new Response(null, { status: 413 }); }
    chunks.push(value);
  }
  const headers = new Headers({ "Content-Type": "application/json", Origin: origin });
  const cookie = request.headers.get("cookie")?.split(";").map(p => p.trim()).find(p => p.startsWith("HoverMCPagent_login="));
  if (cookie) headers.set("Cookie", cookie);
  try {
    const upstream = await fetch(new URL(`/oauth/wallet/${action}`, backend), {
      method: "POST", headers, body: Buffer.concat(chunks), cache: "no-store",
      redirect: "manual", signal: AbortSignal.timeout(15_000),
    });
    const outgoing = new Headers({ "Content-Type": upstream.headers.get("content-type") ?? "application/json", "Cache-Control": "no-store" });
    const setCookie = upstream.headers.get("set-cookie");
    if (setCookie) outgoing.set("Set-Cookie", setCookie);
    return new Response(upstream.body, { status: upstream.status, headers: outgoing });
  } catch {
    return Response.json({ error_description: "Login server unavailable. Try again shortly." }, { status: 502, headers: { "Cache-Control": "no-store" } });
  }
}
