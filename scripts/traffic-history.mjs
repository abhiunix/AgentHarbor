// Collects GitHub traffic stats (views, clones, referrers, popular paths) into
// traffic/data.json and renders traffic/chart{,-dark}.svg + traffic/README.md.
// GitHub only retains 14 days of traffic; running this daily preserves history.
// Zero dependencies; Node 18+. Run: node scripts/traffic-history.mjs
// Requires TRAFFIC_PAT (fine-grained PAT with Administration:read) or a classic
// repo-scope GITHUB_TOKEN; the default Actions token cannot read traffic.

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";

const REPO = "abhiunix/AgentHarbor";
const DIR = "traffic";
const DATA = `${DIR}/data.json`;
const OUT = { light: `${DIR}/chart.svg`, dark: `${DIR}/chart-dark.svg` };

const THEME = {
  light: { surface: "#fcfcfb", views: "#2a78d6", clones: "#2f9e63", grid: "#e1e0d9", axis: "#c3c2b7", muted: "#898781", ink: "#52514e" },
  dark: { surface: "#1a1a19", views: "#3987e5", clones: "#3cb977", grid: "#2c2c2a", axis: "#383835", muted: "#898781", ink: "#c3c2b7" },
};

const token = process.env.TRAFFIC_PAT || process.env.GITHUB_TOKEN;
if (!token) {
  console.error("TRAFFIC_PAT or GITHUB_TOKEN is required (traffic API needs push/admin read access).");
  process.exit(1);
}

async function api(path) {
  const res = await fetch(`https://api.github.com/repos/${REPO}${path}`, {
    headers: { Authorization: `Bearer ${token}`, "User-Agent": "agentharbor-traffic-history", Accept: "application/vnd.github+json" },
  });
  if (!res.ok) throw new Error(`GitHub API ${res.status} for ${path}: ${await res.text()}`);
  return res.json();
}

function loadData() {
  try {
    return JSON.parse(readFileSync(DATA, "utf8"));
  } catch {
    return { views: {}, clones: {}, referrers: {}, paths: {}, repo: {} };
  }
}

// The most recent day in the API window is partial, so keep the max seen per date.
function mergeDaily(store, buckets) {
  for (const b of buckets) {
    const date = b.timestamp.slice(0, 10);
    const prev = store[date];
    if (!prev || b.count >= prev.count) store[date] = { count: b.count, uniques: b.uniques };
  }
}

const [views, clones, referrers, paths, repoInfo] = await Promise.all([
  api("/traffic/views"),
  api("/traffic/clones"),
  api("/traffic/popular/referrers"),
  api("/traffic/popular/paths"),
  api(""),
]);

const data = loadData();
const today = new Date().toISOString().slice(0, 10);
mergeDaily(data.views, views.views);
mergeDaily(data.clones, clones.clones);
data.referrers[today] = referrers.map((r) => ({ referrer: r.referrer, count: r.count, uniques: r.uniques }));
data.paths[today] = paths.map((p) => ({ path: p.path, count: p.count, uniques: p.uniques }));
data.repo[today] = { stars: repoInfo.stargazers_count, forks: repoInfo.forks_count, issues: repoInfo.open_issues_count };

mkdirSync(DIR, { recursive: true });
writeFileSync(DATA, `${JSON.stringify(data, null, 2)}\n`);

// ---- chart ----

const dates = [...new Set([...Object.keys(data.views), ...Object.keys(data.clones)])].sort();
const series = [
  { key: "views", label: "Views", store: data.views, color: (t) => t.views, dash: "" },
  { key: "uviews", label: "Unique visitors", store: data.views, color: (t) => t.views, dash: "4 3", field: "uniques" },
  { key: "clones", label: "Clones", store: data.clones, color: (t) => t.clones, dash: "" },
  { key: "uclones", label: "Unique cloners", store: data.clones, color: (t) => t.clones, dash: "4 3", field: "uniques" },
];

const val = (s, date) => s.store[date]?.[s.field ?? "count"] ?? 0;
const fmt = (iso) => new Date(`${iso}T00:00:00Z`).toLocaleDateString("en-US", { month: "short", day: "numeric", timeZone: "UTC" });

