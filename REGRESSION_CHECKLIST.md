# AgentHarbor Regression Test Checklist

Use this checklist for manual regression runs. Fill **Pass/Fail**, **Build/OS** (e.g. macOS 14, Tauri dev), and **Notes** per run.

**Run order**: B1, B2 first. Then N1–N12, L1–L9, A1–A10, P1–P8, D1–D12, R1–R11, G1–G4, S1–S9, T1–T5. B3–B4 after S5/S6 and D7/D11/D12.

## Run automated checks (B1 + B2)

From the `agentharbor` directory:

```bash
npm run test:regression
```

This runs `npm run build` (B2) then `cargo test` in `src-tauri` (B1). Both must pass.

---

## Latest automated run

| ID | Test             | Pass/Fail | Build/OS | Notes                          |
|----|------------------|-----------|----------|---------------------------------|
| B1 | Rust unit tests  | Pass      | macOS    | 118 passed (run via test:regression) |
| B2 | TypeScript build | Pass      | —        | tsc && vite build OK            |

*N1–T5 and B3–B4 require manual or E2E execution.*

---

## 1. App startup and navigation

| ID   | Test                      | Pass/Fail | Build/OS | Notes |
|------|---------------------------|-----------|----------|--------|
| N1   | App launches              |           |          |        |
| N2   | Default route (Registry)  |           |          |        |
| N3   | Sidebar – Library filters  |           |          |        |
| N4   | Sidebar – Presets expand  |           |          |        |
| N5   | Sidebar – Preset link     |           |          |        |
| N6   | Sidebar – Projects         |           |          |        |
| N7   | Sidebar – Global Config   |           |          |        |
| N8   | Sidebar – Settings        |           |          |        |
| N9   | Header – Search (Registry)|           |          |        |
| N10  | Header – Search (Agents)  |           |          |        |
| N11  | Header – New menu         |           |          |        |
| N12  | Header – Deploy           |           |          |        |

---

## 2. Library (Registry) – capabilities

| ID   | Test                      | Pass/Fail | Build/OS | Notes |
|------|---------------------------|-----------|----------|--------|
| L1   | Load capabilities         |           |          |        |
| L2   | Filter by type            |           |          |        |
| L3   | Open capability detail    |           |          |        |
| L4   | Deploy from detail        |           |          |        |
| L5   | New capability            |           |          |        |
| L6   | Edit custom capability    |           |          |        |
| L7   | Delete custom capability  |           |          |        |
| L8   | Save as preset            |           |          |        |
| L9   | Copy JSON                 |           |          |        |

---

## 3. Agents

| ID   | Test                         | Pass/Fail | Build/OS | Notes |
|------|------------------------------|-----------|----------|--------|
| A1   | Load agents                  |           |          |        |
| A2   | Open agent detail            |           |          |        |
| A3   | Deploy agent                 |           |          |        |
| A4   | New agent                    |           |          |        |
| A5   | Edit agent                   |           |          |        |
| A6   | Delete agent                 |           |          |        |
| A7   | Agent deploy – project       |           |          |        |
| A8   | Agent deploy – preview       |           |          |        |
| A9   | Agent deploy – execute       |           |          |        |
| A10  | Agent deploy – no project    |           |          |        |

---

## 4. Presets

| ID   | Test                      | Pass/Fail | Build/OS | Notes |
|------|---------------------------|-----------|----------|--------|
| P1   | List presets              |           |          |        |
| P2   | Open preset               |           |          |        |
| P3   | Add capabilities          |           |          |        |
| P4   | Remove capability         |           |          |        |
| P5   | Deploy preset             |           |          |        |
| P6   | Delete preset (custom)    |           |          |        |
| P7   | Bundled preset            |           |          |        |
| P8   | Preset not found          |           |          |        |

---

## 5. Deploy wizard (generic)

| ID   | Test                         | Pass/Fail | Build/OS | Notes |
|------|------------------------------|-----------|----------|--------|
| D1   | Open from Header             |           |          |        |
| D2   | Open with pre-selection      |           |          |        |
| D3   | Project step                 |           |          |        |
| D4   | Adapter selection            |           |          |        |
| D5   | Select capabilities/agents   |           |          |        |
| D6   | Preview step                 |           |          |        |
| D7   | Execute deploy               |           |          |        |
| D8   | Backup on deploy             |           |          |        |
| D9   | Deploy failure               |           |          |        |
| D10  | Close wizard                 |           |          |        |
| D11  | Deployed paths – Claude Code |           |          |        |
| D12  | Deployed paths – Cursor      |           |          |        |

---

## 6. Projects

| ID   | Test                    | Pass/Fail | Build/OS | Notes |
|------|-------------------------|-----------|----------|--------|
| R1   | Project list            |           |          |        |
| R2   | Add project             |           |          |        |
| R3   | Remove project          |           |          |        |
| R4   | Project detail          |           |          |        |
| R5   | Redeploy from project   |           |          |        |
| R6   | Open in Finder/Terminal|           |          |        |
| R7   | Drift indicator         |           |          |        |
| R8   | Drift review – Accept   |           |          |        |
| R9   | Drift review – Restore  |           |          |        |
| R10  | Agent memory (project)  |           |          |        |
| R11  | Backups (project)       |           |          |        |

---

## 7. Global Config

| ID   | Test                 | Pass/Fail | Build/OS | Notes |
|------|----------------------|-----------|----------|--------|
| G1   | Load global config   |           |          |        |
| G2   | MCP list             |           |          |        |
| G3   | No config            |           |          |        |
| G4   | Global agent memory  |           |          |        |

---

## 8. Settings

| ID   | Test                   | Pass/Fail | Build/OS | Notes |
|------|------------------------|-----------|----------|--------|
| S1   | General                |           |          |        |
| S2   | Registry               |           |          |        |
| S3   | Sync now               |           |          |        |
| S4   | Deploy                 |           |          |        |
| S5   | Export private data    |           |          |        |
| S6   | Import private data    |           |          |        |
| S7   | Import invalid file    |           |          |        |
| S8   | Secrets                |           |          |        |
| S9   | Backup cleanup         |           |          |        |

---

## 9. Tray and app lifecycle

| ID   | Test              | Pass/Fail | Build/OS | Notes |
|------|-------------------|-----------|----------|--------|
| T1   | Tray – Show       |           |          |        |
| T2   | Tray – Deploy     |           |          |        |
| T3   | Tray – Sync       |           |          |        |
| T4   | Close behavior    |           |          |        |
| T5   | Quit              |           |          |        |

---

## 10. Backend and integration

| ID   | Test                  | Pass/Fail | Build/OS | Notes |
|------|-----------------------|-----------|----------|--------|
| B1   | Rust unit tests       |           |          | `cargo test` |
| B2   | TypeScript build      |           |          | `npm run build` |
| B3   | Import/export private |           |          |        |
| B4   | Deploy file layout    |           |          |        |

---

## Quick commands

- **B1**: `cd agentharbor/src-tauri && cargo test`
- **B2**: `cd agentharbor && npm run build` (or `npx tsc --noEmit && npm run build`)
