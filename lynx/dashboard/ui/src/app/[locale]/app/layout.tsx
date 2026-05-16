
import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { Sidebar } from "./Sidebar";

export default async function AppLayout({
	children,
	params,
}: { children: React.ReactNode; params: Promise<{ locale: string }>; }) {
	const { locale } = await params;

	// Second-layer auth check (proxy handles primary, this handles edge cases)
	const jar = await cookies();
	const hasAccess =
		jar.has("access_token") || jar.has("refresh_token");

	if (!hasAccess) {
		redirect(`/${locale}/login`);
	}

	return (
		<div className="flex h-screen overflow-hidden">
			<Sidebar locale={locale} />
			<main className="flex flex-1 flex-col overflow-y-auto">
				{children}
			</main>
		</div>
	);
}
