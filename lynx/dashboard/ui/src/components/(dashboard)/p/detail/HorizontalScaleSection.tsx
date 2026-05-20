"use client";

import { zodResolver } from "@hookform/resolvers/zod";
import { Network, Plus, Trash2 } from "lucide-react";
import { useState, useTransition } from "react";
import { Controller, useForm } from "react-hook-form";
import { toast } from "sonner";
import {
	addHorizontalScale,
	teardownHorizontalScale,
} from "@/actions/(dashboard)/o/[id]/projects/[proj_id]/scale";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { type AddTunnelInput, addTunnelSchema } from "@/schemas/(dashboard)/p";

interface Agent {
	id: string;
	name: string;
	status: string;
	wg_ip: string;
}

interface Tunnel {
	agent_a_wg_ip: string;
	agent_b_id: string;
	agent_b_wg_ip: string;
	id: string;
	replica_count: number;
	status: string;
}

interface Labels {
	addBtn: string;
	agentB: string;
	confirm: string;
	desc: string;
	dialogTitle: string;
	error: string;
	image: string;
	noTunnels: string;
	replicaCount: string;
	replicas: string;
	status: string;
	success: string;
	targetAgent: string;
	teardownError: string;
	teardownSuccess: string;
	title: string;
}

interface Props {
	agents: Agent[];
	labels: Labels;
	orgId: string;
	projId: string;
	tunnels: Tunnel[];
}

function StatusBadge({ status }: { status: string }) {
	const variant = status === "active" ? "default" : status === "pending" ? "secondary" : "destructive";
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
			className="text-destructive hover:text-destructive"
			disabled={pending}
			onClick={() =>
				startTransition(async () => {
					const r = await teardownHorizontalScale(orgId, projId, tunnelId);
					if (r.ok) toast.success(labels.teardownSuccess);
					else toast.error(labels.teardownError);
				})
			}
			size="sm"
			variant="ghost"
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
		defaultValues: { replica_count: 1 },
		resolver: zodResolver(addTunnelSchema),
	});

	const onSubmit = (data: AddTunnelInput) => {
		toast.promise(
			addHorizontalScale(orgId, projId, data.target_agent_id, data.image, data.replica_count).then((r) => {
				if (!r.ok) throw new Error(r.error);
				setOpen(false);
				reset({ replica_count: 1 });
				return r;
			}),
			{
				error: labels.error,
				loading: labels.confirm,
				success: labels.success,
			},
		);
	};

	return (
		<Dialog
			onOpenChange={(v) => {
				setOpen(v);
				if (!v) reset({ replica_count: 1 });
			}}
			open={open}
		>
			<DialogTrigger asChild>
				<Button className="gap-1.5" size="sm" variant="outline">
					<Plus className="size-3.5" />
					{labels.addBtn}
				</Button>
			</DialogTrigger>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>{labels.dialogTitle}</DialogTitle>
				</DialogHeader>
				<form className="flex flex-col gap-4" onSubmit={handleSubmit(onSubmit)}>
					<Field>
						<FieldLabel>{labels.targetAgent}</FieldLabel>
						{onlineAgents.length === 0 ? (
							<p className="text-sm text-muted-foreground">{labels.error}</p>
						) : (
							<Controller
								control={control}
								name="target_agent_id"
								render={({ field }) => (
									<Select disabled={isSubmitting} onValueChange={field.onChange} value={field.value}>
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
							disabled={isSubmitting}
							placeholder="docker.io/nginx:latest"
						/>
						<FieldError errors={[errors.image]} />
					</Field>
					<Field>
						<FieldLabel htmlFor="tunnel-replicas">{labels.replicas}</FieldLabel>
						<Input
							id="tunnel-replicas"
							max={20}
							min={1}
							type="number"
							{...register("replica_count")}
							disabled={isSubmitting}
						/>
						<FieldError errors={[errors.replica_count]} />
					</Field>
					<Button disabled={isSubmitting || onlineAgents.length === 0} type="submit">
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
				<AddTunnelDialog agents={agents} labels={labels} orgId={orgId} projId={projId} />
			</div>
			<p className="text-sm text-muted-foreground">{labels.desc}</p>
			{tunnels.length === 0 ? (
				<p className="text-sm text-muted-foreground">{labels.noTunnels}</p>
			) : (
				<div className="rounded-lg border divide-y">
					{tunnels.map((t) => (
						<div className="flex items-center justify-between p-3 text-sm" key={t.id}>
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
								<TeardownButton labels={labels} orgId={orgId} projId={projId} tunnelId={t.id} />
							</div>
						</div>
					))}
				</div>
			)}
		</section>
	);
}
