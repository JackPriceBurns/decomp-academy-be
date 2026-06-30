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

// Learner-facing reply, sent when an admin answers a piece of feedback from the
// admin dashboard. Only learners who left an email ever receive it. Like the
// owner notification it carries dynamic fields, so it shares the brand chrome +
// theme directly rather than the OTP-shaped EmailLayout. The Rust api Lambda
// substitutes the __…__ placeholders (HTML-escaped) at send time.
export function FeedbackReplyEmail() {
  return (
    <Html>
      <Head>
        <meta name="color-scheme" content="dark" />
        <meta name="supported-color-schemes" content="dark" />
        <style>{`:root { color-scheme: dark; supported-color-schemes: dark; }`}</style>
      </Head>
      <Preview>A reply to your Decomp Academy feedback</Preview>
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
          <Heading style={headingStyle}>We replied to your feedback</Heading>
          <Text style={lead}>
            Thanks for taking the time to tell us about <strong style={strong}>__LESSON__</strong>.
            Here&rsquo;s our reply:
          </Text>
          <Section style={replyBox}>
            <Text style={replyText}>__REPLY__</Text>
          </Section>
          <Text style={quoteLabel}>Your original feedback</Text>
          <Section style={quoteBox}>
            <Text style={quoteText}>__ORIGINAL__</Text>
          </Section>
          <Hr style={divider} />
          <Text style={footerStyle}>
            You&rsquo;re getting this because you left your email with feedback on Decomp Academy.
            Just reply to this email to keep the conversation going.
          </Text>
        </Container>
      </Body>
    </Html>
  );
}

FeedbackReplyEmail.PreviewProps = {};

export default FeedbackReplyEmail;

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
  margin: "0 0 14px",
  letterSpacing: "-0.02em",
  lineHeight: 1.2,
};

const lead = {
  color: colors.muted,
  fontSize: "15px",
  lineHeight: 1.6,
  margin: "0 0 22px",
};

const strong = {
  color: colors.text,
  fontWeight: 600,
};

// The reply itself gets the accent-bordered card so it reads as the headline.
const replyBox = {
  backgroundColor: colors.bgAlt,
  border: `1px solid ${colors.accentSoft}`,
  borderRadius: "12px",
  padding: "16px 18px",
  margin: "0 0 26px",
};

const replyText = {
  color: colors.text,
  fontSize: "15px",
  lineHeight: 1.6,
  margin: 0,
  whiteSpace: "pre-wrap" as const,
};

const quoteLabel = {
  fontFamily: fonts.mono,
  fontSize: "10px",
  fontWeight: 600,
  letterSpacing: "0.16em",
  textTransform: "uppercase" as const,
  color: colors.faint,
  margin: "0 0 8px",
};

const quoteBox = {
  backgroundColor: colors.bgAlt,
  border: `1px solid ${colors.border}`,
  borderRadius: "12px",
  padding: "14px 16px",
  margin: "0 0 24px",
};

const quoteText = {
  color: colors.muted,
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
