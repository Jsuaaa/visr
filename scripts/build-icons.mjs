#!/usr/bin/env node
// Regenerates the file icons from Miguel Solorio's Symbols icon theme for VS Code (MIT,
// https://github.com/miguelsolorio/vscode-symbols):
//
//   crates/ui/assets/icons/symbols/     the SVGs, as <id>.svg, and the theme's LICENSE.txt
//   crates/ui/src/file_icons/table.rs   the name, extension and folder tables, and the bytes
//
//   node scripts/build-icons.mjs [git ref]     default: the commit pinned below
//
// Needs node 18+ and tar; run by hand, never when building Visr. Both outputs are committed.
import { execFileSync } from "node:child_process";
import {
  mkdtempSync, readFileSync, writeFileSync, rmSync, mkdirSync, copyFileSync, cpSync, readdirSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

// A commit, not a tag: the repository's tags stop at 0.0.9.
const REF = process.argv[2] ?? "2c95b15b0a8ef95ac62855e3784a6024ee6e5d2e";
const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const ICONS = join(ROOT, "crates", "ui", "assets", "icons", "symbols");
const TABLE = join(ROOT, "crates", "ui", "src", "file_icons", "table.rs");

// The theme maps some extensions only through VS Code's language ids, which a file tree can't
// see. These are the extensions VS Code gives those languages; a real fileExtensions entry wins.
const LANGUAGE_EXTENSIONS = {
  php: ["php", "phtml", "php3", "php4", "php5", "phps"],
  vue: ["vue"],
  haskell: ["hs", "lhs"],
  elixir: ["ex", "exs"],
  erlang: ["erl", "hrl"],
  scala: ["scala", "sc"],
  dart: ["dart"],
  clojure: ["clj", "cljs", "cljc", "edn"],
  coffeescript: ["coffee", "cson"],
  fsharp: ["fs", "fsi", "fsx"],
  csharp: ["cs", "csx"],
  razor: ["cshtml", "razor"],
  perl: ["pl", "pm", "pod", "t"],
  r: ["r", "rhistory", "rprofile"],
  graphql: ["graphql", "gql"],
  pug: ["pug", "jade"],
  properties: ["properties", "cfg", "conf", "ini"],
  less: ["less"],
  sass: ["sass"],
  scss: ["scss"],
  stylus: ["styl"],
  tex: ["tex", "ltx", "sty", "cls"],
  julia: ["jl"],
  crystal: ["cr"],
  nix: ["nix"],
  terraform: ["tf", "tfvars"],
  swift: ["swift"],
  handlebars: ["hbs", "handlebars"],
  twig: ["twig"],
  haml: ["haml"],
  "objective-c": ["m"],
  "objective-cpp": ["mm"],
  powershell: ["ps1", "psm1", "psd1"],
  makefile: ["mk", "mak"],
  dockerfile: ["dockerfile", "containerfile"],
};
const LANGUAGE_FILE_NAMES = {
  makefile: ["makefile", "gnumakefile"],
  dockerfile: ["dockerfile", "containerfile"],
};

// A Rust string literal; the names are plain ASCII, anything else is a mistake to look at.
function literal(text) {
  if (!/^[\x20-\x7e]*$/.test(text)) throw new Error(`not plain ASCII: ${JSON.stringify(text)}`);
  return `"${text.replace(/[\\"]/g, "\\$&")}"`;
}

// Sorted by key, which is what the lookups' binary search needs.
function pairs(name, table) {
  const rows = Object.entries(table)
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([key, id]) => `  (${literal(key)}, ${literal(id)}),\n`);
  return `#[rustfmt::skip]\npub const ${name}: &[(&str, &str)] = &[\n${rows.join("")}];\n`;
}

