"use client";
import {useEffect,useState} from "react";
import styles from "./page.module.scss";

const entries=[{id:"quickstart",label:"Installation",group:"GET STARTED"},{id:"account",label:"Registration & login",group:"GET STARTED"},{id:"payments",label:"Request payments",group:"PAYMENT WORKFLOWS"},{id:"agents",label:"Agent to agent",group:"PAYMENT WORKFLOWS"},{id:"mcp",label:"MCP connections",group:"INTEGRATIONS"},{id:"api",label:"API & reference",group:"INTEGRATIONS"}];

export function DocsNavigation(){
  const [query,setQuery]=useState("");
  const [active,setActive]=useState("quickstart");
  const [open,setOpen]=useState(false);
  useEffect(()=>{
    const update=()=>{const current=entries.filter(entry=>(document.getElementById(entry.id)?.getBoundingClientRect().top??Infinity)<180).at(-1);setActive(current?.id??"quickstart")};
    window.addEventListener("scroll",update,{passive:true});update();
    return()=>window.removeEventListener("scroll",update);
  },[]);
  const filtered=entries.filter(entry=>(entry.label+" "+entry.group).toLowerCase().includes(query.toLowerCase()));
  return <aside className={styles.sidebar}><button className={styles.mobileMenu} type="button" onClick={()=>setOpen(!open)} aria-expanded={open} aria-controls="docs-navigation">Documentation menu <span>{open?"−":"+"}</span></button><div id="docs-navigation" className={`${styles.navigation} ${open?styles.navigationOpen:""}`}><label className={styles.search}><svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="8" cy="8" r="5"/><path d="m12 12 5 5"/></svg><input type="search" value={query} onChange={event=>setQuery(event.target.value)} placeholder="Find a topic..." aria-label="Find a documentation topic"/></label><nav aria-label="Documentation topics">{["GET STARTED","PAYMENT WORKFLOWS","INTEGRATIONS"].map(group=>{const items=filtered.filter(entry=>entry.group===group);return items.length?<div className={styles.navGroup} key={group}><span>{group}</span>{items.map(entry=><a className={active===entry.id?styles.active:""} aria-current={active===entry.id?"location":undefined} href={`#${entry.id}`} key={entry.id} onClick={()=>{setActive(entry.id);setOpen(false)}}>{entry.label}<span aria-hidden="true">→</span></a>)}</div>:null})}</nav>{!filtered.length&&<p className={styles.noResults}>No topics found.</p>}<a className={styles.sidebarSource} href="https://github.com/gowithflowpay/flowpay">Source on GitHub <span>&nearr;</span></a></div></aside>;
}

export function CodeBlock({label,code}:{label:string;code:string}){
  const [state,setState]=useState<"idle"|"copied"|"failed">("idle");
  useEffect(()=>{if(state==="idle")return;const timer=setTimeout(()=>setState("idle"),2500);return()=>clearTimeout(timer)},[state]);
  return <div className={styles.codeBlock}><div className={styles.codeHeader}><span><i aria-hidden="true">&gt;_</i>{label}</span><button type="button" aria-label={`Copy ${label}`} onClick={async()=>{try{await navigator.clipboard.writeText(code);setState("copied")}catch{setState("failed")}}}>{state==="copied"?"Copied":state==="failed"?"Select to copy":"Copy"}<svg viewBox="0 0 18 18" aria-hidden="true"><rect x="6" y="6" width="9" height="9" rx="1.5"/><path d="M3 12H2V2h10v1"/></svg></button></div><pre><code>{code}</code></pre><span className={styles.srOnly} role="status">{state==="copied"?`${label} copied`:state==="failed"?"Clipboard unavailable. Select the command to copy it.":""}</span></div>;
}
