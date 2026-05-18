import { Suspense } from "react";
import { cookies } from "next/headers";
import { getTranslations } from "next-intl/server";
import { BACKEND_URL } from "@/lib/api";
import { SessionList } from "@/components/(dashboard)/app/settings/SessionList";
import { SessionListSkeleton } from "@/components/(dashboard)/app/settings/SessionListSkeleton";
import { RotateButton } from "@/components/(dashboard)/app/settings/RotateButton";
import { BrandingForm } from "@/components/(dashboard)/app/settings/BrandingForm";
import { UpdateSection } from "@/components/(dashboard)/app/settings/UpdateSection";
import { DomainSection } from "@/components/(dashboard)/app/settings/DomainSection";
import { MigrationSection } from "@/components/(dashboard)/app/settings/MigrationSection";
import { getMigrationStatus } from "@/actions/(dashboard)/app/settings/migration";
import { ChangePasswordForm } from "@/components/(dashboard)/app/settings/ChangePasswordForm";
import { SingleSessionToggle } from "@/components/(dashboard)/app/settings/SingleSessionToggle";
import { getMe } from "@/actions/(dashboard)/app/settings/profile";
import { RotationLog } from "@/components/(dashboard)/app/settings/RotationLog";

interface Branding {
	company_name: string;
	logo_url: string | null;
	primary_color: string;
	secondary_color: string;
	accent_color: string;
}

interface DomainConfig {
	domain: string | null;
	cert_type: string;
	cert_expires_at: string | null;
	hsts_enabled: boolean;
	port_19443_open: boolean;
	status: string;
	error_message: string | null;
}

const BRANDING_DEFAULTS: Branding = {
	company_name: "Lynx",
	logo_url: null,
	primary_color: "#0f172a",
	secondary_color: "#38bdf8",
	accent_color: "#6366f1",
};

