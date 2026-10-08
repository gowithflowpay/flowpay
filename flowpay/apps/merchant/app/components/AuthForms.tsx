"use client";
import {useRouter} from "next/navigation";
import {authenticatePasskey,createPasskey,passkeyError} from "../../lib/browser-passkeys";
import {useState,type FormEvent,type ReactNode} from "react";

/** Posts to the identity route, which is the only place the session cookie is written. */
async function post(action:string,payload:unknown){
  const response=await fetch(`/api/auth/${action}`,{
    method:"POST",
    headers:{"content-type":"application/json"},
    body:JSON.stringify(payload),
  });
  const data=await response.json().catch(()=>({}));
  if(!response.ok)throw new Error(data?.error?.message??"Something went wrong. Please try again.");
  return data;
}

function AuthShell({title,subtitle,children,footer,videoBackground=false,cardless=false}:{title:string;subtitle:string;children:ReactNode;footer?:ReactNode;videoBackground?:boolean;cardless?:boolean}){
  return <div className={`auth-page${videoBackground?" auth-page-video":""}${cardless?" auth-page-cardless":""}`}>
    {videoBackground?<video className="auth-background-video" autoPlay muted loop playsInline preload="auto" aria-hidden="true"><source src="/assets/merchant-background.mp4" type="video/mp4"/></video>:null}
    <div className="auth-brand">{cardless?<img className="auth-brand-logo" src="/assets/flowpay-mark.svg" alt=""/>:<span className="auth-mark" aria-hidden="true">F</span>}<span>FlowPay</span></div>
    <div className="auth-card">
      <h1>{title}</h1>
      <p className="auth-subtitle">{subtitle}</p>
      {children}
    </div>
    {footer?<div className="auth-footer">{footer}</div>:null}
  </div>;
}

function Notice({tone,children}:{tone:"error"|"success"|"info";children:ReactNode}){
  return <div className={`auth-notice ${tone}`} role={tone==="error"?"alert":"status"}>{children}</div>;
}

function Field({label,name,type="text",value,onChange,placeholder,autoComplete,required=true,hint,minLength}:{
  label:string;name:string;type?:string;value:string;
  onChange:(value:string)=>void;placeholder?:string;autoComplete?:string;
  required?:boolean;hint?:string;minLength?:number;
}){
  return <label className="auth-field">
    <span>{label}</span>
    <input
      name={name}
      type={type}
      value={value}
      placeholder={placeholder}
      autoComplete={autoComplete}
      required={required}
      minLength={minLength}
      onChange={event=>onChange(event.target.value)}
    />
    {hint?<small>{hint}</small>:null}
  </label>;
}

function Submit({busy,children}:{busy:boolean;children:ReactNode}){
  return <button className="auth-submit" type="submit" disabled={busy}>
    {busy?<span className="auth-spinner" aria-hidden="true"/>:null}
    {children}
  </button>;
}

export function SignupForm(){
  const router=useRouter();const [name,setName]=useState("");const [busy,setBusy]=useState(false);const [error,setError]=useState<string|null>(null);
  async function submit(event:FormEvent){
    event.preventDefault();setBusy(true);setError(null);
    try{const started=await post("passkey-signup-start",{business_name:name});const credential=await createPasskey(started.options);await post("passkey-signup-finish",{challenge_id:started.challenge_id,credential});router.replace("/onboarding");router.refresh();}
    catch(reason){setError(passkeyError(reason));setBusy(false);}
  }
  return <AuthShell title="Your business starts here" subtitle="Create a passkey to secure your account. Your device handles sign-in." videoBackground cardless footer={<span>Already have an account? <a href="/login">Sign in</a></span>}>
    <form onSubmit={submit}>{error?<Notice tone="error">{error}</Notice>:null}<Field label="Business name" name="business_name" value={name} onChange={setName} placeholder="Your business" autoComplete="organization"/><p className="passkey-explainer">Use your fingerprint, face, device screen lock, or security key. Wallet and recovery contact come next.</p><Submit busy={busy}>{busy?"Creating your passkey...":"Create account with a passkey"}</Submit></form>
  </AuthShell>;
}

