"use client";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useTranslations } from "next-intl";
import Link from "next/link";
import { useActionState } from "react";
import { toast } from "sonner";
import { registerAction } from "./actions";

const RESERVED = [
	"admin",
	"root",
	"system",
	"lynx",
	"support",
	"api",
	"null",
	"undefined",
];

function validateUsername(v: string, t: ReturnType<typeof useTranslations>) {
	if (v.length < 3) return t("validation.usernameMin");
	if (v.length > 32) return t("validation.usernameMax");
	if (!/^[a-z0-9_-]+$/.test(v)) return t("validation.usernameChars");
	if (/^[-_]|[-_]$/.test(v)) return t("validation.usernameEdge");
	if (RESERVED.includes(v)) return t("validation.usernameReserved");
	return null;
}

function validatePassword(v: string, t: ReturnType<typeof useTranslations>) {
	if (v.length < 12) return t("validation.passwordMin");
	if (v.length > 30) return t("validation.passwordMax");
	if (!/[A-Z]/.test(v)) return t("validation.passwordUppercase");
	if (!/[a-z]/.test(v)) return t("validation.passwordLowercase");
	if (!/[0-9]/.test(v)) return t("validation.passwordNumber");
	if (!/[^A-Za-z0-9]/.test(v)) return t("validation.passwordSpecial");
	return null;
}

type Props = { locale: string };

export function RegisterForm({ locale }: Props) {
	const t = useTranslations("auth.register");

	const [state, formAction, pending] = useActionState(
		registerAction.bind(null, locale),
		null,
	);

	if (state && !state.success) {
		const validationKeys = [
			"usernameMin",
			"usernameMax",
			"usernameChars",
			"usernameEdge",
			"usernameReserved",
			"emailInvalid",
			"passwordMin",
			"passwordMax",
			"passwordUppercase",
			"passwordLowercase",
			"passwordNumber",
			"passwordSpecial",
		];
		const msg = validationKeys.includes(state.error)
			? t(`validation.${state.error}` as never)
			: state.error === "usernameTaken"
				? t("usernameTaken")
				: state.error === "emailTaken"
					? t("emailTaken")
					: t("serverError");
		toast.error(msg);
	}

	return (
		<form
			action={formAction}
			className="flex flex-col gap-5"
			noValidate
		>
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
					pattern="^[a-z0-9_-]{3,32}$"
					onChange={(e) => {
						const err = validateUsername(e.target.value, t);
						e.target.setCustomValidity(err ?? "");
					}}
				/>
			</div>

			<div className="flex flex-col gap-1.5">
				<Label htmlFor="email">{t("email")}</Label>
				<Input
					id="email"
					name="email"
					type="email"
					autoComplete="email"
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
					autoComplete="new-password"
					required
					disabled={pending}
					className="h-10"
					onChange={(e) => {
						const err = validatePassword(e.target.value, t);
						e.target.setCustomValidity(err ?? "");
					}}
				/>
			</div>

			<Button type="submit" disabled={pending} className="w-full h-10 mt-1">
				{pending ? t("submitting") : t("submit")}
			</Button>

			<p className="text-center text-sm text-muted-foreground">
				{t("hasAccount")}{" "}
				<Link
					href={`/${locale}/login`}
					className="font-medium text-foreground underline-offset-4 hover:underline"
				>
					{t("login")}
				</Link>
			</p>
		</form>
	);
}
