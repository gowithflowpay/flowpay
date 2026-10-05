"use client";

import {useState} from "react";
import {CheckIcon,ShieldIcon} from "../../components/Icons";
import styles from "./DeviceApprovalPanel.module.scss";

type Decision="approve"|"deny";

export function DeviceApprovalPanel(){
  const [userCode,setUserCode]=useState("");
  const [busy,setBusy]=useState<Decision|false>(false);
  const [error,setError]=useState("");
  const [result,setResult]=useState<{decision:Decision;deviceName?:string;prefix?:string}|null>(null);

  async function decide(decision:Decision){
    if(busy||result)return;
    setBusy(decision);
    setError("");
    try{
      const response=await fetch("/api/cli/device",{
        method:"POST",
        headers:{"content-type":"application/json"},
        body:JSON.stringify({decision,user_code:userCode.trim().toUpperCase()}),
      });
      const body=await response.json();
      if(!response.ok)throw new Error(body?.error?.message??"Could not process this device request.");
      setResult({decision,deviceName:body.device_name,prefix:body.prefix});
    }catch(value){
      setError(value instanceof Error?value.message:"Could not process this device request.");
    }finally{
      setBusy(false);
    }
  }

  return <>
    <div className="page-head"><div><div className="eyebrow">Developer access</div><h1>Connect a CLI device</h1><p>Approve a terminal or agent to use your FlowPay account.</p></div></div>
    <section className={`panel ${styles.deviceApprovalPanel}`}>
      {result?<div className={`${styles.deviceResult} ${result.decision==="deny"?styles.denied:""}`} role="status">
        <div className={styles.deviceResultIcon}>{result.decision==="approve"?<CheckIcon/>:<ShieldIcon/>}</div>
        <div><h2>{result.decision==="approve"?"Device approved":"Request denied"}</h2>
          {result.decision==="approve"?<><p><strong>{result.deviceName??"FlowPay CLI"}</strong> can now finish signing in. Return to the terminal and wait for setup to complete.</p>{result.prefix?<p className={styles.deviceKeyPrefix}>Key prefix <code>{result.prefix}</code> · revoke it anytime from API Keys.</p>:null}</>:<p>The terminal request was denied. Its temporary code cannot be approved later.</p>}
        </div>
      </div>:<>
        <div className={styles.deviceApprovalIcon}><ShieldIcon/></div>
        <h2>Approve a sign-in request</h2>
        <p className={styles.deviceApprovalCopy}>Run <code>flowpay init</code> in your terminal, then enter the temporary code shown there. Only approve a request you started.</p>
        <form className={styles.deviceApprovalForm} onSubmit={event=>{event.preventDefault();void decide("approve");}}>
          <label htmlFor="device-user-code">Device code</label>
          <input id="device-user-code" value={userCode} onChange={event=>setUserCode(event.target.value.toUpperCase().replace(/[^ABCDEFGHJKMNPQRSTVWXYZ2-9-]/g,"").slice(0,9))} autoComplete="off" autoCapitalize="characters" spellCheck={false} placeholder="ABCD-EFGH" maxLength={9} required pattern="[ABCDEFGHJKMNPQRSTVWXYZ2-9]{4}-[ABCDEFGHJKMNPQRSTVWXYZ2-9]{4}" title="Enter the 8-character code shown in your terminal."/>
          <div className={styles.deviceApprovalDisclosure}><ShieldIcon/><span>Approval creates a revocable API key for this device. The key is delivered to the terminal once and is never displayed here.</span></div>
          {error?<p className="form-error" role="alert">{error}</p>:null}
          <div className={styles.deviceApprovalActions}>
            <button className="btn secondary" type="button" disabled={Boolean(busy)||userCode.length!==9} onClick={()=>void decide("deny")}>{busy==="deny"?"Denying…":"Deny request"}</button>
            <button className="btn primary" type="submit" disabled={Boolean(busy)||userCode.length!==9}>{busy==="approve"?"Approving…":"Approve device"}</button>
          </div>
        </form>
      </>}
    </section>
  </>;
}
