// Bundles and runs export-emails.entry.tsx to regenerate the static email HTML
// the Rust email-sender embeds. Run via `npm run emails:export`.
import { build } from "esbuild";
import { pathToFileURL } from "node:url";
import { mkdirSync } from "node:fs";

const OUT = "node_modules/.cache/export-emails.cjs";
mkdirSync("node_modules/.cache", { recursive: true });

await build({
  entryPoints: ["scripts/export-emails.entry.tsx"],
  bundle: true,
  platform: "node",
  format: "cjs",
  outfile: OUT,
  logLevel: "warning",
});

await import(pathToFileURL(OUT).href);
