// Builds the Qvault website (GitHub Pages) from site/src into site/dist, or
// the directory given as the first argument. No dependencies: each page is an
// HTML fragment wrapped in src/_layout.html.
//
//   node site/build.mjs [outDir]
//
// Page metadata sits in a leading comment:
//   <!-- title: Models and setup
//        description: Download and verify the OCR models. -->
// Pages under src/es/ and src/pt/ are the Spanish and Portuguese versions of
// the page with the same name at the top level. Pages that exist only in
// English are linked from the translated pages as English pages.
// Everything under src/assets/ is copied unchanged.

import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const SRC = join(HERE, "src");
const OUT = process.argv[2] ?? join(HERE, "dist");
const REPO = "https://github.com/santacroce-tech/qvault";
const RELEASES = `${REPO}/releases/latest`;
const LANGS = ["en", "es", "pt"];

/** Interface strings for the layout, per language. */
const UI = {
  en: {
    name: "English",
    skip: "Skip to content",
    menu: "Menu",
    how: "How it works",
    models: "Models",
    architecture: "Architecture",
    privacy: "Privacy",
    terms: "Terms",
    article: "Article",
    source: "Source code",
    download: "Download",
    language: "Language",
    englishOnly: "",
    tagline: "Local PII detection and redaction for legal documents.",
    license: "Free and open source under the MIT License.",
    by: "A project by",
    noTracking: "This site uses no cookies, analytics or third-party scripts.",
  },
  es: {
    name: "Español",
    skip: "Saltar al contenido",
    menu: "Menú",
    how: "Cómo funciona",
    models: "Modelos",
    architecture: "Arquitectura",
    privacy: "Privacidad",
    terms: "Condiciones",
    article: "Artículo",
    source: "Código fuente",
    download: "Descargar",
    language: "Idioma",
    englishOnly: " (en inglés)",
    tagline: "Detección y anonimización local de datos personales en documentos jurídicos.",
    license: "Gratuito y de código abierto bajo la licencia MIT.",
    by: "Un proyecto de",
    noTracking: "Este sitio no usa cookies, analítica ni scripts de terceros.",
  },
  pt: {
    name: "Português",
    skip: "Pular para o conteúdo",
    menu: "Menu",
    how: "Como funciona",
    models: "Modelos",
    architecture: "Arquitetura",
    privacy: "Privacidade",
    terms: "Termos",
    article: "Artigo",
    source: "Código-fonte",
    download: "Baixar",
    language: "Idioma",
    englishOnly: " (em inglês)",
    tagline: "Detecção e tarja local de dados pessoais em documentos jurídicos.",
    license: "Gratuito e de código aberto sob a licença MIT.",
    by: "Um projeto da",
    noTracking: "Este site não usa cookies, analytics nem scripts de terceiros.",
  },
};

const layout = readFileSync(join(SRC, "_layout.html"), "utf8");

function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) return name === "assets" ? [] : walk(p);
    return name.endsWith(".html") && !name.startsWith("_") ? [relative(SRC, p)] : [];
  });
}

function meta(html) {
  const m = html.match(/^\s*<!--([\s\S]*?)-->/);
  const out = { title: "", description: "" };
  if (m) {
    for (const line of m[1].split("\n")) {
      const kv = line.match(/^\s*(\w+):\s*(.*)$/);
      if (kv) out[kv[1]] = kv[2].trim();
    }
  }
  return { ...out, body: m ? html.slice(m[0].length) : html };
}

const esc = (s) => s.replaceAll("&", "&amp;").replaceAll('"', "&quot;").replaceAll("<", "&lt;");

const pages = walk(SRC).map((rel) => {
  const parts = rel.split(sep);
  const lang = LANGS.includes(parts[0]) && parts.length > 1 ? parts[0] : "en";
  const key = lang === "en" ? parts.join("/") : parts.slice(1).join("/");
  return { rel, lang, key };
});
const exists = (lang, key) => pages.some((p) => p.lang === lang && p.key === key);

