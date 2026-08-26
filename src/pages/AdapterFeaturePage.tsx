import { useParams, Navigate } from "react-router-dom";
import { Suspense, lazy } from "react";
import { getAdapterPlugin } from "../lib/adapterPlugins";

// ── Lazy-loaded page components ──────────────────────────────────────────────

// Shared / Claude Code
const AdapterGlobalConfigPage = lazy(() =>
  import("./AdapterGlobalConfigPage").then((m) => ({ default: m.AdapterGlobalConfigPage }))
);
const MemoryPage = lazy(() =>
  import("./MemoryPage").then((m) => ({ default: m.MemoryPage }))
);
const InstructionsPage = lazy(() =>
  import("./InstructionsPage").then((m) => ({ default: m.InstructionsPage }))
);
const PermissionsPage = lazy(() =>
  import("./PermissionsPage").then((m) => ({ default: m.PermissionsPage }))
);
const UsagePage = lazy(() =>
  import("./UsagePage").then((m) => ({ default: m.UsagePage }))
);
const AiAttributionPage = lazy(() =>
  import("./AiAttributionPage").then((m) => ({ default: m.AiAttributionPage }))
);
const PromptsPage = lazy(() =>
  import("./PromptsPage").then((m) => ({ default: m.PromptsPage }))
);
const TranscriptsPage = lazy(() =>
  import("./TranscriptsPage").then((m) => ({ default: m.TranscriptsPage }))
);
const PlansPage = lazy(() =>
  import("./PlansPage").then((m) => ({ default: m.PlansPage }))
);

// Cursor-specific
const CursorRulesPage = lazy(() =>
  import("./CursorRulesPage").then((m) => ({ default: m.CursorRulesPage }))
);
const CursorPermissionsPage = lazy(() =>
  import("./CursorPermissionsPage").then((m) => ({ default: m.CursorPermissionsPage }))
);
const CursorHooksPage = lazy(() =>
  import("./CursorHooksPage").then((m) => ({ default: m.CursorHooksPage }))
);
const CursorPlansPage = lazy(() =>
  import("./CursorPlansPage").then((m) => ({ default: m.CursorPlansPage }))
);
const CursorAnalyticsV2Page = lazy(() =>
  import("./CursorAnalyticsV2Page").then((m) => ({ default: m.CursorAnalyticsV2Page }))
);
const ClaudeAnalyticsV2Page = lazy(() =>
  import("./ClaudeAnalyticsV2Page").then((m) => ({ default: m.ClaudeAnalyticsV2Page }))
);
const KimiAnalyticsV2Page = lazy(() =>
  import("./KimiAnalyticsV2Page").then((m) => ({ default: m.KimiAnalyticsV2Page }))
);
const KimiPromptsPage = lazy(() =>
  import("./KimiPromptsPage").then((m) => ({ default: m.KimiPromptsPage }))
);
const KimiTranscriptsPage = lazy(() =>
  import("./KimiTranscriptsPage").then((m) => ({ default: m.KimiTranscriptsPage }))
);
const KimiPlansPage = lazy(() =>
  import("./KimiPlansPage").then((m) => ({ default: m.KimiPlansPage }))
);
const KimiInstructionsPage = lazy(() =>
  import("./KimiInstructionsPage").then((m) => ({ default: m.KimiInstructionsPage }))
);
const KimiControlPage = lazy(() =>
  import("./KimiControlPage").then((m) => ({ default: m.KimiControlPage }))
);

// Windsurf-specific
const WindsurfRulesPage = lazy(() =>
  import("./WindsurfRulesPage").then((m) => ({ default: m.WindsurfRulesPage }))
);

// Gemini-specific
const GeminiGlobalConfigPage = lazy(() =>
  import("./GeminiGlobalConfigPage").then((m) => ({ default: m.GeminiGlobalConfigPage }))
);
const GeminiMemoryPage = lazy(() =>
  import("./GeminiMemoryPage").then((m) => ({ default: m.GeminiMemoryPage }))
);
const GeminiHooksPage = lazy(() =>
  import("./GeminiHooksPage").then((m) => ({ default: m.GeminiHooksPage }))
);
const GeminiSkillsPage = lazy(() =>
  import("./GeminiSkillsPage").then((m) => ({ default: m.GeminiSkillsPage }))
);
const GeminiAgentsPage = lazy(() =>
  import("./GeminiAgentsPage").then((m) => ({ default: m.GeminiAgentsPage }))
);
const GeminiExtensionsPage = lazy(() =>
  import("./GeminiExtensionsPage").then((m) => ({ default: m.GeminiExtensionsPage }))
);
const GeminiAnalyticsPage = lazy(() =>
  import("./GeminiAnalyticsPage").then((m) => ({ default: m.GeminiAnalyticsPage }))
);

// Codex-specific
const CodexAnalyticsPage = lazy(() =>
  import("./CodexAnalyticsPage").then((m) => ({ default: m.CodexAnalyticsPage }))
);
const CodexSkillsPage = lazy(() =>
  import("./CodexSkillsPage").then((m) => ({ default: m.CodexSkillsPage }))
);
const CodexGlobalConfigPage = lazy(() =>
  import("./CodexGlobalConfigPage").then((m) => ({ default: m.CodexGlobalConfigPage }))
);

