// LocalVault-Fill content script
// 只做界面：在密码框旁渲染 🔑 按钮与账号列表；所有数据请求经 background 转发。
// 布局安全：按钮使用 position:fixed 挂载到 document.body 顶层，不插入输入框父容器，
// 避免破坏页面表单布局（flex/相对定位容器）；列表在焦点离开/Tab/Escape 时自动收起。
//
// iframe 桥：很多登录框（163/QQ 邮箱等）位于跨域 iframe 中，iframe 内渲染的按钮
// 会被 iframe 边界裁剪而不可见。方案：iframe 内的 content script 只负责扫描密码框
// 并上报相对坐标；顶层（window.top）的 content script 负责渲染按钮/列表（跨 iframe
// 叠加在页面上），点击后把填充指令 postMessage 回 iframe 执行。
(() => {
  if (window.__lvFillLoaded) return;
  window.__lvFillLoaded = true;

  const IS_TOP = window === window.top;
  const MSG_SOURCE = "lvfill";
  const LIST_CLASS = "lvfill-list";
  const FILL_STYLE = [
    "position:fixed",
    "z-index:2147483647",
    "display:flex",
    "align-items:center",
    "justify-content:center",
    "width:26px",
    "height:26px",
    "border:none",
    "border-radius:6px",
    "background:#2563eb",
    "color:#fff",
    "font-size:15px",
    "line-height:1",
    "cursor:pointer",
    "box-shadow:0 1px 6px rgba(0,0,0,.25)",
    "padding:0",
  ].join(";");

  // 覆盖标准密码框 + 非标准密码框（text 但 autocomplete/name/id/placeholder 具密码特征）
  const PWD_SELECTORS = [
    'input[type="password"]',
    'input[type="text"][autocomplete="current-password"]',
    'input[type="text"][autocomplete="new-password"]',
    'input[type="text"][autocomplete="password"]',
    'input[type="text"][name*="pass" i]',
    'input[type="text"][id*="pass" i]',
    'input[type="text"][placeholder*="密码" i]',
    'input[type="text"][placeholder*="password" i]',
  ].join(",");

  function escapeHtml(s) {
    return String(s == null ? "" : s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  // 给输入框赋值（触发 input/change，兼容 React/Vue 受控组件）
  function setVal(el, v) {
    const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(proto, "value").set;
    setter.call(el, v);
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  }

  // 判断输入框是否像账号框（宽松关键词，覆盖 QQ 登录框 name="u" 等无特征名）
  function looksLikeUsername(c) {
    if (!(c instanceof HTMLInputElement)) return false;
    const t = (c.type || "").toLowerCase();
    if (t === "password" || t === "hidden" || t === "submit" || t === "button" || t === "checkbox" || t === "radio" || t === "file") return false;
    if (t === "email" || t === "tel") return true;
    const s = ((c.name || "") + " " + (c.id || "") + " " + (c.autocomplete || "") + " " + (c.placeholder || "")).toLowerCase();
    return ["user", "account", "login", "mail", "email", "username", "mobile", "phone", "qq", "uid", "账号", "邮箱", "用户名", "手机"].some((k) => s.includes(k));
  }

  // 排除验证码 / 搜索类输入框，避免错填
  function looksLikeNoise(c) {
    if (!(c instanceof HTMLInputElement)) return false;
    const s = ((c.name || "") + " " + (c.id || "") + " " + (c.placeholder || "") + " " + (c.autocomplete || "")).toLowerCase();
    return ["code", "captcha", "verify", "yzm", "验证码", "图形码", "search", "query", "keyword", "搜索", "token", "csrf"].some((k) => s.includes(k));
  }

  // 在密码框所在文档内找账号输入框：先同容器，再逐级向上扩到整个表单区域
  function fillForm(input, entry) {
    let usernameInput = null;
    const seen = new Set();
    const candidates = [];
    let scope = input.form || input.closest("form") || input.parentElement;
    const root = input.ownerDocument;
    let hops = 0;
    while (scope && scope !== root.documentElement && scope !== root.body && hops < 8) {
      hops++;
      const found = Array.from(scope.querySelectorAll('input[type="text"],input[type="email"],input[type="tel"],input[name],input[autocomplete="username"]'));
      for (const c of found) {
        if (c === input || c.type === "password") continue;
        if (looksLikeNoise(c)) continue;
        if (seen.has(c)) continue;
        seen.add(c);
        candidates.push(c);
      }
      // 关键词命中账号框则立即采用
      const hit = candidates.find(looksLikeUsername);
      if (hit) { usernameInput = hit; break; }
      scope = scope.parentElement;
    }
    // 无关键词命中：优先 email/tel 类型，其次取第一个候选
    if (!usernameInput && candidates.length) {
      usernameInput = candidates.find((c) => (c.type || "").toLowerCase() === "email") || candidates.find((c) => (c.type || "").toLowerCase() === "tel") || candidates[0];
    }
    if (usernameInput && entry.username) setVal(usernameInput, entry.username);
    setVal(input, entry.password);
  }

  function isPasswordVisible(input) {
    return input.offsetParent !== null || input.getBoundingClientRect().width > 0;
  }

  // =====================================================================
  // 顶层：渲染按钮与账号列表（含 iframe 桥按钮）
  // =====================================================================
  if (IS_TOP) {
    const BUTTONS = new Map();            // 同页 input -> btn
    const FRAME_BUTTONS = new Map();      // iframe uid -> { btn, frameEl, rect }
    const ATTACHED = new WeakSet();
    let listEl = null;

    function removeList() {
      if (listEl) {
        listEl.remove();
        listEl = null;
      }
    }

    function clampPosition(left, top, W, H) {
      const vw = window.innerWidth || document.documentElement.clientWidth;
      const vh = window.innerHeight || document.documentElement.clientHeight;
      if (left + W > vw - 4) left = left - W - 8; // 右侧放不下 -> 尝试靠左
      if (left < 4) left = 4;
      if (top < 4) top = 4;
      if (top + H > vh - 4) top = vh - H - 4;
      return { left, top };
    }

    function positionButton(input, btn) {
      if (!btn.isConnected) return;
      const r = input.getBoundingClientRect();
      // 宽或高任一 > 0 即认为可见（部分页面在 CSS 生效前 getBoundingClientRect 返回 0）
      const visible = r.width > 0 || r.height > 0;
      btn.style.display = visible ? "flex" : "none";
      if (!visible) return;
      const GAP = 4;
      const W = 26;
      const H = 26;
      let left = r.right + GAP;
      let top = r.top + (r.height - H) / 2;
      const p = clampPosition(left, top, W, H);
      btn.style.left = p.left + "px";
      btn.style.top = p.top + "px";
    }

    function pruneButtons() {
      for (const [input, btn] of Array.from(BUTTONS.entries())) {
        if (!input.isConnected) {
          btn.remove();
          BUTTONS.delete(input);
          ATTACHED.delete(input);
        }
      }
    }

    function makeButton(input) {
      if (ATTACHED.has(input) || input.closest("." + LIST_CLASS)) return;
      ATTACHED.add(input);
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "lvfill-btn";
      btn.textContent = "🔑";
      btn.title = "LocalVault 填充";
      btn.style.cssText = FILL_STYLE;
      btn.addEventListener("mousedown", (e) => {
        e.preventDefault();
        e.stopPropagation();
      });
      btn.addEventListener("click", (e) => {
        e.preventDefault();
        e.stopPropagation();
        void showList(input, btn);
      });
      (document.body || document.documentElement).appendChild(btn);
      BUTTONS.set(input, btn);
      requestAnimationFrame(() => positionButton(input, btn));
    }

    function scan() {
      pruneButtons();
      document.querySelectorAll(PWD_SELECTORS).forEach((input) => {
        if (isPasswordVisible(input)) makeButton(input);
      });
    }

    function setList(html, left, top) {
      removeList();
      const el = document.createElement("div");
      el.className = LIST_CLASS;
      el.style.cssText = [
        "position:fixed",
        "z-index:2147483647",
        "min-width:240px",
        "max-width:340px",
        "max-height:280px",
        "overflow:auto",
        "background:#ffffff",
        "border:1px solid #d0d7de",
        "border-radius:10px",
        "box-shadow:0 8px 28px rgba(0,0,0,.22)",
        "font-size:13px",
        "color:#1f2328",
        "padding:6px",
        "font-family:system-ui,-apple-system,'Segoe UI',Roboto,sans-serif",
      ].join(";");
      el.style.left = left + "px";
      el.style.top = top + "px";
      el.innerHTML = html;
      document.documentElement.appendChild(el);
      listEl = el;
      return el;
    }

    function renderList(btn, entries) {
      const rows = entries
        .map(
          (e, i) =>
            `<button type="button" data-idx="${i}" style="display:flex;width:100%;align-items:center;gap:8px;padding:7px 8px;border:none;background:transparent;border-radius:6px;cursor:pointer;text-align:left">` +
            `<span style="font-size:15px">🔐</span>` +
            `<span style="flex:1;min-width:0"><b style="display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:#1f2328">${escapeHtml(e.name)}</b>` +
            `<small style="display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:#57606a">${escapeHtml(e.username || "无账号")}</small></span></button>`
        )
        .join("");
      return setList(
        `<div style="padding:5px 8px;color:#57606a;font-size:12px">选择要填充的账号（${entries.length}）</div>${rows}`,
        btn.getBoundingClientRect().left,
        btn.getBoundingClientRect().bottom + 4
      );
    }

    function showError(msg, btn) {
      setList(`<div style="padding:8px 10px;color:#b3261e">${escapeHtml(msg)}</div>`, btn.getBoundingClientRect().left, btn.getBoundingClientRect().bottom + 4);
    }

    // MV3 background service worker 可能休眠/首次唤醒慢：
    // 第一次 5 秒超时后自动重试一次，避免把「唤醒慢」误报成「扩展后台不可用」。
    async function sendMatch(url) {
      let lastErr = null;
      for (let i = 0; i < 2; i++) {
        try {
          const resp = await Promise.race([
            chrome.runtime.sendMessage({ type: "match", url }),
            new Promise((_, rej) => setTimeout(() => rej(new Error("bg-timeout")), 5000)),
          ]);
          if (resp) return resp;
        } catch (e) {
          lastErr = e;
          if (i === 0) continue;
        }
      }
      throw lastErr || new Error("bg-timeout");
    }

    async function showList(input, btn) {
      try {
        const resp = await sendMatch(location.href);
        if (!resp || !resp.ok) {
          showError((resp && resp.body && resp.body.error) || "无法连接填充服务", btn);
          return;
        }
        const entries = resp.body.entries || [];
        if (!entries.length) {
          setList('<div style="padding:8px 10px;color:#57606a">没有匹配的账号（请检查该网址是否已保存为站点地址）</div>', btn.getBoundingClientRect().left, btn.getBoundingClientRect().bottom + 4);
          return;
        }
        const el = renderList(btn, entries);
        el.querySelectorAll("button[data-idx]").forEach((rowBtn) => {
          rowBtn.addEventListener("click", async () => {
            const entry = entries[Number(rowBtn.dataset.idx)];
            removeList();
            const fill = await chrome.runtime.sendMessage({ type: "fill", url: location.href, entryId: entry.id });
            if (!fill || !fill.ok) {
              showError((fill && fill.body && fill.body.error) || "填充失败", btn);
              return;
            }
            fillForm(input, { username: entry.username, password: fill.body.password });
          });
        });
      } catch (e) {
        showError("扩展后台不可用", btn);
      }
    }

    // ---------------- iframe 桥 ----------------
    function findFrame(win) {
      for (const f of document.querySelectorAll("iframe")) {
        try {
          if (f.contentWindow === win) return f;
        } catch (e) { /* 跨域仅比较引用，不读属性 */ }
      }
      return null;
    }

    function makeFrameButton(uid, frameEl) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "lvfill-btn";
      btn.textContent = "🔑";
      btn.title = "LocalVault 填充";
      btn.style.cssText = FILL_STYLE;
      btn.addEventListener("mousedown", (e) => {
        e.preventDefault();
        e.stopPropagation();
      });
      btn.addEventListener("click", (e) => {
        e.preventDefault();
        e.stopPropagation();
        void showFrameList(uid, frameEl, btn);
      });
      document.documentElement.appendChild(btn);
      return btn;
    }

    function positionFrameButton(rec) {
      const btn = rec.btn;
      if (!btn.isConnected) return;
      const fr = rec.frameEl.getBoundingClientRect();
      const d = rec.rect;
      const visible = (fr.width > 0 || fr.height > 0) && (d.w > 0 || d.h > 0);
      btn.style.display = visible ? "flex" : "none";
      if (!visible) return;
      const GAP = 4;
      const W = 26;
      const H = 26;
      // iframe 视口坐标 + iframe 在顶层页面的偏移 = 顶层视口坐标
      let left = fr.left + d.x + d.w + GAP;
      let top = fr.top + d.y + (d.h - H) / 2;
      const p = clampPosition(left, top, W, H);
      btn.style.left = p.left + "px";
      btn.style.top = p.top + "px";
    }

    async function showFrameList(uid, frameEl, btn) {
      try {
        // 优先用登录 iframe 自身 URL 匹配（iframe 内常为真正登录域名，如 QQ 邮箱 xui.ptlogin2.qq.com）；
        // 若 iframe URL 未捕获到，回退顶层 URL。
        const rec = FRAME_BUTTONS.get(uid);
        const matchUrl = (rec && rec.href) || location.href;
        const resp = await sendMatch(matchUrl);
        if (!resp || !resp.ok) {
          showError((resp && resp.body && resp.body.error) || "无法连接填充服务", btn);
          return;
        }
        const entries = resp.body.entries || [];
        if (!entries.length) {
          setList('<div style="padding:8px 10px;color:#57606a">没有匹配的账号（请检查该网址是否已保存为站点地址）</div>', btn.getBoundingClientRect().left, btn.getBoundingClientRect().bottom + 4);
          return;
        }
        const el = renderList(btn, entries);
        el.querySelectorAll("button[data-idx]").forEach((rowBtn) => {
          rowBtn.addEventListener("click", async () => {
            const entry = entries[Number(rowBtn.dataset.idx)];
            removeList();
            const fill = await chrome.runtime.sendMessage({ type: "fill", url: matchUrl, entryId: entry.id });
            if (!fill || !fill.ok) {
              showError((fill && fill.body && fill.body.error) || "填充失败", btn);
              return;
            }
            try {
              frameEl.contentWindow.postMessage(
                { source: MSG_SOURCE, type: "frame-fill", uid, username: entry.username || "", password: fill.body.password },
                "*"
              );
            } catch (err) {
              showError("iframe 填充失败", btn);
            }
          });
        });
      } catch (e) {
        showError("扩展后台不可用", btn);
      }
    }

    function pruneFrameButtons() {
      for (const [uid, rec] of Array.from(FRAME_BUTTONS.entries())) {
        if (!rec.frameEl.isConnected) {
          rec.btn.remove();
          FRAME_BUTTONS.delete(uid);
        }
      }
    }

    window.addEventListener("message", (e) => {
      const d = e.data;
      if (!d || d.source !== MSG_SOURCE) return;
      const frameEl = findFrame(e.source);
      if (!frameEl) return;
      if (d.type === "frame-password") {
        let rec = FRAME_BUTTONS.get(d.uid);
        if (!rec) {
          const btn = makeFrameButton(d.uid, frameEl);
          rec = { btn, frameEl, rect: d };
          FRAME_BUTTONS.set(d.uid, rec);
        } else {
          rec.rect = d;
          rec.frameEl = frameEl;
        }
        // 记录 iframe 自身 URL：匹配时优先用登录 iframe 的域名（如 QQ 邮箱的 xui.ptlogin2.qq.com）
        if (d.href) rec.href = d.href;
        requestAnimationFrame(() => positionFrameButton(rec));
      } else if (d.type === "frame-password-remove") {
        const rec = FRAME_BUTTONS.get(d.uid);
        if (rec) {
          rec.btn.remove();
          FRAME_BUTTONS.delete(d.uid);
        }
      }
    });

    function repositionAll() {
      // 注意 Map.forEach 参数是 (value, key)：value 是按钮、key 是密码框
      BUTTONS.forEach((btn, input) => positionButton(input, btn));
      FRAME_BUTTONS.forEach((rec) => positionFrameButton(rec));
    }

    // 定期重定位：覆盖“初始扫描时页面未布局完成、按钮被隐藏”的情况。
    // 250ms 高频校准：即使缩放事件未被监听捕获，按钮也会在缩放后 250ms 内归位。
    setInterval(() => { repositionAll(); }, 250);

    // 缩放/尺寸变化时重定位：浏览器页面缩放（Ctrl+/- 或 Ctrl+滚轮）在 Chrome/Edge 中
    // 不触发 window.resize，需用 visualViewport 与 ResizeObserver 兜底，
    // 否则填充按钮会停留在缩放前的位置。
    const onViewportChange = () => requestAnimationFrame(() => { repositionAll(); scan(); });
    if (window.visualViewport) {
      window.visualViewport.addEventListener("resize", onViewportChange);
      window.visualViewport.addEventListener("scroll", onViewportChange);
    }
    window.addEventListener("resize", onViewportChange, { passive: true });
    if (window.ResizeObserver) {
      try {
        new ResizeObserver(onViewportChange).observe(document.documentElement);
      } catch (e) {}
    }

    // 监听 DOM 增删 + 密码框相关属性变化（class/style/type 等）。
    // 覆盖“初始隐藏、点击后显示”的密码框（如 163 邮箱登录框）：属性变化时重新扫描。
    new MutationObserver(() => {
      scan();
      pruneFrameButtons();
    }).observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["class", "style", "type", "autocomplete", "name", "id", "placeholder"],
    });

    window.addEventListener("scroll", () => {
      repositionAll();
      if (listEl) removeList();
    }, { passive: true });
    window.addEventListener("resize", repositionAll);
    document.addEventListener("click", (e) => {
      if (listEl && !listEl.contains(e.target) && !e.target.classList.contains("lvfill-btn")) removeList();
    });
    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape" || e.key === "Tab") removeList();
    }, true);
    document.addEventListener("focusin", (e) => {
      const t = e.target;
      if (t instanceof HTMLInputElement && BUTTONS.has(t)) {
        requestAnimationFrame(() => positionButton(t, BUTTONS.get(t)));
      }
      if (!listEl) return;
      if (t === document.body) return;
      if (listEl.contains(t)) return;
      if (t.classList && t.classList.contains("lvfill-btn")) return;
      if (t instanceof HTMLInputElement) return;
      removeList();
    });
    scan();

    // 主动唤醒 background：MV3 service worker 懒加载且首次加载慢，
    // 发送失败（background 尚未就绪）时重试，直到成功。
    (function wakeBg() {
      let tries = 0;
      (function attempt() {
        tries++;
        try {
          chrome.runtime.sendMessage({ type: "page-ready", url: location.href }).then(
            () => {},
            () => { setTimeout(attempt, 2000); }
          );
        } catch (e) {
          setTimeout(attempt, 2000);
        }
      })();
    })();
  }

  // =====================================================================
  // iframe：扫描密码框并上报坐标；接收填充指令
  // =====================================================================
  else {
    const FRAME_PWD = new Map(); // uid -> input
    let uidCounter = 0;

    function getUid(input) {
      for (const [uid, inp] of FRAME_PWD) if (inp === input) return uid;
      const uid = "f" + (++uidCounter) + "-" + Math.random().toString(36).slice(2, 7);
      FRAME_PWD.set(uid, input);
      return uid;
    }

    function report() {
      const inputs = Array.from(document.querySelectorAll(PWD_SELECTORS)).filter(isPasswordVisible);
      const seen = new Set();
      for (const inp of inputs) {
        const uid = getUid(inp);
        seen.add(uid);
        const r = inp.getBoundingClientRect();
        try {
          window.parent.postMessage(
            { source: MSG_SOURCE, type: "frame-password", uid, x: r.x, y: r.y, w: r.width, h: r.height, href: location.href },
            "*"
          );
        } catch (e) { /* 忽略 */ }
      }
      for (const [uid, inp] of Array.from(FRAME_PWD)) {
        if (!inp.isConnected || !seen.has(uid)) {
          try {
            window.parent.postMessage({ source: MSG_SOURCE, type: "frame-password-remove", uid }, "*");
          } catch (e) { /* 忽略 */ }
          FRAME_PWD.delete(uid);
        }
      }
    }

    // 在当前文档中查找消息来源 iframe（跨域仅比较引用）
    function findChildFrame(win) {
      for (const f of document.querySelectorAll("iframe")) {
        try {
          if (f.contentWindow === win) return f;
        } catch (e) { /* 跨域仅比较引用 */ }
      }
      return null;
    }

    window.addEventListener("message", (e) => {
      const d = e.data;
      if (!d || d.source !== MSG_SOURCE) return;
      // 1) 子 iframe 上报密码框坐标：累加本层偏移后继续向上转发（支持多层嵌套 iframe，
      //    如 QQ 邮箱 mail.qq.com → graph.qq.com 授权壳 → xui.ptlogin2.qq.com 密码框）
      if (d.type === "frame-password" || d.type === "frame-password-remove") {
        const child = findChildFrame(e.source);
        if (d.type === "frame-password") {
          const fr = child ? child.getBoundingClientRect() : { left: 0, top: 0 };
          try {
            window.parent.postMessage(
              { source: MSG_SOURCE, type: "frame-password", uid: d.uid, x: d.x + fr.left, y: d.y + fr.top, w: d.w, h: d.h, href: d.href },
              "*"
            );
          } catch (err) { /* 忽略 */ }
        } else {
          try {
            window.parent.postMessage({ source: MSG_SOURCE, type: "frame-password-remove", uid: d.uid }, "*");
          } catch (err) { /* 忽略 */ }
        }
        return;
      }
      // 2) 填充指令：本层若有对应密码框则执行填充，否则向下转发给所有子 iframe
      if (d.type === "frame-fill") {
        const input = FRAME_PWD.get(d.uid);
        if (input) {
          fillForm(input, { username: d.username, password: d.password });
          return;
        }
        for (const f of document.querySelectorAll("iframe")) {
          try {
            f.contentWindow.postMessage(
              { source: MSG_SOURCE, type: "frame-fill", uid: d.uid, username: d.username, password: d.password },
              "*"
            );
          } catch (err) { /* 忽略 */ }
        }
      }
    });

    window.addEventListener("scroll", report, { passive: true });
    window.addEventListener("resize", report);
    new MutationObserver(report).observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["class", "style", "type", "autocomplete", "name", "id", "placeholder"],
    });
    report();
    // 高频坐标校准：页面缩放（Ctrl+/-）等不触发 resize 的场景下，
    // 只要 iframe 内密码框坐标变化，就重新上报给顶层，保证按钮跟随。
    let lastFrameReport = "";
    setInterval(() => {
      const inputs = Array.from(document.querySelectorAll(PWD_SELECTORS)).filter(isPasswordVisible);
      const sig =
        inputs
          .map((i) => {
            const r = i.getBoundingClientRect();
            return Math.round(r.x) + "," + Math.round(r.y) + "," + Math.round(r.width);
          })
          .join("|") + ";" + inputs.length;
      if (sig !== lastFrameReport) {
        lastFrameReport = sig;
        report();
      }
    }, 250);
  }
})();
