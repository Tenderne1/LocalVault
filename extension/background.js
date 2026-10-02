// LocalVault-Fill 后台 Service Worker
// 所有指向本地填充服务的网络请求都在这里发起，content script 不持有 token。
const BASE = "http://127.0.0.1:38527";

async function localFetch(path, options = {}) {
  // 本地服务无响应时 4 秒快速失败，避免 sendResponse 一直挂起
  // 导致 content script 5 秒超时后误报「扩展后台不可用」。
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), 4000);
  try {
    const resp = await fetch(BASE + path, { ...options, signal: ctrl.signal });
    const body = await resp.json().catch(() => ({}));
    return { ok: resp.ok, status: resp.status, body };
  } catch (e) {
    return { ok: false, status: 0, body: { error: "连接本地填充服务失败（服务未启动或超时）" } };
  } finally {
    clearTimeout(timer);
  }
}

function browserName() {
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes("edg")) return "Edge";
  if (ua.includes("chrome")) return "Chrome";
  if (ua.includes("firefox")) return "Firefox";
  return "Chromium";
}

// 兜底注入：部分站点（163/QQ 邮箱等）登录框位于跨域 iframe，
// 自动 content script 注入可能遗漏动态创建的 iframe。
// 对每个非顶层 frame 导航后主动注入 content.js（幂等由 __lvFillLoaded 保证）。
chrome.webNavigation.onCommitted.addListener(async (details) => {
  if (details.frameId === 0) return;                    // 只处理 iframe
  if (/^(chrome|edge|devtools|about:(?!blank))/i.test(details.url || "")) return;
  if (details.tabId < 0) return;
  try {
    await chrome.scripting.executeScript({
      target: { tabId: details.tabId, frameIds: [details.frameId] },
      files: ["content.js"],
    });
  } catch (e) {
    // 页面已导航/注入失败：静默忽略，下次导航会再尝试
  }
});

// 兜底注入：定期对所有标签页的所有 iframe 重注入 content.js。
// content.js 幂等（__lvFillLoaded）；不过滤 about:blank/data:，
// 覆盖“iframe 以 about:blank 起步、随后 JS 写入登录表单”的站点（163/QQ 邮箱等）。
chrome.runtime.onStartup.addListener(() => ensureFrameInjection());
chrome.tabs.onUpdated.addListener((_id, info) => {
  if (info.status === "complete") ensureFrameInjection();
});
// MV3 中 setInterval 会阻止 Service Worker 休眠：每 4 秒全量轮询所有标签页
// 的 iframe 并执行 executeScript，长时间高频运行会被浏览器判定为 SW 不响应/
// 崩溃并自动停用扩展，导致 content script 发消息无响应、误报「扩展后台不可用」。
// 改为 chrome.alarms 低频兜底：alarms 不会阻止 SW 休眠，仅在触发瞬间唤醒执行。
chrome.alarms.create("lvfill-inject", { periodInMinutes: 1 });
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === "lvfill-inject") ensureFrameInjection();
});

async function ensureFrameInjection() {
  try {
    const tabs = await chrome.tabs.query({});
    for (const tab of tabs) {
      if (tab.id == null) continue;
      if (!/^https?:/i.test(tab.url || "")) continue;   // 顶层仍只处理 http(s) 标签页
      try {
        const frames = await chrome.webNavigation.getAllFrames({ tabId: tab.id });
        if (!frames || frames.length === 0) continue;
        const frameIds = (frames || [])
          .filter((f) => f.frameId > 0 && !/^(chrome|edge|devtools)/i.test(f.url || ""))
          .map((f) => f.frameId);
        if (!frameIds.length) continue;
        try {
          await chrome.scripting.executeScript({
            target: { tabId: tab.id, frameIds },
            files: ["content.js"],
          });
        } catch (e) {
          // 注入失败静默忽略：页面可能已导航
        }
      } catch (e) { /* 单标签页失败不阻塞其它标签页 */ }
    }
  } catch (e) { /* 全局失败静默 */ }
}

chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  (async () => {
    if (msg && msg.type === "page-ready") {
      // 页面加载完成，立即兜底注入该页所有 iframe（MV3 中 background 懒加载，
      // 需要 content script 主动唤醒，否则 iframe 注入不会执行）
      // Edge 下 content script 的 sender.tab 可能为 null，用消息里的 url 回查标签页兜底。
      let tabId = sender.tab && sender.tab.id;
      if (tabId == null && msg.url) {
        try {
          // tabs.query 的 url 参数需要 match pattern 格式（*://host/*）
          const u = new URL(String(msg.url));
          const pattern = "*://" + u.host + "/*";
          const tabs = await chrome.tabs.query({ url: pattern });
          tabId = tabs[0] && tabs[0].id;
        } catch (e) { /* 回查失败静默 */ }
      }
      if (tabId == null) return { ok: true };
      try {
        const frames = await chrome.webNavigation.getAllFrames({ tabId });
        if (frames && frames.length > 0) {
          const frameIds = (frames || [])
            .filter((f) => f.frameId > 0 && !/^(chrome|edge|devtools)/i.test(f.url || ""))
            .map((f) => f.frameId);
          if (frameIds.length) {
            try {
              await chrome.scripting.executeScript({
                target: { tabId, frameIds },
                files: ["content.js"],
              });
            } catch (e) { /* 注入失败静默 */ }
          }
        }
      } catch (e) { /* 失败静默 */ }
      return { ok: true };
    }
    if (msg && msg.type === "pair") {
      const code = String(msg.code || "").trim();
      const { ok, status, body } = await localFetch("/api/pair", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ code, name: `LocalVault-Fill (${browserName()})` }),
      });
      if (ok && body.token) {
        // 配对 token 持久化：一次配对后，LocalVault 解锁/重启、浏览器重启均无需重新配对
        await chrome.storage.local.set({ token: body.token });
      }
      return { ok, status, body };
    }
    if (msg && msg.type === "unpair") {
      const { token } = await chrome.storage.local.get("token");
      const { ok, status, body } = await localFetch("/api/unpair", {
        method: "POST",
        headers: { Authorization: `Bearer ${token || ""}` },
      });
      await chrome.storage.local.remove("token");
      return { ok, status, body };
    }
    if (msg && msg.type === "health") {
      const { ok, status, body } = await localFetch("/api/health");
      const { token } = await chrome.storage.local.get("token");
      return { ok, status, body, paired: !!token };
    }
    if (msg && msg.type === "match") {
      const { token } = await chrome.storage.local.get("token");
      if (!token) return { ok: false, status: 401, body: { error: "未配对，请先在扩展弹窗中输入配对码" } };
      const { ok, status, body } = await localFetch(
        `/api/match?url=${encodeURIComponent(msg.url)}`,
        { headers: { Authorization: `Bearer ${token}` } }
      );
      return { ok, status, body };
    }
    if (msg && msg.type === "fill") {
      const { token } = await chrome.storage.local.get("token");
      if (!token) return { ok: false, status: 401, body: { error: "未配对" } };
      const { ok, status, body } = await localFetch("/api/fill", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${token}`,
        },
        body: JSON.stringify({ url: msg.url, entry_id: msg.entryId }),
      });
      return { ok, status, body };
    }
    return { ok: false, status: 400, body: { error: "未知消息" } };
  })().then(sendResponse).catch((err) => {
    // 任何内部异常都必须回复 content script，否则它只能 5 秒超时后
    // 误报「扩展后台不可用」。给出具体错误便于排查。
    sendResponse({ ok: false, status: 500, body: { error: "扩展后台内部错误：" + String((err && err.message) || err) } });
  });
  return true; // 保持异步响应通道
});
