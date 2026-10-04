import { FormEvent, useEffect, useState } from "react";
import { api, MobileStatus } from "../api";

interface Props {
  status: MobileStatus;
  busy: boolean;
  setBusy: (b: boolean) => void;
  error: string;
  setError: (e: string) => void;
  onDone: () => void;
}

type RecoveryTab = "local" | "pc";

export default function UnlockPage({ status, busy, setBusy, error, setError, onDone }: Props) {
  const [pw, setPw] = useState("");
  const [showRecovery, setShowRecovery] = useState(false);
  const [tab, setTab] = useState<RecoveryTab>("local");
  // 手机端密保
  const [recoveryCode, setRecoveryCode] = useState("");
  const [answers, setAnswers] = useState(["", "", ""]);
  // 电脑端 Recovery Code
  const [pcQuestions, setPcQuestions] = useState<string[] | null>(null);
  const [pcAvailable, setPcAvailable] = useState(false);
  const [pcCode, setPcCode] = useState("");
  const [pcAnswers, setPcAnswers] = useState(["", "", ""]);
  const [newPw, setNewPw] = useState("");
  const [newPw2, setNewPw2] = useState("");
  const [bioMsg, setBioMsg] = useState("");

  // 电脑端恢复材料状态（挂载即查：同步拉取过且电脑端设置了密保）
  useEffect(() => {
    api.mobilePcRecoveryStatus().then((r) => {
      setPcAvailable(r.available);
      setPcQuestions(r.questions ?? null);
    }).catch(() => { /* 忽略 */ });
  }, []);

  useEffect(() => {
    if (!showRecovery) return;
    api.mobilePcRecoveryStatus().then((r) => {
      setPcAvailable(r.available);
      setPcQuestions(r.questions ?? null);
    }).catch(() => { /* 忽略 */ });
  }, [showRecovery]);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError("");
    setBusy(true);
    try {
      await api.mobileUnlock(pw);
      onDone();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const bioUnlock = async () => {
    setError("");
    setBioMsg("");
    setBusy(true);
    try {
      await api.mobileBiometricUnlock();
      onDone();
    } catch (err) {
      setBioMsg(String(err));
    } finally {
      setBusy(false);
    }
  };

  const recoverySubmit = async (e: FormEvent) => {
    e.preventDefault();
    setError("");
    setBusy(true);
    try {
      if (tab === "local") {
        await api.mobileRecoveryReset({ recoveryCode, answers, newPassword: newPw, confirm: newPw2 });
      } else {
        await api.mobilePcRecoveryReset({ recoveryCode: pcCode, answers: pcAnswers, newPassword: newPw, confirm: newPw2 });
      }
      setShowRecovery(false);
      setRecoveryCode(""); setAnswers(["", "", ""]);
      setPcCode(""); setPcAnswers(["", "", ""]);
      setNewPw(""); setNewPw2("");
      setPw(newPw);
      await api.mobileUnlock(newPw);
      onDone();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="page">
      <div className="brand">
        <div className="brand-logo">🔐</div>
        <h1>LocalVault</h1>
        <p className="muted">输入主密码解锁</p>
      </div>

      {!showRecovery ? (
        <form className="card" onSubmit={submit}>
          <label className="field">
            <span>主密码</span>
            <input
              type="password"
              value={pw}
              onChange={(e) => setPw(e.target.value)}
              placeholder="输入主密码"
              autoFocus
            />
          </label>

          {error && <div className="error-box">{error}</div>}
          {bioMsg && <div className="error-box">{bioMsg}</div>}

          <button className="btn-primary" type="submit" disabled={busy}>
            {busy ? "解锁中…" : "解锁"}
          </button>

          {status.bioUsable && (
            <button className="btn-secondary" type="button" onClick={() => void bioUnlock()} disabled={busy}>
              🤚 指纹解锁
            </button>
          )}

          <button className="link-btn" type="button" onClick={() => setShowRecovery(true)}>
            忘记主密码？找回
          </button>
        </form>
      ) : (
        <form className="card" onSubmit={recoverySubmit}>
          <h3 className="card-title">🔑 找回主密码</h3>
          <p className="muted">身份验证通过后设置新主密码。本地数据将清空，请重新从电脑端同步恢复。</p>

          {!status.hasSecurity && !pcAvailable && (
            <div className="error-box">
              当前未设置任何找回方式。请在「设置 → 密保及密码修改」中设置密保；或先与电脑端正常同步一次后，使用电脑端 Recovery Code 找回。
            </div>
          )}

          {(status.hasSecurity && pcAvailable) && (
            <div className="recovery-tabs">
              <button type="button" className={tab === "local" ? "chip active" : "chip"} onClick={() => setTab("local")}>手机端密保</button>
              <button type="button" className={tab === "pc" ? "chip active" : "chip"} onClick={() => setTab("pc")}>电脑端 Recovery Code</button>
            </div>
          )}

          {tab === "local" && status.hasSecurity && (
            <>
              <label className="field">
                <span>Recovery Code</span>
                <input value={recoveryCode} onChange={(e) => setRecoveryCode(e.target.value)} placeholder="手机端设置的 18 位代码" autoFocus />
              </label>
              {[0, 1, 2].map((i) => (
                <label className="field" key={i}>
                  <span>密保答案 {i + 1}</span>
                  <input value={answers[i]} onChange={(e) => { const x = [...answers]; x[i] = e.target.value; setAnswers(x); }} placeholder={`第 ${i + 1} 组密保答案`} />
                </label>
              ))}
            </>
          )}

          {tab === "pc" && pcAvailable && (
            <>
              <label className="field">
                <span>电脑端 Recovery Code</span>
                <input value={pcCode} onChange={(e) => setPcCode(e.target.value)} placeholder="电脑端设置的 Recovery Code" autoFocus />
              </label>
              {pcQuestions && (
                <p className="muted">电脑端密保问题：{pcQuestions.map((q, i) => `${i + 1}. ${q}`).join("　")}</p>
              )}
              {[0, 1, 2].map((i) => (
                <label className="field" key={i}>
                  <span>电脑端密保答案 {i + 1}</span>
                  <input value={pcAnswers[i]} onChange={(e) => { const x = [...pcAnswers]; x[i] = e.target.value; setPcAnswers(x); }} placeholder={`电脑端第 ${i + 1} 组密保答案`} />
                </label>
              ))}
            </>
          )}

          {((tab === "local" && status.hasSecurity) || (tab === "pc" && pcAvailable)) && (
            <>
              <label className="field">
                <span>新主密码</span>
                <input type="password" value={newPw} onChange={(e) => setNewPw(e.target.value)} placeholder="至少 8 位，含数字/大小写/特殊符号" />
              </label>
              <label className="field">
                <span>确认新主密码</span>
                <input type="password" value={newPw2} onChange={(e) => setNewPw2(e.target.value)} placeholder="再次输入新主密码" />
              </label>
              {error && <div className="error-box">{error}</div>}
              <button className="btn-primary" type="submit" disabled={busy}>
                {busy ? "验证中…" : "验证并重置主密码"}
              </button>
            </>
          )}
          {!pcAvailable && tab === "pc" && (
            <div className="error-box">尚未获取电脑端恢复材料：请先正常同步一次（配对后拉取数据），再使用此方式找回。</div>
          )}
          <button className="link-btn" type="button" onClick={() => setShowRecovery(false)}>返回解锁</button>
        </form>
      )}
    </div>
  );
}
