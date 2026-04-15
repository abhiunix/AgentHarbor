import { useState, useEffect, useRef } from "react";
import type {
  UniversalCapability,
  CapabilityType,
  McpServer,
  Rule,
  Skill,
  Hook,
  Plugin,
  Custom,
  EnvVariable,
  SkillFile,
} from "../../lib/types";
import { invoke } from "@tauri-apps/api/core";
import { getUsername, getAuthorId, fetchGithubSkill, discoverMcpTools } from "../../lib/tauri";
import { getAdapterIconImg, getAdapterName } from "../../lib/adapterPlugins";
import type { FetchedSkill } from "../../lib/tauri";
import { makeStableCompositeIdWithRetry } from "../../lib/stableId";
import { useRegistryStore } from "../../stores/registryStore";

interface CapabilityEditorProps {
  capability?: UniversalCapability;
  onSave: (capability: UniversalCapability) => void | Promise<void>;
  onCancel: () => void;
}

const typeOptions: { value: CapabilityType; label: string }[] = [
  { value: "mcp", label: "MCP Server" },
  { value: "rule", label: "Rule" },
  { value: "skill", label: "Skill" },
  { value: "hook", label: "Hook" },
  { value: "custom", label: "Custom" },
];

export function CapabilityEditor({ capability, onSave, onCancel }: CapabilityEditorProps) {
  const [capType, setCapType] = useState<CapabilityType>(capability?.type ?? "mcp");
  const [name, setName] = useState(capability?.name ?? "");
  const [description, setDescription] = useState(capability?.description ?? "");
  const [version, setVersion] = useState(capability?.version ?? "1.0.0");
  const [tags, setTags] = useState(capability?.tags.join(", ") ?? "");
  const [adapters, setAdapters] = useState<string[]>(capability?.compatible_agents ?? ["claude-code", "cursor", "windsurf"]);
  const [username, setUsername] = useState("user");
  

  // Use array-based env state with stable IDs to avoid React key/focus issues
  interface EnvEntry { _id: number; key: string; val: EnvVariable }
  const envIdCounter = useRef(0);
  const [mcpEnvEntries, setMcpEnvEntries] = useState<EnvEntry[]>([]);


  const genericEnvIdCounter = useRef(0);
  const [genericEnvEntries, setGenericEnvEntries] = useState<EnvEntry[]>([]);
  const genericEnvAsRecord = (): Record<string, EnvVariable> => {
    const rec: Record<string, EnvVariable> = {};
    for (const entry of genericEnvEntries) {
      if (entry.key) rec[entry.key] = entry.val;
    }
    return rec;
  };
  
  const [ruleScope, setRuleScope] = useState("project");
  const [ruleContent, setRuleContent] = useState("");
  
  const [skillFiles, setSkillFiles] = useState<SkillFile[]>([{ path: "SKILL.md", content: "" }]);

  // New skill fields
  const [skillAllowedTools, setSkillAllowedTools] = useState("");
  const [skillModel, setSkillModel] = useState("");
  const [skillContext, setSkillContext] = useState("");
  const [skillAgent, setSkillAgent] = useState("");
  const [skillArgumentHint, setSkillArgumentHint] = useState("");
  const [skillLicense, setSkillLicense] = useState("");
  const [skillAdvancedOpen, setSkillAdvancedOpen] = useState(false);

  // Skill import from URL
  type SkillEditMode = "create" | "import-url";
  const [skillEditMode, setSkillEditMode] = useState<SkillEditMode>("create");
  const [skillGithubUrl, setSkillGithubUrl] = useState("");
  const [skillFetching, setSkillFetching] = useState(false);
  const [skillFetchError, setSkillFetchError] = useState<string | null>(null);
  const [skillFetchProgress, setSkillFetchProgress] = useState<string[]>([]);
  const [skillHasScriptsWarning, setSkillHasScriptsWarning] = useState(false);
  
  const [hookAdapterFiles, setHookAdapterFiles] = useState<
    Record<string, Array<{ path: string; content: string }>>
  >({});

  const [pluginInstallCmd, setPluginInstallCmd] = useState("");

  const [customAdapterConfigs, setCustomAdapterConfigs] = useState<
    Record<string, Array<{ path: string; content: string }>>
  >({});
  
  const [mcpJsonText, setMcpJsonText] = useState("");
  // MCP tool discovery & access control
  const [mcpEnvMasked, setMcpEnvMasked] = useState<Record<string, boolean>>({});
  const [mcpDiscoveredTools, setMcpDiscoveredTools] = useState<import("../../lib/types").McpTool[]>([]);
  const [mcpToolsLoading, setMcpToolsLoading] = useState(false);
  const [mcpToolsError, setMcpToolsError] = useState<string | null>(null);
  const [mcpAlwaysAllow, setMcpAlwaysAllow] = useState<string[]>([]);
  const [mcpDisabledTools, setMcpDisabledTools] = useState<string[]>([]);
  
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const capabilities = useRegistryStore((s) => s.capabilities);

  useEffect(() => {
    getUsername().then(setUsername).catch(() => setUsername("user"));
  }, []);

  useEffect(() => {
    if (capability) {
      if (capability.type === "mcp") {
        const mcp = capability as McpServer;
        setMcpEnvEntries(
          Object.entries(mcp.env).map(([k, v]) => ({
            _id: envIdCounter.current++,
            key: k,
            // For the UI: label is used as the editable value field.
            // If there's an actual value stored, use it; otherwise keep label.
            val: {
              ...v,
              label: v.value || (v.label.includes("${") ? "" : v.label),
            },
          }))
        );
        setGenericEnvEntries([]);
        // Build JSON from existing MCP data
        const slug = mcp.name.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "my-mcp-server";
        const envObj = Object.keys(mcp.env).length
          ? Object.fromEntries(Object.entries(mcp.env).map(([k]) => [k, `\${${k}}`]))
          : undefined;
        const payload: Record<string, unknown> = mcp.transport === "stdio"
          ? { command: mcp.command, args: mcp.args, ...(envObj ? { env: envObj } : {}) }
          : { url: mcp.url, ...(envObj ? { env: envObj } : {}) };
        setMcpJsonText(JSON.stringify({ mcpServers: { [slug]: payload } }, null, 2));
        // Load tool access fields
        setMcpAlwaysAllow(mcp.always_allow ?? []);
        setMcpDisabledTools(mcp.disabled_tools ?? []);
        if (mcp.tool_list) setMcpDiscoveredTools(mcp.tool_list);
        // Default all env vars to masked
        const masked: Record<string, boolean> = {};
        for (const k of Object.keys(mcp.env)) masked[k] = true;
        setMcpEnvMasked(masked);
      } else if (capability.type === "rule") {
        const rule = capability as Rule;
        setRuleScope(rule.scope);
        setRuleContent(rule.content);
        setGenericEnvEntries(
          Object.entries(rule.env ?? {}).map(([k, v]) => ({
            _id: genericEnvIdCounter.current++,
            key: k,
            val: v,
          }))
        );
      } else if (capability.type === "skill") {
        const skill = capability as Skill;
        // Normalize old adapter-prefixed paths to relative paths
        const normalizedFiles = (skill.files.length > 0 ? skill.files : [{ path: "SKILL.md", content: "" }]).map((f) => {
          let path = f.path;
          // Strip adapter-specific prefixes like .claude/skills/name/, .cursor/skills/name/
          const prefixMatch = path.match(/^\.(claude|cursor|windsurf)\/skills\/[^/]+\/(.+)$/);
          if (prefixMatch) {
            path = prefixMatch[2]; // e.g., "SKILL.md" or "scripts/helper.py"
          }
          return { path, content: f.content };
        });
        // Deduplicate by path (old skills may have same SKILL.md for multiple adapters)
        const seen = new Set<string>();
        const deduped = normalizedFiles.filter((f) => {
          if (seen.has(f.path)) return false;
          seen.add(f.path);
          return true;
        });
        // Ensure SKILL.md is first
        deduped.sort((a, b) => {
          if (a.path === "SKILL.md") return -1;
          if (b.path === "SKILL.md") return 1;
          return 0;
        });
        setSkillFiles(deduped);
        setSkillAllowedTools((skill.allowed_tools ?? []).join(", "));
        setSkillModel(skill.model ?? "");
        setSkillContext(skill.context ?? "");
        setSkillAgent(skill.agent ?? "");
        setSkillArgumentHint(skill.argument_hint ?? "");
        setSkillLicense(skill.license ?? "");
        if (skill.model || skill.context || skill.agent || skill.argument_hint || skill.license) {
          setSkillAdvancedOpen(true);
        }
        setGenericEnvEntries(
          Object.entries(skill.env ?? {}).map(([k, v]) => ({
            _id: genericEnvIdCounter.current++,
            key: k,
            val: v,
          }))
        );
      } else if (capability.type === "hook") {
        const hook = capability as Hook;
        const filesByAdapter: Record<string, Array<{ path: string; content: string }>> = {};
        const defaultPaths: Record<string, string> = {
          "claude-code": ".claude/settings.json",
          cursor: ".cursor/hooks.json",
        };
        if (hook.adapter_configs && Object.keys(hook.adapter_configs).length > 0) {
          for (const [adapterId, val] of Object.entries(hook.adapter_configs)) {
            const obj = val as Record<string, unknown>;
            const filesRaw = obj.files;
            if (Array.isArray(filesRaw) && filesRaw.length > 0) {
              filesByAdapter[adapterId] = filesRaw
                .filter((f): f is Record<string, unknown> => f != null && typeof f === "object")
                .map((f) => ({
                  path: ((f.deploy_path ?? f.path) as string) ?? "",
                  content: (f.content as string) ?? "",
                }));
            } else {
              // Legacy: single config (version/hooks) + optional scripts
              const deployPath = (obj.deploy_path as string) || defaultPaths[adapterId] || "";
              const configContent =
                adapterId === "cursor"
                  ? JSON.stringify({ version: obj.version ?? 1, hooks: obj.hooks ?? {} }, null, 2)
                  : JSON.stringify(obj, null, 2);
              const list: Array<{ path: string; content: string }> = [
                { path: deployPath, content: configContent },
              ];
              if (adapterId === "cursor") {
                const scriptsRaw = obj.scripts;
                if (Array.isArray(scriptsRaw)) {
                  for (const s of scriptsRaw) {
                    const rec = s as Record<string, unknown>;
                    const p = (rec?.path as string) ?? "";
                    const c = (rec?.content as string) ?? "";
                    if (p || c) list.push({ path: p, content: c });
                  }
                } else if (obj.script_path != null || obj.script_content != null) {
                  list.push({
                    path: (obj.script_path as string) ?? "",
                    content: (obj.script_content as string) ?? "",
                  });
                }
              }
              filesByAdapter[adapterId] = list;
            }
          }
        } else {
          if (hook.compatible_agents.includes("claude-code")) {
            filesByAdapter["claude-code"] = [
              {
                path: defaultPaths["claude-code"],
                content: JSON.stringify({
                  hooks: {
                    [hook.event]: [{
                      matcher: hook.matcher || "*",
                      hooks: [{ type: "command", command: hook.command }],
                    }],
                  },
                }, null, 2),
              },
            ];
          }
          if (hook.compatible_agents.includes("cursor")) {
            const eventMap: Record<string, string> = {
              PreToolUse: "afterFileEdit",
              PostToolUse: "afterFileEdit",
              file_save: "afterFileEdit",
              Notification: "stop",
              stop: "stop",
              pre_command: "beforeShellExecution",
              beforeShellExecution: "beforeShellExecution",
            };
            const cursorEvent = eventMap[hook.event] || "afterFileEdit";
            filesByAdapter["cursor"] = [
              {
                path: defaultPaths["cursor"],
                content: JSON.stringify({
                  version: 1,
                  hooks: { [cursorEvent]: [{ command: hook.command }] },
                }, null, 2),
              },
            ];
          }
        }
        setHookAdapterFiles(filesByAdapter);
        setGenericEnvEntries(
          Object.entries((hook as Hook).env ?? {}).map(([k, v]) => ({
            _id: genericEnvIdCounter.current++,
            key: k,
            val: v,
          }))
        );
      } else if (capability.type === "plugin") {
        const plugin = capability as Plugin;
        setPluginInstallCmd(plugin.install_command);
        setGenericEnvEntries(
          Object.entries(plugin.env ?? {}).map(([k, v]) => ({
            _id: genericEnvIdCounter.current++,
            key: k,
            val: v,
          }))
        );
      } else if (capability.type === "custom") {
        const custom = capability as Custom;
        const configs: Record<string, Array<{ path: string; content: string }>> = {};
        for (const [adapterId, val] of Object.entries(custom.adapter_configs)) {
          const cfg = val as Record<string, unknown>;
          if (Array.isArray(cfg.files)) {
            configs[adapterId] = (cfg.files as Array<{ deploy_path?: string; content?: string }>).map(
              (f) => ({ path: f.deploy_path ?? "", content: f.content ?? "" })
            );
          } else if (cfg.deploy_path != null || cfg.content != null) {
            configs[adapterId] = [
              { path: (cfg.deploy_path as string) ?? "", content: (cfg.content as string) ?? "" },
            ];
          } else {
            configs[adapterId] = [{ path: "", content: "" }];
          }
        }
        setCustomAdapterConfigs(configs);
        setGenericEnvEntries(
          Object.entries((custom as Custom).env ?? {}).map(([k, v]) => ({
            _id: genericEnvIdCounter.current++,
            key: k,
            val: v,
          }))
        );
      }
    }
  }, [capability]);

  // Skills use relative paths (SKILL.md, scripts/helper.py) — adapters handle deployment paths.
  // Ensure first file is always SKILL.md for new skills.
  useEffect(() => {
    if (capType !== "skill" || capability) return;
    if (skillFiles.length > 0 && skillFiles[0].path === "SKILL.md") return;
    if (skillFiles.length === 1 && !skillFiles[0].path && !skillFiles[0].content) {
      setSkillFiles([{ path: "SKILL.md", content: "" }]);
    }
  }, [capType, capability, skillFiles]);

  const defaultHookPaths: Record<string, string> = {
    "claude-code": ".claude/settings.json",
    "cursor": ".cursor/hooks.json",
  };

  // Auto-populate hook adapter file list when adapters change (one empty file per adapter)
  useEffect(() => {
    if (capType !== "hook") return;
    setHookAdapterFiles((prev) => {
      const next = { ...prev };
      for (const adapterId of ["claude-code", "cursor"]) {
        if (adapters.includes(adapterId) && !(next[adapterId]?.length)) {
          next[adapterId] = [{ path: defaultHookPaths[adapterId] ?? "", content: "" }];
        }
      }
      for (const key of Object.keys(next)) {
        if (!adapters.includes(key)) delete next[key];
      }
      return next;
    });
  }, [adapters, capType]);

  // Auto-populate custom adapter configs when adapters change
  useEffect(() => {
    if (capType !== "custom") return;
    setCustomAdapterConfigs((prev) => {
      const next = { ...prev };
      for (const adapter of adapters) {
        if (!next[adapter]) {
          next[adapter] = [{ path: "", content: "" }];
        }
      }
      for (const key of Object.keys(next)) {
        if (!adapters.includes(key)) {
          delete next[key];
        }
      }
      return next;
    });
  }, [adapters, capType]);

  const displayId = capability?.id ?? "Assigned on create";

  const handleAdapterToggle = (adapter: string) => {
    setAdapters((prev) =>
      prev.includes(adapter) ? prev.filter((a) => a !== adapter) : [...prev, adapter]
    );
  };


  const handleRemoveEnvVar = (id: number) => {
    const removed = mcpEnvEntries.find((e) => e._id === id);
    setMcpEnvEntries((prev) => prev.filter((e) => e._id !== id));
    // Also remove from JSON env object
    if (removed?.key) {
      try {
        const data = JSON.parse(mcpJsonText);
        const servers = data.mcpServers ?? data;
        const firstKey = Object.keys(servers)[0];
        if (firstKey && servers[firstKey]?.env) {
          delete servers[firstKey].env[removed.key];
          // Remove empty env object
          if (Object.keys(servers[firstKey].env).length === 0) {
            delete servers[firstKey].env;
          }
          setMcpJsonText(JSON.stringify(data.mcpServers ? data : { mcpServers: servers }, null, 2));
        }
      } catch { /* JSON not valid, skip */ }
    }
  };

  const handleAddGenericEnvVar = () => {
    setGenericEnvEntries((prev) => [
      ...prev,
      {
        _id: genericEnvIdCounter.current++,
        key: `VAR_${prev.length + 1}`,
        val: { type: "string", label: "New Variable", required: false, value: "" },
      },
    ]);
  };
  const handleRemoveGenericEnvVar = (id: number) => {
    setGenericEnvEntries((prev) => prev.filter((e) => e._id !== id));
  };

  const handleAddSkillFile = () => {
    setSkillFiles((prev) => [...prev, { path: "", content: "" }]);
  };

  const handleRemoveSkillFile = (index: number) => {
    setSkillFiles((prev) => prev.filter((_, i) => i !== index));
  };

  const handleFetchGithubSkill = async () => {
    if (!skillGithubUrl.trim()) return;
    setSkillFetching(true);
    setSkillFetchError(null);
    setSkillFetchProgress(["Fetching from GitHub..."]);
    setSkillHasScriptsWarning(false);
    try {
      // Try to get GITHUB_TOKEN from Secrets Manager for private repo access
      let githubToken: string | undefined;
      try {
        const secret = await invoke<string | null>("get_secret", { key: "GITHUB_TOKEN" });
        if (secret) githubToken = secret;
      } catch { /* no token available, proceed without auth */ }
      const result: FetchedSkill = await fetchGithubSkill(skillGithubUrl.trim(), githubToken);
      // Populate form fields from fetched data
      if (result.name) setName(result.name);
      if (result.description) setDescription(result.description);
      if (result.license) setSkillLicense(result.license);
      if (result.allowed_tools) setSkillAllowedTools(result.allowed_tools.join(", "));
      if (result.model) setSkillModel(result.model);
      if (result.context) setSkillContext(result.context);
      if (result.agent) setSkillAgent(result.agent);
      if (result.argument_hint) setSkillArgumentHint(result.argument_hint);
      if (result.files.length > 0) {
        // Ensure SKILL.md is first
        const sorted = [...result.files].sort((a, b) => {
          if (a.path === "SKILL.md") return -1;
          if (b.path === "SKILL.md") return 1;
          return 0;
        });
        setSkillFiles(sorted);
      }
      if (result.has_scripts) {
        setSkillHasScriptsWarning(true);
      }
      if (result.model || result.context || result.agent || result.argument_hint || result.license) {
        setSkillAdvancedOpen(true);
      }
      setSkillFetchProgress(result.files.map((f) => `${f.path} \u2713`));
      // Switch to create tab to show populated form
      setSkillEditMode("create");
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.startsWith("RATE_LIMITED:")) {
        const resetTs = parseInt(msg.split(":")[1], 10);
        const now = Math.floor(Date.now() / 1000);
        const waitSecs = Math.max(0, resetTs - now);
        setSkillFetchError(`GitHub API rate limited. Retry in ${Math.ceil(waitSecs / 60)} minutes, or add a GitHub token in Settings.`);
      } else {
        setSkillFetchError(msg);
      }
    } finally {
      setSkillFetching(false);
    }
  };



  useEffect(() => {
    if (capType === "mcp" && !capability && mcpJsonText === "") {
      setMcpJsonText('{\n  "mcpServers": {\n    "my-server": {\n      "command": "npx",\n      "args": ["-y", "@example/mcp-server"]\n    }\n  }\n}');
    }
  }, [capType, capability]);

  const handleSubmit = async () => {
    if (!name.trim()) {
      setError("Name is required");
      return;
    }
    if (adapters.length === 0) {
      setError("Select at least one compatible adapter");
      return;
    }

    setSaving(true);
    setError(null);

    try {
      let id: string;
      if (capability?.id) {
        id = capability.id;
      } else {
        const authorId = await getAuthorId();
        const existingIds = new Set(capabilities.map((c) => c.id));
        id = await makeStableCompositeIdWithRetry(authorId, existingIds);
      }
      const tagList = tags.split(",").map((t) => t.trim()).filter(Boolean);
      
      const baseFields = {
        id,
        name: name.trim(),
        description: description.trim(),
        version,
        author: username,
        visibility: "private" as const,
        tags: tagList,
        compatible_agents: adapters,
      };

      let newCapability: UniversalCapability;

      switch (capType) {
        case "mcp": {
          // Parse JSON to extract server config
          let mcpTransportVal = "stdio";
          let mcpCommandVal = "";
          let mcpArgsVal: string[] = [];
          let mcpUrlVal = "";
          let mcpEnvVal: Record<string, EnvVariable> = {};
          try {
            const data = JSON.parse(mcpJsonText) as Record<string, unknown>;
            const servers = (data.mcpServers as Record<string, unknown>) ?? data;
            const firstKey = Object.keys(servers)[0];
            if (!firstKey) {
              setError("JSON must contain at least one MCP server");
              setSaving(false);
              return;
            }
            const s = servers[firstKey] as Record<string, unknown>;
            const hasUrl = s && typeof s.url === "string" && s.url.length > 0;
            const hasServerUrl = s && typeof s.serverUrl === "string" && (s.serverUrl as string).length > 0;
            mcpTransportVal = (hasUrl || hasServerUrl) ? (s.type === "sse" ? "sse" : "http") : "stdio";
            if (hasUrl || hasServerUrl) {
              mcpUrlVal = String(s.url ?? s.serverUrl ?? "");
            } else {
              mcpCommandVal = String(s.command ?? "");
              mcpArgsVal = Array.isArray(s.args) ? (s.args as string[]) : [];
            }
            // Build env: env entries provide actual values, JSON provides the key list
            // Both sources are kept in sync by the onChange handlers
            for (const entry of mcpEnvEntries) {
              if (entry.key.trim()) {
                mcpEnvVal[entry.key] = {
                  type: "secret",
                  label: `\${${entry.key}}`,
                  required: entry.val.required ?? false,
                  value: entry.val.label || undefined,
                };
              }
            }
            console.log("[MCP Save] transport:", mcpTransportVal, "command:", mcpCommandVal, "url:", mcpUrlVal);
            console.log("[MCP Save] env keys:", Object.keys(mcpEnvVal));
            console.log("[MCP Save] env values:", Object.entries(mcpEnvVal).map(([k, v]) => `${k}=${v.value ? "[set]" : "[empty]"}`));
          } catch (e) {
            setError("Invalid JSON: " + (e instanceof Error ? e.message : String(e)));
            setSaving(false);
            return;
          }
          if (mcpTransportVal === "stdio" && !mcpCommandVal) {
            setError("JSON must include a 'command' field for stdio transport");
            setSaving(false);
            return;
          }
          if (mcpTransportVal !== "stdio" && !mcpUrlVal) {
            setError("JSON must include a 'url' field for HTTP/SSE transport");
            setSaving(false);
            return;
          }
          newCapability = {
            type: "mcp",
            ...baseFields,
            transport: mcpTransportVal,
            command: mcpCommandVal,
            args: mcpArgsVal,
            url: mcpUrlVal,
            env: mcpEnvVal,
            always_allow: mcpAlwaysAllow.length > 0 ? mcpAlwaysAllow : undefined,
            disabled_tools: mcpDisabledTools.length > 0 ? mcpDisabledTools : undefined,
            tool_list: mcpDiscoveredTools.length > 0 ? mcpDiscoveredTools : undefined,
          } as McpServer;
          break;
        }
        case "rule":
          if (!ruleContent.trim()) {
            setError("Content is required for Rule");
            setSaving(false);
            return;
          }
          newCapability = {
            type: "rule",
            ...baseFields,
            scope: ruleScope,
            content: ruleContent,
            env: genericEnvAsRecord(),
          } as Rule;
          break;
        case "skill": {
          const validFiles = skillFiles.filter((f) => f.path.trim() && f.content.trim());
          if (validFiles.length === 0) {
            setError("At least one skill file is required");
            setSaving(false);
            return;
          }
          newCapability = {
            type: "skill",
            ...baseFields,
            scope: "",
            files: skillFiles,
            env: genericEnvAsRecord(),
            allowed_tools: skillAllowedTools.trim() ? skillAllowedTools.split(",").map((t) => t.trim()).filter(Boolean) : undefined,
            model: skillModel.trim() || undefined,
            context: skillContext || undefined,
            agent: skillAgent.trim() || undefined,
            argument_hint: skillArgumentHint.trim() || undefined,
            license: skillLicense.trim() || undefined,
          } as Skill;
          break;
        }
        case "hook": {
          const adapterConfigs: Record<string, unknown> = {};
          let hasConfig = false;
          for (const [adapterId, files] of Object.entries(hookAdapterFiles)) {
            const validFiles = files.filter((f) => f.path.trim() && f.content.trim());
            if (validFiles.length === 0) continue;
            const filesToSave = files.length > 0 ? files : validFiles;
            adapterConfigs[adapterId] = {
              files: filesToSave.map((f) => ({
                deploy_path: (f.path && f.path.trim()) || "",
                content: (f.content && f.content.trim()) || "",
              })),
            };
            hasConfig = true;
          }
          if (!hasConfig) {
            setError("At least one adapter must have at least one file with deploy path and content");
            setSaving(false);
            return;
          }
          let legacyEvent = "PreToolUse";
          let legacyCommand = "";
          let legacyMatcher = "*";
          const firstFiles = Object.values(hookAdapterFiles)[0];
          if (firstFiles?.length) {
            const firstContent = firstFiles[0]?.content?.trim() ?? "";
            if (firstContent.startsWith("{")) {
              try {
                const parsed = JSON.parse(firstContent) as Record<string, unknown>;
                const hooks = parsed?.hooks as Record<string, unknown[]> | undefined;
                if (hooks && typeof hooks === "object") {
                  const firstEvent = Object.keys(hooks)[0];
                  if (firstEvent) {
                    legacyEvent = firstEvent;
                    const firstEntry = hooks[firstEvent]?.[0] as Record<string, unknown> | undefined;
                    if (firstEntry) {
                      legacyCommand = (firstEntry.command as string) ?? "";
                      if (!legacyCommand && Array.isArray(firstEntry.hooks)) {
                        const inner = (firstEntry.hooks as Record<string, unknown>[])[0];
                        legacyCommand = (inner?.command as string) ?? "";
                      }
                      legacyMatcher = (firstEntry.matcher as string) ?? "*";
                    }
                  }
                }
              } catch {
                // ignore
              }
            }
          }
          newCapability = {
            type: "hook",
            ...baseFields,
            event: legacyEvent,
            matcher: legacyMatcher,
            command: legacyCommand,
            timeout_ms: 10000,
            env: genericEnvAsRecord(),
            adapter_configs: adapterConfigs,
          } as Hook;
          break;
        }
        case "plugin":
          if (!pluginInstallCmd.trim()) {
            setError("Install command is required for Plugin");
            setSaving(false);
            return;
          }
          newCapability = {
            type: "plugin",
            ...baseFields,
            install_command: pluginInstallCmd.trim(),
            config: {},
            env: genericEnvAsRecord(),
          } as Plugin;
          break;
        case "custom": {
          const adapterConfigs: Record<string, unknown> = {};
          let hasConfig = false;
          for (const [adapterId, files] of Object.entries(customAdapterConfigs)) {
            const validFiles = files.filter((f) => f.path.trim() && f.content.trim());
            if (validFiles.length === 0) continue;
            // Persist full files array so multiple file slots are saved (like hooks)
            const filesToSave = files.length > 0 ? files : validFiles;
            adapterConfigs[adapterId] = {
              files: filesToSave.map((f) => ({
                deploy_path: (f.path && f.path.trim()) || "",
                content: (f.content && f.content.trim()) || "",
              })),
            };
            hasConfig = true;
          }
          if (!hasConfig) {
            setError("At least one adapter must have at least one file with deploy path and content");
            setSaving(false);
            return;
          }
          newCapability = {
            type: "custom",
            ...baseFields,
            env: genericEnvAsRecord(),
            adapter_configs: adapterConfigs,
          } as Custom;
          break;
        }
      }

      console.log("[MCP Save] Saving capability:", JSON.stringify(newCapability, null, 2).substring(0, 500));
      await onSave(newCapability);
      console.log("[MCP Save] Save completed successfully");
    } catch (err) {
      console.error("[MCP Save] Save FAILED:", err);
      setError(err instanceof Error ? err.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  // Common fields block — extracted so it can be rendered in different positions
  // for skill type (after import panel) vs other types (before config section)
  const commonFieldsBlock = (
    <div className="space-y-4">
      <div>
        <label className="block text-sm font-medium text-text-secondary mb-1">
          Name <span className="text-accent-red">*</span>
        </label>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={`My Custom ${typeOptions.find(o => o.value === capType)?.label ?? 'Capability'}`}
          className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg text-text-primary focus:outline-none focus:border-accent-blue"
        />
        <p className="text-xs text-text-muted mt-1 font-mono">
          ID: {displayId}
        </p>
      </div>

      <div>
        <label className="block text-sm font-medium text-text-secondary mb-1">
          Description
        </label>
        <textarea
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          placeholder="What does this capability do?"
          rows={2}
          className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg text-text-primary focus:outline-none focus:border-accent-blue resize-none"
        />
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div>
          <label className="block text-sm font-medium text-text-secondary mb-1">
            Version
          </label>
          <input
            type="text"
            value={version}
            onChange={(e) => setVersion(e.target.value)}
            placeholder="1.0.0"
            className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg text-text-primary focus:outline-none focus:border-accent-blue"
          />
        </div>
        <div>
          <label className="block text-sm font-medium text-text-secondary mb-1">
            Tags (comma-separated)
          </label>
          <input
            type="text"
            value={tags}
            onChange={(e) => setTags(e.target.value)}
            placeholder="productivity, api"
            className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg text-text-primary focus:outline-none focus:border-accent-blue"
          />
        </div>
      </div>

      <div>
        <label className="block text-sm font-medium text-text-secondary mb-2">
          Compatible Adapters <span className="text-accent-red">*</span>
        </label>
        <div className="flex gap-3">
          {["claude-code", "cursor", "windsurf"].map((adapter) => (
            <label key={adapter} className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={adapters.includes(adapter)}
                onChange={() => handleAdapterToggle(adapter)}
                className="w-4 h-4 rounded border-border accent-accent-blue"
              />
              {getAdapterIconImg(adapter) ? (
                <img src={getAdapterIconImg(adapter)} alt="" className="w-4 h-4 object-contain" />
              ) : null}
              <span className="text-sm text-text-primary">
                {getAdapterName(adapter)}
              </span>
            </label>
          ))}
        </div>
      </div>
    </div>
  );

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-app-card border border-border rounded-xl w-full max-w-2xl max-h-[85vh] flex flex-col shadow-2xl">
        <div className="px-6 py-4 border-b border-border flex items-center justify-between">
          <h2 className="text-lg font-semibold text-text-primary">
            {capability ? "Edit Capability" : "New Capability"}
          </h2>
          <button onClick={onCancel} className="text-text-muted hover:text-text-primary">
            ✕
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {!capability && (
            <div className="flex gap-1 p-1 bg-app-bg rounded-lg">
              {typeOptions.map((opt) => (
                <button
                  key={opt.value}
                  onClick={() => setCapType(opt.value)}
                  className={`flex-1 px-3 py-2 rounded text-sm font-medium transition-colors ${
                    capType === opt.value
                      ? "bg-accent-blue text-white"
                      : "text-text-secondary hover:text-text-primary"
                  }`}
                >
                  {opt.label}
                </button>
              ))}
            </div>
          )}

          {/* Common fields: for non-skill types, shown before config section */}
          {capType !== "skill" && commonFieldsBlock}

          <div className="border-t border-border pt-6">
            <h3 className="text-sm font-semibold text-text-primary mb-4 uppercase tracking-wider">
              {capType.toUpperCase()} Configuration
            </h3>

            {capType === "mcp" && (
              <div className="space-y-4">
                {/* JSON Configuration */}
                <div>
                  <label className="block text-sm font-medium text-text-secondary mb-2">
                    JSON Configuration
                  </label>
                  <textarea
                    value={mcpJsonText}
                    onChange={(e) => {
                      const raw = e.target.value;
                      // Replace smart/curly quotes with straight quotes (macOS auto-substitution)
                      const smartQuotePattern = /[\u201C\u201D\u201E\u201F\u2033\u2036\u2018\u2019\u201A\u201B\u2032\u2035]/;
                      const fixed = smartQuotePattern.test(raw)
                        ? raw.replace(/[\u201C\u201D\u201E\u201F\u2033\u2036]/g, '"').replace(/[\u2018\u2019\u201A\u201B\u2032\u2035]/g, "'")
                        : raw;
                      setMcpJsonText(fixed);
                      // Sync: extract env keys from JSON and update env entries
                      try {
                        const data = JSON.parse(fixed);
                        const servers = data.mcpServers ?? data;
                        const firstKey = Object.keys(servers)[0];
                        if (firstKey) {
                          const jsonEnv = (servers[firstKey] as Record<string, unknown>).env as Record<string, string> | undefined;
                          const jsonKeys = new Set(jsonEnv ? Object.keys(jsonEnv) : []);
                          setMcpEnvEntries((prev) => {
                            // Add new keys from JSON that don't exist in entries
                            const existingKeys = new Set(prev.map((e) => e.key));
                            const toAdd = [...jsonKeys].filter((k) => !existingKeys.has(k));
                            // Remove entries whose keys no longer exist in JSON
                            const kept = prev.filter((e) => jsonKeys.has(e.key) || !e.key);
                            const added = toAdd.map((k) => ({
                              _id: envIdCounter.current++,
                              key: k,
                              val: { type: "secret" as const, label: "", required: false },
                            }));
                            return [...kept, ...added];
                          });
                        }
                      } catch { /* JSON not valid yet, skip sync */ }
                    }}
                    placeholder={'{\n  "mcpServers": {\n    "my-server": {\n      "command": "npx",\n      "args": ["-y", "@example/mcp-server"],\n      "env": {\n        "API_KEY": "${API_KEY}"\n      }\n    }\n  }\n}'}
                    rows={12}
                    className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg font-mono text-sm text-text-primary focus:outline-none focus:border-accent-blue resize-y"
                  />
                  <p className="text-xs text-text-muted mt-1">
                    Paste your MCP server config. Supports stdio, HTTP, SSE, and WebSocket transports.
                  </p>
                </div>

                {/* Environment Variables */}
                <div>
                  <div className="flex items-center justify-between mb-2">
                    <label className="text-sm font-medium text-text-secondary">
                      Environment Variables
                    </label>
                    <button
                      type="button"
                      onClick={() => {
                        const newKey = `VAR_${mcpEnvEntries.length + 1}`;
                        setMcpEnvEntries((prev) => [
                          ...prev,
                          {
                            _id: envIdCounter.current++,
                            key: newKey,
                            val: { type: "secret", label: "", required: false },
                          },
                        ]);
                        setMcpEnvMasked((prev) => ({ ...prev, [newKey]: true }));
                        // Auto-insert placeholder in JSON if possible
                        try {
                          const data = JSON.parse(mcpJsonText);
                          const servers = data.mcpServers ?? data;
                          const firstKey = Object.keys(servers)[0];
                          if (firstKey) {
                            if (!servers[firstKey].env) servers[firstKey].env = {};
                            servers[firstKey].env[newKey] = `\${${newKey}}`;
                            setMcpJsonText(JSON.stringify(data.mcpServers ? data : { mcpServers: servers }, null, 2));
                          }
                        } catch { /* JSON not valid yet, skip auto-insert */ }
                      }}
                      className="text-xs text-accent-blue hover:underline"
                    >
                      + Add Variable
                    </button>
                  </div>
                  {mcpEnvEntries.length === 0 && (
                    <p className="text-xs text-text-muted">No environment variables. Add variables for API keys and secrets.</p>
                  )}
                  {mcpEnvEntries.map((entry) => (
                    <div key={entry._id} className="flex items-center gap-2 mb-2">
                      <input
                        type="text"
                        value={entry.key}
                        onChange={(e) => {
                          const oldKey = entry.key;
                          const newKey = e.target.value;
                          setMcpEnvEntries((prev) =>
                            prev.map((ent) =>
                              ent._id === entry._id ? { ...ent, key: newKey } : ent
                            )
                          );
                          // Update masked state key
                          setMcpEnvMasked((prev) => {
                            const next = { ...prev };
                            if (oldKey in next) {
                              next[newKey] = next[oldKey];
                              delete next[oldKey];
                            }
                            return next;
                          });
                          // Sync key rename to JSON
                          try {
                            const data = JSON.parse(mcpJsonText);
                            const servers = data.mcpServers ?? data;
                            const fk = Object.keys(servers)[0];
                            if (fk && servers[fk]?.env && oldKey in servers[fk].env) {
                              delete servers[fk].env[oldKey];
                              if (newKey) servers[fk].env[newKey] = `\${${newKey}}`;
                              setMcpJsonText(JSON.stringify(data.mcpServers ? data : { mcpServers: servers }, null, 2));
                            }
                          } catch { /* skip */ }
                        }}
                        placeholder="KEY"
                        className="w-36 px-2 py-1.5 bg-app-bg border border-border rounded text-sm font-mono text-text-primary"
                      />
                      <input
                        type={mcpEnvMasked[entry.key] !== false ? "password" : "text"}
                        value={entry.val.label}
                        onChange={(e) => {
                          setMcpEnvEntries((prev) =>
                            prev.map((ent) =>
                              ent._id === entry._id
                                ? { ...ent, val: { ...ent.val, label: e.target.value } }
                                : ent
                            )
                          );
                        }}
                        placeholder="Value"
                        className="flex-1 px-2 py-1.5 bg-app-bg border border-border rounded text-sm font-mono text-text-primary"
                      />
                      <button
                        type="button"
                        onClick={() =>
                          setMcpEnvMasked((prev) => ({
                            ...prev,
                            [entry.key]: !(prev[entry.key] !== false),
                          }))
                        }
                        className="p-1.5 text-text-muted hover:text-text-primary rounded transition-colors"
                        title={mcpEnvMasked[entry.key] !== false ? "Show value" : "Hide value"}
                      >
                        {mcpEnvMasked[entry.key] !== false ? (
                          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" /></svg>
                        ) : (
                          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" /><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" /></svg>
                        )}
                      </button>
                      <button
                        type="button"
                        onClick={() => handleRemoveEnvVar(entry._id)}
                        className="text-text-muted hover:text-accent-red"
                      >
                        ✕
                      </button>
                    </div>
                  ))}
                </div>

                {/* Tool Access */}
                <div className="border border-border rounded-lg">
                  <div className="flex items-center justify-between px-3 py-2">
                    <div className="flex items-center gap-2">
                      <label className="text-sm font-medium text-text-secondary">Tool Access</label>
                      {mcpDiscoveredTools.length > 0 && (
                        <button
                          type="button"
                          onClick={() => {
                            const toolLines = mcpDiscoveredTools
                              .map((t) => `- ${t.name}: ${t.description}`)
                              .join("\n");
                            const appendText = `\n\nAvailable Tools:\n${toolLines}`;
                            setDescription((prev) =>
                              prev.includes("Available Tools:") ? prev : prev + appendText
                            );
                          }}
                          className="text-[10px] px-2 py-0.5 rounded bg-accent-blue/15 text-accent-blue border border-accent-blue/30 hover:bg-accent-blue/25 transition-colors"
                        >
                          Append in Description
                        </button>
                      )}
                    </div>
                    <button
                      type="button"
                      disabled={mcpToolsLoading}
                      onClick={async () => {
                        setMcpToolsLoading(true);
                        setMcpToolsError(null);
                        try {
                          // Parse command/args from JSON
                          const data = JSON.parse(mcpJsonText);
                          const servers = data.mcpServers ?? data;
                          const firstKey = Object.keys(servers)[0];
                          if (!firstKey) throw new Error("No server found in JSON");
                          const s = servers[firstKey] as Record<string, unknown>;
                          // Build env with actual values from entries
                          const envValues: Record<string, string> = {};
                          for (const entry of mcpEnvEntries) {
                            if (entry.key && entry.val.label) {
                              envValues[entry.key] = entry.val.label;
                            }
                          }
                          // Build headers if present
                          const hdrs: Record<string, string> = {};
                          if (s.headers && typeof s.headers === "object") {
                            for (const [hk, hv] of Object.entries(s.headers as Record<string, string>)) {
                              hdrs[hk] = String(hv);
                            }
                          }
                          // Use unified discovery — auto-detects transport
                          const tools = await discoverMcpTools({
                            transport: String(s.type ?? ""),
                            command: String(s.command ?? ""),
                            args: Array.isArray(s.args) ? (s.args as string[]) : [],
                            url: String(s.url ?? ""),
                            serverUrl: String(s.serverUrl ?? ""),
                            env: envValues,
                            headers: hdrs,
                          });
                          setMcpDiscoveredTools(tools);
                          // Only keep selections that still exist in discovered tools
                          const toolNames = new Set(tools.map((t) => t.name));
                          setMcpDisabledTools((prev) => prev.filter((t) => toolNames.has(t)));
                          setMcpAlwaysAllow((prev) => prev.filter((t) => toolNames.has(t)));
                        } catch (err: unknown) {
                          const msg = err instanceof Error ? err.message : String(err);
                          setMcpToolsError(msg);
                        } finally {
                          setMcpToolsLoading(false);
                        }
                      }}
                      className="text-xs text-accent-blue hover:underline disabled:opacity-50"
                    >
                      {mcpToolsLoading ? "Discovering..." : mcpDiscoveredTools.length > 0 ? "Re-discover" : "Discover Tools"}
                    </button>
                  </div>

                  {mcpToolsError && (
                    <div className="mx-3 mb-3 bg-red-500/10 border border-red-500/30 rounded p-2 text-xs text-red-400">
                      {mcpToolsError}
                    </div>
                  )}

                  {mcpToolsLoading && (
                    <div className="px-3 pb-3 text-xs text-text-muted">
                      Spawning server and discovering tools...
                    </div>
                  )}

                  {mcpDiscoveredTools.length > 0 && !mcpToolsLoading && (
                    <div className="border-t border-border px-3 py-2 space-y-1.5 max-h-60 overflow-y-auto">
                      {mcpDiscoveredTools.map((tool) => {
                        const isDisabled = mcpDisabledTools.includes(tool.name);
                        const isAutoApproved = mcpAlwaysAllow.includes(tool.name);
                        return (
                          <div key={tool.name} className="flex items-start gap-2 py-1">
                            <input
                              type="checkbox"
                              checked={!isDisabled}
                              onChange={(e) => {
                                if (e.target.checked) {
                                  setMcpDisabledTools((prev) => prev.filter((t) => t !== tool.name));
                                } else {
                                  setMcpDisabledTools((prev) => [...prev, tool.name]);
                                  setMcpAlwaysAllow((prev) => prev.filter((t) => t !== tool.name));
                                }
                              }}
                              className="mt-0.5 w-4 h-4 rounded border-border accent-accent-blue"
                            />
                            <div className="flex-1 min-w-0">
                              <div className="flex items-center gap-2">
                                <span className={`text-sm font-mono ${isDisabled ? "text-text-muted line-through" : "text-text-primary"}`}>
                                  {tool.name}
                                </span>
                                {!isDisabled && (
                                  <button
                                    type="button"
                                    onClick={() => {
                                      if (isAutoApproved) {
                                        setMcpAlwaysAllow((prev) => prev.filter((t) => t !== tool.name));
                                      } else {
                                        setMcpAlwaysAllow((prev) => [...prev, tool.name]);
                                      }
                                    }}
                                    className={`text-[9px] px-1.5 py-0.5 rounded border ${
                                      isAutoApproved
                                        ? "bg-green-500/15 text-green-400 border-green-500/30"
                                        : "bg-app-bg text-text-muted border-border hover:text-text-secondary"
                                    }`}
                                  >
                                    {isAutoApproved ? "auto-approved" : "auto-approve"}
                                  </button>
                                )}
                              </div>
                              {tool.description && (
                                <p className="text-[11px] text-text-muted truncate">{tool.description}</p>
                              )}
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  )}

                  {mcpDiscoveredTools.length === 0 && !mcpToolsLoading && !mcpToolsError && (
                    <div className="px-3 pb-3 text-xs text-text-muted">
                      Click "Discover Tools" to see available tools from this MCP server.
                    </div>
                  )}
                </div>
              </div>
            )}

            {capType === "rule" && (
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-text-secondary mb-1">
                    Scope
                  </label>
                  <select
                    value={ruleScope}
                    onChange={(e) => setRuleScope(e.target.value)}
                    className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg text-text-primary focus:outline-none focus:border-accent-blue"
                  >
                    <option value="project">Project</option>
                    <option value="global">Global</option>
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium text-text-secondary mb-1">
                    Content (Markdown) <span className="text-accent-red">*</span>
                  </label>
                  <textarea
                    value={ruleContent}
                    onChange={(e) => setRuleContent(e.target.value)}
                    placeholder="## Guidelines&#10;&#10;- Follow best practices..."
                    rows={8}
                    className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg text-text-primary font-mono text-sm focus:outline-none focus:border-accent-blue resize-none"
                  />
                </div>
              </div>
            )}

            {capType === "skill" && (
              <div className="space-y-4">
                {/* Tab: Create New / Import from URL */}
                <div className="flex gap-1 bg-app-bg p-1 rounded-lg">
                  <button
                    type="button"
                    onClick={() => setSkillEditMode("create")}
                    className={`flex-1 px-3 py-1.5 text-sm rounded-md transition-colors ${
                      skillEditMode === "create"
                        ? "bg-accent-blue text-white"
                        : "text-text-secondary hover:text-text-primary"
                    }`}
                  >
                    Create New
                  </button>
                  <button
                    type="button"
                    onClick={() => setSkillEditMode("import-url")}
                    className={`flex-1 px-3 py-1.5 text-sm rounded-md transition-colors ${
                      skillEditMode === "import-url"
                        ? "bg-accent-blue text-white"
                        : "text-text-secondary hover:text-text-primary"
                    }`}
                  >
                    Import from URL
                  </button>
                </div>

                {/* Import from URL panel */}
                {skillEditMode === "import-url" && (
                  <div className="space-y-3">
                    <p className="text-xs text-text-muted">
                      Paste a GitHub URL to a skill folder (e.g. https://github.com/anthropics/skills/tree/main/skills/pdf).
                      For private repos, add a <code className="font-mono text-[10px] bg-[#22232e] px-1 py-0.5 rounded">GITHUB_TOKEN</code> in Settings &gt; Secrets Manager.
                    </p>
                    <div className="flex gap-2">
                      <input
                        type="text"
                        value={skillGithubUrl}
                        onChange={(e) => setSkillGithubUrl(e.target.value)}
                        placeholder="https://github.com/owner/repo/tree/main/skills/my-skill"
                        className="flex-1 px-3 py-2 bg-app-bg border border-border rounded-lg text-sm font-mono text-text-primary focus:outline-none focus:border-accent-blue"
                      />
                      <button
                        type="button"
                        onClick={handleFetchGithubSkill}
                        disabled={skillFetching || !skillGithubUrl.trim()}
                        className="px-4 py-2 bg-accent-blue text-white text-sm rounded-lg hover:bg-accent-blue/90 disabled:opacity-50 disabled:cursor-not-allowed whitespace-nowrap"
                      >
                        {skillFetching ? "Fetching..." : "Fetch & Import"}
                      </button>
                    </div>
                    {skillFetchProgress.length > 0 && (
                      <div className="bg-app-bg border border-border rounded-lg p-3 text-xs font-mono text-text-secondary space-y-1">
                        {skillFetchProgress.map((line, i) => (
                          <div key={i}>{line}</div>
                        ))}
                      </div>
                    )}
                    {skillHasScriptsWarning && (
                      <div className="bg-yellow-500/10 border border-yellow-500/30 rounded-lg p-3 text-xs text-yellow-400">
                        This skill contains a scripts/ directory. Review contents before deploying.
                      </div>
                    )}
                    {skillFetchError && (
                      <div className="bg-red-500/10 border border-red-500/30 rounded-lg p-3 text-xs text-red-400">
                        {skillFetchError}
                      </div>
                    )}
                  </div>
                )}

                {/* Common fields: for skills, shown after import panel */}
                {commonFieldsBlock}

                {/* Create New form (also shown after import populates fields) */}
                {skillEditMode === "create" && (
                  <div className="space-y-4">
                    {/* Allowed Tools */}
                    <div>
                      <label className="block text-sm font-medium text-text-secondary mb-1">
                        Allowed Tools
                      </label>
                      <input
                        type="text"
                        value={skillAllowedTools}
                        onChange={(e) => setSkillAllowedTools(e.target.value)}
                        placeholder="Read, Glob, Grep, Bash"
                        className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg text-sm text-text-primary focus:outline-none focus:border-accent-blue"
                      />
                      <p className="text-xs text-text-muted mt-1">Comma-separated tool names the skill can use without permission</p>
                    </div>

                    {/* Advanced section (collapsible) */}
                    <div className="border border-border rounded-lg">
                      <button
                        type="button"
                        onClick={() => setSkillAdvancedOpen(!skillAdvancedOpen)}
                        className="w-full flex items-center justify-between px-3 py-2 text-sm text-text-secondary hover:text-text-primary"
                      >
                        <span>Advanced</span>
                        <span className="text-xs">{skillAdvancedOpen ? "\u25BE" : "\u25B8"}</span>
                      </button>
                      {skillAdvancedOpen && (
                        <div className="px-3 pb-3 space-y-3 border-t border-border pt-3">
                          <div>
                            <label className="block text-xs font-medium text-text-secondary mb-1">Model</label>
                            <input
                              type="text"
                              value={skillModel}
                              onChange={(e) => setSkillModel(e.target.value)}
                              placeholder="e.g. sonnet, opus, haiku, claude-sonnet-4-6"
                              className="w-full px-2 py-1.5 bg-app-bg border border-border rounded text-sm text-text-primary focus:outline-none focus:border-accent-blue"
                            />
                          </div>
                          <div>
                            <label className="block text-xs font-medium text-text-secondary mb-1">License</label>
                            <input
                              type="text"
                              value={skillLicense}
                              onChange={(e) => setSkillLicense(e.target.value)}
                              placeholder="e.g. MIT, Apache-2.0"
                              className="w-full px-2 py-1.5 bg-app-bg border border-border rounded text-sm text-text-primary focus:outline-none focus:border-accent-blue"
                            />
                          </div>
                          <div className="grid grid-cols-2 gap-3">
                            <div>
                              <label className="block text-xs font-medium text-text-secondary mb-1">Context</label>
                              <select
                                value={skillContext}
                                onChange={(e) => setSkillContext(e.target.value)}
                                className="w-full px-2 py-1.5 bg-app-bg border border-border rounded text-sm text-text-primary focus:outline-none focus:border-accent-blue"
                              >
                                <option value="">None</option>
                                <option value="fork">Fork (subagent)</option>
                              </select>
                            </div>
                            {skillContext === "fork" && (
                              <div>
                                <label className="block text-xs font-medium text-text-secondary mb-1">Agent</label>
                                <input
                                  type="text"
                                  value={skillAgent}
                                  onChange={(e) => setSkillAgent(e.target.value)}
                                  placeholder="e.g. code-reviewer"
                                  className="w-full px-2 py-1.5 bg-app-bg border border-border rounded text-sm text-text-primary focus:outline-none focus:border-accent-blue"
                                />
                              </div>
                            )}
                          </div>
                          <div>
                            <label className="block text-xs font-medium text-text-secondary mb-1">Argument Hint</label>
                            <input
                              type="text"
                              value={skillArgumentHint}
                              onChange={(e) => setSkillArgumentHint(e.target.value)}
                              placeholder="e.g. [file-path]"
                              className="w-full px-2 py-1.5 bg-app-bg border border-border rounded text-sm text-text-primary focus:outline-none focus:border-accent-blue"
                            />
                          </div>
                        </div>
                      )}
                    </div>

                    {/* Files section */}
                    <div>
                      <div className="flex items-center justify-between mb-2">
                        <label className="text-sm font-medium text-text-secondary">
                          Skill Files <span className="text-accent-red">*</span>
                        </label>
                        <button
                          type="button"
                          onClick={handleAddSkillFile}
                          className="text-xs text-accent-blue hover:underline"
                        >
                          + Add File
                        </button>
                      </div>
                      {skillFiles.map((file, index) => {
                        const isSkillMd = index === 0 && (file.path === "SKILL.md" || file.path === "skill.md" || file.path === "");
                        return (
                          <div key={index} className="border border-border rounded-lg p-3 mb-3">
                            <div className="flex items-center gap-2 mb-2">
                              {isSkillMd ? (
                                <div className="flex-1 flex items-center gap-2">
                                  <span className="px-2 py-1 bg-accent-blue/10 border border-accent-blue/20 rounded text-sm font-mono text-accent-blue">
                                    SKILL.md
                                  </span>
                                  <span className="text-[10px] text-text-muted">Entry point (locked)</span>
                                </div>
                              ) : (
                                <input
                                  type="text"
                                  value={file.path}
                                  onChange={(e) => {
                                    const newFiles = [...skillFiles];
                                    newFiles[index] = { ...newFiles[index], path: e.target.value };
                                    setSkillFiles(newFiles);
                                  }}
                                  placeholder="scripts/helper.py"
                                  className="flex-1 px-2 py-1 bg-app-bg border border-border rounded text-sm font-mono text-text-primary"
                                />
                              )}
                              {!isSkillMd && skillFiles.length > 1 && (
                                <button
                                  type="button"
                                  onClick={() => handleRemoveSkillFile(index)}
                                  className="text-text-muted hover:text-accent-red"
                                >
                                  ✕
                                </button>
                              )}
                            </div>
                            <textarea
                              value={file.content}
                              onChange={(e) => {
                                const newFiles = [...skillFiles];
                                newFiles[index] = { ...newFiles[index], content: e.target.value };
                                setSkillFiles(newFiles);
                              }}
                              placeholder={isSkillMd ? "# My Skill\n\nInstructions for the AI agent..." : "File content..."}
                              rows={isSkillMd ? 8 : 4}
                              className="w-full px-2 py-1 bg-app-bg border border-border rounded text-sm font-mono text-text-primary resize-none"
                            />
                          </div>
                        );
                      })}
                    </div>

                    {/* Deploy path preview per adapter */}
                    {adapters.length > 0 && name.trim() && (
                      <div className="border border-border/50 rounded-lg p-3 bg-app-bg/50">
                        <p className="text-xs font-medium text-text-secondary mb-2">Deploy paths</p>
                        <div className="space-y-1.5">
                          {adapters.includes("claude-code") && (
                            <div className="flex items-center gap-2">
                              <span className="text-[10px] font-medium text-accent-blue bg-accent-blue/10 px-1.5 py-0.5 rounded">Claude Code</span>
                              <code className="text-[11px] text-text-muted font-mono">
                                .claude/skills/{name.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "")}/
                              </code>
                            </div>
                          )}
                          {adapters.includes("cursor") && (
                            <div className="flex items-center gap-2">
                              <span className="text-[10px] font-medium text-accent-blue bg-accent-blue/10 px-1.5 py-0.5 rounded">Cursor</span>
                              <code className="text-[11px] text-text-muted font-mono">
                                .cursor/skills/{name.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "")}/
                              </code>
                            </div>
                          )}
                          {adapters.includes("windsurf") && (
                            <div className="flex items-center gap-2">
                              <span className="text-[10px] font-medium text-accent-blue bg-accent-blue/10 px-1.5 py-0.5 rounded">Windsurf</span>
                              <code className="text-[11px] text-text-muted font-mono">
                                .windsurf/skills/{name.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "")}/
                              </code>
                            </div>
                          )}
                        </div>
                        <p className="text-[10px] text-text-muted mt-2">
                          Same files deployed to each adapter with adapter-specific frontmatter.
                        </p>
                      </div>
                    )}
                  </div>
                )}
              </div>
            )}

            {capType === "hook" && (
              <div className="space-y-4">
                <p className="text-xs text-text-muted">
                  Specify deploy path and content for each file per adapter. Content is written as-is to the specified path.
                </p>

                {["claude-code", "cursor"].filter((a) => adapters.includes(a)).map((adapter) => {
                  const files = hookAdapterFiles[adapter] ?? [{ path: defaultHookPaths[adapter] ?? "", content: "" }];
                  const docsUrl =
                    adapter === "claude-code"
                      ? "https://docs.anthropic.com/en/docs/claude-code/hooks"
                      : "https://docs.cursor.com/context/hooks";
                  const adapterLabel = adapter === "claude-code" ? "Claude Code" : "Cursor";
                  return (
                    <div key={adapter} className="border border-border rounded-lg p-3 space-y-3">
                      <div className="flex items-center justify-between gap-2">
                        <span className="text-sm font-medium text-text-primary">{adapterLabel}</span>
                        <div className="flex items-center gap-3">
                          <a
                            href={docsUrl}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="text-xs text-accent-blue hover:underline"
                          >
                            Docs
                          </a>
                          <button
                            type="button"
                            onClick={() =>
                              setHookAdapterFiles((prev) => ({
                                ...prev,
                                [adapter]: [...(prev[adapter] ?? [{ path: "", content: "" }]), { path: defaultHookPaths[adapter] ?? "", content: "" }],
                              }))
                            }
                            className="text-xs text-accent-blue hover:underline"
                          >
                            + Add File
                          </button>
                        </div>
                      </div>
                      {files.map((file, fileIndex) => (
                        <div key={fileIndex} className="bg-app-bg rounded-lg p-2 space-y-2">
                          <div className="flex items-center justify-between gap-2">
                            <label className="block text-xs text-text-muted shrink-0">
                              Deploy Location <span className="text-accent-red">*</span>
                            </label>
                            {files.length > 1 && (
                              <button
                                type="button"
                                onClick={() => {
                                  setHookAdapterFiles((prev) => {
                                    const next = (prev[adapter] ?? []).filter((_, i) => i !== fileIndex);
                                    return { ...prev, [adapter]: next.length > 0 ? next : [{ path: defaultHookPaths[adapter] ?? "", content: "" }] };
                                  });
                                }}
                                className="text-xs text-accent-red hover:underline"
                              >
                                Remove
                              </button>
                            )}
                          </div>
                          <input
                            type="text"
                            value={file.path}
                            onChange={(e) => {
                              const next = [...(hookAdapterFiles[adapter] ?? [])];
                              next[fileIndex] = { ...next[fileIndex], path: e.target.value };
                              setHookAdapterFiles((prev) => ({ ...prev, [adapter]: next }));
                            }}
                            placeholder={adapter === "cursor" ? ".cursor/hooks.json" : ".claude/settings.json"}
                            className="w-full px-2 py-1.5 bg-app-card border border-border rounded text-sm font-mono text-text-primary focus:outline-none focus:border-accent-blue"
                          />
                          <div>
                            <label className="block text-xs text-text-muted mb-1">
                              Content <span className="text-accent-red">*</span>
                            </label>
                            <textarea
                              value={file.content}
                              onChange={(e) => {
                                const next = [...(hookAdapterFiles[adapter] ?? [])];
                                next[fileIndex] = { ...next[fileIndex], content: e.target.value };
                                setHookAdapterFiles((prev) => ({ ...prev, [adapter]: next }));
                              }}
                              rows={5}
                              placeholder="Raw content to write to the file..."
                              className="w-full px-3 py-2 bg-app-card border border-border rounded-lg text-text-primary font-mono text-sm focus:outline-none focus:border-accent-blue resize-none"
                            />
                          </div>
                        </div>
                      ))}
                    </div>
                  );
                })}

                {adapters.includes("windsurf") && (
                  <div className="border border-border/50 rounded-lg p-3 opacity-60">
                    <span className="text-sm text-text-muted">Windsurf does not support hooks.</span>
                  </div>
                )}

                {!adapters.includes("claude-code") && !adapters.includes("cursor") && (
                  <p className="text-sm text-text-muted">
                    Select Claude Code or Cursor above to configure hooks.
                  </p>
                )}
              </div>
            )}

            {capType === "plugin" && (
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-text-secondary mb-1">
                    Install Command <span className="text-accent-red">*</span>
                  </label>
                  <input
                    type="text"
                    value={pluginInstallCmd}
                    onChange={(e) => setPluginInstallCmd(e.target.value)}
                    placeholder="claude plugin install my-plugin"
                    className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg text-text-primary font-mono focus:outline-none focus:border-accent-blue"
                  />
                </div>
              </div>
            )}

            {capType === "custom" && (
              <div className="space-y-4">
                <p className="text-xs text-text-muted">
                  Specify deploy path and content for each file per adapter. Content is written as-is to the specified path.
                </p>

                {adapters.map((adapter) => {
                  const files = customAdapterConfigs[adapter] ?? [{ path: "", content: "" }];
                  return (
                    <div key={adapter} className="border border-border rounded-lg p-3 space-y-3">
                      <div className="flex items-center justify-between">
                        <span className="text-sm font-medium text-text-primary capitalize">
                          {adapter.replace("-", " ")}
                        </span>
                        <button
                          type="button"
                          onClick={() =>
                            setCustomAdapterConfigs((prev) => ({
                              ...prev,
                              [adapter]: [...(prev[adapter] ?? [{ path: "", content: "" }]), { path: "", content: "" }],
                            }))
                          }
                          className="text-xs text-accent-blue hover:underline"
                        >
                          + Add File
                        </button>
                      </div>
                      {files.map((file, fileIndex) => (
                        <div key={fileIndex} className="bg-app-bg rounded-lg p-2 space-y-2">
                          <div className="flex items-center justify-between gap-2">
                            <label className="block text-xs text-text-muted shrink-0">
                              Deploy Location <span className="text-accent-red">*</span>
                            </label>
                            {files.length > 1 && (
                              <button
                                type="button"
                                onClick={() => {
                                  setCustomAdapterConfigs((prev) => {
                                    const next = (prev[adapter] ?? []).filter((_, i) => i !== fileIndex);
                                    return { ...prev, [adapter]: next.length > 0 ? next : [{ path: "", content: "" }] };
                                  });
                                }}
                                className="text-xs text-accent-red hover:underline"
                              >
                                Remove
                              </button>
                            )}
                          </div>
                          <input
                            type="text"
                            value={file.path}
                            onChange={(e) => {
                              const next = [...(customAdapterConfigs[adapter] ?? [])];
                              next[fileIndex] = { ...next[fileIndex], path: e.target.value };
                              setCustomAdapterConfigs((prev) => ({ ...prev, [adapter]: next }));
                            }}
                            placeholder=".claude/my-config.json"
                            className="w-full px-2 py-1.5 bg-app-card border border-border rounded text-sm font-mono text-text-primary focus:outline-none focus:border-accent-blue"
                          />
                          <div>
                            <label className="block text-xs text-text-muted mb-1">
                              Content <span className="text-accent-red">*</span>
                            </label>
                            <textarea
                              value={file.content}
                              onChange={(e) => {
                                const next = [...(customAdapterConfigs[adapter] ?? [])];
                                next[fileIndex] = { ...next[fileIndex], content: e.target.value };
                                setCustomAdapterConfigs((prev) => ({ ...prev, [adapter]: next }));
                              }}
                              rows={5}
                              placeholder="Raw content to write to the file..."
                              className="w-full px-3 py-2 bg-app-card border border-border rounded-lg text-text-primary font-mono text-sm focus:outline-none focus:border-accent-blue resize-none"
                            />
                          </div>
                        </div>
                      ))}
                    </div>
                  );
                })}

                {adapters.length === 0 && (
                  <p className="text-sm text-text-muted">
                    Select at least one adapter above to configure.
                  </p>
                )}
              </div>
            )}

            {(capType === "rule" || capType === "skill" || capType === "hook" || capType === "plugin" || capType === "custom") && (
              <div className="mt-6 pt-4 border-t border-border">
                <div className="flex items-center justify-between mb-2">
                  <label className="text-sm font-medium text-text-secondary">
                    Environment Variables
                  </label>
                  <button
                    onClick={handleAddGenericEnvVar}
                    className="text-xs text-accent-blue hover:underline"
                  >
                    + Add Variable
                  </button>
                </div>
                <p className="text-xs text-text-muted mb-3">
                  Merged into the project&apos;s <code className="bg-app-bg px-1 rounded">.env</code> on deploy. Use in scripts via <code className="bg-app-bg px-1 rounded">process.env.KEY</code> or <code className="bg-app-bg px-1 rounded">os.environ.get(&quot;KEY&quot;)</code>.
                </p>
                {genericEnvEntries.map((entry) => (
                  <div key={entry._id} className="flex gap-2 mb-2 flex-wrap items-center">
                    <input
                      type="text"
                      value={entry.key}
                      onChange={(e) => {
                        setGenericEnvEntries((prev) =>
                          prev.map((ent) =>
                            ent._id === entry._id ? { ...ent, key: e.target.value } : ent
                          )
                        );
                      }}
                      placeholder="KEY"
                      className="w-32 px-2 py-1 bg-app-bg border border-border rounded text-sm font-mono text-text-primary"
                    />
                    <input
                      type="text"
                      value={entry.val.value ?? ""}
                      onChange={(e) => {
                        setGenericEnvEntries((prev) =>
                          prev.map((ent) =>
                            ent._id === entry._id
                              ? { ...ent, val: { ...ent.val, value: e.target.value } }
                              : ent
                          )
                        );
                      }}
                      placeholder="Value (written to .env)"
                      className="flex-1 min-w-[120px] px-2 py-1 bg-app-bg border border-border rounded text-sm font-mono text-text-primary"
                    />
                    <select
                      value={entry.val.type}
                      onChange={(e) => {
                        setGenericEnvEntries((prev) =>
                          prev.map((ent) =>
                            ent._id === entry._id
                              ? { ...ent, val: { ...ent.val, type: e.target.value } }
                              : ent
                          )
                        );
                      }}
                      className="px-2 py-1 bg-app-bg border border-border rounded text-sm text-text-primary"
                    >
                      <option value="string">string</option>
                      <option value="secret">secret</option>
                    </select>
                    <button
                      onClick={() => handleRemoveGenericEnvVar(entry._id)}
                      className="text-text-muted hover:text-accent-red"
                    >
                      ✕
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        {error && (
          <div className="px-6 py-2 bg-accent-red/10 border-t border-accent-red/20">
            <p className="text-sm text-accent-red">{error}</p>
          </div>
        )}

        <div className="px-6 py-4 border-t border-border flex items-center justify-end gap-3">
          <button
            onClick={onCancel}
            className="px-4 py-2 text-sm text-text-secondary hover:text-text-primary transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={saving}
            className="px-6 py-2 bg-accent-blue text-white text-sm font-medium rounded-lg hover:bg-accent-blue/90 transition-colors disabled:opacity-50"
          >
            {saving ? "Saving..." : capability ? "Update" : "Create"}
          </button>
        </div>
      </div>
    </div>
  );
}
