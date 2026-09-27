import {api} from "../../lib/api";
import {HistoryTable} from "./HistoryTable";

export default async function HistoryPage(){
  let rows:any[]=[];
  let error="";
  try{rows=(await api("/v1/payments?limit=250"))?.data??[];}catch(value){error=value instanceof Error?value.message:"Unable to load payment history."}
  return <div className="history-page">
    <header className="history-hero"><div><span>Transaction history</span><h1>Every crypto payment,<br/>in one clear ledger.</h1><p>Search by customer, reference, payment ID, network, asset, or status.</p></div><a href="/payments/new">Create payment</a></header>
    {error?<p className="data-notice" role="status">{error}</p>:<HistoryTable rows={rows}/>}
  </div>;
}