// DeepSeek-specific
const DeepSeekAnalyticsV2Page = lazy(() =>
  import("./DeepSeekAnalyticsV2Page").then((m) => ({ default: m.DeepSeekAnalyticsV2Page }))
);
const DeepSeekPromptsPage = lazy(() =>
  import("./DeepSeekPromptsPage").then((m) => ({ default: m.DeepSeekPromptsPage }))
);
const DeepSeekTranscriptsPage = lazy(() =>
  import("./DeepSeekTranscriptsPage").then((m) => ({ default: m.DeepSeekTranscriptsPage }))
);
const DeepSeekPlansPage = lazy(() =>
  import("./DeepSeekPlansPage").then((m) => ({ default: m.DeepSeekPlansPage }))
);
const DeepSeekInstructionsPage = lazy(() =>
  import("./DeepSeekInstructionsPage").then((m) => ({ default: m.DeepSeekInstructionsPage }))
);

// ── Adapter+Feature → Component mapping ──────────────────────────────────────

type LazyPage = React.LazyExoticComponent<React.ComponentType<unknown>>;

/**
 * Two-level lookup: adapter → feature → component.
 * This allows the same featureId (e.g. "permissions") to resolve to
 * different components depending on the adapter.
 */
const ADAPTER_FEATURE_COMPONENTS: Record<string, Record<string, LazyPage>> = {
  "claude-code": {
    "global-config": AdapterGlobalConfigPage as LazyPage,
    instructions: InstructionsPage as LazyPage,
    memory: MemoryPage as LazyPage,
    permissions: PermissionsPage as LazyPage,
    "analytics-v2": ClaudeAnalyticsV2Page as LazyPage,
    usage: UsagePage as LazyPage,
    prompts: PromptsPage as LazyPage,
    transcripts: TranscriptsPage as LazyPage,
    plans: PlansPage as LazyPage,
  },
  cursor: {
    "global-config": AdapterGlobalConfigPage as LazyPage,
    rules: CursorRulesPage as LazyPage,
    permissions: CursorPermissionsPage as LazyPage,
    hooks: CursorHooksPage as LazyPage,
    plans: CursorPlansPage as LazyPage,
    attribution: AiAttributionPage as LazyPage,
    "analytics-v2": CursorAnalyticsV2Page as LazyPage,
    transcripts: TranscriptsPage as LazyPage,
  },
  windsurf: {
    "global-config": AdapterGlobalConfigPage as LazyPage,
    rules: WindsurfRulesPage as LazyPage,
  },
  gemini: {
    "global-config": GeminiGlobalConfigPage as LazyPage,
    memory: GeminiMemoryPage as LazyPage,
    hooks: GeminiHooksPage as LazyPage,
    skills: GeminiSkillsPage as LazyPage,
    agents: GeminiAgentsPage as LazyPage,
    extensions: GeminiExtensionsPage as LazyPage,
    analytics: GeminiAnalyticsPage as LazyPage,
  },
  "claude-desktop": {
    "global-config": AdapterGlobalConfigPage as LazyPage,
  },
  codex: {
    "global-config": CodexGlobalConfigPage as LazyPage,
    skills: CodexSkillsPage as LazyPage,
    analytics: CodexAnalyticsPage as LazyPage,
  },
  deepseek: {
    "analytics-v2": DeepSeekAnalyticsV2Page as LazyPage,
    prompts: DeepSeekPromptsPage as LazyPage,
    transcripts: DeepSeekTranscriptsPage as LazyPage,
    plans: DeepSeekPlansPage as LazyPage,
    instructions: DeepSeekInstructionsPage as LazyPage,
  },
  kimi: {
    "analytics-v2": KimiAnalyticsV2Page as LazyPage,
    prompts: KimiPromptsPage as LazyPage,
    transcripts: KimiTranscriptsPage as LazyPage,
    plans: KimiPlansPage as LazyPage,
    instructions: KimiInstructionsPage as LazyPage,
    control: KimiControlPage as LazyPage,
  },
};

// ── Page component ───────────────────────────────────────────────────────────

export function AdapterFeaturePage() {
  const { adapterId, featureId } = useParams<{
    adapterId: string;
    featureId: string;
  }>();

  // Validate adapter exists in plugin registry
  if (!adapterId || !getAdapterPlugin(adapterId)) {
    return <Navigate to="/" replace />;
  }

  // Validate feature exists for this adapter
  const plugin = getAdapterPlugin(adapterId)!;
  const feature = plugin.features.find((f) => f.id === featureId);
  if (!feature) {
    return <Navigate to="/" replace />;
  }

  // Look up the component via adapter+feature
  const adapterMap = ADAPTER_FEATURE_COMPONENTS[adapterId];
  const Component = adapterMap?.[featureId!];
  if (!Component) {
    return (
      <div className="p-6 text-center">
        <p className="text-text-muted">
          Feature &quot;{featureId}&quot; is not yet implemented for {plugin.name}.
        </p>
      </div>
    );
  }

  return (
    <Suspense
      fallback={
        <div className="p-6 text-text-muted">Loading...</div>
      }
    >
      <Component />
    </Suspense>
  );
}
