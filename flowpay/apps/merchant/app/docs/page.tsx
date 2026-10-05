import styles from "./page.module.scss";

const install="npm install -g https://api.pixuno.xyz/downloads/flowpay-cli.tgz";
export const metadata={title:"FlowPay documentation | CLI, MCP and agents"};

export default function Documentation(){
  return <div className={styles.docs}>
    <header><a href="/">FlowPay <span>/ docs</span></a><a href="/login">Sign in</a></header>
    <div className={styles.layout}>
      <aside><span>BUILD WITH FLOWPAY</span><nav aria-label="Documentation"><a href="#quickstart">Quick start</a><a href="#account">Email login</a><a href="#payments">Request payments</a><a href="#agents">Agent to agent</a><a href="#mcp">MCP connections</a><a href="#api">API and source</a></nav></aside>
      <main>
        <div className={styles.intro}><span>DEVELOPER DOCUMENTATION</span><h1>Payments from your terminal.<br/>Tools for your agents.</h1><p>Register once, sign in with an email code, and create a payment request from the CLI. Connect compatible AI clients through an authorization page.</p></div>
        <section id="quickstart"><h2>Install the CLI</h2><p>Requires Node.js 20 or later. The public download bundles the FlowPay SDK.</p><pre><code>{install+"\nflowpay --help"}</code></pre></section>
        <section id="account"><h2>Create an account. Sign in by email.</h2><pre><code>{"flowpay register\nflowpay login"}</code></pre><p>Registration asks for your business name, contact name, email and EVM settlement address. Enter the code delivered to your email to confirm your account. Login asks only for email and a one-time code.</p><p>For an existing merchant account, <code>flowpay init</code> opens device approval in your browser. Credentials stay in your local FlowPay configuration. Use <code>flowpay logout</code> to clear them.</p></section>
        <section id="payments"><h2>Request 20 USDC on Sepolia</h2><pre><code>{"flowpay request 20 usdc eth"}</code></pre><p>The CLI prints a payment ID, a wallet address and a checkout URL you can share. It monitors the payment until the backend confirms receipt. Here, <code>eth</code> means Ethereum Sepolia testnet. Send the requested test asset on the exact network shown in checkout.</p><p>Use <code>--no-wait</code> to return immediately, or resume monitoring with <code>flowpay payment wait PAYMENT_ID</code>. A detected transfer is still pending until confirmation.</p></section>
        <section id="agents"><h2>Agent to agent</h2><p>Agents can create requests, share the checkout URL or address, and read payment state using structured JSON. Assign a separate approved CLI configuration to each agent.</p><pre><code>{"flowpay request 20 usdc eth --json\nflowpay payment status PAYMENT_ID --json\nflowpay payment wait PAYMENT_ID --json"}</code></pre><p>Set <code>FLOWPAY_CONFIG_DIR</code> to isolate an agent's credentials. Keep tokens out of prompts and logs. Payment creation and monitoring do not sign or broadcast a wallet transfer; the payer submits the transfer using its wallet.</p></section>
        <section id="mcp"><h2>Connect ChatGPT or Claude</h2><p>Add the hosted MCP server URL to a client that supports remote MCP and OAuth:</p><pre><code>https://mcp.pixuno.xyz/mcp</code></pre><p>The client opens FlowPay's authorization page. Sign in by email, review the app and requested payment permissions, then approve. Authorization uses OAuth with PKCE; credentials are exchanged by the client without appearing in the conversation.</p><p>Review and disconnect approved apps in <a href="/developers">Developers</a>. Client support and connector setup vary by product and plan.</p></section>
        <section id="api"><h2>API and source</h2><p>API base URL: <code>https://api.pixuno.xyz</code>. Payment endpoints use merchant credentials or scoped OAuth access tokens. Use payment IDs to reconcile requests and signed webhooks to receive lifecycle events.</p><div className={styles.links}><a href="https://github.com/gowithflowpay/flowpay/blob/main/flowpay/docs/cli.md">CLI reference</a><a href="https://github.com/gowithflowpay/flowpay/blob/main/flowpay/apps/mcp/README.md">MCP reference</a><a href="https://github.com/gowithflowpay/flowpay">Source code</a></div></section>
      </main>
    </div>
  </div>;
}
