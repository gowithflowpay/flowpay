"use client";
import {useState} from "react";
import {createPasskey,passkeyError} from "../../lib/browser-passkeys";
export function PasskeySettings(){
 const [busy,setBusy]=useState(false);const [notice,setNotice]=useState("");const [error,setError]=useState("");
 async function post(action:string,payload:unknown){const response=await fetch(`/api/auth/${action}`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(payload)});const body=await response.json();if(!response.ok)throw new Error(body.error?.message??"Passkey setup failed");return body;}
 async function add(){setBusy(true);setNotice("");setError("");try{const started=await post("passkey-register-start",{});const credential=await createPasskey(started.options);await post("passkey-register-finish",{challenge_id:started.challenge_id,credential});setNotice("Passkey added. You can use it the next time you sign in.");}catch(reason){setError(passkeyError(reason));}finally{setBusy(false)}}
 return <section className="workspace-card settings-passkeys"><header className="workspace-section-heading"><div><h2>Passkeys</h2><p>Sign in with your fingerprint, face, device screen lock, or security key.</p></div></header><div className="settings-passkey-body"><p>Add a passkey to this account before signing out. You can keep another passkey on a second device.</p><button className="workspace-primary" type="button" disabled={busy} onClick={add}>{busy?"Waiting for your device...":"Add a passkey"}</button>{notice?<p role="status" className="settings-success">{notice}</p>:null}{error?<p role="alert" className="settings-error">{error}</p>:null}</div></section>;
}
