"use client";
import "../workspace.scss";
import "../mobile-workspace.scss";
import {usePathname} from "next/navigation";
import type {ReactNode} from "react";
import {ChevronDownIcon} from "./Icons";
import {Sidebar} from "./Sidebar";
import {ThemeToggle} from "./ThemeToggle";

// These routes own the whole viewport, so they get no navigation chrome.
const BARE_PATHS=["/login","/signup","/verify","/onboarding"];

function initials(name:string|null){
  const source=(name??"FlowPay").trim();
  if(!source)return "FP";
  return source.split(/\s+/).slice(0,2).map(word=>word[0]).join("").toUpperCase();
}

export function AppShell({
  merchantName,
  contactName,
  children,
}:{
  merchantName:string|null;
  contactName:string|null;
  children:ReactNode;
}){
  const pathname=usePathname();
  if(pathname==="/"||pathname==="/docs"||pathname.startsWith("/docs/"))return <>{children}</>;
  if(BARE_PATHS.some(path=>pathname===path||pathname.startsWith(`${path}/`))){
    return <main className="main auth-main">{children}</main>;
  }
  return <div className="app-shell clean-shell">
    <header className="topbar">
      <a className="logo" href="/dashboard"><span>FlowPay</span></a>
      <Sidebar merchantName={merchantName}/>
      <div className="topbar-actions">
        <ThemeToggle/>
        <a className="store-switcher" href="/settings" title={contactName??undefined}>
          <span>{merchantName??"Your business"}</span><ChevronDownIcon/>
        </a>
        <a className="account-avatar" href="/settings" aria-label="Account settings">{initials(merchantName)}</a>
      </div>
    </header>
    <main className="main">{children}</main>
  </div>;
}
