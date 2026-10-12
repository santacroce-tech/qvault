// Checks that every internal link, image and stylesheet in the built site
// resolves to a file in site/dist (or the directory given as the first
// argument), and that every #fragment points at an existing id. External
// links are listed but not fetched. Exits non-zero on any broken link.
//
//   node site/check-links.mjs [distDir]

import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const DIST = resolve(process.argv[2] ?? join(dirname(fileURLToPath(import.meta.url)), "dist"));

const walk = (dir) =>
  readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });

const html = walk(DIST).filter((p) => p.endsWith(".html"));
const ids = new Map(html.map((p) => [p, new Set([...readFileSync(p, "utf8").matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]))]));

let broken = 0;
const external = new Set();
for (const page of html) {
  const src = readFileSync(page, "utf8");
  for (const [, attr, url] of src.matchAll(/\s(href|src)="([^"]*)"/g)) {
    if (/^(https?:|mailto:)/.test(url)) {
      external.add(url);
      continue;
    }
    if (/^[a-z]+:/i.test(url) || url.startsWith("//")) {
      console.error(`${relative(DIST, page)}: unexpected scheme ${attr}="${url}"`);
      broken++;
      continue;
    }
    const [path, frag] = url.split("#");
    let target = path ? resolve(dirname(page), path) : page;
    if (path && (path.endsWith("/") || (existsSync(target) && statSync(target).isDirectory()))) target = join(target, "index.html");
    if (!target.startsWith(DIST) || !existsSync(target)) {
      console.error(`${relative(DIST, page)}: missing ${attr}="${url}"`);
      broken++;
      continue;
    }
    if (frag && target.endsWith(".html") && !ids.get(target)?.has(frag)) {
      console.error(`${relative(DIST, page)}: missing anchor ${url}`);
      broken++;
    }
  }
}
console.log(`Checked ${html.length} pages; ${external.size} distinct external links (not fetched).`);
if (broken) {
  console.error(`${broken} broken link(s).`);
  process.exit(1);
}
console.log("All internal links resolve.");