const tmp = mkdtempSync(join(tmpdir(), "symbols-icons-"));
try {
  const res = await fetch(`https://codeload.github.com/miguelsolorio/vscode-symbols/tar.gz/${REF}`);
  if (!res.ok) throw new Error(`download failed: ${res.status} ${res.statusText}`);
  writeFileSync(join(tmp, "src.tgz"), Buffer.from(await res.arrayBuffer()));
  execFileSync("tar", ["xzf", "src.tgz"], { cwd: tmp });
  const repo = join(tmp, readdirSync(tmp).find((name) => name !== "src.tgz"));
  const manifest = JSON.parse(readFileSync(join(repo, "src", "symbol-icon-theme.json"), "utf8"));
  const defs = manifest.iconDefinitions;
  const languages = manifest.languageIds ?? {};

  // VS Code matches names case-insensitively.
  const lower = (map = {}) =>
    Object.fromEntries(Object.entries(map).map(([key, id]) => [key.toLowerCase(), id]));
  function withLanguages(table, extra) {
    const out = {};
    for (const [language, keys] of Object.entries(extra)) {
      const id = languages[language];
      if (id) for (const key of keys) out[key] = id;
    }
    return { ...out, ...table };
  }

  const data = {
    file: manifest.file,
    folder: manifest.folder,
    fileNames: withLanguages(lower(manifest.fileNames), LANGUAGE_FILE_NAMES),
    fileExtensions: withLanguages(lower(manifest.fileExtensions), LANGUAGE_EXTENSIONS),
    folderNames: lower(manifest.folderNames),
  };

  const used = new Set([data.file, data.folder]);
  for (const table of [data.fileNames, data.fileExtensions, data.folderNames]) {
    for (const id of Object.values(table)) used.add(id);
  }
  const missing = [...used].filter((id) => !defs[id]);
  if (missing.length) throw new Error(`theme names icons it does not define: ${missing.join(", ")}`);
  // Ids become file names and Rust paths; reject any that could escape or break them.
  const unsafe = [...used].filter((id) => !/^[a-z0-9_-]+$/i.test(id));
  if (unsafe.length) throw new Error(`unsafe icon ids: ${unsafe.join(", ")}`);

  // Copy into a staging folder first so a failed copy leaves the current icons untouched.
  const ids = [...used].sort();
  const staged = join(tmp, "icons");
  mkdirSync(staged);
  for (const id of ids) copyFileSync(join(repo, "src", defs[id].iconPath), join(staged, `${id}.svg`));
  copyFileSync(join(repo, "LICENSE"), join(staged, "LICENSE.txt"));

  // Sorted by the path the table is keyed by: "folder.svg" comes after "folder-assets.svg".
  const byPath = [...ids].sort((a, b) => {
    const [x, y] = [`${a}.svg`, `${b}.svg`];
    return x < y ? -1 : x > y ? 1 : 0;
  });
  const symbols = byPath
    .map((id) =>
      `  ("icons/symbols/${id}.svg", include_bytes!("../../assets/icons/symbols/${id}.svg")),\n`)
    .join("");
  const table =
    `// @generated by scripts/build-icons.mjs from vscode-symbols ${REF} (MIT). Do not edit.\n\n` +
    `pub const FILE: &str = ${literal(data.file)};\n` +
    `pub const FOLDER: &str = ${literal(data.folder)};\n\n` +
    `${pairs("FILE_NAMES", data.fileNames)}\n` +
    `${pairs("EXTENSIONS", data.fileExtensions)}\n` +
    `${pairs("FOLDER_NAMES", data.folderNames)}\n` +
    `#[rustfmt::skip]\npub const SYMBOLS: &[(&str, &[u8])] = &[\n${symbols}];\n`;

  rmSync(ICONS, { recursive: true, force: true });
  cpSync(staged, ICONS, { recursive: true });
  mkdirSync(dirname(TABLE), { recursive: true });
  writeFileSync(TABLE, table);
  console.log(
    `vscode-symbols ${REF.slice(0, 12)}: ${ids.length} icons, ` +
      `${Object.keys(data.fileNames).length} file names, ` +
      `${Object.keys(data.fileExtensions).length} extensions, ` +
      `${Object.keys(data.folderNames).length} folder names`,
  );
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
