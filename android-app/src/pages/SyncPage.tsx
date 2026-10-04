import { useCallback, useEffect, useState } from "react";
import { api, HealthInfo, MobileStatus } from "../api";
import Scanner from "../components/Scanner";

interface Props {
  status: MobileStatus;
  refresh: () => Promise<void>;
  /** 立即锁定（右上角 🔒 按钮） */
  onLock?: () => void;
}

export default function SyncPage({ status, refresh, onLock }: Props) {
  const [showScanner, setShowScanner] = useState(false);
  const [health, setHealth] = useState<HealthInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [manualPayload, setManualPayload] = useState("");
  const [autoSync, setAutoSync] = useState(() => localStorage.getItem("lv_mobile_auto_sync") === "1");

  const checkHealth = useCallback(async () => {
    if (!status.pairing) return;
    try {
      const h = await api.syncHealth(status.pairing.ip, status.pairing.port);
      setHealth(h);
    } catch {
      setHealth(null);
    }
  }, [status.pairing]);

  useEffect(() => {
    checkHealth();
    const t = setInterval(checkHealth, 5000);
    return () => clearInterval(t);
  }, [checkHealth]);

  const handleScan = async (payload: string) => {
    setShowScanner(false);
    setMsg(null);
    setBusy(true);
    try {
      const p = await api.syncConnect(payload);
      setMsg({ ok: true, text: `配对成功！已连接 ${p.ip}:${p.port}` });
      await refresh();
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const handleManual = async () => {
    const v = manualPayload.trim();
    if (!v) {
      setMsg({ ok: false, text: "请粘贴或输入二维码内容" });
      return;
    }
    await handleScan(v);
  };

  const handlePull = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const n = await api.syncPull();
      setMsg({ ok: true, text: `拉取成功，当前共 ${n} 条密码` });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const handleDisconnect = async () => {
    setBusy(true);
    setMsg(null);
    try {
      await api.syncDisconnect();
      setMsg({ ok: true, text: "已断开配对" });
      await refresh();
      setHealth(null);
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const handlePush = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const n = await api.syncPush();
      setMsg({ ok: true, text: `已推送 ${n} 条到桌面端（桌面端「手机同步」可查看同步日志）` });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const toggleAutoSync = (on: boolean) => {
    localStorage.setItem("lv_mobile_auto_sync", on ? "1" : "");
    setAutoSync(on);
    setMsg({ ok: true, text: on ? "已开启：手机端新增/编辑/删除会自动同步到电脑" : "已关闭自动同步" });
  };

  return (
    <div className="page">
      <header className="page-header">
        <div className="header-actions">
          <button className="mini-btn lock-btn" onClick={async () => { try { await api.mobileLockAll(); onLock?.(); } catch { /* 忽略 */ } }}>🔒 锁定</button>
        </div>
      </header>

      <div className="card">
        <div className="row-between">
          <span>配对状态</span>
          <span className={status.paired ? "badge-ok" : "badge-off"}>
            {status.paired ? "已配对" : "未配对"}
          </span>
        </div>
        {status.pairing && (
          <>
            <div className="row-between">
              <span>桌面端地址</span>
              <span className="mono">{status.pairing.ip}:{status.pairing.port}</span>
            </div>
            <div className="row-between">
              <span>桌面端服务</span>
              <span className={health?.running ? "badge-ok" : "badge-off"}>
                {health?.running ? "运行中" : "无法连接"}
              </span>
            </div>
            <div className="row-between">
              <span>本地密码条目</span>
              <span>{status.entryCount} 条</span>
            </div>
          </>
        )}
      </div>

      {msg && (
        <div className={msg.ok ? "info-box ok" : "info-box err"}>{msg.text}</div>
      )}

      {!status.paired ? (
        <div className="card">
          <h3>首次配对</h3>
          <p className="muted">
            在桌面端「手机同步」弹窗中生成配对码，然后用手机扫描屏幕上的二维码。
          </p>
          <button className="btn-primary" onClick={() => setShowScanner(true)} disabled={busy}>
            📷 扫描桌面端二维码
          </button>

          <div className="divider">或手动输入二维码内容</div>
          <input
            className="text-input mono"
            value={manualPayload}
            onChange={(e) => setManualPayload(e.target.value)}
            placeholder="localvault://sync?ip=..&port=..&code=..&key=.."
          />
          <button className="btn-secondary" onClick={handleManual} disabled={busy}>
            手动配对
          </button>
        </div>
      ) : (
        <div className="card">
          <h3>已配对设备</h3>
          <p className="muted">
            配对后，只要桌面端 LocalVault 处于解锁状态，即可随时同步。
          </p>
          <div className="row-between auto-sync-row">
            <div>
              <b>自动同步到电脑</b>
              <p className="muted">勾选后，手机端新建 / 编辑 / 删除条目会自动推送到电脑</p>
            </div>
            <label className="switch">
              <input type="checkbox" checked={autoSync} onChange={(e) => toggleAutoSync(e.target.checked)} />
              <span className="slider" />
            </label>
          </div>
          <button className="btn-primary" onClick={handlePull} disabled={busy}>
            {busy ? "同步中…" : "从电脑拉取"}
          </button>
          <button className="btn-secondary" onClick={handlePush} disabled={busy}>
            推送本地改动到电脑
          </button>
          <button className="btn-danger" onClick={handleDisconnect} disabled={busy}>
            断开配对
          </button>
        </div>
      )}

      {showScanner && (
        <Scanner onScan={handleScan} onClose={() => setShowScanner(false)} />
      )}
    </div>
  );
}
