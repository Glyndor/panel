import { Geist, Geist_Mono } from "next/font/google";
import { NextIntlClientProvider } from "next-intl";
import { getMessages } from "next-intl/server";
import { notFound } from "next/navigation";
import { cookies } from "next/headers";
import type { Metadata } from "next";
import { Toaster } from "@/components/ui/sonner";
import { ThemeProvider } from "@/components/ThemeProvider";
import { routing } from "@/i18n/routing";
import { BACKEND_URL } from "@/lib/api";
import "../globals.css";

const geistSans = Geist({ variable: "--font-geist-sans", subsets: ["latin"] });
const geistMono = Geist_Mono({
	variable: "--font-geist-mono",
	subsets: ["latin"],
});

interface Branding {
	company_name: string;
	logo_url: string | null;
	primary_color: string;
	secondary_color: string;
	accent_color: string;
}

const BRANDING_DEFAULTS: Branding = {
	company_name: "Lynx",
	logo_url: null,
	primary_color: "#0f172a",
	secondary_color: "#38bdf8",
	accent_color: "#6366f1",
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

export async function generateMetadata({
	params,
}: {
	params: Promise<{ locale: string }>;
}): Promise<Metadata> {
	await params;
	const branding = await fetchBranding();
	return {
		title: branding.company_name,
		description: "Distributed infrastructure orchestration",
		robots: { index: false, follow: false },
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

	const [messages, branding] = await Promise.all([
		getMessages(),
		fetchBranding(),
	]);

	const brandVars = {
		"--brand-primary": branding.primary_color,
		"--brand-secondary": branding.secondary_color,
		"--brand-accent": branding.accent_color,
	} as React.CSSProperties;

	return (
		<html
			lang={locale}
			style={brandVars}
			className={`${geistSans.variable} ${geistMono.variable} h-full antialiased`}
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
