import { FormEvent, useCallback, useEffect, useState } from "react";
import { api, CategoryInfo, MobileStatus } from "../api";

/** 蓝奏云手动下载页（GitHub 网络不稳定时的备选通道；访问密码见 LANZOU_PWD） */
const LANZOU_URL = "https://wwbak.lanzoub.com/b01n4i7v7i";
const LANZOU_PWD = "1xms";

interface Props {
  status: MobileStatus;
  refresh: () => Promise<void>;
  onLock: () => void;
}

export default function SettingsPage({ status, refresh, onLock }: Props) {
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [cats, setCats] = useState<CategoryInfo[]>([]);
  const [newCat, setNewCat] = useState("");
  const [editingCat, setEditingCat] = useState<string | null>(null);
  const [renameVal, setRenameVal] = useState("");
  const [catsOpen, setCatsOpen] = useState<boolean>(() => localStorage.getItem("lv_settings_cats_open") === "1");
  const toggleCats = () => {
    setCatsOpen((prev) => {
      const next = !prev;
      localStorage.setItem("lv_settings_cats_open", next ? "1" : "0");
      return next;
    });
  };

  // ---- 密保及密码修改（参照电脑端：先验证当前密码，再勾选要修改的项目）----
  const [secOpen, setSecOpen] = useState(false);
  const [secVerified, setSecVerified] = useState(false);
  const [secCurrent, setSecCurrent] = useState("");
  const [changeMaster, setChangeMaster] = useState(false);
  const [newPw, setNewPw] = useState("");
  const [newPw2, setNewPw2] = useState("");
  const [changeRecovery, setChangeRecovery] = useState(false);
  const [secQuestions, setSecQuestions] = useState(["", "", ""]);
  const [secAnswers, setSecAnswers] = useState(["", "", ""]);
  const [recoveryCode, setRecoveryCode] = useState<string | null>(null);

  // ---- 指纹解锁开关 ----
  const [bioOn, setBioOn] = useState<boolean>(() => localStorage.getItem("lv_mobile_bio") === "1");
  const toggleBio = (on: boolean) => {
    setBioOn(on);
    localStorage.setItem("lv_mobile_bio", on ? "1" : "0");
  };
  const [bioTestMsg, setBioTestMsg] = useState("");
  const [bioTesting, setBioTesting] = useState(false);
  const testBio = async () => {
    setBioTestMsg("");
    setBioTesting(true);
    try {
      await api.mobileTestBiometric();
      setBioTestMsg("✅ 指纹验证成功！指纹功能正常。");
    } catch (e) {
      setBioTestMsg("指纹验证未通过或已取消：" + String(e).slice(0, 60));
    } finally {
      setBioTesting(false);
    }
  };

  // ---- 备份与恢复 ----
  const [backupText, setBackupText] = useState("");
  const [importPw, setImportPw] = useState("");
  const [backupBusy, setBackupBusy] = useState(false);

  // ---- 折叠面板（指纹/备份/更新/锁定默认收起） ----
  const [fold, setFold] = useState<Record<string, boolean>>({});
  const toggleFold = (k: string) => setFold((f) => ({ ...f, [k]: !f[k] }));

  // ---- 自动锁定时间（"" = 永不） ----
  const [autoLock, setAutoLock] = useState(() => localStorage.getItem("lv_mobile_auto_lock") || "");
  const changeAutoLock = (v: string) => {
    setAutoLock(v);
    localStorage.setItem("lv_mobile_auto_lock", v);
    setMsg({ ok: true, text: v ? `已设置：无操作 ${v} 分钟后自动锁定` : "已关闭自动锁定（永不锁定）" });
  };

  const loadCats = useCallback(async () => {
    try {
      setCats(await api.mobileCategoryList());
    } catch { /* 忽略 */ }
  }, []);

  useEffect(() => {
    loadCats();
  }, [loadCats]);

  const autoSync = async (): Promise<boolean> => {
    if (localStorage.getItem("lv_mobile_auto_sync") !== "1") return true;
    try {
      await api.syncPush();
      return true;
    } catch {
      return false;
    }
  };

  const createCat = async () => {
    const name = newCat.trim();
    if (!name) return;
    try {
      setCats(await api.mobileCategoryCreate(name, "📁"));
      const pushed = await autoSync();
      setNewCat("");
      setMsg({ ok: true, text: pushed ? "分类已创建，已推送到电脑" : "分类已创建；同步到电脑失败，稍后自动重试" });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  const renameCat = async (oldName: string) => {
    const n = renameVal.trim();
    if (!n) return;
    try {
      setCats(await api.mobileCategoryRename(oldName, n, "📁"));
      const pushed = await autoSync();
      setEditingCat(null);
      setRenameVal("");
      setMsg({ ok: true, text: pushed ? "分类已重命名，已推送到电脑" : "分类已重命名；同步到电脑失败，稍后自动重试" });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  const deleteCat = async (name: string) => {
    if (!window.confirm(`删除分类「${name}」？该分类下的条目将回到「默认」分类。`)) return;
    try {
      setCats(await api.mobileCategoryDelete(name));
      const pushed = await autoSync();
      setMsg({ ok: true, text: pushed ? "分类已删除，已推送到电脑" : "分类已删除；同步到电脑失败，稍后自动重试" });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  // ---- 密保及密码修改流程 ----
  const verifyCurrent = async (e: FormEvent) => {
    e.preventDefault();
    setMsg(null);
    setBusy(true);
    try {
      await api.mobileUnlock(secCurrent);
      setSecVerified(true);
      setSecCurrent("");
      setMsg({ ok: true, text: "当前主密码验证通过，请选择要修改的项目" });
    } catch (err) {
      setMsg({ ok: false, text: "当前主密码不正确：" + String(err) });
    } finally {
      setBusy(false);
    }
  };

  const saveSecurity = async (e: FormEvent) => {
    e.preventDefault();
    setMsg(null);
    setBusy(true);
    try {
      if (changeMaster && (!newPw || !newPw2)) {
        throw new Error("请填写新主密码与确认密码");
      }
      if (changeRecovery && secAnswers.some((a) => !a.trim())) {
        throw new Error("每个密保答案不能为空");
      }
      const code = await api.mobileUpdateSecurity({
        currentPassword: secCurrent,
        newPassword: changeMaster ? newPw : undefined,
        newConfirm: changeMaster ? newPw2 : undefined,
        questions: changeRecovery ? secQuestions : undefined,
        answers: changeRecovery ? secAnswers : undefined,
      });
      if (code) setRecoveryCode(code);
      setSecVerified(false);
      setChangeMaster(false);
      setChangeRecovery(false);
      setNewPw(""); setNewPw2("");
      setSecQuestions(["", "", ""]); setSecAnswers(["", "", ""]);
      setMsg({ ok: true, text: code ? "安全设置已更新；请务必保存新的 Recovery Code" : "安全设置已更新" });
      await refresh();
    } catch (err) {
      setMsg({ ok: false, text: String(err) });
    } finally {
      setBusy(false);
    }
  };

  // ---- 备份导出/导入 ----
  const exportBackup = async () => {
    setBackupBusy(true);
    setMsg(null);
    try {
      const text = await api.mobileExportBackup();
      await api.mobileShareText(text, "LocalVault 加密备份 v" + new Date().toISOString().slice(0, 10));
      setMsg({ ok: true, text: "备份已生成，请在分享面板中选择保存位置（文件/微信/云盘均可）。导入时需输入本备份对应的主密码。" });
    } catch (e) {
      setMsg({ ok: false, text: "导出失败：" + String(e) });
    } finally {
      setBackupBusy(false);
    }
  };

  const importBackup = async (e: FormEvent) => {
    e.preventDefault();
    if (!backupText.trim()) return;
    setBackupBusy(true);
    setMsg(null);
    try {
      const count = await api.mobileImportBackup(backupText.trim(), importPw);
      setBackupText("");
      setImportPw("");
      setMsg({ ok: true, text: `导入成功，当前本地共 ${count} 条条目（重复条目保留较新版本）` });
      await refresh();
    } catch (err) {
      setMsg({ ok: false, text: String(err) });
    } finally {
      setBackupBusy(false);
    }
  };

  const lockNow = async () => {
    try {
      await api.mobileLockAll();
      onLock();
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  // ---- 自动更新（与电脑版同一套 updater 机制：GitHub Releases 静态 JSON + 签名校验）----
  const [updateMsg, setUpdateMsg] = useState("");
  const [updateBusy, setUpdateBusy] = useState(false);
  const [hasUpdate, setHasUpdate] = useState(false);
  const checkUpdate = async () => {
    setUpdateMsg("");
    setUpdateBusy(true);
    setHasUpdate(false);
    try {
      const info = await api.mobileCheckUpdate();
      if (!info) {
        setUpdateMsg("✅ 已是最新版本");
      } else {
        setUpdateMsg(`发现新版本 ${info.version}（当前 ${info.currentVersion}），点击「下载并安装」即可更新`);
        setHasUpdate(true);
      }
    } catch (e) {
      setUpdateMsg("检查更新失败（GitHub 网络不稳定），可直接点击「蓝奏云下载」手动下载最新版。" + String(e).slice(0, 40));
    } finally {
      setUpdateBusy(false);
    }
  };
  const downloadAndInstall = async () => {
    setUpdateMsg("正在下载更新…（完成后将打开系统安装器）");
    setUpdateBusy(true);
    try {
      const r = await api.mobileDownloadUpdate();
      if (r === "ok") {
        setUpdateMsg("已打开系统安装器，请按提示完成安装；安装完成后建议重新打开 App。");
        setHasUpdate(false);
      } else {
        setUpdateMsg("更新失败：" + String(r).slice(0, 60));
      }
    } catch (e) {
      setUpdateMsg("更新失败：" + String(e).slice(0, 80));
    } finally {
      setUpdateBusy(false);
    }
  };
  // 蓝奏云手动下载（国内网络 GitHub 不稳定时的备选通道）
  const goLanzou = async () => {
    try {
      const r = await api.mobileOpenUrl(LANZOU_URL);
      setUpdateMsg(r === "ok" ? "已打开浏览器，前往蓝奏云下载页。访问密码：" + LANZOU_PWD : r);
    } catch (e) {
      setUpdateMsg("打开蓝奏云失败：" + String(e).slice(0, 60));
    }
  };

  return (
    <div className="page">
      <header className="page-header">
        <div className="header-actions">
          <button className="mini-btn lock-btn" onClick={() => void lockNow()}>🔒 锁定</button>
        </div>
      </header>

      {msg && (
        <div className={msg.ok ? "info-box ok" : "info-box err"}>{msg.text}</div>
      )}

      <div className="card">
        <div className="row-between">
          <span>配对状态</span>
          <span className={status.paired ? "badge-ok" : "badge-off"}>
            {status.paired ? "已配对" : "未配对"}
          </span>
        </div>
        <div className="row-between">
          <span>本地条目</span>
          <span>{status.entryCount} 条</span>
        </div>
      </div>

      <div className="card">
        <h3 className="collapse-head" onClick={toggleCats}>📂 分类管理 <span className="collapse-arrow">{catsOpen ? "▼" : "▶"}</span></h3>
        {catsOpen && (
          <>
            {cats.length === 0 ? (
              <p className="muted">暂无分类。新建分类后，编辑条目时可选。</p>
            ) : (
              <div className="cat-list">
                {cats.map((c) => (
                  <div key={c.name} className="cat-row">
                    <span className="cat-name">{c.icon} {c.name}</span>
                    {editingCat === c.name ? (
                      <span className="cat-actions">
                        <input className="text-input" value={renameVal} onChange={(e) => setRenameVal(e.target.value)} placeholder="新名称" />
                        <button className="mini-btn" onClick={() => void renameCat(c.name)}>保存</button>
                        <button className="mini-btn" onClick={() => setEditingCat(null)}>取消</button>
                      </span>
                    ) : (
                      <span className="cat-actions">
                        <button className="mini-btn" onClick={() => { setEditingCat(c.name); setRenameVal(c.name); }}>重命名</button>
                        <button className="mini-btn danger" onClick={() => void deleteCat(c.name)}>删除</button>
                      </span>
                    )}
                  </div>
                ))}
              </div>
            )}
            <div className="cat-create">
              <input className="text-input" value={newCat} onChange={(e) => setNewCat(e.target.value)} placeholder="新分类名称" />
              <button className="mini-btn" onClick={() => void createCat()}>新建</button>
            </div>
            <p className="muted">分类变更会在开启「自动同步」时推送到电脑端。</p>
          </>
        )}
      </div>

      <div className="card">
        <h3 className="collapse-head" onClick={() => setSecOpen((v) => !v)}>🔑 密保及密码修改 <span className="collapse-arrow">{secOpen ? "▼" : "▶"}</span></h3>
        {secOpen && (
          <>
            {recoveryCode && (
              <div className="info-box ok">
                <p className="muted">新的 Recovery Code（请立即保存，遗失后只能重新设置密保）：</p>
                <p className="recovery-code">{recoveryCode}</p>
              </div>
            )}
            {!secVerified ? (
              <form onSubmit={verifyCurrent}>
                <p className="muted">修改主密码或密保前，需要先验证当前主密码。</p>
                <label className="field">
                  <span>当前主密码</span>
                  <input type="password" value={secCurrent} onChange={(e) => setSecCurrent(e.target.value)} autoFocus />
                </label>
                <button className="btn-primary" type="submit" disabled={busy}>{busy ? "验证中…" : "验证并继续"}</button>
              </form>
            ) : (
              <form onSubmit={saveSecurity}>
                <p className="muted">验证通过。请选择要修改的内容；未勾选的项目保持不变。</p>
                <label className="checkRow"><input type="checkbox" checked={changeMaster} onChange={(e) => setChangeMaster(e.target.checked)} /> 修改主密码</label>
                {changeMaster && (
                  <>
                    <label className="field"><span>新主密码</span><input type="password" value={newPw} onChange={(e) => setNewPw(e.target.value)} placeholder="至少 8 位，含数字/大小写/特殊符号" /></label>
                    <label className="field"><span>确认新主密码</span><input type="password" value={newPw2} onChange={(e) => setNewPw2(e.target.value)} placeholder="再次输入" /></label>
                  </>
                )}
                <label className="checkRow"><input type="checkbox" checked={changeRecovery} onChange={(e) => setChangeRecovery(e.target.checked)} /> 修改密保（自动生成新 Recovery Code）</label>
                {changeRecovery && (
                  <div className="security-fields">
                    {[0, 1, 2].map((i) => (
                      <div key={i} className="cat-row">
                        <input className="text-input" value={secQuestions[i]} onChange={(e) => { const x = [...secQuestions]; x[i] = e.target.value; setSecQuestions(x); }} placeholder={`问题 ${i + 1}`} />
                        <input className="text-input" value={secAnswers[i]} onChange={(e) => { const x = [...secAnswers]; x[i] = e.target.value; setSecAnswers(x); }} placeholder="答案（不可为空）" />
                      </div>
                    ))}
                    <p className="muted">修改密保后，旧 Recovery Code 立即失效并生成新的。</p>
                  </div>
                )}
                <button className="btn-primary" type="submit" disabled={busy}>{busy ? "保存中…" : "保存修改"}</button>
              </form>
            )}
          </>
        )}
      </div>

      <div className="card">
        <h3 className="collapse-head" onClick={() => toggleFold("bio")}>🤚 指纹解锁 <span className="collapse-arrow">{fold["bio"] ? "▼" : "▶"}</span></h3>
        {fold["bio"] && (
          <>
            <div className="row-between">
              <span>指纹解锁</span>
              <label className="switch">
                <input type="checkbox" checked={bioOn} onChange={(e) => toggleBio(e.target.checked)} />
                <span className="slider" />
              </label>
            </div>
            {status.bioAvailable ? (
              <p className="muted">开启后，先正常解锁一次，之后锁定即可用指纹快速解锁；「立即锁定」后仍可用指纹解锁。App 退出重开也能用指纹解锁。</p>
            ) : (
              <p className="muted">当前设备未检测到可用指纹。请先在手机 系统设置 → 指纹/生物识别 中录入指纹，回到这里开启开关即可使用。</p>
            )}
            <div className="btn-row">
              <button className="btn-primary" onClick={() => void testBio()} disabled={bioTesting}>
                {bioTesting ? "验证中…" : "🔓 立即测试指纹"}
              </button>
            </div>
            {bioTestMsg && <div className={bioTestMsg.includes("成功") ? "info-box ok" : "error-box"}>{bioTestMsg}</div>}
          </>
        )}
      </div>

      <div className="card">
        <h3 className="collapse-head" onClick={() => toggleFold("backup")}>💾 备份与恢复 <span className="collapse-arrow">{fold["backup"] ? "▼" : "▶"}</span></h3>
        {fold["backup"] && (
          <>
            <p className="muted">导出的是一串加密密文（不是密钥、也不是明文密码），只有输入你的主密码才能解开；保存在微信/网盘/云盘都是安全的。恢复时输入该备份对应的主密码即可合并导入。</p>
            <button className="btn-primary" onClick={() => void exportBackup()} disabled={backupBusy}>
              {backupBusy ? "处理中…" : "📤 导出加密备份"}
            </button>
            <form onSubmit={importBackup}>
              <label className="field">
                <span>导入备份文本</span>
                <textarea value={backupText} onChange={(e) => setBackupText(e.target.value)} rows={4} placeholder="粘贴备份文本…" />
              </label>
              <label className="field">
                <span>备份对应的主密码</span>
                <input type="password" value={importPw} onChange={(e) => setImportPw(e.target.value)} placeholder="输入备份时对应的主密码" />
              </label>
              <button className="btn-secondary" type="submit" disabled={backupBusy}>
                {backupBusy ? "处理中…" : "📥 导入并合并"}
              </button>
            </form>
          </>
        )}
      </div>

      <div className="card">
        <h3 className="collapse-head" onClick={() => toggleFold("update")}>🔄 检查更新 <span className="collapse-arrow">{fold["update"] ? "▼" : "▶"}</span></h3>
        {fold["update"] && (
          <>
            <p className="muted">与电脑版同一套自动更新机制（GitHub Releases + 签名校验）；GitHub 网络不稳定时可直接从蓝奏云下载。</p>
            <div className="btn-row">
              <button className="btn-secondary" onClick={() => void checkUpdate()} disabled={updateBusy}>
                {updateBusy ? "处理中…" : "🔍 检查更新"}
              </button>
              {hasUpdate && (
                <button className="btn-primary" onClick={() => void downloadAndInstall()} disabled={updateBusy}>
                  {updateBusy ? "处理中…" : "⬇️ 下载并安装"}
                </button>
              )}
              <button className="btn-secondary" onClick={() => void goLanzou()} disabled={updateBusy}>
                📦 蓝奏云下载
              </button>
            </div>
            {updateMsg && <div className={updateMsg.includes("✅") || updateMsg.includes("已打开") ? "info-box ok" : updateMsg.includes("失败") ? "error-box" : "info-box"}>{updateMsg}</div>}
            <p className="muted">蓝奏云访问密码：{LANZOU_PWD}</p>
          </>
        )}
      </div>

      <div className="card">
        <h3 className="collapse-head" onClick={() => toggleFold("lock")}>🔒 锁定与安全 <span className="collapse-arrow">{fold["lock"] ? "▼" : "▶"}</span></h3>
        {fold["lock"] && (
          <>
            <button className="btn-danger" onClick={lockNow}>
              🔒 立即锁定
            </button>
            <p className="foot-note">锁定后需要重新输入主密码才能查看数据。</p>
            <div className="divider" />
            <p className="muted">自动锁定：无操作达到设定时间后自动锁定，期间有任何点击/触摸则重置计时。</p>
            <div className="auto-lock-row">
              {[
                { v: "", label: "永不" },
                { v: "1", label: "1 分钟" },
                { v: "5", label: "5 分钟" },
                { v: "10", label: "10 分钟" },
                { v: "30", label: "30 分钟" },
              ].map((o) => (
                <button
                  key={o.label}
                  className={autoLock === o.v ? "seg-btn active" : "seg-btn"}
                  onClick={() => changeAutoLock(o.v)}
                >
                  {o.label}
                </button>
              ))}
            </div>
          </>
        )}
      </div>

    </div>
  );
}
