"use client";
import {useState} from "react";
import styles from "./ConsentPanel.module.scss";

export function ConsentPanel({requestId,clientName,callbackHost,scopes,email}:{requestId:string;clientName:string;callbackHost:string;scopes:string[];email:string}){
  const [busy,setBusy]=useState<"approve"|"deny"|null>(null);
  const [error,setError]=useState("");
  async function decide(approved:boolean){
    setBusy(approved?"approve":"deny");setError("");
    try{
      const response=await fetch(`/api/oauth/requests/${requestId}`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({approved})});
      const body=await response.json();
      if(!response.ok)throw new Error(body?.error?.message??"Unable to authorize this connection");
      const target=new URL(body.redirect_uri);
      if(target.hostname!==callbackHost)throw new Error("The callback does not match this connection");
      window.location.assign(target.toString());
    }catch(value){setError(value instanceof Error?value.message:"Unable to connect");setBusy(null);}
  }
  return <section className={styles.consent}>
    <a href="/dashboard" className={styles.brand}><img src="/assets/flowpay-mark.svg" alt=""/>FlowPay</a>
    <div className={styles.card}>
      <span className={styles.eyebrow}>Account connection</span>
      <h1>Connect {clientName}</h1><p className={styles.subtitle}>Allow this app to use FlowPay with your account.</p>
      <div className={styles.account}><span>Signed in as</span><strong>{email}</strong></div>
      <h2>This app is requesting access to</h2>
      <ul>{scopes.map(scope=><li key={scope}>{scope==="payments:write"?"Create crypto payment requests and checkout links":"View your payments and their status"}</li>)}</ul>
      <p className={styles.note}>Your wallet keys are never shared. You can disconnect this app from Developer Access at any time.</p>
      <div className={styles.callback}><span>Returning to</span><strong>{callbackHost}</strong></div>
      {error?<p className={styles.error} role="alert">{error}</p>:null}
      <div className={styles.actions}><button type="button" className="btn secondary" disabled={Boolean(busy)} onClick={()=>void decide(false)}>{busy==="deny"?"Returning...":"Cancel"}</button><button type="button" className="btn primary" disabled={Boolean(busy)} onClick={()=>void decide(true)}>{busy==="approve"?"Connecting...":"Authorize connection"}</button></div>
    </div>
  </section>;
}
