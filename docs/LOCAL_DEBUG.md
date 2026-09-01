# Local Debug

Windows PowerShell:
```powershell
npm install
npm.cmd run tauri:dev
```

如果 PowerShell 禁止 npm.ps1：
```powershell
npm.cmd install
npm.cmd run tauri:dev
```

第一次启动：
1. 创建主密码
2. 进入恢复设置
3. 保存 Recovery Code
4. 设置三组自定义恢复问题
5. 添加测试密码
6. 重启程序验证解锁

当前开发版把 SQLite 设置为 DELETE journal + synchronous=FULL，降低直接复制数据库备份时遗漏 WAL/SHM 的风险。生产版本仍需在 Windows 上做失电/崩溃恢复测试。
