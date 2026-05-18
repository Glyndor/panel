
import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { BACKEND_URL } from "@/lib/api";
import { Sidebar } from "./Sidebar";

interface Branding {
	company_name: string;
	logo_url: string | null;
	primary_color: string;
	secondary_color: string;
	accent_color: string;
}

const DEFAULTS: Branding = {
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
		if (!res.ok) return DEFAULTS;
		return (await res.json()) as Branding;
	} catch {
		return DEFAULTS;
	}
}

export default async function AppLayout({
	children,
	params,
}: { children: React.ReactNode; params: Promise<{ locale: string }>; }) {
	const { locale } = await params;

	const jar = await cookies();
	const hasAccess = jar.has("access_token") || jar.has("refresh_token");

	if (!hasAccess) {
		redirect(`/${locale}/login`);
	}

	const branding = await fetchBranding();

	return (
		<div className="flex h-screen overflow-hidden">
			<Sidebar
				locale={locale}
				companyName={branding.company_name}
				logoUrl={branding.logo_url}
			/>
			<main className="flex flex-1 flex-col overflow-y-auto">
				{children}
			</main>
		</div>
	);
}