export function LoginForm({next}:{next:string|null}){
  const router=useRouter();const [busy,setBusy]=useState(false);const [error,setError]=useState<string|null>(null);
  async function submit(event:FormEvent){
    event.preventDefault();setBusy(true);setError(null);
    try{const started=await post("passkey-login-start",{});const credential=await authenticatePasskey(started.options);const data=await post("passkey-login-finish",{challenge_id:started.challenge_id,credential});const target=next&&next.startsWith("/")&&!next.startsWith("//")?next:"/dashboard";router.replace(data.merchant?.onboarding_completed?target:"/onboarding");router.refresh();}
    catch(reason){setError(passkeyError(reason));setBusy(false);}
  }
  return <AuthShell title="Welcome back" subtitle="Your passkey is all you need to sign in." videoBackground cardless footer={<span>New to FlowPay? <a href="/signup">Create an account</a></span>}><form onSubmit={submit}>{error?<Notice tone="error">{error}</Notice>:null}<div className="passkey-signin-mark" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><circle cx="8" cy="8" r="4"/><path d="m11 11 9 9m-3-3 3-3m-6 0 3-3"/></svg></div><p className="passkey-explainer">Choose your saved FlowPay passkey and approve the request on your device.</p><Submit busy={busy}>{busy?"Waiting for your passkey...":"Sign in with a passkey"}</Submit></form></AuthShell>;
}

export function OnboardingForm({defaultName,defaultContact}:{defaultName:string;defaultContact:string|null}){
 const router=useRouter();const [step,setStep]=useState(1);const [address,setAddress]=useState("");const [email,setEmail]=useState("");const [contact,setContact]=useState(defaultContact??"");const [busy,setBusy]=useState(false);const [error,setError]=useState<string|null>(null);
 async function submit(event:FormEvent){event.preventDefault();setError(null);if(step===1){if(address&&!/^0x[0-9a-fA-F]{40}$/.test(address.trim())){setError("Enter a valid EVM wallet address.");return;}setStep(2);return;}setBusy(true);try{await post("onboarding",{business_name:defaultName,contact_name:contact||undefined,evm_settlement_address:address.trim()||undefined,recovery_email:email.trim()||undefined,require_passkey:true});router.replace("/dashboard");router.refresh();}catch(reason){setError(reason instanceof Error?reason.message:"Could not finish setup.");setBusy(false);}}
 return <AuthShell title={step===1?"Where should payments settle?":"Add a recovery contact"} subtitle={step===1?"Your passkey is ready. Add your settlement wallet, or do this later.":"An optional contact for account updates. Email is never your sign-in method."} footer={<span>Step {step} of 2. Both details can be added later.</span>}><div className="onboarding-progress"><i className="complete"/><i className={step===2?"complete":""}/></div><form onSubmit={submit}>{error?<Notice tone="error">{error}</Notice>:null}{step===1?<Field label="Settlement wallet (optional)" name="evm_settlement_address" value={address} onChange={setAddress} placeholder="0x..." autoComplete="off" required={false} hint="Use a wallet address on a supported EVM network."/>:<><Field label="Recovery email (optional)" name="recovery_email" type="email" value={email} onChange={setEmail} placeholder="you@company.com" autoComplete="email" required={false} hint="You can verify this email later. No code is sent during setup."/><Field label="Contact name (optional)" name="contact_name" value={contact} onChange={setContact} autoComplete="name" required={false}/></>}<Submit busy={busy}>{step===1?"Continue":"Finish setup"}</Submit>{step===1?<button className="auth-link-button" type="button" onClick={()=>{setAddress("");setStep(2)}}>Add wallet later</button>:<button className="auth-link-button" type="button" onClick={()=>setStep(1)}>Back to wallet</button>}</form></AuthShell>;
}
