pub mod web;

/// Embedded HTML/JS for the minimal webview interface.
/// Kept as a const to avoid external file dependencies.
pub const APP_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>BGMI Event Bot</title>
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    background: #0a0a0f;
    color: #e0e0e0;
    min-height: 100vh;
}
.header {
    background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
    padding: 16px 24px;
    border-bottom: 1px solid #2a2a4a;
    display: flex;
    align-items: center;
    justify-content: space-between;
}
.header h1 { font-size: 18px; color: #fff; }
.header .status {
    font-size: 12px;
    padding: 4px 12px;
    border-radius: 12px;
    background: #1b5e20;
    color: #a5d6a7;
}
.container { padding: 24px; max-width: 900px; margin: 0 auto; }
.section {
    background: #12121a;
    border: 1px solid #1e1e3a;
    border-radius: 8px;
    padding: 20px;
    margin-bottom: 16px;
}
.section h2 {
    font-size: 14px;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: #888;
    margin-bottom: 12px;
}
.account-list { list-style: none; }
.account-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px;
    border-bottom: 1px solid #1e1e3a;
}
.account-item:last-child { border-bottom: none; }
.account-name { font-weight: 500; }
.account-status {
    font-size: 12px;
    padding: 2px 8px;
    border-radius: 4px;
}
.account-status.active { background: #1b5e20; color: #a5d6a7; }
.account-status.inactive { background: #4a1010; color: #ef9a9a; }
.btn {
    padding: 8px 16px;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
    font-weight: 500;
    transition: all 0.2s;
}
.btn-primary { background: #1565c0; color: white; }
.btn-primary:hover { background: #1976d2; }
.btn-danger { background: #b71c1c; color: white; }
.btn-danger:hover { background: #c62828; }
.btn-sm { padding: 4px 10px; font-size: 12px; }
input[type="text"], textarea {
    width: 100%;
    padding: 10px 12px;
    background: #0a0a12;
    border: 1px solid #2a2a4a;
    border-radius: 6px;
    color: #e0e0e0;
    font-family: monospace;
    font-size: 13px;
    margin-bottom: 8px;
}
.event-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 12px; }
.event-card {
    background: #1a1a28;
    border: 1px solid #2a2a4a;
    border-radius: 8px;
    padding: 14px;
}
.event-card h3 { font-size: 13px; margin-bottom: 6px; }
.event-card .reward { color: #ffd54f; font-size: 12px; }
.event-card .progress-bar {
    height: 4px;
    background: #2a2a4a;
    border-radius: 2px;
    margin-top: 8px;
    overflow: hidden;
}
.event-card .progress-bar .fill {
    height: 100%;
    background: #4caf50;
    transition: width 0.3s;
}
.log-output {
    background: #050508;
    border: 1px solid #1e1e3a;
    border-radius: 6px;
    padding: 12px;
    font-family: monospace;
    font-size: 12px;
    max-height: 200px;
    overflow-y: auto;
    color: #78909c;
}
.actions { display: flex; gap: 8px; margin-top: 12px; }
</style>
</head>
<body>
<div class="header">
    <h1>BGMI Event Bot</h1>
    <span class="status" id="global-status">Idle</span>
</div>
<div class="container">
    <div class="section">
        <h2>Accounts</h2>
        <ul class="account-list" id="account-list">
            <li style="color:#666; padding:12px;">No accounts added. Paste auth token below.</li>
        </ul>
        <div style="margin-top:12px;">
            <input type="text" id="token-input" placeholder="Paste auth token (base64)..." />
            <div class="actions">
                <button class="btn btn-primary" onclick="addAccount()">Import Account</button>
            </div>
        </div>
    </div>

    <div class="section">
        <h2>Active Events</h2>
        <div class="event-grid" id="event-grid">
            <div style="color:#666; grid-column: 1/-1;">Connect an account to view events.</div>
        </div>
        <div class="actions">
            <button class="btn btn-primary" onclick="refreshEvents()">Refresh Events</button>
            <button class="btn btn-primary" onclick="claimAll()">Claim All Available</button>
        </div>
    </div>

    <div class="section">
        <h2>Match Simulator</h2>
        <p style="font-size:13px; color:#888; margin-bottom:12px;">
            Run idle matches to accumulate playtime for time-based rewards.
        </p>
        <div class="actions">
            <button class="btn btn-primary" onclick="startMatch()">Start Match</button>
            <button class="btn btn-danger" onclick="stopMatch()">Stop</button>
        </div>
    </div>

    <div class="section">
        <h2>Log</h2>
        <div class="log-output" id="log-output"></div>
    </div>
</div>

<script>
function invoke(cmd, args) {
    window.ipc.postMessage(JSON.stringify({ cmd, args: args || {} }));
}

function addAccount() {
    const token = document.getElementById('token-input').value.trim();
    if (!token) return;
    invoke('add_account', { token });
    document.getElementById('token-input').value = '';
}

function refreshEvents() { invoke('refresh_events'); }
function claimAll() { invoke('claim_all'); }
function startMatch() { invoke('start_match'); }
function stopMatch() { invoke('stop_match'); }

function appendLog(msg) {
    const el = document.getElementById('log-output');
    const ts = new Date().toLocaleTimeString();
    el.innerHTML += `<div>[${ts}] ${msg}</div>`;
    el.scrollTop = el.scrollHeight;
}

function updateAccounts(accounts) {
    const list = document.getElementById('account-list');
    if (!accounts.length) {
        list.innerHTML = '<li style="color:#666;padding:12px;">No accounts added.</li>';
        return;
    }
    list.innerHTML = accounts.map(a => `
        <li class="account-item">
            <span class="account-name">${a.display_name}</span>
            <span class="account-status ${a.session_active ? 'active' : 'inactive'}">
                ${a.session_active ? 'Online' : 'Offline'}
            </span>
            <button class="btn btn-danger btn-sm" onclick="invoke('remove_account',{id:'${a.id}'})">Remove</button>
        </li>
    `).join('');
}

function updateEvents(events) {
    const grid = document.getElementById('event-grid');
    if (!events.length) {
        grid.innerHTML = '<div style="color:#666;grid-column:1/-1;">No active events.</div>';
        return;
    }
    grid.innerHTML = events.map(e => `
        <div class="event-card">
            <h3>${e.name}</h3>
            <div class="reward">${e.rewards.map(r => r.name).join(', ')}</div>
            <div class="progress-bar"><div class="fill" style="width:${e.progress*100}%"></div></div>
        </div>
    `).join('');
}

// receive messages from Rust backend
window.addEventListener('message', (e) => {
    try {
        const msg = JSON.parse(e.data);
        if (msg.type === 'log') appendLog(msg.text);
        if (msg.type === 'accounts') updateAccounts(msg.data);
        if (msg.type === 'events') updateEvents(msg.data);
        if (msg.type === 'status') document.getElementById('global-status').textContent = msg.text;
    } catch {}
});

appendLog('UI initialized. Waiting for backend...');
</script>
</body>
</html>"#;
