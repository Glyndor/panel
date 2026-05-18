import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import { cookies } from "next/headers";
import { notFound } from "next/navigation";
import { NextIntlClientProvider } from "next-intl";
import { getMessages } from "next-intl/server";
import { ThemeProvider } from "@/components/ThemeProvider";
import { Toaster } from "@/components/ui/sonner";
import { routing } from "@/i18n/routing";
import { BACKEND_URL } from "@/lib/api";
import "../globals.css";

const geistSans = Geist({ subsets: ["latin"], variable: "--font-geist-sans" });
const geistMono = Geist_Mono({
	subsets: ["latin"],
	variable: "--font-geist-mono",
});

interface Branding {
	accent_color: string;
	company_name: string;
	logo_url: string | null;
	primary_color: string;
	secondary_color: string;
}

const BRANDING_DEFAULTS: Branding = {
	accent_color: "#6366f1",
	company_name: "Lynx",
	logo_url: null,
	primary_color: "#0f172a",
	secondary_color: "#38bdf8",
};

async function fetchBranding(): Promise<Branding> {
	try {
		const res = await fetch(`${BACKEND_URL}/branding`, {
			next: { revalidate: 60 },
		});
		if (!res.ok) return BRANDING_DEFAULTS;
		return (await res.json()) as Branding;
	} catch {
		return BRANDING_DEFAULTS;
	}
}

export async function generateMetadata({ params }: { params: Promise<{ locale: string }> }): Promise<Metadata> {
	await params;
	const branding = await fetchBranding();
	return {
		description: "Distributed infrastructure orchestration",
		robots: { follow: false, index: false },
		title: branding.company_name,
	};
}

export default async function LocaleLayout({
	children,
	params,
}: {
	children: React.ReactNode;
	params: Promise<{ locale: string }>;
}) {
	const { locale } = await params;

	if (!routing.locales.includes(locale as "en" | "es")) {
		notFound();
	}

	const jar = await cookies();
	// Theme preference is stored in a non-HttpOnly cookie so ThemeProvider can read it.
	// Falls back to "system" if not set.
	const defaultTheme = jar.get("theme_preference")?.value ?? "system";

	const [messages, branding] = await Promise.all([getMessages(), fetchBranding()]);

	const brandVars = {
		"--brand-accent": branding.accent_color,
		"--brand-primary": branding.primary_color,
		"--brand-secondary": branding.secondary_color,
	} as React.CSSProperties;

	return (
		<html
			className={`${geistSans.variable} ${geistMono.variable} h-full antialiased`}
			lang={locale}
			style={brandVars}
			suppressHydrationWarning
		>
			<body className="min-h-full flex flex-col bg-background text-foreground">
				<ThemeProvider defaultTheme={defaultTheme}>
					<NextIntlClientProvider messages={messages}>
						{children}
						<Toaster />
					</NextIntlClientProvider>
				</ThemeProvider>
			</body>
		</html>
	);
}
