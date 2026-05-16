"use client";

import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
	DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { BACKEND_URL } from "@/lib/api";
import { Plus } from "lucide-react";
import { useState, useTransition } from "react";
import { toast } from "sonner";

type RegisteredAgent = {
	id: string;
	wg_ip: string;
	sync_token: string;
};

type Props = {
	token: string;
	label: string;
	successTitle: string;
	successDesc: string;
	agentIdLabel: string;
	wgIpLabel: string;
	syncTokenLabel: string;
	warnOnce: string;
	doneLabel: string;
};

export function RegisterAgentDialog({
	token,
	label,
	successTitle,
	successDesc,
	agentIdLabel,
	wgIpLabel,
	syncTokenLabel,
	warnOnce,
	doneLabel,
}: Props) {
	const [open, setOpen] = useState(false);
	const [name, setName] = useState("");
	const [registered, setRegistered] = useState<RegisteredAgent | null>(null);
	const [pending, start] = useTransition();

	function handleRegister() {
		if (!name.trim()) return;
		start(async () => {
			try {
				const res = await fetch(`${BACKEND_URL}/agents`, {
					method: "POST",
					headers: {
						"Content-Type": "application/json",
						Authorization: `Bearer ${token}`,
					},
					body: JSON.stringify({ name: name.trim() }),
				});
				if (!res.ok) {
					toast.error("Failed to register agent");
					return;
				}
				const data = (await res.json()) as RegisteredAgent;
				setRegistered(data);
			} catch {
				toast.error("Network error");
			}
		});
	}

	function handleClose() {
		setOpen(false);
		setRegistered(null);
		setName("");
	}

	return (
		<Dialog open={open} onOpenChange={setOpen}>
			<DialogTrigger asChild>
				<Button size="sm">
					<Plus className="size-4 mr-1" />
					{label}
				</Button>
			</DialogTrigger>
			<DialogContent className="max-w-lg">
				{!registered ? (
					<>
						<DialogHeader>
							<DialogTitle>{label}</DialogTitle>
							<DialogDescription>
								Provide a name for the agent. A WireGuard IP will be assigned automatically.
							</DialogDescription>
						</DialogHeader>
						<div className="flex flex-col gap-3 py-2">
							<Label htmlFor="agent-name">Name</Label>
							<Input
								id="agent-name"
								placeholder="prod-vps-01"
								value={name}
								onChange={(e) => setName(e.target.value)}
								onKeyDown={(e) => e.key === "Enter" && handleRegister()}
								disabled={pending}
							/>
						</div>
						<DialogFooter>
							<Button
								onClick={handleRegister}
								disabled={pending || !name.trim()}
							>
								{pending ? "Registering…" : "Register"}
							</Button>
						</DialogFooter>
					</>
				) : (
					<>
						<DialogHeader>
							<DialogTitle>{successTitle}</DialogTitle>
							<DialogDescription>{successDesc}</DialogDescription>
						</DialogHeader>
						<div className="flex flex-col gap-3 py-2 text-sm">
							<Field label={agentIdLabel} value={registered.id} />
							<Field label={wgIpLabel} value={registered.wg_ip} />
							<Field label={syncTokenLabel} value={registered.sync_token} secret />
						</div>
						<p className="text-xs text-destructive font-medium">{warnOnce}</p>
						<DialogFooter>
							<Button onClick={handleClose}>{doneLabel}</Button>
						</DialogFooter>
					</>
				)}
			</DialogContent>
		</Dialog>
	);
}

function Field({
	label,
	value,
	secret,
}: {
	label: string;
	value: string;
	secret?: boolean;
}) {
	const [revealed, setRevealed] = useState(!secret);

	return (
		<div className="space-y-1">
			<p className="text-xs font-medium text-muted-foreground">{label}</p>
			<div className="flex items-center gap-2">
				<code className="flex-1 truncate rounded bg-muted px-2 py-1 text-xs font-mono select-all">
					{revealed ? value : "•".repeat(Math.min(value.length, 32))}
				</code>
				{secret && (
					<Button
						variant="ghost"
						size="sm"
						className="shrink-0 text-xs"
						onClick={() => setRevealed((r) => !r)}
					>
						{revealed ? "Hide" : "Reveal"}
					</Button>
				)}
			</div>
		</div>
	);
}
