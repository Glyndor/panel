"use client";

import { zodResolver } from "@hookform/resolvers/zod";
import { Plus } from "lucide-react";
import { useState } from "react";
import { useForm } from "react-hook-form";
import { toast } from "sonner";
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
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { BACKEND_URL } from "@/lib/api";
import { type RegisterAgentInput, registerAgentSchema } from "@/schemas/(dashboard)/app/agents";

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
	const [registered, setRegistered] = useState<RegisteredAgent | null>(null);

	const {
		register,
		handleSubmit,
		reset,
		formState: { errors, isSubmitting },
	} = useForm<RegisterAgentInput>({
		resolver: zodResolver(registerAgentSchema),
	});

	const onSubmit = (data: RegisterAgentInput) => {
		toast.promise(
			fetch(`${BACKEND_URL}/agents`, {
				body: JSON.stringify({ name: data.name }),
				headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
				method: "POST",
			}).then(async (res) => {
				if (!res.ok) throw new Error("failed");
				const agent = (await res.json()) as RegisteredAgent;
				setRegistered(agent);
				return agent;
			}),
			{
				error: "Failed to register agent",
				loading: "Registering…",
				success: successTitle,
			},
		);
	};

	function handleClose() {
		setOpen(false);
		setRegistered(null);
		reset();
	}

	return (
		<Dialog
			onOpenChange={(v) => {
				if (!v) handleClose();
				else setOpen(true);
			}}
			open={open}
		>
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
						<form className="flex flex-col gap-3 py-2" onSubmit={handleSubmit(onSubmit)}>
							<Field>
								<FieldLabel htmlFor="agent-name">Name</FieldLabel>
								<Input
									id="agent-name"
									placeholder="prod-vps-01"
									{...register("name")}
									disabled={isSubmitting}
								/>
								<FieldError errors={[errors.name]} />
							</Field>
							<DialogFooter>
								<Button disabled={isSubmitting} type="submit">
									{isSubmitting ? "Registering…" : "Register"}
								</Button>
							</DialogFooter>
						</form>
					</>
				) : (
					<>
						<DialogHeader>
							<DialogTitle>{successTitle}</DialogTitle>
							<DialogDescription>{successDesc}</DialogDescription>
						</DialogHeader>
						<div className="flex flex-col gap-3 py-2 text-sm">
							<AgentField label={agentIdLabel} value={registered.id} />
							<AgentField label={wgIpLabel} value={registered.wg_ip} />
							<AgentField label={syncTokenLabel} secret value={registered.sync_token} />
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

function AgentField({ label, value, secret }: { label: string; value: string; secret?: boolean }) {
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
						className="shrink-0 text-xs"
						onClick={() => setRevealed((r) => !r)}
						size="sm"
						variant="ghost"
					>
						{revealed ? "Hide" : "Reveal"}
					</Button>
				)}
			</div>
		</div>
	);
}
