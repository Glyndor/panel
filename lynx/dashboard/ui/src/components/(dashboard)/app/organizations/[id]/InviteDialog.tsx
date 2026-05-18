"use client";

import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useRouter } from "next/navigation";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
	DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel, FieldError } from "@/components/ui/field";
import { inviteMemberSchema, type InviteMemberInput } from "@/schemas/(dashboard)/app/organizations/[id]";
import { inviteMember } from "@/actions/(dashboard)/app/organizations/[id]";

interface Props {
	orgId: string;
	labels: {
		trigger: string;
		title: string;
		username: string;
		role: string;
		invite: string;
		success: string;
		error: string;
	};
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
		resolver: zodResolver(inviteMemberSchema),
		defaultValues: { role: "member" },
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
				loading: labels.invite,
				success: labels.success,
				error: labels.error,
			},
		);
	};

	return (
		<Dialog open={open} onOpenChange={(v) => { setOpen(v); if (!v) reset(); }}>
			<DialogTrigger asChild>
				<Button size="sm">{labels.trigger}</Button>
			</DialogTrigger>
			<DialogContent className="max-w-sm">
				<DialogHeader>
					<DialogTitle>{labels.title}</DialogTitle>
				</DialogHeader>
				<form onSubmit={handleSubmit(onSubmit)} className="flex flex-col gap-4 mt-2">
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
							disabled={isSubmitting}
							className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm"
						>
							<option value="viewer">Viewer</option>
							<option value="member">Member</option>
							<option value="admin">Admin</option>
						</select>
						<FieldError errors={[errors.role]} />
					</Field>
					<Button type="submit" disabled={isSubmitting}>
						{isSubmitting ? "…" : labels.invite}
					</Button>
				</form>
			</DialogContent>
		</Dialog>
	);
}
