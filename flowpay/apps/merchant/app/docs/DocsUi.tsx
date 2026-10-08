"use client";

import {useEffect,useRef,useState} from "react";
import type {ReactNode} from "react";
import styles from "./page.module.scss";

export const topics=[
  {id:"quickstart",label:"Quickstart",group:"Get started",keywords:"installation terminal cli npm"},
  {id:"account",label:"Authentication",group:"Get started",keywords:"registration login email account"},
  {id:"payments",label:"Request payments",group:"Payment workflows",keywords:"checkout usdc wallet"},
  {id:"agents",label:"Agent workflows",group:"Payment workflows",keywords:"automation json"},
  {id:"mcp",label:"MCP connections",group:"Integrations",keywords:"chatgpt claude oauth"},
  {id:"api",label:"API reference",group:"Integrations",keywords:"rest curl sdk http"},
  {id:"webhooks",label:"Webhooks",group:"Integrations",keywords:"events notifications settlement"},
];

export function Icon({name="arrow",className}:{name?:string;className?:string}){
  const paths:Record<string,ReactNode>={
    arrow:<path d="M5 12h14m-6-6 6 6-6 6"/>,
    external:<><path d="M14 5h5v5M19 5l-9 9"/><path d="M19 14v5H5V5h5"/></>,
    terminal:<path d="m5 7 5 5-5 5m8 0h6"/>,
    layers:<path d="m12 3 9 5-9 5-9-5 9-5Zm-9 9 9 5 9-5m-18 5 9 5 9-5"/>,
    code:<path d="m8 7-5 5 5 5m8-10 5 5-5 5m-3-13-2 16"/>,
    link:<><path d="m10 13 4-4m-5 7-2 2a4 4 0 0 1-6-6l4-4a4 4 0 0 1 6 0m2-1 2-2a4 4 0 0 1 6 6l-4 4a4 4 0 0 1-6 0"/></>,
    check:<path d="m5 12 4 4L19 6"/>,
    search:<><circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 4 4"/></>,
    copy:<><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 4H4v12"/></>,
    menu:<path d="M4 6h16M4 12h16M4 18h16"/>,
    close:<path d="m6 6 12 12M6 18 18 6"/>,
    book:<path d="M12 5v15m0-15C8 2 4 3 2 4v15c4-2 7-1 10 1 3-2 6-3 10-1V4c-2-1-6-2-10 1Z"/>,
  };
  return <svg className={className} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]??paths.arrow}</svg>;
}

function useActiveTopic(){
  const [active,setActive]=useState("quickstart");
  useEffect(()=>{
    const update=()=>setActive(topics.filter(topic=>(document.getElementById(topic.id)?.getBoundingClientRect().top??Infinity)<190).at(-1)?.id??"quickstart");
    window.addEventListener("scroll",update,{passive:true});update();
    return()=>window.removeEventListener("scroll",update);
  },[]);
  return active;
}

export function DocsNavigation(){
  const [query,setQuery]=useState("");
  const [open,setOpen]=useState(false);
  const active=useActiveTopic();
  const search=useRef<HTMLInputElement>(null);
  useEffect(()=>{
    const keyboard=(event:KeyboardEvent)=>{
      if((event.metaKey||event.ctrlKey)&&event.key.toLowerCase()==="k"){
        event.preventDefault();setOpen(true);requestAnimationFrame(()=>search.current?.focus());
      }
      if(event.key==="Escape"){setOpen(false);search.current?.blur();}
    };
    window.addEventListener("keydown",keyboard);return()=>window.removeEventListener("keydown",keyboard);
  },[]);
  const filtered=topics.filter(topic=>(topic.label+" "+topic.group+" "+topic.keywords).toLowerCase().includes(query.toLowerCase().trim()));
  return <aside className={styles.sidebar}>
    <button className={styles.mobileMenu} type="button" onClick={()=>setOpen(!open)} aria-expanded={open} aria-controls="docs-navigation"><Icon name={open?"close":"menu"}/>Documentation menu<Icon/></button>
    <div id="docs-navigation" className={`${styles.navigation} ${open?styles.navigationOpen:""}`}>
      <label className={styles.search}><Icon name="search"/><input ref={search} type="search" value={query} onChange={event=>setQuery(event.target.value)} placeholder="Search docs" aria-label="Find a documentation topic"/><kbd>Ctrl K</kbd></label>
      <nav aria-label="Documentation topics">{["Get started","Payment workflows","Integrations"].map(group=>{
        const items=filtered.filter(topic=>topic.group===group);
        return items.length?<div className={styles.navGroup} key={group}><span>{group}</span>{items.map(topic=><a className={active===topic.id?styles.active:""} aria-current={active===topic.id?"location":undefined} href={`#${topic.id}`} key={topic.id} onClick={()=>setOpen(false)}>{topic.label}<Icon/></a>)}</div>:null;
      })}</nav>
      {!filtered.length&&<p className={styles.noResults} role="status">No matching topics. Try “payments” or “API”.</p>}
      <div className={styles.sidebarNote}><Icon name="code"/><strong>Built for builders.</strong><p>Start in the terminal.<br/>Make it your own.</p><a href="https://github.com/gowithflowpay/flowpay">Explore the source <Icon name="external"/></a></div>
    </div>
  </aside>;
}

export function CodeBlock({label,code,language="bash"}:{label:string;code:string;language?:string}){
  const [state,setState]=useState<"idle"|"copied"|"failed">("idle");
  const pre=useRef<HTMLPreElement>(null);
  useEffect(()=>{if(state==="idle")return;const timer=setTimeout(()=>setState("idle"),2500);return()=>clearTimeout(timer);},[state]);
  const copy=async()=>{
    try{await navigator.clipboard.writeText(code);setState("copied");}
    catch{const selection=window.getSelection();if(pre.current&&selection){const range=document.createRange();range.selectNodeContents(pre.current);selection.removeAllRanges();selection.addRange(range);}setState("failed");}
  };
  const lines=code.split("\n");
  return <div className={styles.codeBlock}>
    <div className={styles.codeHeader}><span><Icon name={language==="bash"?"terminal":"code"}/>{label}</span><div><span className={styles.language}>{language}</span><button type="button" aria-label={`Copy ${label}`} onClick={copy}><Icon name={state==="copied"?"check":"copy"}/>{state==="copied"?"Copied":state==="failed"?"Select to copy":"Copy"}</button></div></div>
    <pre ref={pre} tabIndex={0} aria-label={`${label} code`}><code>{lines.map((line,index)=><span key={index}><span className={styles.lineNumber} aria-hidden="true">{index+1}</span><span>{line||" "}</span>{index<lines.length-1?"\n":""}</span>)}</code></pre>
    <span className={styles.srOnly} role="status">{state==="copied"?`${label} copied`:state==="failed"?"Clipboard unavailable. The command is selected for copying.":""}</span>
  </div>;
}

export function TableOfContents(){
  const active=useActiveTopic();
  return <aside className={styles.toc}><span>On this page</span><nav aria-label="On this page">{topics.map(topic=><a key={topic.id} href={`#${topic.id}`} className={active===topic.id?styles.tocActive:undefined}>{topic.label}</a>)}</nav><div><Icon name="layers"/><strong>Your next payment<br/>starts here.</strong><p>Create an account and connect your settlement wallet.</p><a href="/signup">Get started <Icon/></a></div></aside>;
}
