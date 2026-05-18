"use client";

import { zodResolver } from "@hookform/resolvers/zod";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { useForm } from "react-hook-form";
import { toast } from "sonner";
import { loginAction } from "@/actions/(auth)/login";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { type LoginInput, loginSchema } from "@/schemas/(auth)/login";

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
			error: (e: Error) => {
				if (e.message === "rateLimited") return t("rateLimited", { minutes: 15 });
				if (e.message === "invalidCredentials") return t("invalidCredentials");
				return t("serverError");
			},
			loading: t("submitting"),
			success: t("submit"),
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
		<form className="flex flex-col gap-5" onSubmit={handleSubmit(onSubmit)}>
			<Field>
				<FieldLabel htmlFor="username">{t("username")}</FieldLabel>
				<Input
					id="username"
					{...register("username")}
					autoComplete="username"
					className="h-10"
					disabled={isSubmitting}
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
					className="h-10"
					disabled={isSubmitting}
				/>
				<FieldError errors={[errors.password]} />
			</Field>

			<Button className="w-full h-10 mt-1" disabled={isSubmitting} type="submit">
				{isSubmitting ? t("submitting") : t("submit")}
			</Button>

			<p className="text-center text-sm text-muted-foreground">
				{t("noAccount")}{" "}
				<Link
					className="font-medium text-foreground underline-offset-4 hover:underline"
					href={`/${locale}/register`}
				>
					{t("register")}
				</Link>
			</p>
		</form>
	);
}
