import { Suspense } from "react";
import { cookies } from "next/headers";
import { getTranslations } from "next-intl/server";
import { BACKEND_URL } from "@/lib/api";
import { SessionList } from "./SessionList";
import { SessionListSkeleton } from "./SessionListSkeleton";
import { RotateButton } from "./RotateButton";
import { BrandingForm } from "./BrandingForm";

interface Branding {
	company_name: string;
	logo_url: string | null;
	primary_color: string;
	secondary_color: string;
	accent_color: string;
}

const BRANDING_DEFAULTS: Branding = {
	company_name: "Lynx",
	logo_url: null,
	primary_color: "#0f172a",
	secondary_color: "#38bdf8",
	accent_color: "#6366f1",
};

async function fetchBranding(): Promise<Branding> {
	try {
		const res = await fetch(`${BACKEND_URL}/branding`, {
			cache: "no-store",
		});
		if (!res.ok) return BRANDING_DEFAULTS;
		return (await res.json()) as Branding;
	} catch {
		return BRANDING_DEFAULTS;
	}
}

export default async function SettingsPage({
	params,
}: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const [t, jar, branding] = await Promise.all([
		getTranslations({ locale, namespace: "app.settings" }),
		cookies(),
		fetchBranding(),
	]);
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
					{t("branding")}
				</h2>
				<div className="rounded-lg border p-4">
					<BrandingForm
						initial={branding}
						labels={{
							companyName: t("brandingCompanyName"),
							logoUrl: t("brandingLogoUrl"),
							primaryColor: t("brandingPrimaryColor"),
							secondaryColor: t("brandingSecondaryColor"),
							accentColor: t("brandingAccentColor"),
							save: t("brandingSave"),
							saved: t("brandingSaved"),
							error: t("brandingError"),
						}}
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
