
import { getTranslations } from "next-intl/server";
import { RegisterForm } from "@/components/(auth)/register/RegisterForm";

export default async function RegisterPage({
	params,
}: { params: Promise<{ locale: string }>; }) {
	const { locale } = await params;
	const t = await getTranslations({ locale, namespace: "auth.register" });

	return (
		<main className="min-h-screen flex items-center justify-center bg-background px-4">
			<div className="w-full max-w-sm">
				<div className="mb-8 text-center">
					<h1 className="text-2xl font-bold tracking-tight">{t("title")}</h1>
					<p className="mt-1 text-sm text-muted-foreground">{t("subtitle")}</p>
				</div>

				<RegisterForm locale={locale} />

				<Branding locale={locale} />
			</div>
		</main>
	);
}

async function Branding({ locale }: { locale: string }) {
	const t = await getTranslations({ locale, namespace: "branding" });
	return (
		<footer className="mt-10 text-center text-xs text-muted-foreground">
			{t("madeWith")}{" "}
			<a
				href="https://github.com/Jaro-c"
				target="_blank"
				rel="noopener noreferrer"
				className="font-medium text-foreground hover:underline"
			>
				{t("author")}
			</a>
			{" · "}
			<a
				href="https://github.com/Jaro-c/Lynx"
				target="_blank"
				rel="noopener noreferrer"
				className="font-medium text-foreground hover:underline"
			>
				lynx
			</a>
		</footer>
	);
}
