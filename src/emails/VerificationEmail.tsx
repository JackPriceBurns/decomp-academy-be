import { EmailLayout } from "./EmailLayout";

// Email-ownership flows: confirming a new account (signup) or a changed address
// (verify-email). Password resets have their own email (./PasswordResetEmail).
export type VerificationPurpose = "signup" | "verify-email";

export interface VerificationEmailProps {
  code: string;
  purpose: VerificationPurpose;
}

const COPY: Record<VerificationPurpose, { heading: string; body: string; subject: string }> = {
  signup: {
    heading: "Verify your email",
    body: "Enter the code below to finish creating your Decomp Academy account.",
    subject: "Verify your Decomp Academy account",
  },
  "verify-email": {
    heading: "Confirm your email",
    body: "Enter the code below to confirm your new email address.",
    subject: "Confirm your Decomp Academy email",
  },
};

export function verificationSubject(purpose: VerificationPurpose): string {
  return COPY[purpose].subject;
}

export function VerificationEmail({ code, purpose }: VerificationEmailProps) {
  const copy = COPY[purpose];
  return (
    <EmailLayout
      preview={`Your Decomp Academy code: ${code}`}
      title={copy.heading}
      code={code}
      footer={
        <>
          If you didn&rsquo;t request this, you can safely ignore this email &mdash; nobody can access
          your account without the code above.
        </>
      }
    >
      {copy.body}
    </EmailLayout>
  );
}

VerificationEmail.PreviewProps = {
  code: "482910",
  purpose: "signup",
} satisfies VerificationEmailProps;

export default VerificationEmail;
