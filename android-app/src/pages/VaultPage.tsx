import { useCallback, useEffect, useState } from "react";
import { api, CategoryInfo, EntryDetail, EntryDraft, EntryListItem, TrashItemInfo } from "../api";
import { registerBackHandler } from "../back";

const emptyDraft: EntryDraft = {
  id: "", type: "网站", name: "", username: "", email: "", phone: "",
  password: "", nickname: "", url: "", notes: "", category: "默认",
  tags: [], favorite: false,
};

type View = "list" | "detail" | "edit" | "trash";

interface Props {
  /** 立即锁定（右上角 🔒 按钮）：锁定后回到解锁页 */
  onLock?: () => void;
}

export default function VaultPage({ onLock }: Props) {
  const [items, setItems] = useState<EntryListItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [view, setView] = useState<View>("list");
  const [detail, setDetail] = useState<EntryDetail | null>(null);
  const [editing, setEditing] = useState<EntryDraft | null>(null);
  const [revealedPw, setRevealedPw] = useState("");
  const [menuItem, setMenuItem] = useState<EntryListItem | null>(null); // 长按菜单
  const [categories, setCategories] = useState<CategoryInfo[]>([]);
  const [trash, setTrash] = useState<TrashItemInfo[]>([]);
  // ---- 搜索 / 筛选 / 排序 / 多选 ----
  const [query, setQuery] = useState("");
  const [filterCat, setFilterCat] = useState("全部");
  const [sortMode, setSortMode] = useState<"updatedDesc" | "updatedAsc" | "nameAsc" | "nameDesc">("updatedDesc");
  const [multiMode, setMultiMode] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [moveCatOpen, setMoveCatOpen] = useState(false);
  const [catPickerOpen, setCatPickerOpen] = useState(false); // 分类下拉选择
  const [sortPickerOpen, setSortPickerOpen] = useState(false); // 排序下拉选择

  const load = useCallback(async () => {
    try {
      setItems(await api.mobileList());
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setLoading(false);
    }
  }, []);

  const loadCategories = useCallback(async () => {
    try {
      setCategories(await api.mobileCategoryList());
    } catch { /* 忽略 */ }
  }, []);

  useEffect(() => {
    load();
    loadCategories();
  }, [load, loadCategories]);

  // 自动同步（后台 30s 拉取）成功后会广播 lv-pulled：密码库界面自动刷新，无需手动操作
  useEffect(() => {
    const onPulled = () => {
      void load();
      void loadCategories();
    };
    window.addEventListener("lv-pulled", onPulled);
    return () => window.removeEventListener("lv-pulled", onPulled);
  }, [load, loadCategories]);

  /** 刷新按钮：拉取桌面端最新数据并重载列表 */
  const syncAndReload = async () => {
    try {
      await api.syncPull();
      await load();
      await loadCategories();
      setMsg({ ok: true, text: "已同步最新数据" });
    } catch (e) {
      setMsg({ ok: false, text: "同步失败：" + String(e) });
    }
  };

  // 排序文案（点击下拉选择）
  const sortLabel: Record<string, string> = {
    updatedDesc: "修改时间 ↓", updatedAsc: "修改时间 ↑", nameAsc: "名称 A→Z", nameDesc: "名称 Z→A",
  };

  const filtered = items.filter((it) => {
    if (filterCat !== "全部" && it.category !== filterCat) return false;
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return [it.name, it.username, it.url, it.category].some((f) => (f || "").toLowerCase().includes(q));
  }).sort((a, b) => {
    if (sortMode === "nameAsc") return (a.name || "").localeCompare(b.name || "", "zh");
    if (sortMode === "nameDesc") return (b.name || "").localeCompare(a.name || "", "zh");
    if (sortMode === "updatedAsc") return a.updatedAt - b.updatedAt;
    return b.updatedAt - a.updatedAt;
  });

  // ---- 多选操作 ----
  const toggleSelect = (id: string) => {
    setSelected((prev) => {
      const n = new Set(prev);
      if (n.has(id)) n.delete(id); else n.add(id);
      return n;
    });
  };
  const batchDelete = async () => {
    if (selected.size === 0) return;
    if (!window.confirm(`删除选中的 ${selected.size} 条？（可到回收站恢复）`)) return;
    try {
      for (const id of selected) {
        await api.mobileDeleteEntry(id);
      }
      const pushed = await autoSync();
      setSelected(new Set());
      setMultiMode(false);
      await load();
      setMsg({ ok: true, text: pushed ? `已删除 ${selected.size} 条，已推送到电脑` : `已删除 ${selected.size} 条；同步到电脑失败，稍后自动重试` });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };
  const batchMoveCat = async (cat: string) => {
    if (selected.size === 0) return;
    setMoveCatOpen(false);
    try {
      for (const id of selected) {
        const d = await api.mobileEntryDetail(id);
        await api.mobileSaveEntry({ ...d, password: "", category: cat, tags: [] });
      }
      const pushed = await autoSync();
      setSelected(new Set());
      setMultiMode(false);
      await load();
      setMsg({ ok: true, text: pushed ? `已移动 ${selected.size} 条到「${cat}」，已推送到电脑` : `已移动 ${selected.size} 条到「${cat}」；同步到电脑失败，稍后自动重试` });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  // Android 返回键：编辑/详情/回收站进入时 pushState，返回键触发 history.back → popstate → 回到列表
  useEffect(() => {
    const onPop = () => {
      setView("list");
      setDetail(null);
      setEditing(null);
      setRevealedPw("");
    };
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, []);

  // Android 返回键桥（优先于 App 兜底）：弹窗 → 多选 → 内层视图，逐级返回
  useEffect(() => {
    return registerBackHandler(() => {
      if (menuItem) { setMenuItem(null); return true; }
      if (catPickerOpen) { setCatPickerOpen(false); return true; }
      if (sortPickerOpen) { setSortPickerOpen(false); return true; }
      if (moveCatOpen) { setMoveCatOpen(false); return true; }
      if (multiMode) {
        setMultiMode(false);
        setSelected(new Set());
        return true;
      }
      if (view !== "list") {
        backToList();
        return true;
      }
      return false;
    });
  }, [menuItem, catPickerOpen, sortPickerOpen, moveCatOpen, multiMode, view]);

  const lockNow = async () => {
    try {
      await api.mobileLockAll();
      onLock?.();
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  const pushView = (v: View) => {
    window.history.pushState({ v }, "");
    setView(v);
  };

  const backToList = () => {
    if (window.history.length > 1) {
      window.history.back();
    } else {
      setView("list");
      setDetail(null);
      setEditing(null);
      setRevealedPw("");
    }
  };

  /** 勾选"自动同步到电脑"时，本地改动后自动推送；失败不抛错，仅返回是否成功（后台 30s 会自动重试补推） */
  const autoSync = async (): Promise<boolean> => {
    if (localStorage.getItem("lv_mobile_auto_sync") !== "1") return true;
    try {
      await api.syncPush();
      return true;
    } catch {
      return false;
    }
  };

  const openDetail = async (id: string) => {
    try {
      const d = await api.mobileEntryDetail(id);
      setDetail(d);
      setRevealedPw("");
      pushView("detail");
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  const startNew = () => {
    setEditing({ ...emptyDraft });
    pushView("edit");
  };

  const startEdit = (d: EntryDetail | EntryListItem) => {
    setEditing({
      id: d.id, type: d.type, name: d.name, username: d.username,
      email: "email" in d ? (d as EntryDetail).email : "",
      phone: "phone" in d ? (d as EntryDetail).phone : "",
      password: "", nickname: "nickname" in d ? (d as EntryDetail).nickname : "",
      url: d.url, notes: "notes" in d ? (d as EntryDetail).notes : "",
      category: d.category, tags: [], favorite: d.favorite,
    });
    pushView("edit");
  };

  const saveDraft = async () => {
    if (!editing) return;
    if (!editing.name.trim()) {
      setMsg({ ok: false, text: "名称不能为空" });
      return;
    }
    try {
      await api.mobileSaveEntry(editing);
      const pushed = await autoSync();
      setEditing(null);
      backToList();
      await load();
      await loadCategories();
      setMsg({
        ok: true,
        text: pushed ? "已保存，已推送到电脑" : "已保存；同步到电脑失败，稍后自动重试",
      });
    } catch (e) {
      setMsg({ ok: false, text: "保存失败：" + String(e) });
    }
  };

  const del = async (id: string) => {
    if (!window.confirm("确认删除这条密码？（可到回收站恢复）")) return;
    try {
      await api.mobileDeleteEntry(id);
      const pushed = await autoSync();
      setMenuItem(null);
      setDetail(null);
      setView("list");
      await load();
      setMsg({
        ok: true,
        text: pushed ? "已移入回收站，已推送到电脑" : "已移入回收站；同步到电脑失败，稍后自动重试",
      });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  const genPassword = async () => {
    try {
      const p = await api.mobileGeneratePassword(16);
      setEditing((ed) => (ed ? { ...ed, password: p } : ed));
      setMsg({ ok: true, text: "已生成 16 位强密码" });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  const copyText = async (text: string, label: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setMsg({ ok: true, text: `${label}已复制（请及时清理剪贴板）` });
    } catch {
      setMsg({ ok: false, text: "复制失败" });
    }
  };

  // ---------- 回收站 ----------
  const openTrash = async () => {
    try {
      setTrash(await api.mobileTrashList());
      pushView("trash");
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };
  const restore = async (id: string) => {
    try {
      await api.mobileRestoreEntry(id);
      const pushed = await autoSync();
      setTrash(await api.mobileTrashList());
      await load();
      setMsg({
        ok: true,
        text: pushed ? "已恢复，已推送到电脑" : "已恢复；同步到电脑失败，稍后自动重试",
      });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };
  const purge = async (id: string) => {
    if (!window.confirm("彻底删除后无法恢复，确认？")) return;
    try {
      await api.mobileTrashPurge(id);
      const pushed = await autoSync();
      setTrash(await api.mobileTrashList());
      setMsg({
        ok: true,
        text: pushed ? "已彻底删除" : "已彻底删除；同步到电脑失败，稍后自动重试",
      });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };
  const clearTrash = async () => {
    if (!window.confirm("清空回收站？全部条目将被彻底删除，无法恢复。")) return;
    try {
      await api.mobileTrashClear();
      const pushed = await autoSync();
      setTrash([]);
      setMsg({
        ok: true,
        text: pushed ? "回收站已清空" : "回收站已清空；同步到电脑失败，稍后自动重试",
      });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  // ---------- 编辑/新建表单 ----------
  if (view === "edit" && editing) {
    const set = (k: keyof EntryDraft, v: string | boolean | string[]) =>
      setEditing({ ...editing, [k]: v });
    return (
      <div className="page">
        <header className="page-header">
          <h2>{editing.id ? "✏️ 编辑条目" : "➕ 新建条目"}</h2>
          <p className="muted">密码仅在本机加密保存，同步时加密传输</p>
        </header>
        {msg && <div className={msg.ok ? "info-box ok" : "info-box err"}>{msg.text}</div>}
        <div className="card form-card">
          <label>名称 *<input value={editing.name} onChange={(e) => set("name", e.target.value)} placeholder="例如：公司邮箱" /></label>
          <label>类型<select value={editing.type} onChange={(e) => set("type", e.target.value)}>
            {["网站", "应用", "邮箱", "银行卡", "身份证", "其他"].map((t) => (
              <option key={t} value={t}>{t}</option>
            ))}
          </select></label>
          <label>账号<input value={editing.username} onChange={(e) => set("username", e.target.value)} placeholder="用户名 / 账号" /></label>
          <label>邮箱<input value={editing.email} onChange={(e) => set("email", e.target.value)} placeholder="邮箱（可选）" /></label>
          <label>手机<input value={editing.phone} onChange={(e) => set("phone", e.target.value)} placeholder="手机号（可选）" /></label>
          <label>昵称<input value={editing.nickname} onChange={(e) => set("nickname", e.target.value)} placeholder="昵称（可选）" /></label>
          <label>密码
            <div className="pw-row">
              <input type="text" value={editing.password} onChange={(e) => set("password", e.target.value)} placeholder="新建时必须填写；编辑留空则不修改" />
              <button className="mini-btn" onClick={() => void genPassword()}>🔑 生成</button>
              {editing.id && (
                <button className="mini-btn" onClick={async () => { try { set("password", await api.mobileEntryPassword(editing.id)); } catch { /* 忽略 */ } }}>原密码</button>
              )}
            </div>
          </label>
          <label>网址<input value={editing.url} onChange={(e) => set("url", e.target.value)} placeholder="https://…（可选）" /></label>
          <label>分类<select value={editing.category} onChange={(e) => set("category", e.target.value)}>
            {categories.length === 0
              ? <option value="默认">默认</option>
              : categories.map((c) => <option key={c.name} value={c.name}>{c.icon} {c.name}</option>)
            }
          </select></label>
          <label>备注<textarea value={editing.notes} onChange={(e) => set("notes", e.target.value)} rows={3} placeholder="备注（可选）" /></label>
          <label className="checkRow"><input type="checkbox" checked={editing.favorite} onChange={(e) => set("favorite", e.target.checked)} /> 标记为收藏</label>
        </div>
        <div className="form-actions">
          <button className="mini-btn" onClick={backToList}>取消</button>
          <button className="primary-btn" onClick={() => void saveDraft()}>保存</button>
        </div>
      </div>
    );
  }

  // ---------- 详情页 ----------
  if (view === "detail" && detail) {
    const fields: { label: string; value: string; copyKey: string }[] = [
      { label: "名称", value: detail.name, copyKey: "name" },
      { label: "类型", value: detail.type, copyKey: "type" },
      { label: "账号", value: detail.username, copyKey: "username" },
      { label: "邮箱", value: detail.email, copyKey: "email" },
      { label: "手机", value: detail.phone, copyKey: "phone" },
      { label: "昵称", value: detail.nickname, copyKey: "nickname" },
      { label: "网址", value: detail.url, copyKey: "url" },
      { label: "备注", value: detail.notes, copyKey: "notes" },
      { label: "分类", value: detail.category, copyKey: "category" },
    ];
    return (
      <div className="page">
        <header className="page-header">
          <h2>📄 {detail.name || "未命名"}{detail.favorite && <span className="star">★</span>}</h2>
          <button className="mini-btn" onClick={backToList}>← 返回</button>
        </header>
        {msg && <div className={msg.ok ? "info-box ok" : "info-box err"}>{msg.text}</div>}
        <div className="card form-card">
          {fields.map((f) =>
            f.value ? (
              <div key={f.copyKey} className="detail-row">
                <span className="detail-label">{f.label}</span>
                <span className="detail-value mono">{f.value}</span>
                <button className="mini-btn" onClick={() => void copyText(f.value, f.label + " ")}>复制</button>
              </div>
            ) : null
          )}
          <div className="detail-row">
            <span className="detail-label">密码</span>
            <span className="detail-value mono">{revealedPw || "••••••••"}</span>
            <button className="mini-btn" onClick={async () => {
              try {
                const p = await api.mobileEntryPassword(detail.id);
                setRevealedPw(revealedPw ? "" : p);
              } catch (e) { setMsg({ ok: false, text: String(e) }); }
            }}>{revealedPw ? "隐藏" : "查看"}</button>
            <button className="mini-btn" onClick={async () => {
              try { await copyText(await api.mobileEntryPassword(detail.id), "密码 "); } catch (e) { setMsg({ ok: false, text: String(e) }); }
            }}>复制</button>
          </div>
        </div>
        <div className="form-actions">
          <button className="mini-btn" onClick={() => { setDetail(null); setView("list"); }}>返回列表</button>
          <button className="primary-btn" onClick={() => startEdit(detail)}>✏️ 编辑</button>
          <button className="mini-btn danger" onClick={() => void del(detail.id)}>删除</button>
        </div>
      </div>
    );
  }

  // ---------- 回收站 ----------
  if (view === "trash") {
    return (
      <div className="page">
        <header className="page-header">
          <h2>🗑️ 回收站</h2>
          <button className="mini-btn" onClick={backToList}>← 返回</button>
        </header>
        {msg && <div className={msg.ok ? "info-box ok" : "info-box err"}>{msg.text}</div>}
        {trash.length === 0 ? (
          <div className="card empty-card"><p>回收站是空的</p></div>
        ) : (
          <>
            <div className="list">
              {trash.map((t) => (
                <div key={t.id} className="entry-card">
                  <div className="entry-main">
                    <div className="entry-title"><span>{t.name || "未命名"}</span></div>
                    <div className="entry-sub">{t.type} · {t.category} · {new Date(t.deletedAt).toLocaleString()} 删除</div>
                  </div>
                  <div className="entry-actions">
                    <button className="mini-btn" onClick={() => void restore(t.id)}>恢复</button>
                    <button className="mini-btn danger" onClick={() => void purge(t.id)}>彻底删除</button>
                  </div>
                </div>
              ))}
            </div>
            <button className="new-btn danger" onClick={() => void clearTrash()}>🗑️ 清空回收站</button>
          </>
        )}
      </div>
    );
  }

  // ---------- 列表页 ----------
  return (
    <div className="page">
      <header className="page-header">
        <div className="header-actions">
          <button className="mini-btn refresh-btn" onClick={() => void syncAndReload()}>🔄 刷新</button>
          <button className="mini-btn lock-btn" onClick={() => void lockNow()}>🔒 锁定</button>
        </div>
      </header>

      {msg && (
        <div className={msg.ok ? "info-box ok" : "info-box err"}>{msg.text}</div>
      )}

      {/* 工具条：搜索 / 分类下拉 / 排序下拉 / 多选 */}
      <div className="vault-toolbar">
        <input
          className="text-input search-input"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="🔍 搜索名称 / 账号 / 网址 / 分类…"
        />
        <div className="toolbar-row toolbar-row-3">
          <button className={filterCat === "全部" ? "toolbar-select" : "toolbar-select active"} onClick={() => setCatPickerOpen(true)}>
            📁 {filterCat}
          </button>
          <button className="toolbar-select" onClick={() => setSortPickerOpen(true)}>
            ⇅ {sortLabel[sortMode]}
          </button>
          <button className={multiMode ? "toolbar-select active" : "toolbar-select"} onClick={() => { setMultiMode((m) => !m); setSelected(new Set()); }}>
            ☑️ 多选
          </button>
        </div>
      </div>

      <button className="new-btn" onClick={startNew}>➕ 新建条目</button>
      <button className="new-btn secondary-btn" onClick={() => void openTrash()}>🗑️ 回收站</button>

      {loading ? (
        <div className="center-screen"><div className="spinner" /></div>
      ) : filtered.length === 0 ? (
        <div className="card empty-card">
          <p>{items.length === 0 ? "暂无数据" : "没有匹配的条目"}</p>
          <p className="muted">{items.length === 0 ? "点击上方「新建条目」添加，或在「同步」页拉取桌面端数据" : "换个关键词或分类试试"}</p>
        </div>
      ) : (
        <div className="list">
          {filtered.map((it) => (
            <div
              key={it.id}
              className={multiMode ? "entry-card selectable" : "entry-card"}
              onClick={() => { if (multiMode) toggleSelect(it.id); else void openDetail(it.id); }}
              onContextMenu={(e) => { e.preventDefault(); if (!multiMode) setMenuItem(it); }}
            >
              {multiMode && (
                <span className={selected.has(it.id) ? "sel-box on" : "sel-box"}>{selected.has(it.id) ? "✓" : ""}</span>
              )}
              <div className="entry-main">
                <div className="entry-title">
                  <span>{it.name || "未命名"}</span>
                  {it.favorite && <span className="star">★</span>}
                </div>
                <div className="entry-sub">
                  {it.username || "无账号"} · {it.category}
                </div>
                <div className="entry-pw">
                  <span className="muted">{it.passwordMasked}</span>
                </div>
              </div>
              <div className="entry-chevron">›</div>
            </div>
          ))}
        </div>
      )}

      {/* 多选操作栏 */}
      {multiMode && (
        <div className="batch-bar">
          <button className="mini-btn" onClick={() => setSelected(selected.size === filtered.length ? new Set() : new Set(filtered.map((f) => f.id)))}>
            {selected.size === filtered.length && filtered.length > 0 ? "取消全选" : "全选"}
          </button>
          <span className="batch-count">已选 {selected.size} 条</span>
          <button className="mini-btn" onClick={() => { if (selected.size > 0) setMoveCatOpen(true); }} disabled={selected.size === 0}>移到分类</button>
          <button className="mini-btn danger" onClick={() => void batchDelete()} disabled={selected.size === 0}>删除</button>
          <button className="mini-btn" onClick={() => { setMultiMode(false); setSelected(new Set()); }}>完成</button>
        </div>
      )}

      {/* 分类下拉选择 */}
      {catPickerOpen && (
        <div className="menu-overlay" onClick={() => setCatPickerOpen(false)}>
          <div className="action-sheet" onClick={(e) => e.stopPropagation()}>
            <div className="action-sheet-title">选择分类</div>
            <button className={filterCat === "全部" ? "active" : ""} onClick={() => { setFilterCat("全部"); setCatPickerOpen(false); }}>🗂️ 全部</button>
            {categories.map((c) => (
              <button key={c.name} className={filterCat === c.name ? "active" : ""} onClick={() => { setFilterCat(c.name); setCatPickerOpen(false); }}>{c.icon} {c.name}</button>
            ))}
            <button className="cancel" onClick={() => setCatPickerOpen(false)}>取消</button>
          </div>
        </div>
      )}

      {/* 排序下拉选择 */}
      {sortPickerOpen && (
        <div className="menu-overlay" onClick={() => setSortPickerOpen(false)}>
          <div className="action-sheet" onClick={(e) => e.stopPropagation()}>
            <div className="action-sheet-title">排序方式</div>
            {(["updatedDesc", "updatedAsc", "nameAsc", "nameDesc"] as const).map((m) => (
              <button key={m} className={sortMode === m ? "active" : ""} onClick={() => { setSortMode(m); setSortPickerOpen(false); }}>{sortLabel[m]}</button>
            ))}
            <button className="cancel" onClick={() => setSortPickerOpen(false)}>取消</button>
          </div>
        </div>
      )}

      {/* 批量移动分类选择 */}
      {moveCatOpen && (
        <div className="menu-overlay" onClick={() => setMoveCatOpen(false)}>
          <div className="action-sheet" onClick={(e) => e.stopPropagation()}>
            <div className="action-sheet-title">移动到分类（{selected.size} 条）</div>
            {categories.map((c) => (
              <button key={c.name} onClick={() => void batchMoveCat(c.name)}>{c.icon} {c.name}</button>
            ))}
            <button className="cancel" onClick={() => setMoveCatOpen(false)}>取消</button>
          </div>
        </div>
      )}

      {menuItem && (
        <div className="menu-overlay" onClick={() => setMenuItem(null)}>
          <div className="action-sheet" onClick={(e) => e.stopPropagation()}>
            <div className="action-sheet-title">{menuItem.name || "未命名"}</div>
            <button onClick={() => { setMenuItem(null); void openDetail(menuItem.id); }}>📄 查看详情</button>
            <button onClick={() => { setMenuItem(null); void (async () => { try { await copyText(await api.mobileEntryPassword(menuItem.id), "密码 "); } catch (e) { setMsg({ ok: false, text: String(e) }); } })(); }}>📋 复制密码</button>
            <button onClick={() => { setMenuItem(null); startEdit(menuItem); }}>✏️ 编辑</button>
            <button className="danger" onClick={() => { setMenuItem(null); void del(menuItem.id); }}>🗑️ 删除</button>
            <button className="cancel" onClick={() => setMenuItem(null)}>取消</button>
          </div>
        </div>
      )}
    </div>
  );
}
