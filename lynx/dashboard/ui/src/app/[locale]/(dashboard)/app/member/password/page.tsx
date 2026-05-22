import { getTranslations } from "next-intl/server";
import { ChangePasswordForm } from "@/components/(dashboard)/member/password/ChangePasswordForm";

export default async function PasswordPage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const t = await getTranslations({ locale, namespace: "app.settings" });

	return (
		<div className="flex flex-col p-6 gap-6 max-w-xl">
			<h1 className="text-xl font-semibold">{t("changePassword")}</h1>
			<div className="rounded-lg border p-4">
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
		</div>
	);
}
