// 자동 업데이트용 latest.json을 만든다 (GitHub Releases에 설치 파일·서명과 함께 올린다).
//
//   cd app && node scripts/latest-json.mjs "릴리스 노트 한 줄"
//
// 서명(.sig)은 `TAURI_SIGNING_PRIVATE_KEY`를 주고 `npm run tauri build -- --bundles nsis`로 빌드하면
// 설치 파일 옆에 생긴다. 앱은 tauri.conf.json의 endpoints(…/releases/latest/download/latest.json)를
// 읽으므로, 릴리스는 사전 릴리스(Pre-release)가 아니어야 한다 (latest에서 빠진다).

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const app = join(dirname(fileURLToPath(import.meta.url)), "..");
const conf = JSON.parse(readFileSync(join(app, "src-tauri", "tauri.conf.json"), "utf8"));
const version = conf.version;
const dir = join(app, "..", "target", "release", "bundle", "nsis");
const installer = `${conf.productName}_${version}_x64-setup.exe`;
const signature = readFileSync(join(dir, `${installer}.sig`), "utf8").trim();
// GitHub는 올린 파일 이름의 공백을 점으로 바꾼다.
const asset = installer.replaceAll(" ", ".");

const latest = {
  version,
  notes: process.argv[2] ?? "",
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature,
      url: `https://github.com/iwtkmn0219/numpad-deck/releases/download/v${version}/${asset}`,
    },
  },
};

const out = join(dir, "latest.json");
writeFileSync(out, JSON.stringify(latest, null, 2) + "\n");
console.log(out);