const DOMAIN_DEFAULTS: DomainConfig = {
	domain: null,
	cert_type: "self_signed",
	cert_expires_at: null,
	hsts_enabled: false,
	port_19443_open: true,
	status: "unconfigured",
	error_message: null,
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
			headers: { Authorization: `Bearer ${token}` },
			cache: "no-store",
		});
		if (!res.ok) return DOMAIN_DEFAULTS;
		return (await res.json()) as DomainConfig;
	} catch {
		return DOMAIN_DEFAULTS;
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
								locale={locale}
								labels={{
									currentPassword: t("currentPassword"),
									newPassword: t("newPassword"),
									btn: t("changePasswordBtn"),
									success: t("changePasswordSuccess"),
									wrong: t("changePasswordWrong"),
									error: t("changePasswordError"),
								}}
							/>
						</div>
						<div className="border-t pt-4">
							<SingleSessionToggle
								initial={me.single_session}
								labels={{
									label: t("singleSession"),
									desc: t("singleSessionDesc"),
									success: t("singleSessionSuccess"),
									error: t("singleSessionError"),
								}}
							/>
						</div>
					</div>
				</section>
			)}

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
					{t("domain")}
				</h2>
				<div className="rounded-lg border p-4">
					<DomainSection
						initial={domainCfg}
						labels={{
							desc: t("domainDesc"),
							current: t("domainCurrent"),
							none: t("domainNone"),
							input: t("domainInput"),
							email: t("domainEmail"),
							setup: t("domainSetup"),
							pending: t("domainPending"),
							active: t("domainActive"),
							error: t("domainError"),
							unconfigured: t("domainUnconfigured"),
							verify: t("domainVerify"),
							dnsOk: t("domainDnsOk"),
							dnsFail: t("domainDnsFail"),
							verifyError: t("domainVerifyError"),
							setupError: t("domainSetupError"),
							hsts: t("domainHsts"),
							hstsDesc: t("domainHstsDesc"),
							hstsEnable: t("domainHstsEnable"),
							hstsDisable: t("domainHstsDisable"),
							hstsSuccess: t("domainHstsSuccess"),
							hstsError: t("domainHstsError"),
							closePort: t("domainClosePort"),
							closePortDesc: t("domainClosePortDesc"),
							closePortBtn: t("domainClosePortBtn"),
							closePortConfirm: t("domainClosePortConfirm"),
							closePortSuccess: t("domainClosePortSuccess"),
							closePortError: t("domainClosePortError"),
							cert: t("domainCert"),
							certSelfSigned: t("domainCertSelfSigned"),
							certLE: t("domainCertLE"),
							certCloudflare: t("domainCertCloudflare"),
							certCustom: t("domainCertCustom"),
							certExpires: t("domainCertExpires"),
							certUpload: t("domainCertUpload"),
							certUploadCloudflare: t("domainCertUploadCloudflare"),
							certUploadCustom: t("domainCertUploadCustom"),
							certPem: t("domainCertPem"),
							certPemPlaceholder: t("domainCertPemPlaceholder"),
							certKeyPem: t("domainCertKeyPem"),
							certKeyPemPlaceholder: t("domainCertKeyPemPlaceholder"),
							certKeyOptional: t("domainCertKeyOptional"),
							certUploadSuccess: t("domainCertUploadSuccess"),
							certUploadError: t("domainCertUploadError"),
						}}
					/>
				</div>
			</section>

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
				<div className="flex flex-col gap-2 mt-2">
					<p className="text-xs font-medium text-muted-foreground">
						{t("rotationLog")}
					</p>
					<Suspense fallback={<div className="rounded-lg border h-20 animate-pulse bg-muted/30" />}>
						<RotationLog token={token} locale={locale} />
					</Suspense>
				</div>
			</section>

			<section className="flex flex-col gap-3">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
					{t("updates")}
				</h2>
				<div className="rounded-lg border p-4">
					<p className="text-sm text-muted-foreground mb-3">
						{t("updatesDesc")}
					</p>
					<UpdateSection
						labels={{
							checkBtn: t("updateCheck"),
							current: t("updateCurrent"),
							latest: t("updateLatest"),
							upToDate: t("updateUpToDate"),
							updateAvailable: t("updateAvailable"),
							triggerBtn: t("updateTrigger"),
							triggerSuccess: t("updateTriggerSuccess"),
							triggerError: t("updateTriggerError"),
							checkError: t("updateCheckError"),
						}}
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

			{migrationState && (
				<section className="flex flex-col gap-3">
					<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
						{t("migration")}
					</h2>
					<div className="rounded-lg border p-4">
						<MigrationSection
							initial={migrationState}
							labels={{
								title: t("migration"),
								desc: t("migrationDesc"),
								sourceTitle: t("migrationSourceTitle"),
								sourceDesc: t("migrationSourceDesc"),
								targetUrl: t("migrationTargetUrl"),
								token: t("migrationToken"),
								startMigration: t("migrationStart"),
								targetTitle: t("migrationTargetTitle"),
								targetDesc: t("migrationTargetDesc"),
								prepareBtn: t("migrationPrepare"),
								preparedToken: t("migrationPreparedToken"),
								copyToken: t("migrationCopyToken"),
								abortBtn: t("migrationAbort"),
								confirmShutdown: t("migrationConfirmShutdown"),
								confirmShutdownMsg: t("migrationConfirmShutdownMsg"),
								statusIdle: t("migrationStatusIdle"),
								statusPreparing: t("migrationStatusPreparing"),
								statusTransferring: t("migrationStatusTransferring"),
								statusNotifying: t("migrationStatusNotifying"),
								statusWaiting: t("migrationStatusWaiting"),
								statusCompleted: t("migrationStatusCompleted"),
								statusAborted: t("migrationStatusAborted"),
								statusError: t("migrationStatusError"),
								agentsProgress: t("migrationAgentsProgress"),
								error: t("migrationStatusError"),
								prepareError: t("migrationPrepareError"),
								startError: t("migrationStartError"),
								abortSuccess: t("migrationAbortSuccess"),
								abortError: t("migrationAbortError"),
								shutdownError: t("migrationShutdownError"),
							}}
						/>
					</div>
				</section>
			)}

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