function render(t) {
  const W = 720, H = 340, M = { top: 30, right: 20, bottom: 58, left: 44 };
  const pw = W - M.left - M.right, ph = H - M.top - M.bottom;
  const yMax = Math.max(4, Math.ceil(Math.max(...dates.flatMap((d) => series.map((s) => val(s, d)))) * 1.15));
  const X = (i) => M.left + (dates.length > 1 ? (i / (dates.length - 1)) * pw : pw / 2);
  const Y = (n) => M.top + ph - (n / yMax) * ph;

  const lines = series
    .map((s) => {
      const d = dates.map((date, i) => `${i === 0 ? "M" : "L"} ${X(i).toFixed(1)} ${Y(val(s, date)).toFixed(1)}`).join(" ");
      return `<path d="${d}" fill="none" stroke="${s.color(t)}" stroke-width="2" stroke-linejoin="round"${s.dash ? ` stroke-dasharray="${s.dash}"` : ""}/>`;
    })
    .join("\n  ");

  const yTicks = [...new Set([0, Math.round(yMax / 2), yMax])];
  const grid = yTicks
    .map((n) => `<line x1="${M.left}" y1="${Y(n).toFixed(1)}" x2="${M.left + pw}" y2="${Y(n).toFixed(1)}" stroke="${t.grid}" stroke-width="1"/>
  <text x="${M.left - 8}" y="${(Y(n) + 3.5).toFixed(1)}" text-anchor="end" fill="${t.muted}" font-size="11">${n}</text>`)
    .join("\n  ");

  const tickIdx = [...new Set([0, Math.floor((dates.length - 1) / 2), dates.length - 1])];
  const xTicks = tickIdx
    .map((i) => `<text x="${X(i).toFixed(1)}" y="${M.top + ph + 18}" text-anchor="${i === 0 ? "start" : i === dates.length - 1 ? "end" : "middle"}" fill="${t.muted}" font-size="11">${fmt(dates[i])}</text>`)
    .join("\n  ");

  const legend = series
    .map((s, i) => {
      const lx = M.left + i * 168;
      const ly = H - 16;
      return `<line x1="${lx}" y1="${ly - 4}" x2="${lx + 22}" y2="${ly - 4}" stroke="${s.color(t)}" stroke-width="2"${s.dash ? ` stroke-dasharray="${s.dash}"` : ""}/>
  <text x="${lx + 28}" y="${ly}" fill="${t.ink}" font-size="11">${s.label}</text>`;
    })
    .join("\n  ");

  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-label="Daily GitHub traffic for ${REPO}, ${fmt(dates[0])} to ${fmt(dates.at(-1))}">
  <rect width="${W}" height="${H}" fill="${t.surface}" rx="6"/>
  <g font-family="system-ui, -apple-system, 'Segoe UI', sans-serif">
  ${grid}
  <line x1="${M.left}" y1="${M.top + ph}" x2="${M.left + pw}" y2="${M.top + ph}" stroke="${t.axis}" stroke-width="1"/>
  ${xTicks}
  ${lines}
  ${legend}
  </g>
</svg>
`;
}

for (const [mode, path] of Object.entries(OUT)) writeFileSync(path, render(THEME[mode]));

// ---- summary ----

const sum = (store, field) => Object.values(store).reduce((a, v) => a + v[field], 0);
const latestRepo = data.repo[today];
const table = (rows, header) =>
  [`| ${header.join(" | ")} |`, `| ${header.map(() => "---").join(" | ")} |`, ...rows.map((r) => `| ${r.join(" | ")} |`)].join("\n");

const md = `# Traffic history

Collected daily from the GitHub Traffic API, which only retains 14 days; this file and \`data.json\` preserve the full record since collection began. Last updated ${today}.

![Daily traffic chart](./chart.svg#gh-light-mode-only)
![Daily traffic chart](./chart-dark.svg#gh-dark-mode-only)

## Totals since collection began (${dates[0]})

${table(
  [
    ["Views", sum(data.views, "count"), sum(data.views, "uniques")],
    ["Clones", sum(data.clones, "count"), sum(data.clones, "uniques")],
  ],
  ["Metric", "Total", "Unique (sum of daily)"]
)}

Stars ${latestRepo.stars} | Forks ${latestRepo.forks} | Open issues ${latestRepo.issues}

## Referrers (last 14 days)

${table(data.referrers[today].map((r) => [r.referrer, r.count, r.uniques]), ["Site", "Views", "Unique visitors"])}

## Popular content (last 14 days)

${table(data.paths[today].map((p) => [p.path.replace(/^\/abhiunix\/AgentHarbor/, "") || "/", p.count, p.uniques]), ["Path", "Views", "Unique visitors"])}
`;

writeFileSync(`${DIR}/README.md`, md);
console.log(`${DATA}: ${dates.length} days, ${sum(data.views, "count")} views, ${sum(data.clones, "count")} clones`);
