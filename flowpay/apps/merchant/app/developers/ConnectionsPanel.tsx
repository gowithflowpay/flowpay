"use client";
import {useState} from "react";
type Connection={id:string;client_name:string;scopes:string[];revoked:boolean};
export function ConnectionsPanel({initial}:{initial:Connection[]}){
  const [connections,setConnections]=useState(initial);
  const [busy,setBusy]=useState<string|null>(null);
  const [error,setError]=useState("");
  async function disconnect(id:string){
    setBusy(id);setError("");
    try{
      const response=await fetch(`/api/oauth/connections/${id}`,{method:"DELETE"});
      const body=await response.json();
      if(!response.ok)throw new Error(body?.error?.message??"Unable to disconnect");
      setConnections(current=>current.map(item=>item.id===id?{...item,revoked:true}:item));
    }catch(value){setError(value instanceof Error?value.message:"Unable to disconnect");}finally{setBusy(null);}
  }
  return <section className="panel" style={{marginBottom:28,padding:24}}><h2 style={{marginTop:0}}>Connected apps</h2><p>Manage the apps authorized to use your FlowPay account.</p>{error?<p role="alert" className="form-error">{error}</p>:null}{connections.length?connections.map(connection=><div key={connection.id} style={{display:"flex",alignItems:"center",justifyContent:"space-between",gap:16,flexWrap:"wrap",padding:"16px 0",borderTop:"1px solid rgba(128,128,128,.2)"}}><div><strong>{connection.client_name}</strong><p style={{fontSize:12,margin:"4px 0"}}>{connection.scopes.map(scope=>scope==="payments:write"?"Create payments":"Read payments").join(" / ")}</p></div>{connection.revoked?<span className="status bad">Disconnected</span>:<button className="btn secondary" type="button" disabled={Boolean(busy)} onClick={()=>void disconnect(connection.id)}>{busy===connection.id?"Disconnecting...":"Disconnect"}</button>}</div>):<p>No apps connected yet. Add the FlowPay MCP URL in ChatGPT or Claude, then authorize your account.</p>}</section>;
}
