"use client";

import { zodResolver } from "@hookform/resolvers/zod";
import { useRouter } from "next/navigation";
import { useForm } from "react-hook-form";
import { toast } from "sonner";
import { changePassword } from "@/actions/(dashboard)/member/profile";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { type ChangePasswordInput, changePasswordSchema } from "@/schemas/(dashboard)/settings";

interface Labels {
	btn: string;
	currentPassword: string;
	error: string;
	newPassword: string;
	success: string;
	wrong: string;
}

interface Props {
	labels: Labels;
	locale: string;
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
			error: (e: Error) => (e.message === "wrong" ? labels.wrong : labels.error),
			loading: labels.btn,
			success: labels.success,
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
		<form className="flex flex-col gap-3 max-w-sm" onSubmit={handleSubmit(onSubmit)}>
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

			<Button disabled={isSubmitting} type="submit">
				{labels.btn}
			</Button>
		</form>
	);
}
