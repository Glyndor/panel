import Image from "next/image";
import { getTranslations } from "next-intl/server";
import { RegisterForm } from "@/components/(auth)/register/RegisterForm";

export default async function RegisterPage({ params }: { params: Promise<{ locale: string }> }) {
	const { locale } = await params;
	const [t, tBrand] = await Promise.all([
		getTranslations({ locale, namespace: "auth.register" }),
		getTranslations({ locale, namespace: "branding" }),
	]);

	return (
		<div className="w-full max-w-sm">
			<div className="mb-8 flex flex-col items-center gap-3 lg:hidden">
				<Image src="/logo.webp" alt="Lynx" width={56} height={56} priority />
				<p className="text-sm font-semibold">Lynx</p>
			</div>

			<div className="mb-7">
				<h1 className="text-2xl font-bold tracking-tight">{t("title")}</h1>
				<p className="mt-1.5 text-sm text-muted-foreground">{t("subtitle")}</p>
			</div>

			<RegisterForm locale={locale} />

			<footer className="mt-10 text-center text-xs text-muted-foreground lg:hidden">
				{tBrand("madeWith")}{" "}
				<a
					className="font-medium text-foreground hover:underline"
					href="https://github.com/Jaro-c"
					rel="noopener noreferrer"
					target="_blank"
				>
					{tBrand("author")}
				</a>
				{" · "}
				<a
					className="font-medium text-foreground hover:underline"
					href="https://github.com/Glyndor/panel"
					rel="noopener noreferrer"
					target="_blank"
				>
					lynx
				</a>
			</footer>
		</div>
	);
}
