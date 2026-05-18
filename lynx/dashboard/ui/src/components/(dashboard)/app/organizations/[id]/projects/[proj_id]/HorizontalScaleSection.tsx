"use client";

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
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Trash2, Plus, Network } from "lucide-react";
import { addHorizontalScale, teardownHorizontalScale } from "./scaleActions";

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
		status === "active"
			? "default"
			: status === "pending"
				? "secondary"
				: "destructive";
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
	const [targetAgentId, setTargetAgentId] = useState("");
	const [image, setImage] = useState("");
	const [replicas, setReplicas] = useState("1");
	const [pending, startTransition] = useTransition();

	const onlineAgents = agents.filter((a) => a.status === "online");

	const handleSubmit = (e: React.FormEvent) => {
		e.preventDefault();
		if (!targetAgentId || !image) return;
		startTransition(async () => {
			const r = await addHorizontalScale(
				orgId,
				projId,
				targetAgentId,
				image,
				Math.max(1, parseInt(replicas) || 1),
			);
			if (r.ok) {
				toast.success(labels.success);
				setOpen(false);
				setImage("");
				setReplicas("1");
				setTargetAgentId("");
			} else {
				toast.error(labels.error);
			}
		});
	};

	return (
		<Dialog open={open} onOpenChange={setOpen}>
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
				<form onSubmit={handleSubmit} className="flex flex-col gap-4">
					<div className="flex flex-col gap-1.5">
						<Label>{labels.targetAgent}</Label>
						{onlineAgents.length === 0 ? (
							<p className="text-sm text-muted-foreground">{labels.error}</p>
						) : (
							<Select value={targetAgentId} onValueChange={setTargetAgentId}>
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
					</div>
					<div className="flex flex-col gap-1.5">
						<Label>{labels.image}</Label>
						<Input
							value={image}
							onChange={(e) => setImage(e.target.value)}
							placeholder="docker.io/nginx:latest"
							required
						/>
					</div>
					<div className="flex flex-col gap-1.5">
						<Label>{labels.replicas}</Label>
						<Input
							type="number"
							min={1}
							max={20}
							value={replicas}
							onChange={(e) => setReplicas(e.target.value)}
						/>
					</div>
					<Button type="submit" disabled={pending || !targetAgentId || !image}>
						{labels.confirm}
					</Button>
				</form>
			</DialogContent>
		</Dialog>
	);
}

export function HorizontalScaleSection({
	orgId,
	projId,
	tunnels,
	agents,
	labels,
}: Props) {
	return (
		<section className="flex flex-col gap-3">
			<div className="flex items-center justify-between">
				<h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wider flex items-center gap-1.5">
					<Network className="size-3.5" />
					{labels.title}
				</h2>
				<AddTunnelDialog
					orgId={orgId}
					projId={projId}
					agents={agents}
					labels={labels}
				/>
			</div>
			<p className="text-sm text-muted-foreground">{labels.desc}</p>
			{tunnels.length === 0 ? (
				<p className="text-sm text-muted-foreground">{labels.noTunnels}</p>
			) : (
				<div className="rounded-lg border divide-y">
					{tunnels.map((t) => (
						<div
							key={t.id}
							className="flex items-center justify-between p-3 text-sm"
						>
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
								<TeardownButton
									orgId={orgId}
									projId={projId}
									tunnelId={t.id}
									labels={labels}
								/>
							</div>
						</div>
					))}
				</div>
			)}
		</section>
	);
}
