import { cookies } from "next/headers";
import { getTranslations } from "next-intl/server";
import { Suspense } from "react";
import { SessionList } from "@/components/(dashboard)/member/sessions/SessionList";
import { SessionListSkeleton } from "@/components/(dashboard)/member/sessions/SessionListSkeleton";

export default async function SessionsPage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const [t, jar] = await Promise.all([getTranslations({ locale, namespace: "app.settings" }), cookies()]);
	const token = jar.get("access_token")?.value ?? "";

	return (
		<div className="flex flex-col p-6 gap-6 max-w-xl">
			<h1 className="text-xl font-semibold">{t("sessions")}</h1>
			<Suspense fallback={<SessionListSkeleton />}>
				<SessionList locale={locale} token={token} />
			</Suspense>
		</div>
	);
}
