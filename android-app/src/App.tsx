import { useCallback, useEffect, useState } from "react";
import { api, MobileStatus } from "./api";
import { dispatchBack, registerBackHandler } from "./back";
import SetupPage from "./pages/SetupPage";
import UnlockPage from "./pages/UnlockPage";
import MainPage from "./pages/MainPage";

type Phase = "loading" | "setup" | "unlock" | "main";

export default function App() {
  const [phase, setPhase] = useState<Phase>("loading");
  const [status, setStatus] = useState<MobileStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [toast, setToast] = useState("");

  const showToast = useCallback((t: string) => {
    setToast(t);
    window.setTimeout(() => setToast(""), 2200);
  }, []);

  const refresh = useCallback(async () => {
    try {
      console.log("[LV] mobile_status invoke...");
      const s = await api.mobileStatus();
      console.log("[LV] mobile_status ok:", JSON.stringify(s));
      setStatus(s);
      setError("");
      setPhase(s.hasMaster ? (s.unlocked ? "main" : "unlock") : "setup");
    } catch (e) {
      console.error("[LV] mobile_status error:", e);
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    console.log("[LV] App mounted, calling refresh");
    refresh();
  }, [refresh]);

  // Android 返回键桥：MainActivity 的 onBackPressed 会调 window.__lvHandleBack
  useEffect(() => {
    (window as unknown as { __lvHandleBack?: () => boolean }).__lvHandleBack = () => dispatchBack();
    return () => {
      delete (window as unknown as { __lvHandleBack?: () => boolean }).__lvHandleBack;
    };
  }, []);

  // 兜底返回处理（注册最早 → 最后处理）：列表/设置等无内层状态时，两次返回退出
  useEffect(() => {
    let lastBack = 0;
    return registerBackHandler(() => {
      const now = Date.now();
      if (now - lastBack < 2000) {
        lastBack = 0;
        void api.mobileExit();
        return true;
      }
      lastBack = now;
      showToast("再按一次返回键退出");
      return true;
    });
  }, [showToast]);

  // 自动锁定：到设定时间自动锁定，期间有点击/触摸等操作则重置计时；"永不"（未设置）不生效
  useEffect(() => {
    if (phase !== "main") return;
    const raw = localStorage.getItem("lv_mobile_auto_lock");
    const mins = raw ? parseInt(raw, 10) : 0;
    if (!mins || mins <= 0) return;
    let timer: number | undefined;
    const reset = () => {
      if (timer) window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        void api.mobileLockAll().catch(() => {});
        refresh();
      }, mins * 60 * 1000);
    };
    const evts = ["pointerdown", "touchstart", "keydown", "click"];
    evts.forEach((e) => window.addEventListener(e, reset, { passive: true }));
    reset();
    return () => {
      evts.forEach((e) => window.removeEventListener(e, reset));
      if (timer) window.clearTimeout(timer);
    };
  }, [phase, refresh]);

  // 自动同步：开启后每 30s 双向同步——把手机本地改动补推到桌面端（电脑锁定/离线时自动重试），
  // 同时拉取桌面端最新数据到手机（电脑端修改会在此自动同步到手机）；拉取成功后广播事件让密码库刷新
  useEffect(() => {
    if (phase !== "main" || !status?.paired) return;
    if (localStorage.getItem("lv_mobile_auto_sync") !== "1") return;
    const t = setInterval(async () => {
      try {
        await api.syncPush();
      } catch { /* 电脑未解锁/离线，静默等下次重试 */ }
      try {
        await api.syncPull();
        window.dispatchEvent(new CustomEvent("lv-pulled"));
      } catch { /* 拉取失败静默 */ }
    }, 30000);
    return () => clearInterval(t);
  }, [phase, status?.paired]);

  if (phase === "loading") {
    return (
      <div className="center-screen">
        <div className="spinner" />
        <p>正在载入 LocalVault…</p>
        {error && (
          <p style={{ color: "#dc2626", marginTop: 12, fontSize: 13, wordBreak: "break-all", padding: "0 24px", textAlign: "center" }}>
            初始化失败：{error}
          </p>
        )}
      </div>
    );
  }

  if (phase === "setup") {
    return (
      <SetupPage
        busy={busy}
        setBusy={setBusy}
        error={error}
        setError={setError}
        onDone={() => refresh()}
      />
    );
  }

  if (phase === "unlock") {
    return (
      <UnlockPage
        status={status!}
        busy={busy}
        setBusy={setBusy}
        error={error}
        setError={setError}
        onDone={() => refresh()}
      />
    );
  }

  return (
    <>
      <MainPage
        status={status!}
        refresh={refresh}
        onLock={() => refresh()}
      />
      {toast && <div className="toast">{toast}</div>}
    </>
  );
}
