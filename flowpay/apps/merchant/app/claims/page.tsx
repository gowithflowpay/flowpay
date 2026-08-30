import {api} from "../../lib/api";
import {statusTone,short} from "../../lib/format";
import {ClaimIcon} from "../components/Icons";
export default async function Claims(){
  try{
  const r=await api("/v1/claims?limit=100");
  const rows=r?.data??[];
  const open=rows.filter((c:any)=>!["RECOVERED","REJECTED","NOT_RECOVERABLE"].includes(c?.status)).length;
  return <><div className="page-head"><div><div className="eyebrow">Claims</div><h1>Payment exceptions</h1><p>Agent-assisted investigations begin only when deterministic payment processing cannot safely resolve a case.</p></div><div className="metric-icon"><ClaimIcon/></div></div>{open>0?<div className="claim-alert"><div className="alert-icon"><ClaimIcon/></div><div><strong>{open} active investigation{open===1?"":"s"}</strong><span>Review recoverable cases, model-selected tool traces and approval checkpoints.</span></div></div>:null}<section className="panel"><div className="panel-head"><h2>Claims</h2><span className="code-badge">{rows.length} total</span></div><div className="table-wrap"><table className="table"><thead><tr><th>Claim</th><th>Payment</th><th>Actual transaction</th><th>Destination</th><th>Status</th></tr></thead><tbody>{rows.map((c:any)=>{const status=c?.status??"UNKNOWN";return <tr key={c?.id}><td><a className="id-link" href={`/claims/${c?.id}`}>{c?.id??"—"}</a></td><td className="id-link">{c?.payment_id??"—"}</td><td>{c?.actual_asset??"?"} on {c?.actual_chain??"unknown"}</td><td className="mono">{short(c?.recovery_destination)}</td><td><span className={`status ${statusTone(status)}`}>{status.replaceAll("_"," ")}</span></td></tr>;})}</tbody></table>{rows.length===0?<div className="empty-note">No recovery claims have been created.</div>:null}</div></section></>;
  }catch(error){return <><div className="page-head"><div><div className="eyebrow">Claims</div><h1>Claims</h1><p>{error instanceof Error?error.message:"Failed to load claims."}</p></div></div></>;}
}