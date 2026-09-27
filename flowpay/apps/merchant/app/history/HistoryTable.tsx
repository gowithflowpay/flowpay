"use client";
import {useMemo,useState} from "react";
import {networkAsset,networkLabel,short,statusTone,tokenAsset} from "../../lib/format";

export function HistoryTable({rows}:{rows:any[]}){
  const [query,setQuery]=useState("");
  const [status,setStatus]=useState("ALL");
  const filtered=useMemo(()=>rows.filter(row=>{
    const text=[row.id,row.reference,row.asset,row.chain,row.status,row.merchant_name].join(" ").toLowerCase();
    return text.includes(query.toLowerCase())&&(status==="ALL"||row.status===status);
  }),[rows,query,status]);
  const total=filtered.filter(row=>["COMPLETED","CONFIRMED","RECOVERED"].includes(row.status)).reduce((sum,row)=>sum+(Number(row.amount)||0),0);
  return <section className="history-ledger">
    <div className="history-stats"><div><span>Records</span><strong>{filtered.length}</strong></div><div><span>Completed volume</span><strong>${total.toLocaleString(undefined,{maximumFractionDigits:2})}</strong></div><div><span>Crypto rails</span><strong>{new Set(filtered.map(row=>row.chain)).size}</strong></div></div>
    <div className="history-toolbar"><label><span className="sr-only">Search history</span><input value={query} onChange={event=>setQuery(event.target.value)} placeholder="Search payments"/></label><select value={status} onChange={event=>setStatus(event.target.value)} aria-label="Filter by status"><option value="ALL">All statuses</option><option value="COMPLETED">Completed</option><option value="WAITING">Waiting</option><option value="PARTIAL">Partial</option><option value="EXPIRED">Expired</option><option value="FAILED">Failed</option></select></div>
    <div className="history-table-wrap"><table><thead><tr><th>Payment</th><th>Reference</th><th>Asset</th><th>Network</th><th>Date</th><th>Status</th></tr></thead><tbody>{filtered.map(row=><tr key={row.id}><td><a href={`/payments/${row.id}`}><strong>{short(row.id,10,5)}</strong><small>{row.merchant_name??"Crypto checkout"}</small></a></td><td>{row.reference??"—"}</td><td><span className="history-asset"><img src={tokenAsset(row.asset??"USDC")} alt=""/><b>{row.amount} {row.asset}</b></span></td><td><span className="history-network"><img src={networkAsset(row.chain)} alt=""/>{networkLabel(row.chain)}</span></td><td>{formatDate(row.updated_at??row.created_at)}</td><td><span className={`status ${statusTone(row.status)}`}>{String(row.status).replaceAll("_"," ")}</span></td></tr>)}</tbody></table>{filtered.length===0?<div className="history-empty">No payments match this view.</div>:null}</div>
  </section>;
}

function formatDate(value:string){const date=new Date(value);return Number.isNaN(date.getTime())?"—":new Intl.DateTimeFormat("en-US",{month:"short",day:"numeric",year:"numeric",hour:"numeric",minute:"2-digit"}).format(date)}
