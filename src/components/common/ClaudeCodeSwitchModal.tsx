import { useEffect, useState } from "react";
import {
  applyClaudeCodeProvider,
  getClaudeCodeSettings,
  testOllamaConnection,
  launchClaudeViaOllama,
  type ClaudeCodeProvider,
} from "../../lib/tauri";

interface Props {
  open: boolean;
  onClose: () => void;
}

type TestStatus = "idle" | "testing" | "ok" | "fail";

export function ClaudeCodeSwitchModal({ open, onClose }: Props) {
  const [provider, setProvider] = useState<ClaudeCodeProvider>("anthropic");
  const [baseUrl, setBaseUrl] = useState("http://localhost:11434");
  const [model, setModel] = useState("");
  const [authToken, setAuthToken] = useState("ollama");
  const [applying, setApplying] = useState(false);
  const [launching, setLaunching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);
  const [testStatus, setTestStatus] = useState<TestStatus>("idle");
  const [testMessage, setTestMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setError(null);
    setSuccess(false);
    setTestStatus("idle");
    setTestMessage(null);
    getClaudeCodeSettings()
      .then((cc) => {
        setProvider(cc.provider);
        setBaseUrl(cc.ollama_base_url || "http://localhost:11434");
        setModel(cc.ollama_model || "");
        setAuthToken(cc.ollama_auth_token || "ollama");
      })
      .catch((e) => setError(String(e)));
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, onClose]);

  if (!open) return null;

  const ollamaInvalid =
    provider === "ollama" &&
    (!model.trim() || !/^https?:\/\//.test(baseUrl.trim()));

  async function handleTest() {
    setTestStatus("testing");
    setTestMessage(null);
    try {
      await testOllamaConnection(baseUrl);
      setTestStatus("ok");
    } catch (e) {
      setTestStatus("fail");
      setTestMessage(String(e));
    }
  }

  async function handleLaunch() {
    setLaunching(true);
    setError(null);
    try {
      // Apply first so settings.json is up to date, then open Terminal
      await applyClaudeCodeProvider({
        provider: "ollama",
        ollama_base_url: baseUrl.trim(),
        ollama_model: model.trim(),
        ollama_auth_token: authToken.trim() || "ollama",
      });
      await launchClaudeViaOllama(model.trim());
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setLaunching(false);
    }
  }

  async function handleApply() {
    setApplying(true);
    setError(null);
    try {
      await applyClaudeCodeProvider({
        provider,
        ollama_base_url: baseUrl.trim(),
        ollama_model: model.trim(),
        ollama_auth_token: authToken.trim() || "ollama",
      });
      setSuccess(true);
      setTimeout(() => {
        setSuccess(false);
        onClose();
      }, 1200);
    } catch (e) {
      setError(String(e));
    } finally {
      setApplying(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="bg-app-card border border-border rounded-xl w-full max-w-lg shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-6 py-4 border-b border-border flex items-center justify-between">
          <h2 className="text-lg font-semibold text-text-primary">
            Switch Claude Code Model
          </h2>
          <button
            onClick={onClose}
            className="text-text-muted hover:text-text-primary"
          >
            ✕
          </button>
        </div>

        <div className="p-6 space-y-5">
          <p className="text-sm text-text-secondary">
            Switch Claude Code between Anthropic's cloud API and a local Ollama
            (or compatible proxy) endpoint. Writes to{" "}
            <code className="font-mono text-text-primary">~/.claude/settings.json</code>.
          </p>

          <div className="space-y-2">
            <label className="flex items-center gap-2 cursor-pointer p-3 rounded-lg bg-app-bg hover:bg-app-card-hover transition-colors">
              <input
                type="radio"
                checked={provider === "anthropic"}
                onChange={() => setProvider("anthropic")}
                className="w-4 h-4 rounded border-border accent-accent-blue"
              />
              <div>
                <p className="text-sm text-text-primary">Anthropic (cloud)</p>
                <p className="text-xs text-text-muted">
                  Default Claude Code behavior. Clears any local overrides.
                </p>
              </div>
            </label>

            <label className="flex items-center gap-2 cursor-pointer p-3 rounded-lg bg-app-bg hover:bg-app-card-hover transition-colors">
              <input
                type="radio"
                checked={provider === "ollama"}
                onChange={() => setProvider("ollama")}
                className="w-4 h-4 rounded border-border accent-accent-blue"
              />
              <div>
                <p className="text-sm text-text-primary">Ollama / local</p>
                <p className="text-xs text-text-muted">
                  Routes Claude Code to a local endpoint via{" "}
                  <code className="font-mono">ANTHROPIC_BASE_URL</code>.
                </p>
              </div>
            </label>
          </div>

          {provider === "ollama" && (
            <div className="space-y-3 p-4 rounded-lg border border-border">
              <p className="text-xs text-text-muted">
                Requires Ollama installed and running.{" "}
                <a
                  href="https://ollama.com"
                  target="_blank"
                  rel="noreferrer"
                  className="text-accent-blue hover:underline"
                >
                  Install Ollama ↗
                </a>
              </p>

              <div>
                <label className="block text-xs font-medium text-text-secondary mb-1">
                  Base URL
                </label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    value={baseUrl}
                    onChange={(e) => {
                      setBaseUrl(e.target.value);
                      setTestStatus("idle");
                    }}
                    placeholder="http://localhost:11434"
                    className="flex-1 px-3 py-2 bg-app-bg border border-border rounded-md text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent-blue"
                  />
                  <button
                    onClick={handleTest}
                    disabled={testStatus === "testing" || !/^https?:\/\//.test(baseUrl)}
                    className="px-3 py-2 text-xs rounded-md border border-border text-text-secondary hover:text-text-primary hover:bg-app-card-hover disabled:opacity-50"
                  >
                    {testStatus === "testing" ? "Testing…" : "Test"}
                  </button>
                </div>
                {testStatus === "ok" && (
                  <p className="text-xs text-accent-green mt-1">● Reachable</p>
                )}
                {testStatus === "fail" && (
                  <p className="text-xs text-accent-red mt-1">
                    ● {testMessage || "Unreachable"}
                  </p>
                )}
              </div>

              <div>
                <label className="block text-xs font-medium text-text-secondary mb-1">
                  Model <span className="text-accent-red">*</span>
                </label>
                <input
                  type="text"
                  value={model}
                  onChange={(e) => setModel(e.target.value)}
                  placeholder="qwen2.5-coder:7b"
                  className="w-full px-3 py-2 bg-app-bg border border-border rounded-md text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent-blue font-mono"
                />
                <p className="text-xs text-text-muted mt-1">
                  Written as <code className="font-mono">ANTHROPIC_MODEL</code>.
                  Recommended:{" "}
                  <code className="font-mono text-text-primary">qwen2.5-coder:7b</code>{" "}
                  or{" "}
                  <code className="font-mono text-text-primary">deepseek-coder-v2:16b</code>.
                </p>
              </div>

              <div>
                <label className="block text-xs font-medium text-text-secondary mb-1">
                  Auth Token{" "}
                  <span className="text-text-muted">(default: ollama)</span>
                </label>
                <input
                  type="text"
                  value={authToken}
                  onChange={(e) => setAuthToken(e.target.value)}
                  placeholder="ollama"
                  className="w-full px-3 py-2 bg-app-bg border border-border rounded-md text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent-blue font-mono"
                />
                <p className="text-xs text-text-muted mt-1">
                  Written as <code className="font-mono">ANTHROPIC_API_KEY</code>{" "}
                  so Claude Code uses this instead of your Anthropic account.
                </p>
              </div>

              <div className="pt-1 border-t border-border">
                <p className="text-xs text-text-muted mb-2">
                  Uses Ollama's native Claude Code integration —{" "}
                  <code className="font-mono">ollama launch claude</code>. No
                  separate proxy needed.
                </p>
                <button
                  onClick={handleLaunch}
                  disabled={launching || ollamaInvalid}
                  className="w-full py-2 bg-accent-green/20 text-accent-green text-sm rounded-md font-medium hover:bg-accent-green/30 border border-accent-green/30 disabled:opacity-50"
                >
                  {launching
                    ? "Opening Terminal…"
                    : "Apply & Launch Claude Code"}
                </button>
                <p className="text-xs text-text-muted mt-1 text-center">
                  Saves settings then opens Terminal with{" "}
                  <code className="font-mono">
                    ollama launch claude --model {model || "<model>"}
                  </code>
                </p>
              </div>
            </div>
          )}

          {error && (
            <div className="px-3 py-2 rounded-md border border-accent-red/40 bg-accent-red/10 text-sm text-accent-red">
              {error}
            </div>
          )}

          {success && (
            <div className="px-3 py-2 rounded-md border border-accent-green/40 bg-accent-green/10 text-sm text-accent-green">
              {provider === "ollama"
                ? 'Applied. Use "Apply & Launch Claude Code" or run ollama launch claude --model <model> in a new terminal.'
                : "Applied. Claude Code will use Anthropic cloud on next launch."}
            </div>
          )}
        </div>

        <div className="px-6 py-4 border-t border-border flex justify-end gap-2">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm text-text-secondary hover:text-text-primary"
          >
            Cancel
          </button>
          <button
            onClick={handleApply}
            disabled={applying || ollamaInvalid}
            className="px-4 py-2 bg-accent-blue text-white text-sm rounded-md font-medium hover:bg-accent-blue/90 disabled:opacity-50"
          >
            {applying ? "Applying…" : "Apply"}
          </button>
        </div>
      </div>
    </div>
  );
}
