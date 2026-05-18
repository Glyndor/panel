"use client";

import { zodResolver } from "@hookform/resolvers/zod";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { useForm } from "react-hook-form";
import { toast } from "sonner";
import { registerAction } from "@/actions/(auth)/register";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { type RegisterInput, registerSchema } from "@/schemas/(auth)/register";

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
			error: (e: Error) => {
				if (e.message === "usernameTaken") return t("usernameTaken");
				if (e.message === "emailTaken") return t("emailTaken");
				return t("serverError");
			},
			loading: t("submitting"),
			success: t("submit"),
		});

		try {
			await promise;
			router.push(`/${locale}/login`);
		} catch {
			// handled by toast
		}
	};

	return (
		<form className="flex flex-col gap-5" noValidate onSubmit={handleSubmit(onSubmit)}>
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
				<FieldLabel htmlFor="email">{t("email")}</FieldLabel>
				<Input
					id="email"
					type="email"
					{...register("email")}
					autoComplete="email"
					className="h-10"
					disabled={isSubmitting}
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
					className="h-10"
					disabled={isSubmitting}
				/>
				<FieldError errors={[errors.password]} />
			</Field>

			<Button className="w-full h-10 mt-1" disabled={isSubmitting} type="submit">
				{isSubmitting ? t("submitting") : t("submit")}
			</Button>

			<p className="text-center text-sm text-muted-foreground">
				{t("hasAccount")}{" "}
				<Link
					className="font-medium text-foreground underline-offset-4 hover:underline"
					href={`/${locale}/login`}
				>
					{t("login")}
				</Link>
			</p>
		</form>
	);
}
