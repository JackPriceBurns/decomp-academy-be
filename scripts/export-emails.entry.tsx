// Pre-renders the react-email templates to static HTML + plaintext with an
// __OTP_CODE__ placeholder, which the Rust email-sender Lambda includes at build
// time and substitutes at runtime. Run via `npm run emails:export`. react-email
// stays the authoring source; it is no longer in the Lambda runtime path.
import { render } from "@react-email/render";
import { createElement } from "react";
import { mkdirSync, writeFileSync } from "node:fs";
import { VerificationEmail } from "../src/emails/VerificationEmail";
import { PasswordResetEmail } from "../src/emails/PasswordResetEmail";
import { FeedbackNotificationEmail } from "../src/emails/FeedbackNotificationEmail";
import { FeedbackReplyEmail } from "../src/emails/FeedbackReplyEmail";

const CODE = "__OTP_CODE__";
const OUT = "api/emails";

const targets = [
  ["verification_signup", createElement(VerificationEmail, { code: CODE, purpose: "signup" })],
  ["verification_verify_email", createElement(VerificationEmail, { code: CODE, purpose: "verify-email" })],
  ["password_reset", createElement(PasswordResetEmail, { code: CODE })],
  // Owner notification — its dynamic fields are __…__ placeholders the Rust api
  // Lambda substitutes at send time (no OTP code).
  ["feedback_notification", createElement(FeedbackNotificationEmail)],
  // Learner-facing reply — same __…__ placeholder substitution by the api Lambda.
  ["feedback_reply", createElement(FeedbackReplyEmail)],
] as const;

async function main() {
  mkdirSync(OUT, { recursive: true });
  for (const [name, el] of targets) {
    const html = await render(el);
    const text = await render(el, { plainText: true });
    writeFileSync(`${OUT}/${name}.html`, html);
    writeFileSync(`${OUT}/${name}.txt`, text);
    console.log(`wrote ${OUT}/${name}.{html,txt}`);
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
