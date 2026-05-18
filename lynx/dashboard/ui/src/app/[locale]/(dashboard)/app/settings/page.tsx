import { cookies } from "next/headers";
import { getTranslations } from "next-intl/server";
import { Suspense } from "react";
import { getMigrationStatus } from "@/actions/(dashboard)/app/settings/migration";
import { getMe } from "@/actions/(dashboard)/app/settings/profile";
import { BrandingForm } from "@/components/(dashboard)/app/settings/BrandingForm";
import { ChangePasswordForm } from "@/components/(dashboard)/app/settings/ChangePasswordForm";
import { DomainSection } from "@/components/(dashboard)/app/settings/DomainSection";
import { MigrationSection } from "@/components/(dashboard)/app/settings/MigrationSection";
import { RotateButton } from "@/components/(dashboard)/app/settings/RotateButton";
import { RotationLog } from "@/components/(dashboard)/app/settings/RotationLog";
import { SessionList } from "@/components/(dashboard)/app/settings/SessionList";
import { SessionListSkeleton } from "@/components/(dashboard)/app/settings/SessionListSkeleton";
import { SingleSessionToggle } from "@/components/(dashboard)/app/settings/SingleSessionToggle";
import { UpdateSection } from "@/components/(dashboard)/app/settings/UpdateSection";
import { BACKEND_URL } from "@/lib/api";

interface Branding {
	accent_color: string;
	company_name: string;
	logo_url: string | null;
	primary_color: string;
	secondary_color: string;
}

interface DomainConfig {
	cert_expires_at: string | null;
	cert_type: string;
	domain: string | null;
	error_message: string | null;
	hsts_enabled: boolean;
	port_19443_open: boolean;
	status: string;
}

const BRANDING_DEFAULTS: Branding = {
	accent_color: "#6366f1",
	company_name: "Lynx",
	logo_url: null,
	primary_color: "#0f172a",
	secondary_color: "#38bdf8",
};

