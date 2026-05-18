"use client";

import { useState, useTransition } from "react";
import { toast } from "sonner";
import { useRouter } from "next/navigation";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
	DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { inviteMember } from "./actions";

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
	const [username, setUsername] = useState("");
	const [role, setRole] = useState("member");
	const [isPending, startTransition] = useTransition();

	function handleSubmit(e: React.FormEvent) {
		e.preventDefault();
		if (!username.trim()) return;
		startTransition(async () => {
			const result = await inviteMember(orgId, username.trim(), role);
			if (result.ok) {
				toast.success(labels.success);
				setOpen(false);
				setUsername("");
				setRole("member");
				router.refresh();
			} else {
				toast.error(labels.error, { description: result.error });
			}
		});
	}

	return (
		<Dialog open={open} onOpenChange={setOpen}>
			<DialogTrigger asChild>
				<Button size="sm">{labels.trigger}</Button>
			</DialogTrigger>
			<DialogContent className="max-w-sm">
				<DialogHeader>
					<DialogTitle>{labels.title}</DialogTitle>
				</DialogHeader>
				<form onSubmit={handleSubmit} className="flex flex-col gap-4 mt-2">
					<div className="flex flex-col gap-1.5">
						<Label htmlFor="invite-username">{labels.username}</Label>
						<Input
							id="invite-username"
							value={username}
							onChange={(e) => setUsername(e.target.value)}
							autoComplete="off"
							required
						/>
					</div>
					<div className="flex flex-col gap-1.5">
						<Label htmlFor="invite-role">{labels.role}</Label>
						<select
							id="invite-role"
							value={role}
							onChange={(e) => setRole(e.target.value)}
							className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm"
						>
							<option value="viewer">Viewer</option>
							<option value="member">Member</option>
							<option value="admin">Admin</option>
						</select>
					</div>
					<Button type="submit" disabled={isPending}>
						{isPending ? "…" : labels.invite}
					</Button>
				</form>
			</DialogContent>
		</Dialog>
	);
}
