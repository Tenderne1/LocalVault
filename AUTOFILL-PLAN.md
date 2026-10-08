# LocalVault v1.9.3 自动填充架构说明

> 本文件描述 v1.9.3「自动登录填充版」的实现架构（自 v1.9.2 起定型）。**注意：这是实现完成后的归档说明，不是开发计划。** 功能以 `src-tauri/src/autofill.rs`、`src/main.tsx`（🌐 浏览器填充弹窗）与 `extension/` 为准。

## 方案总览

```
┌──────────────┐   总开关 + 解锁联动  ┌────────────────────────────┐
│ LocalVault   │ ────────────────▶ │  本地填充服务（Rust）        │
│ 解锁状态      │                  │  tiny_http · 127.0.0.1:38527 │
└──────────────┘                  │  health / match / fill       │
        │                         │  / pair / unpair（Bearer）   │
        │ Vault 锁定事件           └──────────────▲─────────────┘
        │ 强制停止+销毁令牌+断配对                │ HTTPS/Fetch（仅回环）
        ▼                                        │
   AutofillBridge ───────────────────────────────┘
                   扩展（MV3）
                   service worker 持令牌代发请求
                   content script 只渲染 🔑 按钮与列表
```

## 组件

| 组件 | 路径 | 职责 |
|---|---|---|
| 填充服务 | `src-tauri/src/autofill.rs` | HTTP 服务、令牌/配对管理、域名匹配、状态持久化 |
| 后端接入 | `src-tauri/src/main.rs` | 命令注册、系统菜单「🌐 浏览器填充」、Vault 锁定联动 |
| Vault 接口 | `src-tauri/src/vault.rs` | `is_unlocked()`、`list_entries()`（新增） |
| 前端弹窗 | `src/main.tsx` + `src/styles.css` | 开关、状态展示、配对码生成、使用步骤 |
| 扩展 | `extension/` | MV3：`background.js`（代发请求）、`content.js`（🔑 按钮）、`popup.*`（配对） |

## 接口

| 接口 | 方法 | 鉴权 | 说明 |
|---|---|---|---|
| `/api/health` | GET | 无 | 探活：服务状态、端口、Vault 解锁状态、配对数 |
| `/api/pair` | POST | 无（6 位码） | 配对码换令牌；码 5 分钟一次性 |
| `/api/match` | GET | Bearer | 按 `url` 匹配条目（子域双向），**永不返回密码** |
| `/api/fill` | POST | Bearer | 校验域名匹配后下发密码（仅点击填充时刻） |
| `/api/unpair` | POST | Bearer | 断开配对、销毁令牌 |

## 状态机

- **Vault 锁定 / 未解锁**：开关置灰不可开启；即使此前已启用，锁定事件（`vault_locked()`）会强制停止服务、销毁令牌、清空配对并复位开关。
- **Vault 解锁 + 开关关（默认）**：服务完全不启动。
- **Vault 解锁 + 开关开**：启动服务；此时才可生成配对码、配对扩展。
- **服务运行中**：`autofill.json`（与 vault.db 同目录）持久化开关状态；重启软件后开关保持，但服务不自动启动（必须重新解锁 + 手动开启——开关持久化的是用户意图，服务进程本身仅在运行时存在）。

## 安全设计

- 服务仅绑定 `127.0.0.1`；token 为 128 位随机；配对码 6 位、5 分钟、一次性。
- 扩展：content script 无令牌；所有请求经 service worker（`chrome.storage.session`）转发。
- `fill` 校验条目 URL 与当前页面域名匹配（同一子域双向规则），防止跨站下发密码。
- CORS 仅用于回环请求；`match` 响应结构不含 `password` 字段。

## 域名匹配规则

条目「网址」host 与页面 host：相等，或一方是另一方的子域（以 `.` 为边界）即匹配。例：`example.com` ↔ `login.example.com`；`example.com` 与 `notexample.com` 不匹配。

## 测试

- `src-tauri` 下 `cargo test`：覆盖 host 解析、子域匹配、配对码格式、令牌生成、开关持久化、HTTP 接口鉴权（health/pair/match/fill/unpair、一次性码、过期码、错误方法、未知路由）、Vault 锁定后 503。
- 端到端验收（虚拟桌面）：解锁 → 开关默认关（服务不启动）→ 手动开 → 生成码 → 扩展配对 → 页面填充；关开关 → 服务停/令牌毁/配对清；锁定 → 开关置灰 + 强制停止。
