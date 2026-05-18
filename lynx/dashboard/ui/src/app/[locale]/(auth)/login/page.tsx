import { getTranslations } from "next-intl/server";
import { LoginForm } from "@/components/(auth)/login/LoginForm";
import { BACKEND_URL } from "@/lib/api";

async function fetchCompanyName(): Promise<string> {
	try {
		const res = await fetch(`${BACKEND_URL}/branding`, {
			next: { revalidate: 60 },
		});
		if (!res.ok) return "Lynx";
		const data = (await res.json()) as { company_name?: string };
		return data.company_name ?? "Lynx";
	} catch {
		return "Lynx";
	}
}

export default async function LoginPage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const [t, companyName] = await Promise.all([
		getTranslations({ locale, namespace: "auth.login" }),
		fetchCompanyName(),
	]);

	return (
		<main className="min-h-screen flex items-center justify-center bg-background px-4">
			<div className="w-full max-w-sm">
				<div className="mb-8 text-center">
					<h1 className="text-2xl font-bold tracking-tight">{t("title", { company: companyName })}</h1>
					<p className="mt-1 text-sm text-muted-foreground">{t("subtitle")}</p>
				</div>

				<LoginForm locale={locale} />

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
				className="font-medium text-foreground hover:underline"
				href="https://github.com/Jaro-c"
				rel="noopener noreferrer"
				target="_blank"
			>
				{t("author")}
			</a>
			{" · "}
			<a
				className="font-medium text-foreground hover:underline"
				href="https://github.com/Jaro-c/Lynx"
				rel="noopener noreferrer"
				target="_blank"
			>
				lynx
			</a>
		</footer>
	);
}
