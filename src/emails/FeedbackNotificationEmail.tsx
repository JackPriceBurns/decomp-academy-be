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
import { colors, fonts } from "./theme";

// Owner notification sent whenever a learner submits feedback. It carries several
// dynamic fields (sentiment, lesson, message, …) so it doesn't reuse the
// OTP-shaped EmailLayout — it shares the same brand chrome + theme directly. The
// Rust api Lambda substitutes the __…__ placeholders (HTML-escaped) at send time.
export function FeedbackNotificationEmail() {
  return (
    <Html>
      <Head>
        <meta name="color-scheme" content="dark" />
        <meta name="supported-color-schemes" content="dark" />
        <style>{`:root { color-scheme: dark; supported-color-schemes: dark; }`}</style>
      </Head>
      <Preview>__SENTIMENT__ · __LESSON__</Preview>
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
          <Heading style={headingStyle}>New feedback</Heading>
          <Section style={detailsBox}>
            <Row label="Sentiment" value="__SENTIMENT__" />
            <Row label="Lesson" value="__LESSON__" />
            <Row label="From" value="__EMAIL__" />
            <Row label="Source" value="__SOURCE__" />
            <Row label="Received" value="__TIME__" />
          </Section>
          <Text style={msgLabel}>Message</Text>
          <Section style={msgBox}>
            <Text style={msgText}>__MESSAGE__</Text>
          </Section>
          <Hr style={divider} />
          <Text style={footerStyle}>
            Sent automatically by Decomp Academy &mdash; manage every submission in the admin
            dashboard.
          </Text>
        </Container>
      </Body>
    </Html>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <Text style={rowStyle}>
      <span style={rowLabel}>{label}</span>{" "}
      <span style={rowValue}>{value}</span>
    </Text>
  );
}

FeedbackNotificationEmail.PreviewProps = {};

export default FeedbackNotificationEmail;

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
  margin: "0 0 18px",
  letterSpacing: "-0.02em",
  lineHeight: 1.2,
};

const detailsBox = {
  backgroundColor: colors.bgAlt,
  border: `1px solid ${colors.border}`,
  borderRadius: "12px",
  padding: "16px 18px 8px",
  margin: "0 0 22px",
};

const rowStyle = {
  margin: "0 0 10px",
  fontSize: "14px",
  lineHeight: 1.5,
};

const rowLabel = {
  display: "inline-block",
  width: "92px",
  color: colors.faint,
  fontFamily: fonts.mono,
  fontSize: "11px",
  fontWeight: 600,
  letterSpacing: "0.08em",
  textTransform: "uppercase" as const,
  verticalAlign: "top",
};

const rowValue = {
  color: colors.text,
  fontSize: "14px",
};

const msgLabel = {
  fontFamily: fonts.mono,
  fontSize: "10px",
  fontWeight: 600,
  letterSpacing: "0.16em",
  textTransform: "uppercase" as const,
  color: colors.faint,
  margin: "0 0 8px",
};

const msgBox = {
  backgroundColor: colors.bgAlt,
  border: `1px solid ${colors.border}`,
  borderRadius: "12px",
  padding: "14px 16px",
  margin: "0 0 24px",
};

const msgText = {
  color: colors.text,
  fontSize: "14px",
  lineHeight: 1.6,
  margin: 0,
  whiteSpace: "pre-wrap" as const,
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
