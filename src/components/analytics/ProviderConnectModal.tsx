/**
 * Modal for connecting a provider (token input, API key, or device flow).
 */
import { useState } from "react";
import { useAnalyticsStore } from "../../stores/analyticsStore";

// Map provider IDs to their token types
const PROVIDER_TOKEN_CONFIG: Record<string, { keyType: string; placeholder: string; label: string }> = {
  "cursor": { keyType: "session-token", placeholder: "WorkosCursorSessionToken=...", label: "Cursor Session Token" },
  "openrouter": { keyType: "api-key", placeholder: "sk-or-v1-...", label: "OpenRouter API Key" },
  "kimi": { keyType: "auth-token", placeholder: "JWT token from kimi-auth cookie", label: "Kimi Auth Token" },
  "kimi-k2": { keyType: "api-key", placeholder: "API key", label: "Kimi K2 API Key" },
  "deepseek": { keyType: "api-key", placeholder: "sk-...", label: "DeepSeek API Key" },
  "moonshot": { keyType: "api-key", placeholder: "sk-...", label: "Moonshot API Key" },
  "zai": { keyType: "api-key", placeholder: "API key", label: "z.ai API Key" },
  "augment": { keyType: "session-token", placeholder: "Session token", label: "Augment Session Token" },
  "amp": { keyType: "session-cookie", placeholder: "session=...", label: "Amp Session Cookie" },
  "droid": { keyType: "bearer-token", placeholder: "Bearer token", label: "Droid/Factory Token" },
};

export function ProviderConnectModal({
  providerId,
  providerName,
  authType,
  onClose,
}: {
  providerId: string;
  providerName: string;
  authType: string;
  onClose: () => void;
}) {
  const { saveProviderToken } = useAnalyticsStore();
  const [token, setToken] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const config = PROVIDER_TOKEN_CONFIG[providerId];

  const handleSave = async () => {
    if (!token.trim() || !config) return;
    setSaving(true);
    setError(null);
    try {
      await saveProviderToken(providerId, config.keyType, token.trim());
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  // Auto-detect or local-file providers don't need manual token
  if (authType === "auto-detect" || authType === "local-file") {
    return (
      <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50" onClick={onClose}>
        <div className="bg-[#1a1b23] rounded-xl border border-[#2a2b36] p-6 w-[420px] max-w-[90vw]" onClick={(e) => e.stopPropagation()}>
          <h3 className="text-sm font-semibold text-text-primary mb-3">{providerName}</h3>
          <p className="text-xs text-text-secondary mb-4">
            {authType === "auto-detect"
              ? "This provider is auto-detected from your local CLI credentials. No manual setup needed."
              : "This provider reads from local configuration files. No manual setup needed."}
          </p>
          <p className="text-xs text-text-muted mb-4">
            {authType === "auto-detect"
              ? "Make sure the CLI tool is installed and you're logged in."
              : "Make sure the application is installed."}
          </p>
          <button onClick={onClose} className="px-3 py-1.5 bg-[#2a2b36] text-text-primary rounded-lg text-xs hover:bg-[#32333e]">
            Close
          </button>
        </div>
      </div>
    );
  }

  // CLI-based providers
  if (authType === "cli") {
    return (
      <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50" onClick={onClose}>
        <div className="bg-[#1a1b23] rounded-xl border border-[#2a2b36] p-6 w-[420px] max-w-[90vw]" onClick={(e) => e.stopPropagation()}>
          <h3 className="text-sm font-semibold text-text-primary mb-3">{providerName}</h3>
          <p className="text-xs text-text-secondary mb-4">
            This provider requires the CLI tool to be installed and you to be logged in.
          </p>
          <code className="block text-xs bg-[#0e0f13] text-accent-green p-2 rounded mb-4 font-mono">
            {providerId === "kiro" ? "kiro-cli login" : `${providerId} login`}
          </code>
          <button onClick={onClose} className="px-3 py-1.5 bg-[#2a2b36] text-text-primary rounded-lg text-xs hover:bg-[#32333e]">
            Close
          </button>
        </div>
      </div>
    );
  }

  // Device flow (Copilot)
  if (authType === "device-flow") {
    return (
      <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50" onClick={onClose}>
        <div className="bg-[#1a1b23] rounded-xl border border-[#2a2b36] p-6 w-[420px] max-w-[90vw]" onClick={(e) => e.stopPropagation()}>
          <h3 className="text-sm font-semibold text-text-primary mb-3">{providerName}</h3>
          <p className="text-xs text-text-secondary mb-4">
            GitHub Copilot uses device flow authentication. This feature is coming soon.
          </p>
          <button onClick={onClose} className="px-3 py-1.5 bg-[#2a2b36] text-text-primary rounded-lg text-xs hover:bg-[#32333e]">
            Close
          </button>
        </div>
      </div>
    );
  }

  // Token/API key input
  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50" onClick={onClose}>
      <div className="bg-[#1a1b23] rounded-xl border border-[#2a2b36] p-6 w-[480px] max-w-[90vw]" onClick={(e) => e.stopPropagation()}>
        <h3 className="text-sm font-semibold text-text-primary mb-1">{providerName}</h3>
        <p className="text-xs text-text-muted mb-4">
          Enter your {config?.label || "token"} for more accurate analytics.
          <span className="text-text-muted"> (Optional — stored securely in your OS keychain)</span>
        </p>

        <input
          type="password"
          value={token}
          onChange={(e) => setToken(e.target.value)}
          placeholder={config?.placeholder || "Token..."}
          className="w-full px-3 py-2 bg-[#0e0f13] border border-[#2a2b36] rounded-lg text-xs text-text-primary font-mono placeholder:text-text-muted focus:outline-none focus:border-accent-blue"
          onKeyDown={(e) => e.key === "Enter" && handleSave()}
        />

        {error && <p className="text-xs text-red-400 mt-2">{error}</p>}

        <div className="flex justify-end gap-2 mt-4">
          <button onClick={onClose} className="px-3 py-1.5 text-text-secondary text-xs hover:text-text-primary">
            Cancel
          </button>
          <button
            onClick={handleSave}
            disabled={!token.trim() || saving}
            className="px-4 py-1.5 bg-accent-blue text-white rounded-lg text-xs font-medium hover:bg-accent-blue/90 disabled:opacity-50"
          >
            {saving ? "Saving..." : "Connect"}
          </button>
        </div>
      </div>
    </div>
  );
}
