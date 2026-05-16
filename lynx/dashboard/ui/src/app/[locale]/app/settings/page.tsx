import { Suspense } from "react";
import { cookies } from "next/headers";
import { getTranslations } from "next-intl/server";
import { SessionList } from "./SessionList";
import { SessionListSkeleton } from "./SessionListSkeleton";
import { RotateButton } from "./RotateButton";

export default async function SettingsPage({
	params,
}: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const t = await getTranslations({ locale, namespace: "app.settings" });
	const jar = await cookies();
	const token = jar.get("access_token")?.value ?? "";

	return (
		<div className="flex flex-col p-6 gap-8 max-w-3xl">
			<h1 className="text-xl font-semibold">{t("title")}</h1>

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
					{t("security")}
				</h2>
				<div className="rounded-lg border p-4 flex items-center justify-between gap-4">
					<div className="min-w-0">
						<p className="text-sm font-medium">{t("rotateKeys")}</p>
						<p className="mt-0.5 text-xs text-muted-foreground">
							{t("rotateKeysDesc")}
						</p>
					</div>
					<RotateButton
						locale={locale}
						label={t("rotateKeysBtn")}
						confirmMsg={t("rotateKeysConfirm")}
					/>
				</div>
			</section>

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
					{t("sessions")}
				</h2>
				<Suspense fallback={<SessionListSkeleton />}>
					<SessionList token={token} locale={locale} />
				</Suspense>
			</section>
		</div>
	);
}
