"use client";

import { zodResolver } from "@hookform/resolvers/zod";
import { Eye, EyeOff } from "lucide-react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { useState } from "react";
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
	const [showPassword, setShowPassword] = useState(false);
	const [showConfirm, setShowConfirm] = useState(false);

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
		<form className="flex flex-col gap-4" noValidate onSubmit={handleSubmit(onSubmit)}>
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
				<div className="relative">
					<Input
						id="password"
						type={showPassword ? "text" : "password"}
						{...register("password")}
						autoComplete="new-password"
						className="h-10 pr-10"
						disabled={isSubmitting}
					/>
					<button
						aria-label={showPassword ? "Hide password" : "Show password"}
						className="absolute inset-y-0 right-0 flex items-center px-3 text-muted-foreground hover:text-foreground transition-colors cursor-pointer select-none"
						onClick={() => setShowPassword((v) => !v)}
						tabIndex={-1}
						type="button"
					>
						{showPassword ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
					</button>
				</div>
				<FieldError errors={[errors.password]} />
			</Field>

			<Field>
				<FieldLabel htmlFor="confirm_password">{t("confirmPassword")}</FieldLabel>
				<div className="relative">
					<Input
						id="confirm_password"
						type={showConfirm ? "text" : "password"}
						{...register("confirm_password")}
						autoComplete="new-password"
						className="h-10 pr-10"
						disabled={isSubmitting}
					/>
					<button
						aria-label={showConfirm ? "Hide password" : "Show password"}
						className="absolute inset-y-0 right-0 flex items-center px-3 text-muted-foreground hover:text-foreground transition-colors cursor-pointer select-none"
						onClick={() => setShowConfirm((v) => !v)}
						tabIndex={-1}
						type="button"
					>
						{showConfirm ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
					</button>
				</div>
				<FieldError errors={[errors.confirm_password]} />
			</Field>

			<Button className="w-full h-10 mt-2" disabled={isSubmitting} type="submit" variant="brand">
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
