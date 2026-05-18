"use client";

import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useRouter } from "next/navigation";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel, FieldError } from "@/components/ui/field";
import { changePasswordSchema, type ChangePasswordInput } from "@/schemas/(dashboard)/app/settings";
import { changePassword } from "@/actions/(dashboard)/app/settings/profile";

interface Labels {
	currentPassword: string;
	newPassword: string;
	btn: string;
	success: string;
	wrong: string;
	error: string;
}

interface Props {
	locale: string;
	labels: Labels;
}

export function ChangePasswordForm({ locale, labels }: Props) {
	const router = useRouter();

	const {
		register,
		handleSubmit,
		reset,
		formState: { errors, isSubmitting },
	} = useForm<ChangePasswordInput>({
		resolver: zodResolver(changePasswordSchema),
	});

	const onSubmit = async (data: ChangePasswordInput) => {
		const promise = changePassword(data.current_password, data.new_password).then((r) => {
			if (!r.ok) throw new Error(r.status === 401 ? "wrong" : "error");
			return r;
		});

		toast.promise(promise, {
			loading: labels.btn,
			success: labels.success,
			error: (e: Error) => (e.message === "wrong" ? labels.wrong : labels.error),
		});

		try {
			await promise;
			reset();
			router.push(`/${locale}/login`);
		} catch {
			// handled by toast
		}
	};

	return (
		<form onSubmit={handleSubmit(onSubmit)} className="flex flex-col gap-3 max-w-sm">
			<Field>
				<FieldLabel htmlFor="current_password">{labels.currentPassword}</FieldLabel>
				<Input
					id="current_password"
					type="password"
					{...register("current_password")}
					autoComplete="current-password"
					disabled={isSubmitting}
				/>
				<FieldError errors={[errors.current_password]} />
			</Field>

			<Field>
				<FieldLabel htmlFor="new_password">{labels.newPassword}</FieldLabel>
				<Input
					id="new_password"
					type="password"
					{...register("new_password")}
					autoComplete="new-password"
					disabled={isSubmitting}
				/>
				<FieldError errors={[errors.new_password]} />
			</Field>

			<Button type="submit" disabled={isSubmitting}>
				{labels.btn}
			</Button>
		</form>
	);
}
