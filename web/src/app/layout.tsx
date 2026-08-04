import type { Metadata } from "next";
import { Geist } from "next/font/google";
import { HistoryShell } from "@/components/HistoryShell";
import { SourceFilterProvider } from "@/components/SourceFilter";
import { ThemeBootScript } from "@/components/ThemeBootScript";
import { DateTimeFormatProvider } from "@/components/useDateTimeFormat";
import { MessageBadgePrefsProvider } from "@/components/useMessageBadgePrefs";
import { ThemeProvider } from "@/components/useTheme";
import { THEME_BOOT_SCRIPT } from "@/lib/theme";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "Message Vault",
  description: "Browse your messages in one place",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="en"
      className={`${geistSans.variable} h-full`}
      data-theme="dark"
      suppressHydrationWarning
    >
      <body className="h-full overflow-hidden bg-bg text-text antialiased">
        <ThemeBootScript script={THEME_BOOT_SCRIPT} />
        <ThemeProvider>
          <DateTimeFormatProvider>
            <MessageBadgePrefsProvider>
              <SourceFilterProvider>
                <HistoryShell>{children}</HistoryShell>
              </SourceFilterProvider>
            </MessageBadgePrefsProvider>
          </DateTimeFormatProvider>
        </ThemeProvider>
      </body>
    </html>
  );
}