function render({ rel, lang, key }) {
  const { title, description, body } = meta(readFileSync(join(SRC, rel), "utf8"));
  const depth = rel.split(sep).length - 1;
  const root = depth ? "../".repeat(depth) : "./";
  const t = UI[lang];
  // Where a page lives for a given language; falls back to English.
  const href = (k, l = lang) => {
    if (exists(l, k)) return l === "en" ? `${root}${k}` : `${root}${l}/${k}`;
    return `${root}${k}`;
  };
  const enOnly = (k) => (lang !== "en" && !exists(lang, k) ? t.englishOnly : "");
  const link = (k, label) =>
    `<a href="${href(k)}"${k === key ? ' aria-current="page"' : ""}${enOnly(k) ? ' hreflang="en"' : ""}>${label}${enOnly(k)}</a>`;

  const nav = [
    `<a href="${href("index.html")}#how">${t.how}</a>`,
    link("models.html", t.models),
    link("architecture.html", t.architecture),
    link("privacy.html", t.privacy),
  ].join("\n");

  const langLinks = LANGS.map((l) => {
    const target = exists(l, key) ? href(key, l) : href("index.html", l);
    const cur = l === lang ? ' aria-current="true"' : "";
    return `<a href="${target}" lang="${l}" hreflang="${l}"${cur}>${l.toUpperCase()}</a>`;
  }).join("");

  const alternates = LANGS.filter((l) => exists(l, key))
    .map((l) => `<link rel="alternate" hreflang="${l}" href="${href(key, l)}">`)
    .join("\n  ");

  const footerLinks = [
    link("models.html", t.models),
    link("architecture.html", t.architecture),
    link("privacy.html", t.privacy),
    link("terms.html", t.terms),
    link("blog/qvault-local-redaction.html", t.article),
    `<a href="${REPO}">${t.source}</a>`,
  ].join("\n");

  const fullTitle = key === "index.html" ? title : `${title} · Qvault`;
  const fill = (s) =>
    s
      .replaceAll("{{root}}", root)
      .replaceAll("{{repo}}", REPO)
      .replaceAll("{{releases}}", RELEASES)
      .replaceAll("{{home}}", href("index.html"))
      .replaceAll("{{models}}", href("models.html"))
      .replaceAll("{{architecture}}", href("architecture.html"))
      .replaceAll("{{privacyPage}}", href("privacy.html"))
      .replaceAll("{{termsPage}}", href("terms.html"));

  return layout
    .replaceAll("{{lang}}", lang)
    .replaceAll("{{title}}", esc(fullTitle))
    .replaceAll("{{description}}", esc(description))
    .replaceAll("{{alternates}}", alternates)
    .replaceAll("{{homeHref}}", href("index.html"))
    .replaceAll("{{skip}}", t.skip)
    .replaceAll("{{menu}}", t.menu)
    .replaceAll("{{nav}}", nav)
    .replaceAll("{{download}}", t.download)
    .replaceAll("{{language}}", t.language)
    .replaceAll("{{langLinks}}", langLinks)
    .replaceAll("{{tagline}}", t.tagline)
    .replaceAll("{{license}}", t.license)
    .replaceAll("{{by}}", t.by)
    .replaceAll("{{noTracking}}", t.noTracking)
    .replaceAll("{{footerLinks}}", footerLinks)
    .replaceAll("{{year}}", String(new Date().getFullYear()))
    .replace("{{content}}", () => fill(body))
    .replaceAll("{{root}}", root)
    .replaceAll("{{releases}}", RELEASES)
    .replaceAll("{{repo}}", REPO);
}

if (existsSync(OUT)) rmSync(OUT, { recursive: true, force: true });
mkdirSync(OUT, { recursive: true });
for (const page of pages) {
  const dest = join(OUT, page.rel);
  mkdirSync(dirname(dest), { recursive: true });
  writeFileSync(dest, render(page));
}
cpSync(join(SRC, "assets"), join(OUT, "assets"), { recursive: true });
writeFileSync(join(OUT, ".nojekyll"), "");
console.log(`Built ${pages.length} pages into ${relative(process.cwd(), OUT) || "."}`);
