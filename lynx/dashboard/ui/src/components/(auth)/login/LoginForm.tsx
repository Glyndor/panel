"use client";

import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useTranslations } from "next-intl";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel, FieldError } from "@/components/ui/field";
import { loginSchema, type LoginInput } from "@/schemas/(auth)/login";
import { loginAction } from "@/actions/(auth)/login";

type Props = { locale: string };

export function LoginForm({ locale }: Props) {
	const t = useTranslations("auth.login");
	const router = useRouter();

	const {
		register,
		handleSubmit,
		formState: { errors, isSubmitting },
	} = useForm<LoginInput>({
		resolver: zodResolver(loginSchema),
	});

	const onSubmit = async (data: LoginInput) => {
		const promise = loginAction(locale, data).then((r) => {
			if (!r.success) throw new Error(r.error);
			return r;
		});

		toast.promise(promise, {
			loading: t("submitting"),
			success: t("submit"),
			error: (e: Error) => {
				if (e.message === "rateLimited") return t("rateLimited", { minutes: 15 });
				if (e.message === "invalidCredentials") return t("invalidCredentials");
				return t("serverError");
			},
		});

		try {
			const r = await promise;
			if (r.forcePasswordChange) {
				router.push(`/${locale}/app/settings?change_password=1`);
			} else {
				router.push(`/${locale}/app`);
			}
		} catch {
			// handled by toast
		}
	};

	return (
		<form onSubmit={handleSubmit(onSubmit)} className="flex flex-col gap-5">
			<Field>
				<FieldLabel htmlFor="username">{t("username")}</FieldLabel>
				<Input
					id="username"
					{...register("username")}
					autoComplete="username"
					disabled={isSubmitting}
					className="h-10"
				/>
				<FieldError errors={[errors.username]} />
			</Field>

			<Field>
				<FieldLabel htmlFor="password">{t("password")}</FieldLabel>
				<Input
					id="password"
					type="password"
					{...register("password")}
					autoComplete="current-password"
					disabled={isSubmitting}
					className="h-10"
				/>
				<FieldError errors={[errors.password]} />
			</Field>

			<Button type="submit" disabled={isSubmitting} className="w-full h-10 mt-1">
				{isSubmitting ? t("submitting") : t("submit")}
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
