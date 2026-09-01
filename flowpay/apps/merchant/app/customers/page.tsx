import {api} from "../../lib/api";

export default async function Customers(){
  const result=await api("/v1/payments?limit=100");
  const payments=result?.data??[];
  const customers=Array.from(new Set(payments.map((payment:any)=>payment?.reference).filter(Boolean))) as string[];
  return <><div className="page-head"><div><div className="eyebrow">Customers</div><h1>Customers</h1><p>Customer references collected from live payments.</p></div></div><section className="panel"><div className="panel-head"><h2>All customers</h2><span className="code-badge">{customers.length} records</span></div><div className="table-wrap"><table className="table"><thead><tr><th>Customer reference</th><th>Payments</th></tr></thead><tbody>{customers.map(customer=><tr key={customer}><td>{customer}</td><td>{payments.filter((payment:any)=>payment?.reference===customer).length}</td></tr>)}</tbody></table>{customers.length===0?<div className="empty-note">Customers appear here after payments are created.</div>:null}</div></section></>;
}