const DOMAIN_DEFAULTS: DomainConfig = {
	cert_expires_at: null,
	cert_type: "self_signed",
	domain: null,
	error_message: null,
	hsts_enabled: false,
	port_19443_open: true,
	status: "unconfigured",
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

async function fetchDomainConfig(token: string): Promise<DomainConfig> {
	try {
		const res = await fetch(`${BACKEND_URL}/domain`, {
			cache: "no-store",
			headers: { Authorization: `Bearer ${token}` },
		});
		if (!res.ok) return DOMAIN_DEFAULTS;
		return (await res.json()) as DomainConfig;
	} catch {
		return DOMAIN_DEFAULTS;
	}
}

export default async function SettingsPage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const [t, jar, branding] = await Promise.all([
		getTranslations({ locale, namespace: "app.settings" }),
		cookies(),
		fetchBranding(),
	]);
	const token = jar.get("access_token")?.value ?? "";
	const [domainCfg, migrationState, me] = await Promise.all([
		fetchDomainConfig(token),
		getMigrationStatus(),
		getMe(),
	]);

	return (
		<div className="flex flex-col p-6 gap-8 max-w-3xl">
			<h1 className="text-xl font-semibold">{t("title")}</h1>

			{me && (
				<section className="flex flex-col gap-3">
					<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
						{t("profile")}
					</h2>
					<div className="rounded-lg border p-4 flex flex-col gap-4">
						<div>
							<p className="text-xs text-muted-foreground">{t("profileUsername")}</p>
							<p className="text-sm font-medium font-mono">{me.username}</p>
						</div>
						<div className="border-t pt-4">
							<p className="text-sm font-medium mb-3">{t("changePassword")}</p>
							<ChangePasswordForm
								labels={{
									btn: t("changePasswordBtn"),
									currentPassword: t("currentPassword"),
									error: t("changePasswordError"),
									newPassword: t("newPassword"),
									success: t("changePasswordSuccess"),
									wrong: t("changePasswordWrong"),
								}}
								locale={locale}
							/>
						</div>
						<div className="border-t pt-4">
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
				</section>
			)}

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t("domain")}</h2>
				<div className="rounded-lg border p-4">
					<DomainSection
						initial={domainCfg}
						labels={{
							active: t("domainActive"),
							cert: t("domainCert"),
							certCloudflare: t("domainCertCloudflare"),
							certCustom: t("domainCertCustom"),
							certExpires: t("domainCertExpires"),
							certKeyOptional: t("domainCertKeyOptional"),
							certKeyPem: t("domainCertKeyPem"),
							certKeyPemPlaceholder: t("domainCertKeyPemPlaceholder"),
							certLE: t("domainCertLE"),
							certPem: t("domainCertPem"),
							certPemPlaceholder: t("domainCertPemPlaceholder"),
							certSelfSigned: t("domainCertSelfSigned"),
							certUpload: t("domainCertUpload"),
							certUploadCloudflare: t("domainCertUploadCloudflare"),
							certUploadCustom: t("domainCertUploadCustom"),
							certUploadError: t("domainCertUploadError"),
							certUploadSuccess: t("domainCertUploadSuccess"),
							closePort: t("domainClosePort"),
							closePortBtn: t("domainClosePortBtn"),
							closePortConfirm: t("domainClosePortConfirm"),
							closePortDesc: t("domainClosePortDesc"),
							closePortError: t("domainClosePortError"),
							closePortSuccess: t("domainClosePortSuccess"),
							current: t("domainCurrent"),
							desc: t("domainDesc"),
							dnsFail: t("domainDnsFail"),
							dnsOk: t("domainDnsOk"),
							email: t("domainEmail"),
							error: t("domainError"),
							hsts: t("domainHsts"),
							hstsDesc: t("domainHstsDesc"),
							hstsDisable: t("domainHstsDisable"),
							hstsEnable: t("domainHstsEnable"),
							hstsError: t("domainHstsError"),
							hstsSuccess: t("domainHstsSuccess"),
							input: t("domainInput"),
							none: t("domainNone"),
							pending: t("domainPending"),
							setup: t("domainSetup"),
							setupError: t("domainSetupError"),
							unconfigured: t("domainUnconfigured"),
							verify: t("domainVerify"),
							verifyError: t("domainVerifyError"),
						}}
					/>
				</div>
			</section>

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t("security")}</h2>
				<div className="rounded-lg border p-4 flex items-center justify-between gap-4">
					<div className="min-w-0">
						<p className="text-sm font-medium">{t("rotateKeys")}</p>
						<p className="mt-0.5 text-xs text-muted-foreground">{t("rotateKeysDesc")}</p>
					</div>
					<RotateButton confirmMsg={t("rotateKeysConfirm")} label={t("rotateKeysBtn")} locale={locale} />
				</div>
				<div className="flex flex-col gap-2 mt-2">
					<p className="text-xs font-medium text-muted-foreground">{t("rotationLog")}</p>
					<Suspense fallback={<div className="rounded-lg border h-20 animate-pulse bg-muted/30" />}>
						<RotationLog locale={locale} token={token} />
					</Suspense>
				</div>
			</section>

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t("updates")}</h2>
				<div className="rounded-lg border p-4">
					<p className="text-sm text-muted-foreground mb-3">{t("updatesDesc")}</p>
					<UpdateSection
						labels={{
							checkBtn: t("updateCheck"),
							checkError: t("updateCheckError"),
							current: t("updateCurrent"),
							latest: t("updateLatest"),
							triggerBtn: t("updateTrigger"),
							triggerError: t("updateTriggerError"),
							triggerSuccess: t("updateTriggerSuccess"),
							updateAvailable: t("updateAvailable"),
							upToDate: t("updateUpToDate"),
						}}
					/>
				</div>
			</section>

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t("branding")}</h2>
				<div className="rounded-lg border p-4">
					<BrandingForm
						initial={branding}
						labels={{
							accentColor: t("brandingAccentColor"),
							companyName: t("brandingCompanyName"),
							error: t("brandingError"),
							logoUrl: t("brandingLogoUrl"),
							primaryColor: t("brandingPrimaryColor"),
							save: t("brandingSave"),
							saved: t("brandingSaved"),
							secondaryColor: t("brandingSecondaryColor"),
						}}
					/>
				</div>
			</section>

			{migrationState && (
				<section className="flex flex-col gap-3">
					<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
						{t("migration")}
					</h2>
					<div className="rounded-lg border p-4">
						<MigrationSection
							initial={migrationState}
							labels={{
								abortBtn: t("migrationAbort"),
								abortError: t("migrationAbortError"),
								abortSuccess: t("migrationAbortSuccess"),
								agentsProgress: t("migrationAgentsProgress", {
									confirmed: "{confirmed}",
									total: "{total}",
								}),
								confirmShutdown: t("migrationConfirmShutdown"),
								confirmShutdownMsg: t("migrationConfirmShutdownMsg"),
								copyToken: t("migrationCopyToken"),
								desc: t("migrationDesc"),
								error: t("migrationStatusError"),
								prepareBtn: t("migrationPrepare"),
								preparedToken: t("migrationPreparedToken"),
								prepareError: t("migrationPrepareError"),
								shutdownError: t("migrationShutdownError"),
								sourceDesc: t("migrationSourceDesc"),
								sourceTitle: t("migrationSourceTitle"),
								startError: t("migrationStartError"),
								startMigration: t("migrationStart"),
								statusAborted: t("migrationStatusAborted"),
								statusCompleted: t("migrationStatusCompleted"),
								statusError: t("migrationStatusError"),
								statusIdle: t("migrationStatusIdle"),
								statusNotifying: t("migrationStatusNotifying"),
								statusPreparing: t("migrationStatusPreparing"),
								statusTransferring: t("migrationStatusTransferring"),
								statusWaiting: t("migrationStatusWaiting"),
								targetDesc: t("migrationTargetDesc"),
								targetTitle: t("migrationTargetTitle"),
								targetUrl: t("migrationTargetUrl"),
								title: t("migration"),
								token: t("migrationToken"),
							}}
						/>
					</div>
				</section>
			)}

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t("sessions")}</h2>
				<Suspense fallback={<SessionListSkeleton />}>
					<SessionList locale={locale} token={token} />
				</Suspense>
			</section>
		</div>
	);
}
