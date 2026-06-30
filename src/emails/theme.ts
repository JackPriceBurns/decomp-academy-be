// Email palette — mirrors the frontend design tokens (decomp-academy-fe
// src/lib/theme.ts) so transactional mail reads as the same product. Email
// clients can't use Tailwind classes or CSS vars, so the hexes are inlined here.
export const colors = {
  bg: "#0e0c16", // page background       (fe bg.DEFAULT)
  bgAlt: "#0a0810", // inset / code surface  (fe bg.inset)
  surface: "#17141f", // the card              (fe bg.soft)
  border: "#221c30", // hairline              (fe line.DEFAULT)
  borderStrong: "#2e2740", // stronger hairline     (fe line.strong)
  bright: "#f0f3f8", // headings              (fe content.bright)
  text: "#e6ebf2", // body / strong labels  (fe content.primary)
  muted: "#8b97a6", // captions, lead copy   (fe content.muted)
  faint: "#7a8696", // footer, disabled      (fe content.faint)
  accent: "#8b6cf0", // GameCube-purple accent (fe accent.DEFAULT)
  accentSoft: "#6c4fd6", // accent border / brand violet (fe accent.soft)
  accentTint: "rgba(108, 79, 214, 0.16)", // accent fill behind the logo mark
} as const;

export const fonts = {
  sans: '-apple-system, BlinkMacSystemFont, "Inter", "Segoe UI", Roboto, Helvetica, Arial, sans-serif',
  mono: '"JetBrains Mono", ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace',
} as const;
