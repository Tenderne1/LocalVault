const $ = (id) => document.getElementById(id);

function setMsg(text, kind) {
  const m = $("msg");
  m.textContent = text || "";
  m.className = "msg " + (kind || "");
}

async function refresh() {
  const r = await chrome.runtime.sendMessage({ type: "health" }).catch(() => null);
  const dot = $("statusDot");
  if (r && r.ok && r.body.running) {
    dot.className = "dot on";
  } else {
    dot.className = "dot off";
    if (r && r.body && r.body.error) setMsg("本机填充服务未运行：" + r.body.error, "err");
  }
  if (r && r.paired) {
    $("pairCard").style.display = "none";
    $("pairedCard").style.display = "";
  } else {
    $("pairCard").style.display = "";
    $("pairedCard").style.display = "none";
  }
  return r;
}

$("pairBtn").addEventListener("click", async () => {
  const code = $("pairCode").value.trim();
  if (!/^\d{6}$/.test(code)) {
    setMsg("请输入 6 位数字配对码", "err");
    return;
  }
  setMsg("正在连接…");
  const r = await chrome.runtime.sendMessage({ type: "pair", code });
  if (r && r.ok && r.body && r.body.token) {
    setMsg("配对成功！");
    $("pairCode").value = "";
    await refresh();
  } else {
    setMsg((r && r.body && r.body.error) || "配对失败，请检查配对码", "err");
  }
});

$("unpairBtn").addEventListener("click", async () => {
  await chrome.runtime.sendMessage({ type: "unpair" });
  setMsg("已断开配对");
  await refresh();
});

refresh();
