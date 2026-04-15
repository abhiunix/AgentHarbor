const fs = require("fs");
const path = require("path");

const root = path.join(__dirname, "..");

const tauriConfPath = path.join(root, "src-tauri", "tauri.conf.json");
const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, "utf-8"));

const parts = tauriConf.version.split(".").map(Number);
parts[2] += 1;
const newVersion = parts.join(".");

tauriConf.version = newVersion;
fs.writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + "\n");

const pkgPath = path.join(root, "package.json");
const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf-8"));
pkg.version = newVersion;
fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");

const cargoPath = path.join(root, "src-tauri", "Cargo.toml");
let cargo = fs.readFileSync(cargoPath, "utf-8");
cargo = cargo.replace(/^version = ".*"/m, `version = "${newVersion}"`);
fs.writeFileSync(cargoPath, cargo);

console.log(`Version bumped to ${newVersion}`);
