import {Suspense} from "react";
import {ClaimClient} from "./ClaimClient";

function ClaimLoading(){
  return <main className="claim-loading" aria-live="polite">
    <span>Loading recovery claim…</span>
  </main>;
}

export default function ClaimPage(){
  return <Suspense fallback={<ClaimLoading/>}>
    <ClaimClient/>
  </Suspense>;
}
