
import { getTranslations } from "next-intl/server";

export default async function OverviewPage({
	params,
}: { params: Promise<{ locale: string }>; }) {
	const { locale } = await params;
	const t = await getTranslations({ locale, namespace: "app.overview" });

	return (
		<div className="flex flex-col p-6 gap-6">
			<h1 className="text-xl font-semibold">{t("title")}</h1>

			<div className="flex flex-1 items-center justify-center rounded-lg border border-dashed min-h-64">
				<div className="text-center max-w-xs">
					<p className="text-sm font-medium">{t("noAgents")}</p>
					<p className="mt-1 text-xs text-muted-foreground">
						{t("noAgentsDesc")}
					</p>
				</div>
			</div>
		</div>
	);
}
