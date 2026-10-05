import {api} from "../../lib/api";
import {ApiKeysPanel} from "./ApiKeysPanel";
import styles from "./IntegrationAccess.module.scss";
import {ConnectionsPanel} from "./ConnectionsPanel";

export default async function ApiKeysPage(){
  const result=await api("/v1/api-keys");
  const connections=await api("/v1/oauth/connections").catch(()=>({data:[]}));
  return <>
    <section className={styles.access}>
      <div><span className={styles.eyebrow}>Developer access</span><h2>Connect your terminal and agents</h2><p>Sign in with your email or approve a CLI device. Your local MCP integration can use the same account to connect your site.</p><div className={styles.actions}><a className="btn primary" href="/dashboard/devices">Connect a device</a><a className="btn secondary" href="https://github.com/gowithflowpay/flowpay/blob/main/flowpay/docs/cli.md" target="_blank" rel="noreferrer">CLI guide</a></div></div>
      <div className={styles.steps}><span>Sign in with an email code</span><code>flowpay login --email you@company.com</code><span>Or approve this terminal in your dashboard</span><code>flowpay init</code></div>
    </section>
    <ConnectionsPanel initial={connections.data??[]}/>
    <ApiKeysPanel initialKeys={result.data??[]}/>
  </>;
}
