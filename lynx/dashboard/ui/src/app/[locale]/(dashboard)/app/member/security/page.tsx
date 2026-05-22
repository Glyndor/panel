import { getTranslations } from "next-intl/server";
import { getMe } from "@/actions/(dashboard)/member/profile";
import { SingleSessionToggle } from "@/components/(dashboard)/member/security/SingleSessionToggle";

export default async function SecurityPage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const [t, me] = await Promise.all([getTranslations({ locale, namespace: "app.settings" }), getMe()]);

	if (!me) return null;

	return (
		<div className="flex flex-col p-6 gap-6 max-w-xl">
			<h1 className="text-xl font-semibold">{t("security")}</h1>
			<div className="rounded-lg border p-4">
				<SingleSessionToggle
					initial={me.single_session}
					labels={{
						desc: t("singleSessionDesc"),
						error: t("singleSessionError"),
						label: t("singleSession"),
						success: t("singleSessionSuccess"),
					}}
				/>
			</div>
		</div>
	);
}
