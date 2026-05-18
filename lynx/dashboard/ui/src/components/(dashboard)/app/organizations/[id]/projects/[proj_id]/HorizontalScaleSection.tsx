"use client";

import { useForm, Controller } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useState, useTransition } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
	DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel, FieldError } from "@/components/ui/field";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Trash2, Plus, Network } from "lucide-react";
import { addTunnelSchema, type AddTunnelInput } from "@/schemas/(dashboard)/app/organizations/[id]/projects/[proj_id]";
import { addHorizontalScale, teardownHorizontalScale } from "@/actions/(dashboard)/app/organizations/[id]/projects/[proj_id]/scale";

interface Agent {
	id: string;
	name: string;
	wg_ip: string;
	status: string;
}

interface Tunnel {
	id: string;
	agent_b_id: string;
	agent_a_wg_ip: string;
	agent_b_wg_ip: string;
	replica_count: number;
	status: string;
}

interface Labels {
	title: string;
	desc: string;
	addBtn: string;
	dialogTitle: string;
	targetAgent: string;
	image: string;
	replicas: string;
	confirm: string;
	success: string;
	error: string;
	teardownSuccess: string;
	teardownError: string;
	noTunnels: string;
	agentB: string;
	replicaCount: string;
	status: string;
}

interface Props {
	orgId: string;
	projId: string;
	tunnels: Tunnel[];
	agents: Agent[];
	labels: Labels;
}

function StatusBadge({ status }: { status: string }) {
	const variant =
		status === "active" ? "default" : status === "pending" ? "secondary" : "destructive";
	return <Badge variant={variant}>{status}</Badge>;
}

function TeardownButton({
	orgId,
	projId,
	tunnelId,
	labels,
}: {
	orgId: string;
	projId: string;
	tunnelId: string;
	labels: { teardownSuccess: string; teardownError: string };
}) {
	const [pending, startTransition] = useTransition();
	return (
		<Button
			variant="ghost"
			size="sm"
			className="text-destructive hover:text-destructive"
			disabled={pending}
			onClick={() =>
				startTransition(async () => {
					const r = await teardownHorizontalScale(orgId, projId, tunnelId);
					if (r.ok) toast.success(labels.teardownSuccess);
					else toast.error(labels.teardownError);
				})
			}
		>
			<Trash2 className="size-3.5" />
		</Button>
	);
}

function AddTunnelDialog({
	orgId,
	projId,
	agents,
	labels,
}: {
	orgId: string;
	projId: string;
	agents: Agent[];
	labels: Labels;
}) {
	const [open, setOpen] = useState(false);
	const onlineAgents = agents.filter((a) => a.status === "online");

	const {
		register,
		handleSubmit,
		control,
		reset,
		formState: { errors, isSubmitting },
	} = useForm<AddTunnelInput>({
		resolver: zodResolver(addTunnelSchema),
		defaultValues: { replica_count: 1 },
	});

	const onSubmit = (data: AddTunnelInput) => {
		toast.promise(
			addHorizontalScale(orgId, projId, data.target_agent_id, data.image, data.replica_count).then(
				(r) => {
					if (!r.ok) throw new Error(r.error);
					setOpen(false);
					reset({ replica_count: 1 });
					return r;
				},
			),
			{
				loading: labels.confirm,
				success: labels.success,
				error: labels.error,
			},
		);
	};

	return (
		<Dialog open={open} onOpenChange={(v) => { setOpen(v); if (!v) reset({ replica_count: 1 }); }}>
			<DialogTrigger asChild>
				<Button size="sm" variant="outline" className="gap-1.5">
					<Plus className="size-3.5" />
					{labels.addBtn}
				</Button>
			</DialogTrigger>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>{labels.dialogTitle}</DialogTitle>
				</DialogHeader>
				<form onSubmit={handleSubmit(onSubmit)} className="flex flex-col gap-4">
					<Field>
						<FieldLabel>{labels.targetAgent}</FieldLabel>
						{onlineAgents.length === 0 ? (
							<p className="text-sm text-muted-foreground">{labels.error}</p>
						) : (
							<Controller
								control={control}
								name="target_agent_id"
								render={({ field }) => (
									<Select value={field.value} onValueChange={field.onChange} disabled={isSubmitting}>
										<SelectTrigger>
											<SelectValue placeholder={labels.targetAgent} />
										</SelectTrigger>
										<SelectContent>
											{onlineAgents.map((a) => (
												<SelectItem key={a.id} value={a.id}>
													{a.name} ({a.wg_ip})
												</SelectItem>
											))}
										</SelectContent>
									</Select>
								)}
							/>
						)}
						<FieldError errors={[errors.target_agent_id]} />
					</Field>
					<Field>
						<FieldLabel htmlFor="tunnel-image">{labels.image}</FieldLabel>
						<Input
							id="tunnel-image"
							{...register("image")}
							placeholder="docker.io/nginx:latest"
							disabled={isSubmitting}
						/>
						<FieldError errors={[errors.image]} />
					</Field>
					<Field>
						<FieldLabel htmlFor="tunnel-replicas">{labels.replicas}</FieldLabel>
						<Input
							id="tunnel-replicas"
							type="number"
							min={1}
							max={20}
							{...register("replica_count")}
							disabled={isSubmitting}
						/>
						<FieldError errors={[errors.replica_count]} />
					</Field>
					<Button type="submit" disabled={isSubmitting || onlineAgents.length === 0}>
						{isSubmitting ? "…" : labels.confirm}
					</Button>
				</form>
			</DialogContent>
		</Dialog>
	);
}

export function HorizontalScaleSection({ orgId, projId, tunnels, agents, labels }: Props) {
	return (
		<section className="flex flex-col gap-3">
			<div className="flex items-center justify-between">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider flex items-center gap-1.5">
					<Network className="size-3.5" />
					{labels.title}
				</h2>
				<AddTunnelDialog orgId={orgId} projId={projId} agents={agents} labels={labels} />
			</div>
			<p className="text-sm text-muted-foreground">{labels.desc}</p>
			{tunnels.length === 0 ? (
				<p className="text-sm text-muted-foreground">{labels.noTunnels}</p>
			) : (
				<div className="rounded-lg border divide-y">
					{tunnels.map((t) => (
						<div key={t.id} className="flex items-center justify-between p-3 text-sm">
							<div className="flex flex-col gap-0.5">
								<span className="font-mono text-xs text-muted-foreground">
									{t.agent_a_wg_ip} → {t.agent_b_wg_ip}
								</span>
								<span className="text-xs text-muted-foreground">
									{labels.replicaCount}: {t.replica_count}
								</span>
							</div>
							<div className="flex items-center gap-2">
								<StatusBadge status={t.status} />
								<TeardownButton orgId={orgId} projId={projId} tunnelId={t.id} labels={labels} />
							</div>
						</div>
					))}
				</div>
			)}
		</section>
	);
}
