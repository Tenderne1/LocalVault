import { FormEvent, useState } from "react";
import { api } from "../api";

interface Props {
  busy: boolean;
  setBusy: (b: boolean) => void;
  error: string;
  setError: (e: string) => void;
  onDone: () => void;
}

const rules = ["至少 8 位", "包含数字", "包含小写字母", "包含大写字母", "包含特殊符号"];
const defaultQuestions = ["我最喜欢的一本童年读物是什么？", "我自己定义的长期不变短语是什么？", "我记得的第一个特别地点是什么？"];

export default function SetupPage({ busy, setBusy, error, setError, onDone }: Props) {
  const [p1, setP1] = useState("");
  const [p2, setP2] = useState("");
  const [questions, setQuestions] = useState<string[]>(defaultQuestions);
  const [answers, setAnswers] = useState(["", "", ""]);
  const [recoveryCode, setRecoveryCode] = useState<string | null>(null);
  const [skipSecurity, setSkipSecurity] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError("");
    setBusy(true);
    try {
      // 1. 设置主密码
      await api.mobileSetup(p1, p2);
      // 2. 配置密保（生成 Recovery Code；未跳过时必填）
      if (!skipSecurity) {
        if (answers.some((a) => !a.trim())) {
          throw new Error("请填写每组密保答案");
        }
        const code = await api.mobileUpdateSecurity({
          currentPassword: p1,
          questions,
          answers,
        });
        if (code) {
          setRecoveryCode(code);
          setBusy(false); // 成功路径必须重置 busy，否则"我已保存"按钮保持禁用
        }
      } else {
        setRecoveryCode(null);
        onDone();
      }
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  };

  const confirmSaved = () => {
    onDone();
  };

  if (recoveryCode) {
    return (
      <div className="page">
        <div className="brand">
          <div className="brand-logo">🔐</div>
          <h1>LocalVault 手机端</h1>
          <p className="muted">保存你的 Recovery Code</p>
        </div>
        <form className="card" onSubmit={(e) => { e.preventDefault(); confirmSaved(); }}>
          <p className="muted">这是找回主密码的唯一凭据（配合 3 组密保答案）。请截图保存或抄写下来，遗失后无法找回密码。</p>
          <p className="recovery-code">{recoveryCode}</p>
          {error && <div className="error-box">{error}</div>}
          <button className="btn-primary" type="submit">
            我已保存 Recovery Code，进入 App
          </button>
          <p className="build-tag">build final</p>
        </form>
      </div>
    );
  }

  return (
    <div className="page">
      <div className="brand">
        <div className="brand-logo">🔐</div>
        <h1>LocalVault 手机端</h1>
        <p className="muted">首次使用，请设置主密码</p>
      </div>

      <form className="card" onSubmit={submit}>
        <label className="field">
          <span>主密码</span>
          <input
            type="password"
            value={p1}
            onChange={(e) => setP1(e.target.value)}
            placeholder="设置主密码"
            autoFocus
          />
        </label>
        <label className="field">
          <span>确认主密码</span>
          <input
            type="password"
            value={p2}
            onChange={(e) => setP2(e.target.value)}
            placeholder="再次输入"
          />
        </label>

        <div className="rule-hint">
          {rules.map((r) => (
            <span key={r} className="rule-chip">✓ {r}</span>
          ))}
        </div>

        {!skipSecurity && (
          <div className="security-fields">
            <h3 className="card-title">🔑 设置密保（推荐）</h3>
            <p className="muted">忘记主密码时，用「Recovery Code + 3 组密保答案」找回。</p>
            {[0, 1, 2].map((i) => (
              <label className="field" key={i}>
                <span>密保问题 {i + 1}</span>
                <input value={questions[i]} onChange={(e) => { const x = [...questions]; x[i] = e.target.value; setQuestions(x); }} placeholder="密保问题" />
                <input value={answers[i]} onChange={(e) => { const x = [...answers]; x[i] = e.target.value; setAnswers(x); }} placeholder="答案（不可为空，可使用中文）" />
              </label>
            ))}
          </div>
        )}

        {error && <div className="error-box">{error}</div>}

        <button className="btn-primary" type="submit" disabled={busy}>
          {busy ? "处理中…" : skipSecurity ? "创建并进入" : "创建并生成 Recovery Code"}
        </button>
        {!skipSecurity ? (
          <button className="link-btn" type="button" onClick={() => setSkipSecurity(true)}>
            暂不设置密保（忘记主密码将无法找回）
          </button>
        ) : (
          <button className="link-btn" type="button" onClick={() => setSkipSecurity(false)}>
            返回设置密保
          </button>
        )}
      </form>

      <p className="foot-note">
        主密码只保存在本机，用于加密你的密码数据；密保用于忘记主密码时找回。
      </p>
    </div>
  );
}
