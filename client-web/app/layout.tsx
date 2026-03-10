import type { Metadata } from "next";
import { Providers } from "./providers";
import { AppNavBar } from "@/components/NavBar";
import "./globals.css";

export const metadata: Metadata = {
  title: "TwoGreenSuns",
  description: "Home energy management dashboard",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className="dark">
      <body>
        <Providers>
          <AppNavBar />
          {children}
        </Providers>
      </body>
    </html>
  );
}
