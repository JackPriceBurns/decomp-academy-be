import { EmailLayout } from "./EmailLayout";

export interface PasswordResetEmailProps {
  code: string;
}

const SUBJECT = "Reset your Decomp Academy password";

export function passwordResetSubject(): string {
  return SUBJECT;
}

export function PasswordResetEmail({ code }: PasswordResetEmailProps) {
  return (
    <EmailLayout
      preview={`Your Decomp Academy password reset code: ${code}`}
      title="Reset your password"
      code={code}
      codeLabel="Reset code"
      footer={
        <>
          If you didn&rsquo;t request a password reset, you can safely ignore this email &mdash; your
          password won&rsquo;t change until this code is used.
        </>
      }
    >
      Enter the code below to set a new password for your Decomp Academy account.
    </EmailLayout>
  );
}

PasswordResetEmail.PreviewProps = {
  code: "482910",
} satisfies PasswordResetEmailProps;

export default PasswordResetEmail;
