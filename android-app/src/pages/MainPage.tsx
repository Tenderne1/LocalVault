import { useState } from "react";
import { MobileStatus } from "../api";
import VaultPage from "./VaultPage";
import SyncPage from "./SyncPage";
import SettingsPage from "./SettingsPage";

interface Props {
  status: MobileStatus;
  refresh: () => Promise<void>;
  onLock: () => void;
}

type Tab = "vault" | "sync" | "settings";

export default function MainPage({ status, refresh, onLock }: Props) {
  const [tab, setTab] = useState<Tab>("vault");

  return (
    <div className="app-shell">
      <div className="tab-content">
        {tab === "vault" && <VaultPage onLock={onLock} />}
        {tab === "sync" && <SyncPage status={status} refresh={refresh} onLock={onLock} />}
        {tab === "settings" && (
          <SettingsPage status={status} refresh={refresh} onLock={onLock} />
        )}
      </div>

      <nav className="bottom-tabs">
        <button className={tab === "vault" ? "tab active" : "tab"} onClick={() => setTab("vault")}>
          <span className="tab-icon">🗂️</span>
          <span>密码库</span>
        </button>
        <button className={tab === "sync" ? "tab active" : "tab"} onClick={() => setTab("sync")}>
          <span className="tab-icon">🔄</span>
          <span>同步</span>
        </button>
        <button className={tab === "settings" ? "tab active" : "tab"} onClick={() => setTab("settings")}>
          <span className="tab-icon">⚙️</span>
          <span>设置</span>
        </button>
      </nav>
    </div>
  );
}
