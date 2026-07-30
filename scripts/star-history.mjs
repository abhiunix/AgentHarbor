// Generates docs/assets/star-history{,-dark}.svg from the GitHub stargazers API.
// Zero dependencies; Node 18+. Run: node scripts/star-history.mjs
// GITHUB_TOKEN is optional locally, required in CI to avoid rate limits.

const REPO = "abhiunix/AgentHarbor";
const OUT = { light: "docs/assets/star-history.svg", dark: "docs/assets/star-history-dark.svg" };

const THEME = {
  light: { surface: "#fcfcfb", line: "#2a78d6", grid: "#e1e0d9", axis: "#c3c2b7", muted: "#898781", ink: "#52514e" },
  dark: { surface: "#1a1a19", line: "#3987e5", grid: "#2c2c2a", axis: "#383835", muted: "#898781", ink: "#c3c2b7" },
};

async function fetchStarDates() {
  const headers = { Accept: "application/vnd.github.star+json", "User-Agent": "agentharbor-star-chart" };
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  const dates = [];
  for (let page = 1; page <= 400; page++) {
    const res = await fetch(`https://api.github.com/repos/${REPO}/stargazers?per_page=100&page=${page}`, { headers });
    if (!res.ok) throw new Error(`GitHub API ${res.status}: ${await res.text()}`);
    const batch = await res.json();
    for (const s of batch) dates.push(new Date(s.starred_at));
    if (batch.length < 100) break;
  }
  return dates.sort((a, b) => a - b);
}

const fmt = (d, span) =>
  span > 1000 * 60 * 60 * 24 * 180
    ? d.toLocaleDateString("en-US", { month: "short", year: "numeric" })
    : d.toLocaleDateString("en-US", { month: "short", day: "numeric" });

function render(dates, t) {
  const W = 720, H = 320, M = { top: 28, right: 48, bottom: 36, left: 40 };
  const pw = W - M.left - M.right, ph = H - M.top - M.bottom;
  const now = new Date();
  const x0 = dates[0].getTime(), x1 = now.getTime(), span = Math.max(x1 - x0, 1);
  const yMax = Math.max(Math.ceil(dates.length * 1.15), 4);
  const X = (ms) => M.left + ((ms - x0) / span) * pw;
  const Y = (n) => M.top + ph - (n / yMax) * ph;

  // step-after path: stars are discrete events
  let d = `M ${X(dates[0].getTime()).toFixed(1)} ${Y(0).toFixed(1)}`;
  dates.forEach((dt, i) => {
    d += ` L ${X(dt.getTime()).toFixed(1)} ${Y(i).toFixed(1)} L ${X(dt.getTime()).toFixed(1)} ${Y(i + 1).toFixed(1)}`;
  });
  d += ` L ${X(x1).toFixed(1)} ${Y(dates.length).toFixed(1)}`;

  const yTicks = [...new Set([0, Math.round(yMax / 2), yMax])];
  const grid = yTicks
    .map((n) => `<line x1="${M.left}" y1="${Y(n).toFixed(1)}" x2="${M.left + pw}" y2="${Y(n).toFixed(1)}" stroke="${t.grid}" stroke-width="1"/>
  <text x="${M.left - 8}" y="${(Y(n) + 3.5).toFixed(1)}" text-anchor="end" fill="${t.muted}" font-size="11">${n}</text>`)
    .join("\n  ");
  const xTicks = [dates[0], new Date(x0 + span / 2), now]
    .map((dt, i) => `<text x="${X(dt.getTime()).toFixed(1)}" y="${H - 12}" text-anchor="${i === 0 ? "start" : i === 2 ? "end" : "middle"}" fill="${t.muted}" font-size="11">${fmt(dt, span)}</text>`)
    .join("\n  ");

  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-label="Cumulative GitHub stars for ${REPO}: ${dates.length} as of ${fmt(now, span)}">
  <rect width="${W}" height="${H}" fill="${t.surface}" rx="6"/>
  <g font-family="system-ui, -apple-system, 'Segoe UI', sans-serif">
  ${grid}
  <line x1="${M.left}" y1="${M.top + ph}" x2="${M.left + pw}" y2="${M.top + ph}" stroke="${t.axis}" stroke-width="1"/>
  ${xTicks}
  <path d="${d}" fill="none" stroke="${t.line}" stroke-width="2" stroke-linejoin="round"/>
  <circle cx="${X(x1).toFixed(1)}" cy="${Y(dates.length).toFixed(1)}" r="4" fill="${t.line}"/>
  <text x="${(X(x1) + 8).toFixed(1)}" y="${(Y(dates.length) + 4).toFixed(1)}" fill="${t.ink}" font-size="12" font-weight="600">${dates.length}</text>
  </g>
</svg>
`;
}

const dates = await fetchStarDates();
if (dates.length === 0) {
  console.error("No stargazers returned; leaving existing SVGs untouched.");
  process.exit(0);
}
const { writeFileSync } = await import("node:fs");
for (const [mode, path] of Object.entries(OUT)) {
  writeFileSync(path, render(dates, THEME[mode]));
  console.log(`${path}: ${dates.length} stars`);
}
