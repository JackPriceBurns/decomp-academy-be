import {
  Body,
  Container,
  Head,
  Heading,
  Hr,
  Html,
  Img,
  Preview,
  Section,
  Text,
} from "@react-email/components";
import type { ReactNode } from "react";
import { colors, fonts } from "./theme";

// Shared chrome for every transactional email: brand lockup, card, code box,
// and footer — styled to the frontend design system (see ./theme). Individual
// emails (verification, password reset) supply only their copy.
export interface EmailLayoutProps {
  /** Inbox preview text. */
  preview: string;
  /** Card heading. */
  title: string;
  /** The one-time code. */
  code: string;
  /** Small mono label above the code; defaults to "Verification code". */
  codeLabel?: string;
  /** Reassurance line under the divider. */
  footer: ReactNode;
  /** Lead paragraph between heading and code. */
  children: ReactNode;
}

export function EmailLayout({
  preview,
  title,
  code,
  codeLabel = "Verification code",
  footer,
  children,
}: EmailLayoutProps) {
  return (
    <Html>
      <Head>
        {/* Declare the mail as dark-only so color-scheme-aware clients
            (Apple Mail, iOS, Outlook) render our dark design as-is instead of
            auto-inverting / light-ifying it. */}
        <meta name="color-scheme" content="dark" />
        <meta name="supported-color-schemes" content="dark" />
        <style>{`:root { color-scheme: dark; supported-color-schemes: dark; }`}</style>
      </Head>
      <Preview>{preview}</Preview>
      <Body style={bodyStyle}>
        <Container style={container}>
          <Section style={brandRow}>
            <Img
              src="https://decomp-academy.dev/brand/favicon/favicon-128.png"
              width="34"
              height="34"
              alt=""
              style={logoImg}
            />
            <span style={wordmark}>Decomp Academy</span>
          </Section>
          <Heading style={headingStyle}>{title}</Heading>
          <Text style={lead}>{children}</Text>
          <Section style={codeBox}>
            <Text style={codeLabelStyle}>{codeLabel}</Text>
            <Text style={codeText}>{code}</Text>
          </Section>
          <Hr style={divider} />
          <Text style={footerStyle}>{footer}</Text>
        </Container>
      </Body>
    </Html>
  );
}

const bodyStyle = {
  backgroundColor: colors.bg,
  fontFamily: fonts.sans,
  color: colors.text,
  colorScheme: "dark" as const,
  margin: 0,
  padding: "40px 16px",
};

const container = {
  backgroundColor: colors.surface,
  border: `1px solid ${colors.border}`,
  borderRadius: "16px",
  padding: "40px 32px",
  maxWidth: "520px",
  margin: "0 auto",
};

// Brand lockup: the {dA} app-icon tile beside the wordmark — inline-block so it
// survives Gmail/Outlook stripping flex/grid. The image is hosted on the site;
// if a client blocks images, the wordmark text still carries the brand.
const brandRow = {
  textAlign: "center" as const,
  margin: "0 0 30px",
};

const logoImg = {
  display: "inline-block",
  width: "34px",
  height: "34px",
  borderRadius: "8px",
  verticalAlign: "middle",
};

const wordmark = {
  marginLeft: "10px",
  fontFamily: fonts.sans,
  fontSize: "17px",
  fontWeight: 700,
  letterSpacing: "-0.01em",
  color: colors.bright,
  verticalAlign: "middle",
};

const headingStyle = {
  fontFamily: fonts.sans,
  fontSize: "23px",
  fontWeight: 700,
  color: colors.bright,
  margin: "0 0 14px",
  letterSpacing: "-0.02em",
  lineHeight: 1.2,
};

const lead = {
  color: colors.muted,
  fontSize: "15px",
  lineHeight: 1.6,
  margin: "0 0 24px",
};

const codeBox = {
  backgroundColor: colors.bgAlt,
  border: `1px solid ${colors.border}`,
  borderRadius: "12px",
  padding: "18px 16px 20px",
  textAlign: "center" as const,
  margin: "8px 0 26px",
};

const codeLabelStyle = {
  fontFamily: fonts.mono,
  fontSize: "10px",
  fontWeight: 600,
  letterSpacing: "0.16em",
  textTransform: "uppercase" as const,
  color: colors.faint,
  margin: "0 0 10px",
};

const codeText = {
  fontFamily: fonts.mono,
  fontSize: "32px",
  letterSpacing: "0.4em",
  color: colors.accent,
  fontWeight: 700,
  margin: 0,
  paddingLeft: "0.4em",
};

const divider = {
  borderColor: colors.border,
  margin: "0 0 20px",
};

const footerStyle = {
  color: colors.faint,
  fontSize: "12px",
  lineHeight: 1.5,
  margin: 0,
};
