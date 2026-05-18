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
import { registerSchema, type RegisterInput } from "@/schemas/(auth)/register";
import { registerAction } from "@/actions/(auth)/register";

type Props = { locale: string };

export function RegisterForm({ locale }: Props) {
	const t = useTranslations("auth.register");
	const router = useRouter();

	const {
		register,
		handleSubmit,
		formState: { errors, isSubmitting },
	} = useForm<RegisterInput>({
		resolver: zodResolver(registerSchema),
	});

	const onSubmit = async (data: RegisterInput) => {
		const promise = registerAction(locale, data).then((r) => {
			if (!r.success) throw new Error(r.error);
			return r;
		});

		toast.promise(promise, {
			loading: t("submitting"),
			success: t("submit"),
			error: (e: Error) => {
				if (e.message === "usernameTaken") return t("usernameTaken");
				if (e.message === "emailTaken") return t("emailTaken");
				return t("serverError");
			},
		});

		try {
			await promise;
			router.push(`/${locale}/login`);
		} catch {
			// handled by toast
		}
	};

	return (
		<form onSubmit={handleSubmit(onSubmit)} className="flex flex-col gap-5" noValidate>
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
				<FieldLabel htmlFor="email">{t("email")}</FieldLabel>
				<Input
					id="email"
					type="email"
					{...register("email")}
					autoComplete="email"
					disabled={isSubmitting}
					className="h-10"
				/>
				<FieldError errors={[errors.email]} />
			</Field>

			<Field>
				<FieldLabel htmlFor="password">{t("password")}</FieldLabel>
				<Input
					id="password"
					type="password"
					{...register("password")}
					autoComplete="new-password"
					disabled={isSubmitting}
					className="h-10"
				/>
				<FieldError errors={[errors.password]} />
			</Field>

			<Button type="submit" disabled={isSubmitting} className="w-full h-10 mt-1">
				{isSubmitting ? t("submitting") : t("submit")}
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
