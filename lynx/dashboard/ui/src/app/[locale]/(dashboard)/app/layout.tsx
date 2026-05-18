import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { Sidebar } from "@/components/(dashboard)/app/Sidebar";
import { BACKEND_URL } from "@/lib/api";

interface Branding {
	accent_color: string;
	company_name: string;
	logo_url: string | null;
	primary_color: string;
	secondary_color: string;
}

const DEFAULTS: Branding = {
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
		if (!res.ok) return DEFAULTS;
		return (await res.json()) as Branding;
	} catch {
		return DEFAULTS;
	}
}

async function fetchIsAdmin(token: string): Promise<boolean> {
	try {
		const res = await fetch(`${BACKEND_URL}/auth/me`, {
			cache: "no-store",
			headers: { Authorization: `Bearer ${token}` },
		});
		if (!res.ok) return false;
		const data = (await res.json()) as { is_admin?: boolean };
		return data.is_admin === true;
	} catch {
		return false;
	}
}

export default async function AppLayout({
	children,
	params,
}: {
	children: React.ReactNode;
	params: Promise<{ locale: string }>;
}) {
	const { locale } = await params;

	const jar = await cookies();
	const token = jar.get("access_token")?.value;
	const hasAccess = !!token || jar.has("refresh_token");

	if (!hasAccess) {
		redirect(`/${locale}/login`);
	}

	const [branding, isAdmin] = await Promise.all([
		fetchBranding(),
		token ? fetchIsAdmin(token) : Promise.resolve(false),
	]);

	return (
		<div className="flex h-screen overflow-hidden">
			<Sidebar
				companyName={branding.company_name}
				isAdmin={isAdmin}
				locale={locale}
				logoUrl={branding.logo_url}
			/>
			<main className="flex flex-1 flex-col overflow-y-auto">{children}</main>
		</div>
	);
}
