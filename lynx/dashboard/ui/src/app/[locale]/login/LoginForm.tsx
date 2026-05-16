"use client";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useTranslations } from "next-intl";
import Link from "next/link";
import { useActionState } from "react";
import { toast } from "sonner";
import { loginAction } from "./actions";

type Props = { locale: string };

export function LoginForm({ locale }: Props) {
	const t = useTranslations("auth.login");

	const [state, formAction, pending] = useActionState(
		loginAction.bind(null, locale),
		null,
	);

	if (state && !state.success) {
		const msg =
			state.error === "rateLimited"
				? t("rateLimited", { minutes: state.retryAfter ?? 15 })
				: state.error === "invalidCredentials"
					? t("invalidCredentials")
					: t("serverError");
		toast.error(msg);
	}

	return (
		<form action={formAction} className="flex flex-col gap-5">
			<div className="flex flex-col gap-1.5">
				<Label htmlFor="username">{t("username")}</Label>
				<Input
					id="username"
					name="username"
					type="text"
					autoComplete="username"
					required
					disabled={pending}
					className="h-10"
				/>
			</div>

			<div className="flex flex-col gap-1.5">
				<Label htmlFor="password">{t("password")}</Label>
				<Input
					id="password"
					name="password"
					type="password"
					autoComplete="current-password"
					required
					disabled={pending}
					className="h-10"
				/>
			</div>

			<Button type="submit" disabled={pending} className="w-full h-10 mt-1">
				{pending ? t("submitting") : t("submit")}
			</Button>

			<p className="text-center text-sm text-muted-foreground">
				{t("noAccount")}{" "}
				<Link
					href={`/${locale}/register`}
					className="font-medium text-foreground underline-offset-4 hover:underline"
				>
					{t("register")}
				</Link>
			</p>
		</form>
	);
}
