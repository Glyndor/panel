import { getTranslations } from "next-intl/server";
import { LocaleSwitcher } from "@/components/(dashboard)/LocaleSwitcher";
import { ThemeToggle } from "@/components/(dashboard)/ThemeToggle";

export default async function PreferencesPage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const t = await getTranslations({ locale, namespace: "app.settings" });

	return (
		<div className="flex flex-col p-6 gap-6 max-w-xl">
			<h1 className="text-xl font-semibold">{t("preferences")}</h1>
			<div className="rounded-lg border p-4 flex flex-col gap-4">
				<div className="flex items-center justify-between gap-4">
					<div>
						<p className="text-sm font-medium">{t("theme")}</p>
						<p className="text-xs text-muted-foreground">{t("themeDesc")}</p>
					</div>
					<ThemeToggle />
				</div>
				<div className="border-t pt-4 flex items-center justify-between gap-4">
					<div>
						<p className="text-sm font-medium">{t("language")}</p>
						<p className="text-xs text-muted-foreground">{t("languageDesc")}</p>
					</div>
					<LocaleSwitcher locale={locale} />
				</div>
			</div>
		</div>
	);
}
