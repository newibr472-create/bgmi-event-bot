use anyhow::Result;
use std::sync::Arc;
use tracing::info;

use crate::AppState;
use super::api_routes;

pub async fn start_server(state: Arc<AppState>) -> Result<()> {
    let app = api_routes().with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("dashboard available at http://localhost:3000");

    axum::serve(listener, app).await?;
    Ok(())
}

pub const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>BGMI Event Bot</title>
<style>
:root {
    --bg-primary: #0c0c14;
    --bg-secondary: #13131f;
    --bg-tertiary: #1a1a2b;
    --border: #252540;
    --text-primary: #e8e8ee;
    --text-secondary: #8888a0;
    --accent: #3b82f6;
    --accent-hover: #2563eb;
    --success: #22c55e;
    --danger: #ef4444;
    --warning: #f59e0b;
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body {
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    background: var(--bg-primary);
    color: var(--text-primary);
    line-height: 1.5;
    min-height: 100vh;
}
.topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 28px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
}
.topbar h1 {
    font-size: 16px;
    font-weight: 600;
    letter-spacing: -0.3px;
}
.topbar .indicator {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text-secondary);
}
.indicator .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--danger);
}
.indicator .dot.active { background: var(--success); }
.wrap {
    max-width: 960px;
    margin: 0 auto;
    padding: 28px 24px;
    display: grid;
    gap: 20px;
}
.card {
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 22px;
}
.card-title {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.8px;
    color: var(--text-secondary);
    margin-bottom: 14px;
}
.controls {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
}
.btn {
    padding: 9px 18px;
    border: none;
    border-radius: 7px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s, transform 0.1s;
}
.btn:active { transform: scale(0.97); }
.btn-primary { background: var(--accent); color: #fff; }
.btn-primary:hover { background: var(--accent-hover); }
.btn-success { background: #166534; color: #bbf7d0; }
.btn-success:hover { background: #15803d; }
.btn-danger { background: #7f1d1d; color: #fecaca; }
.btn-danger:hover { background: #991b1b; }
.input-row {
    display: flex;
    gap: 10px;
    margin-bottom: 14px;
}
.input-row input {
    flex: 1;
    padding: 10px 14px;
    background: var(--bg-primary);
    border: 1px solid var(--border);
    border-radius: 7px;
    color: var(--text-primary);
    font-family: 'JetBrains Mono', monospace;
    font-size: 13px;
    outline: none;
    transition: border-color 0.15s;
}
.input-row input:focus { border-color: var(--accent); }
.input-row input::placeholder { color: #555570; }
.account-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
}
.account-table th {
    text-align: left;
    padding: 8px 12px;
    color: var(--text-secondary);
    font-weight: 500;
    border-bottom: 1px solid var(--border);
}
.account-table td {
    padding: 10px 12px;
    border-bottom: 1px solid #1a1a2e;
}
.account-table tr:last-child td { border-bottom: none; }
.badge {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 600;
}
.badge-online { background: #14532d; color: #86efac; }
.badge-offline { background: #451a1a; color: #fca5a5; }
.event-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 12px;
}
.event-item {
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 14px;
}
.event-item h4 {
    font-size: 13px;
    font-weight: 500;
    margin-bottom: 4px;
}
.event-item .meta {
    font-size: 11px;
    color: var(--text-secondary);
    margin-bottom: 8px;
}
.progress-track {
    height: 4px;
    background: #252540;
    border-radius: 2px;
    overflow: hidden;
}
.progress-track .fill {
    height: 100%;
    background: var(--accent);
    border-radius: 2px;
    transition: width 0.3s;
}
.log-box {
    background: var(--bg-primary);
    border: 1px solid var(--border);
    border-radius: 7px;
    padding: 14px;
    max-height: 240px;
    overflow-y: auto;
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px;
    color: var(--text-secondary);
    line-height: 1.7;
}
.log-box .entry { padding: 1px 0; }
.log-box .ts { color: #555570; margin-right: 8px; }
.empty-state {
    color: var(--text-secondary);
    font-size: 13px;
    padding: 16px 0;
    text-align: center;
}
</style>
</head>
<body>
<div class="topbar">
    <h1>BGMI Event Bot</h1>
    <div class="indicator">
        <div class="dot" id="status-dot"></div>
        <span id="status-text">Stopped</span>
    </div>
</div>
<div class="wrap">
    <div class="card">
        <div class="card-title">Bot Control</div>
        <div class="controls">
            <button class="btn btn-success" onclick="startBot()">Start Bot</button>
            <button class="btn btn-danger" onclick="stopBot()">Stop Bot</button>
        </div>
    </div>

    <div class="card">
        <div class="card-title">Accounts</div>
        <div class="input-row">
            <input type="text" id="token-input" placeholder="Paste BGMI auth token (base64)..." />
            <button class="btn btn-primary" onclick="addAccount()">Import</button>
        </div>
        <div id="accounts-container">
            <div class="empty-state">No accounts imported yet.</div>
        </div>
    </div>

    <div class="card">
        <div class="card-title">Active Events</div>
        <div class="event-grid" id="events-container">
            <div class="empty-state" style="grid-column:1/-1;">Start bot to load events.</div>
        </div>
    </div>

    <div class="card">
        <div class="card-title">Activity Log</div>
        <div class="log-box" id="log-box">
            <div class="entry"><span class="ts">--:--:--</span>Dashboard loaded. Waiting for activity.</div>
        </div>
    </div>
</div>

<script>
const API = '';

async function api(method, path, body) {
    const opts = { method, headers: {'Content-Type': 'application/json'} };
    if (body) opts.body = JSON.stringify(body);
    const res = await fetch(API + path, opts);
    return res.json();
}

async function addAccount() {
    const input = document.getElementById('token-input');
    const token = input.value.trim();
    if (!token) return;
    const data = await api('POST', '/api/accounts', { token });
    input.value = '';
    if (data.error) { alert(data.error); return; }
    refreshAccounts();
    refreshLogs();
}

async function startBot() {
    await api('POST', '/api/start');
    refreshStatus();
    refreshLogs();
}

async function stopBot() {
    await api('POST', '/api/stop');
    refreshStatus();
    refreshLogs();
}

async function refreshStatus() {
    const data = await api('GET', '/api/status');
    const dot = document.getElementById('status-dot');
    const txt = document.getElementById('status-text');
    if (data.running) {
        dot.classList.add('active');
        txt.textContent = 'Running';
    } else {
        dot.classList.remove('active');
        txt.textContent = 'Stopped';
    }
}

async function refreshAccounts() {
    const data = await api('GET', '/api/accounts');
    const el = document.getElementById('accounts-container');
    if (!data.accounts || data.accounts.length === 0) {
        el.innerHTML = '<div class="empty-state">No accounts imported yet.</div>';
        return;
    }
    let html = '<table class="account-table"><thead><tr><th>Name</th><th>Open ID</th><th>Status</th></tr></thead><tbody>';
    for (const a of data.accounts) {
        const badge = a.session_active
            ? '<span class="badge badge-online">Online</span>'
            : '<span class="badge badge-offline">Offline</span>';
        html += `<tr><td>${esc(a.display_name)}</td><td>${esc(a.open_id)}</td><td>${badge}</td></tr>`;
    }
    html += '</tbody></table>';
    el.innerHTML = html;
}

async function refreshEvents() {
    const data = await api('GET', '/api/events');
    const el = document.getElementById('events-container');
    if (!data.events || data.events.length === 0) {
        el.innerHTML = '<div class="empty-state" style="grid-column:1/-1;">No claimable events.</div>';
        return;
    }
    el.innerHTML = data.events.map(e => `
        <div class="event-item">
            <h4>${esc(e.name)}</h4>
            <div class="meta">${esc(JSON.stringify(e.event_type))}</div>
            <div class="progress-track"><div class="fill" style="width:${Math.round(e.progress*100)}%"></div></div>
        </div>
    `).join('');
}

async function refreshLogs() {
    const data = await api('GET', '/api/logs');
    const el = document.getElementById('log-box');
    if (!data.logs || data.logs.length === 0) return;
    el.innerHTML = data.logs.map(l => {
        const t = new Date(l.timestamp).toLocaleTimeString();
        return `<div class="entry"><span class="ts">${t}</span>${esc(l.message)}</div>`;
    }).join('');
    el.scrollTop = el.scrollHeight;
}

function esc(s) {
    if (!s) return '';
    return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}

// initial load + poll
refreshStatus();
refreshAccounts();
refreshLogs();
setInterval(() => { refreshStatus(); refreshLogs(); }, 5000);
setInterval(refreshEvents, 15000);
</script>
</body>
</html>
"##;
