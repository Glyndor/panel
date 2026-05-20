import { getTranslations } from "next-intl/server";
import { MemberNav } from "@/components/(dashboard)/member/MemberNav";

export default async function MemberLayout({
	children,
	params,
}: {
	children: React.ReactNode;
	params: Promise<{ locale: string }>;
}) {
	const { locale } = await params;
	const t = await getTranslations({ locale, namespace: "app.settings" });

	const navItems = [
		{ href: `/${locale}/app/member/profile`, label: t("profile") },
		{ href: `/${locale}/app/member/password`, label: t("password") },
		{ href: `/${locale}/app/member/security`, label: t("security") },
		{ href: `/${locale}/app/member/preferences`, label: t("preferences") },
		{ href: `/${locale}/app/member/sessions`, label: t("sessions") },
	];

	return (
		<div className="flex min-h-full">
			<aside className="w-48 shrink-0 border-r p-3 flex flex-col gap-0.5">
				<p className="px-3 py-2 text-xs font-medium text-muted-foreground uppercase tracking-wider">
					{t("member")}
				</p>
				<MemberNav items={navItems} />
			</aside>
			<div className="flex-1">{children}</div>
		</div>
	);
}
