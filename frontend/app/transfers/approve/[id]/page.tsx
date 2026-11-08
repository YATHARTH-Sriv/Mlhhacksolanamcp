import { Suspense } from "react";
import Approval from "./approval";

export default function Page() {
  return <Suspense fallback={<p className="p-8" role="status">Loading transfer…</p>}><Approval /></Suspense>;
}
