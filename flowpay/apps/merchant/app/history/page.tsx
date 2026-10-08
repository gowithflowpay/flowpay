import {api} from "../../lib/api";
import {requireMerchant} from "../../lib/session";
import {HistoryTable} from "./HistoryTable";
import {LinkIcon} from "../components/Icons";
export default async function HistoryPage(){
 await requireMerchant();let rows:any[]=[];let unavailable=false;let cursor:string|null=null;
 try{const result=await api("/v1/payments?limit=100");rows=result?.data??[];cursor=result?.next_cursor??null;}catch{unavailable=true;}
 return <div className="workspace-page"><header className="workspace-heading"><div><span className="workspace-eyebrow">PAYMENT RECORDS</span><h1>History</h1><p>A clear record of your payments, across every asset and network.</p></div><a className="workspace-primary" href="/payments/new"><LinkIcon/>Create payment link</a></header><HistoryTable rows={rows} initialCursor={cursor} unavailable={unavailable}/></div>;
}
