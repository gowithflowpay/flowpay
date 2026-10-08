import {api} from "../../lib/api";
import {requireMerchant} from "../../lib/session";
import {WebhookSettings} from "./WebhookSettings";
export default async function Settings(){
 const merchant=await requireMerchant();let endpoints:any[]=[];let unavailable=false;
 try{endpoints=(await api("/v1/webhooks",{signal:AbortSignal.timeout(10000)}))?.data??[];}catch{unavailable=true;}
 return <div className="workspace-page"><header className="workspace-heading"><div><span className="workspace-eyebrow">YOUR ACCOUNT</span><h1>Settings</h1><p>Manage your business details, settlement wallet and payment notifications.</p></div></header><section className="workspace-card settings-account"><header className="workspace-section-heading"><div><h2>Business account</h2><p>The account and wallet connected to your payment workspace.</p></div></header><dl><div><dt>Business name</dt><dd>{merchant.business_name}</dd></div><div><dt>Email address</dt><dd>{merchant.email||"Not configured"}</dd></div><div><dt>Settlement wallet</dt><dd className="settings-wallet">{merchant.settlement_address||"No settlement wallet configured"}</dd></div></dl></section><WebhookSettings initialEndpoints={endpoints} unavailable={unavailable}/></div>;
}
