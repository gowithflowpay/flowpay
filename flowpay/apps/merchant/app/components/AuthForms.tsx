"use client";
import {useRouter} from "next/navigation";
import {useEffect,useRef,useState,type FormEvent,type ReactNode} from "react";

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
  const router=useRouter();
  const [businessName,setBusinessName]=useState("");
  const [contactName,setContactName]=useState("");
  const [email,setEmail]=useState("");
  const [password,setPassword]=useState("");
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState<string|null>(null);

  async function submit(event:FormEvent){
    event.preventDefault();
    setBusy(true);
    setError(null);
    try{
      const data=await post("signup",{
        business_name:businessName,
        contact_name:contactName||undefined,
        email,
        password,
      });
      const target=new URLSearchParams({email:String(data.email??email)});
      if(data.email_sent===false)target.set("notice","delivery");
      router.push(`/verify?${target.toString()}`);
    }catch(caught){
      setError(caught instanceof Error?caught.message:"Could not create your account");
      setBusy(false);
    }
  }

  return <AuthShell
    title="Create your account"
    videoBackground
    cardless
    subtitle="Start accepting payments in minutes. No card required."
    footer={<span>Already have an account? <a href="/login">Sign in</a></span>}
  >
    <form onSubmit={submit} noValidate={false}>
      {error?<Notice tone="error">{error}</Notice>:null}
      <Field label="Business name" name="business_name" value={businessName} onChange={setBusinessName} placeholder="Acme Store" autoComplete="organization"/>
      <Field label="Your name" name="contact_name" value={contactName} onChange={setContactName} placeholder="Ada Lovelace" autoComplete="name" required={false}/>
      <Field label="Work email" name="email" type="email" value={email} onChange={setEmail} placeholder="you@company.com" autoComplete="email"/>
      <Field label="Password" name="password" type="password" value={password} onChange={setPassword} placeholder="At least 8 characters" autoComplete="new-password" minLength={8}/>
      <Submit busy={busy}>Create account</Submit>
    </form>
  </AuthShell>;
}

export function LoginForm({next}:{next:string|null}){
  const router=useRouter();
  const [email,setEmail]=useState("");
  const [password,setPassword]=useState("");
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState<string|null>(null);

  async function submit(event:FormEvent){
    event.preventDefault();
    setBusy(true);
    setError(null);
    try{
      const data=await post("login",{email,password});
      const onboarded=Boolean(data?.merchant?.onboarding_completed);
      const destination=onboarded?(next&&next.startsWith("/")?next:"/dashboard"):"/onboarding";
      router.replace(destination);
      router.refresh();
    }catch(caught){
      const message=caught instanceof Error?caught.message:"Could not sign you in";
      if(/verify your email/i.test(message)){
        router.push(`/verify?${new URLSearchParams({email,purpose:"login"}).toString()}`);
        return;
      }
      setError(message);
      setBusy(false);
    }
  }

  return <AuthShell
    title="Welcome back"
    videoBackground
    cardless
    subtitle="Sign in to manage payments, transfers and settings."
    footer={<span>New to FlowPay? <a href="/signup">Create an account</a></span>}
  >
    <form onSubmit={submit}>
      {error?<Notice tone="error">{error}</Notice>:null}
      <Field label="Email" name="email" type="email" value={email} onChange={setEmail} placeholder="you@company.com" autoComplete="email"/>
      <Field label="Password" name="password" type="password" value={password} onChange={setPassword} placeholder="Your password" autoComplete="current-password"/>
      <Submit busy={busy}>Sign in</Submit>
    </form>
  </AuthShell>;
}

