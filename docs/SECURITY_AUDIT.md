# Production Security Audit Checklist

## Crypto
- [ ] Argon2id 参数基于目标 Windows 硬件基准测试并冻结。
- [ ] 每个 Vault 独立随机 salt。
- [ ] XChaCha20-Poly1305 nonce 不重复。
- [ ] AAD 绑定格式/版本/Entry ID。
- [ ] DEK/KEK 分离，主密码不持久化。
- [ ] 禁止自行实现密码算法。

## Recovery
- [ ] Recovery Code 使用 CSPRNG。
- [ ] 安全问答不是唯一认证因素。
- [ ] 生日、出生地、小学等公开信息不得单独解锁。
- [ ] 恢复尝试应有限速/失败延迟。
- [ ] 恢复成功后可要求重新设置主密码/重新包裹 DEK。

## SQLite
- [ ] Windows 崩溃/断电恢复测试。
- [ ] 临时数据库写入 + fsync + 原子替换策略。
- [ ] Restore 前保留 `.pre_restore.bak`。
- [ ] Schema migration journal / rollback。

## Windows
- [ ] EXE/MSI Authenticode SHA-256。
- [ ] RFC3161 timestamp。
- [ ] 清洁 Windows VM 安装/卸载。
- [ ] SmartScreen/Publisher 信誉。

## Updater
- [ ] updater public key 已替换。
- [ ] updater private key 只在 CI Secret。
- [ ] HTTPS endpoint。
- [ ] 签名失败拒绝更新。
- [ ] rollback 测试。

## Supply chain
- [ ] Cargo.lock / package-lock.json
- [ ] cargo audit / npm audit
- [ ] SBOM
- [ ] dependency/license review
- [ ] fuzz/property tests

## Release gate
- [ ] 正确密码
- [ ] 错误密码
- [ ] ciphertext/tag/nonce 篡改
- [ ] DB 损坏
- [ ] backup/restore
- [ ] auto-lock
- [ ] recovery flow
- [ ] installation/uninstallation
- [ ] updater
- [ ] signed binaries
