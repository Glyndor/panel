import { getTranslations } from "next-intl/server";
import { getMe } from "@/actions/(dashboard)/member/profile";

export default async function ProfilePage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const [t, me] = await Promise.all([getTranslations({ locale, namespace: "app.settings" }), getMe()]);

	if (!me) return null;

	return (
		<div className="flex flex-col p-6 gap-6 max-w-xl">
			<h1 className="text-xl font-semibold">{t("profile")}</h1>
			<div className="rounded-lg border p-4">
				<p className="text-xs text-muted-foreground">{t("profileUsername")}</p>
				<p className="text-sm font-medium font-mono mt-0.5">{me.username}</p>
			</div>
		</div>
	);
}