export function VerifyForm({email,initialNotice}:{email:string;initialNotice:string|null}){
  const router=useRouter();
  const [address,setAddress]=useState(email);
  const [code,setCode]=useState("");
  const [busy,setBusy]=useState(false);
  const [resending,setResending]=useState(false);
  const [cooldown,setCooldown]=useState(0);
  const [error,setError]=useState<string|null>(null);
  const [notice,setNotice]=useState<string|null>(null);
  const inputRef=useRef<HTMLInputElement>(null);

  useEffect(()=>{inputRef.current?.focus();},[]);
  useEffect(()=>{
    if(cooldown<=0)return;
    const timer=setTimeout(()=>setCooldown(value=>value-1),1000);
    return ()=>clearTimeout(timer);
  },[cooldown]);

  async function submit(event:FormEvent){
    event.preventDefault();
    setBusy(true);
    setError(null);
    try{
      const data=await post("verify",{email:address,code});
      const onboarded=Boolean(data?.merchant?.onboarding_completed);
      router.replace(onboarded?"/dashboard":"/onboarding");
      router.refresh();
    }catch(caught){
      setError(caught instanceof Error?caught.message:"We could not verify that code");
      setCode("");
      setBusy(false);
    }
  }

  async function resend(){
    setResending(true);
    setError(null);
    setNotice(null);
    try{
      await post("resend",{email:address,purpose:"SIGNUP"});
      setNotice(`A new code is on its way to ${address}.`);
      setCooldown(45);
    }catch(caught){
      setError(caught instanceof Error?caught.message:"We could not send a new code");
    }finally{
      setResending(false);
    }
  }

  return <AuthShell
    title="Check your email"
    subtitle={address?`We sent a 6 digit code to ${address}.`:"Enter the 6 digit code we emailed you."}
    footer={<span>Wrong address? <a href="/signup">Start again</a></span>}
  >
    <form onSubmit={submit}>
      {error?<Notice tone="error">{error}</Notice>:null}
      {notice?<Notice tone="info">{notice}</Notice>:null}
      {initialNotice==="delivery"?<Notice tone="info">Your account is ready, but the first email could not be delivered. Use resend below.</Notice>:null}
      {!email?<Field label="Email" name="email" type="email" value={address} onChange={setAddress} autoComplete="email"/>:null}
      <label className="auth-field">
        <span>Verification code</span>
        <input
          ref={inputRef}
          className="auth-code-input"
          name="code"
          inputMode="numeric"
          autoComplete="one-time-code"
          maxLength={6}
          placeholder="——————"
          value={code}
          onChange={event=>setCode(event.target.value.replace(/\D/g,"").slice(0,6))}
          required
        />
      </label>
      <Submit busy={busy}>Verify and continue</Submit>
      <button className="auth-link-button" type="button" onClick={resend} disabled={resending||cooldown>0}>
        {cooldown>0?`Resend code in ${cooldown}s`:"Didn't get a code? Resend"}
      </button>
    </form>
  </AuthShell>;
}

export function OnboardingForm({defaultName,defaultContact}:{defaultName:string;defaultContact:string|null}){
  const router=useRouter();
  const [step,setStep]=useState(1);
  const [businessName,setBusinessName]=useState(defaultName==="FlowPay"?"":defaultName);
  const [contactName,setContactName]=useState(defaultContact??"");
  const [address,setAddress]=useState("");
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState<string|null>(null);
  const [apiKey,setApiKey]=useState<string|null>(null);
  const [copied,setCopied]=useState(false);

  async function submit(event:FormEvent){
    event.preventDefault();
    setBusy(true);
    setError(null);
    try{
      const data=await post("onboarding",{
        business_name:businessName,
        contact_name:contactName||undefined,
        evm_settlement_address:address.trim()||undefined,
      });
      setApiKey(data?.api_key??null);
      setStep(2);
      router.refresh();
    }catch(caught){
      setError(caught instanceof Error?caught.message:"We could not save your details");
    }finally{
      setBusy(false);
    }
  }

  async function copyKey(){
    if(!apiKey)return;
    try{
      await navigator.clipboard.writeText(apiKey);
      setCopied(true);
      setTimeout(()=>setCopied(false),2000);
    }catch{
      setCopied(false);
    }
  }

  if(step===2){
    return <AuthShell
      title="You're ready to accept payments"
      subtitle="Your settlement wallet is linked and your workspace is live."
    >
      {apiKey?<Notice tone="success">
        <strong>Your API key</strong>
        <p>Copy it now — for security it is shown only once.</p>
        <div className="auth-secret">
          <code>{apiKey}</code>
          <button type="button" onClick={copyKey}>{copied?"Copied":"Copy"}</button>
        </div>
      </Notice>:null}
      <button className="auth-submit" type="button" onClick={()=>{router.replace("/dashboard");router.refresh();}}>
        Go to dashboard
      </button>
    </AuthShell>;
  }

  return <AuthShell
    title="Set up your workspace"
    subtitle="One detail and you can create your first payment link."
    footer={<span>You can change these later in settings.</span>}
  >
    <form onSubmit={submit}>
      {error?<Notice tone="error">{error}</Notice>:null}
      <Field label="Business name" name="business_name" value={businessName} onChange={setBusinessName} placeholder="Acme Store" autoComplete="organization"/>
      <Field label="Settlement wallet" name="evm_settlement_address" value={address} onChange={setAddress} placeholder="0x…" autoComplete="off" required={false} hint="Add the EVM wallet where FlowPay will settle your crypto payments."/>
      <Field label="Contact name" name="contact_name" value={contactName} onChange={setContactName} placeholder="Ada Lovelace" autoComplete="name" required={false}/>
      <Submit busy={busy}>Finish setup</Submit>
    </form>
  </AuthShell>;
}
