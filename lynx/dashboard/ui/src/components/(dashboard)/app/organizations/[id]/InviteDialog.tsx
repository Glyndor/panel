"use client";

import { zodResolver } from "@hookform/resolvers/zod";
import { useRouter } from "next/navigation";
import { useState } from "react";
import { useForm } from "react-hook-form";
import { toast } from "sonner";
import { inviteMember } from "@/actions/(dashboard)/app/organizations/[id]";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { type InviteMemberInput, inviteMemberSchema } from "@/schemas/(dashboard)/app/organizations/[id]";

interface Props {
	labels: {
		trigger: string;
		title: string;
		username: string;
		role: string;
		invite: string;
		success: string;
		error: string;
	};
	orgId: string;
}

export function InviteDialog({ orgId, labels }: Props) {
	const router = useRouter();
	const [open, setOpen] = useState(false);

	const {
		register,
		handleSubmit,
		reset,
		formState: { errors, isSubmitting },
	} = useForm<InviteMemberInput>({
		defaultValues: { role: "member" },
		resolver: zodResolver(inviteMemberSchema),
	});

	const onSubmit = (data: InviteMemberInput) => {
		toast.promise(
			inviteMember(orgId, data.username, data.role).then((r) => {
				if (!r.ok) throw new Error(r.error);
				setOpen(false);
				reset();
				router.refresh();
			}),
			{
				error: labels.error,
				loading: labels.invite,
				success: labels.success,
			},
		);
	};

	return (
		<Dialog
			onOpenChange={(v) => {
				setOpen(v);
				if (!v) reset();
			}}
			open={open}
		>
			<DialogTrigger asChild>
				<Button size="sm">{labels.trigger}</Button>
			</DialogTrigger>
			<DialogContent className="max-w-sm">
				<DialogHeader>
					<DialogTitle>{labels.title}</DialogTitle>
				</DialogHeader>
				<form className="flex flex-col gap-4 mt-2" onSubmit={handleSubmit(onSubmit)}>
					<Field>
						<FieldLabel htmlFor="invite-username">{labels.username}</FieldLabel>
						<Input
							id="invite-username"
							{...register("username")}
							autoComplete="off"
							disabled={isSubmitting}
						/>
						<FieldError errors={[errors.username]} />
					</Field>
					<Field>
						<FieldLabel htmlFor="invite-role">{labels.role}</FieldLabel>
						<select
							id="invite-role"
							{...register("role")}
							className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm"
							disabled={isSubmitting}
						>
							<option value="viewer">Viewer</option>
							<option value="member">Member</option>
							<option value="admin">Admin</option>
						</select>
						<FieldError errors={[errors.role]} />
					</Field>
					<Button disabled={isSubmitting} type="submit">
						{isSubmitting ? "…" : labels.invite}
					</Button>
				</form>
			</DialogContent>
		</Dialog>
	);
}
