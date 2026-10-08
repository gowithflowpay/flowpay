"use client";
import {useState,type FormEvent} from "react";
import {useRouter} from "next/navigation";

export function SettlementWallet({address}:{address:string|null}){
 const router=useRouter();const [value,setValue]=useState("");const [saved,setSaved]=useState<string|null>(null);const [busy,setBusy]=useState(false);const [error,setError]=useState("");
 const configured=address||saved;
 async function save(event:FormEvent){
  event.preventDefault();setError("");const wallet=value.trim();
  if(!/^0x[0-9a-fA-F]{40}$/.test(wallet)||/^0x0{40}$/i.test(wallet)){setError("Enter a valid, non-zero EVM wallet address.");return;}
  setBusy(true);
  try{const response=await fetch("/api/settlement-wallet",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({address:wallet})});const result=await response.json();if(!response.ok)throw new Error(result.error?.message??"Could not save your wallet. Try again.");setSaved(result.settlement_address);router.refresh();}
  catch(reason){setError(reason instanceof Error?reason.message:"Could not save your wallet. Try again.");}
  finally{setBusy(false);}
 }
 if(configured)return <><span className="settings-wallet-address">{configured}</span>{saved?<span className="settings-wallet-success" role="status">Settlement wallet saved.</span>:null}</>;
 return <form className="settlement-wallet-form" onSubmit={save}><label htmlFor="settlement-wallet-address">Add your settlement wallet</label><div className="settlement-wallet-input"><input id="settlement-wallet-address" name="settlement_wallet" value={value} onChange={event=>setValue(event.target.value)} placeholder="0x..." autoComplete="off" spellCheck={false} required disabled={busy} aria-describedby="settlement-wallet-hint"/><button className="workspace-primary" type="submit" disabled={busy}>{busy?"Saving...":"Save wallet"}</button></div><small id="settlement-wallet-hint">Enter your public EVM wallet address. Confirm it is yours before saving.</small>{error?<p role="alert">{error}</p>:null}</form>;
}
