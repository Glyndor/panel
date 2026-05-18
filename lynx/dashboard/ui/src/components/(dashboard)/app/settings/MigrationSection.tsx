"use client";

import { useState, useTransition } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ArrowRightLeft, AlertTriangle } from "lucide-react";

interface MigrationState {
	status: string;
	role: string;
	target_url: string | null;
	agents_total: number;
	agents_confirmed: number;
	error_message: string | null;
	started_at: string | null;
}

interface Labels {
	title: string;
	desc: string;
	sourceTitle: string;
	sourceDesc: string;
	targetUrl: string;
	token: string;
	startMigration: string;
	targetTitle: string;
	targetDesc: string;
	prepareBtn: string;
	preparedToken: string;
	copyToken: string;
	abortBtn: string;
	confirmShutdown: string;
	confirmShutdownMsg: string;
	statusIdle: string;
	statusPreparing: string;
	statusTransferring: string;
	statusNotifying: string;
	statusWaiting: string;
	statusCompleted: string;
	statusAborted: string;
	statusError: string;
	agentsProgress: string;
	error: string;
	prepareError: string;
	startError: string;
	abortSuccess: string;
	abortError: string;
	shutdownError: string;
}

interface Props {
	initial: MigrationState;
	labels: Labels;
}

const STATUS_VARIANT: Record<string, "default" | "secondary" | "destructive"> =
	{
		idle: "secondary",
		preparing: "secondary",
		transferring: "secondary",
		notifying_agents: "secondary",
		waiting_agents: "secondary",
		completed: "default",
		aborted: "secondary",
		error: "destructive",
	};

import {
	prepareMigration,
	startMigration,
	abortMigration,
	confirmMigrationShutdown,
} from "./migrationActions";

export function MigrationSection({ initial, labels }: Props) {
	const [state, setState] = useState<MigrationState>(initial);
	const [targetUrl, setTargetUrl] = useState("");
	const [migrationToken, setMigrationToken] = useState("");
	const [receivedToken, setReceivedToken] = useState<string | null>(null);
	const [pending, startTransition] = useTransition();

	const handlePrepare = () => {
		startTransition(async () => {
			const r = await prepareMigration();
			if (r.ok && r.migration_token) {
				setReceivedToken(r.migration_token);
				setState((prev) => ({ ...prev, status: "preparing", role: "target" }));
			} else {
				toast.error(labels.prepareError);
			}
		});
	};

	const handleStart = (e: React.FormEvent) => {
		e.preventDefault();
		if (!targetUrl || !migrationToken) return;
		startTransition(async () => {
			const r = await startMigration(targetUrl, migrationToken);
			if (r.ok) {
				setState((prev) => ({
					...prev,
					status: "transferring",
					role: "source",
					target_url: targetUrl,
				}));
				setTargetUrl("");
				setMigrationToken("");
			} else {
				toast.error(labels.startError);
			}
		});
	};

	const handleAbort = () => {
		startTransition(async () => {
			const r = await abortMigration();
			if (r.ok) {
				setState((prev) => ({ ...prev, status: "aborted" }));
				toast.success(labels.abortSuccess);
			} else {
				toast.error(labels.abortError);
			}
		});
	};

	const handleShutdown = () => {
		if (!window.confirm(labels.confirmShutdownMsg)) return;
		startTransition(async () => {
			const r = await confirmMigrationShutdown();
			if (r.ok) {
				setState((prev) => ({ ...prev, status: "completed" }));
			} else {
				toast.error(labels.shutdownError);
			}
		});
	};

	const copyToken = () => {
		if (receivedToken) {
			navigator.clipboard.writeText(receivedToken).catch(() => {});
			toast.success("Copied");
		}
	};

	const isActive = !["idle", "completed", "aborted", "error"].includes(
		state.status,
	);

	return (
		<div className="flex flex-col gap-4">
			<p className="text-sm text-muted-foreground">{labels.desc}</p>

			{/* Status */}
			<div className="flex items-center gap-2 flex-wrap">
				<ArrowRightLeft className="size-4 text-muted-foreground" />
				<Badge variant={STATUS_VARIANT[state.status] ?? "secondary"}>
					{labels[`status${state.status.charAt(0).toUpperCase() + state.status.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase()).slice(1)}` as keyof typeof labels] ?? state.status}
				</Badge>
				{state.target_url && (
					<span className="font-mono text-xs text-muted-foreground">
						→ {state.target_url}
					</span>
				)}
			</div>

			{/* Agent progress */}
			{(state.status === "waiting_agents" || state.status === "notifying_agents") && (
				<p className="text-sm text-muted-foreground">
					{labels.agentsProgress
						.replace("{confirmed}", String(state.agents_confirmed))
						.replace("{total}", String(state.agents_total))}
				</p>
			)}

			{/* Error */}
			{state.error_message && (
				<div className="flex items-start gap-2 text-sm text-destructive">
					<AlertTriangle className="size-4 shrink-0 mt-0.5" />
					<span>{state.error_message}</span>
				</div>
			)}

			{/* Idle: offer both source and target flows */}
			{state.status === "idle" && (
				<div className="grid sm:grid-cols-2 gap-4">
					{/* Source side: migrate to VPS-B */}
					<div className="rounded-lg border p-4 flex flex-col gap-3">
						<div>
							<p className="text-sm font-medium">{labels.sourceTitle}</p>
							<p className="text-xs text-muted-foreground mt-0.5">
								{labels.sourceDesc}
							</p>
						</div>
						<form onSubmit={handleStart} className="flex flex-col gap-2">
							<div className="flex flex-col gap-1">
								<Label className="text-xs">{labels.targetUrl}</Label>
								<Input
									value={targetUrl}
									onChange={(e) => setTargetUrl(e.target.value)}
									placeholder="https://1.2.3.4:19443"
									disabled={pending}
								/>
							</div>
							<div className="flex flex-col gap-1">
								<Label className="text-xs">{labels.token}</Label>
								<Input
									value={migrationToken}
									onChange={(e) => setMigrationToken(e.target.value)}
									placeholder="migration token from VPS-B"
									disabled={pending}
								/>
							</div>
							<Button
								type="submit"
								size="sm"
								disabled={!targetUrl || !migrationToken || pending}
							>
								{labels.startMigration}
							</Button>
						</form>
					</div>

					{/* Target side: prepare to receive */}
					<div className="rounded-lg border p-4 flex flex-col gap-3">
						<div>
							<p className="text-sm font-medium">{labels.targetTitle}</p>
							<p className="text-xs text-muted-foreground mt-0.5">
								{labels.targetDesc}
							</p>
						</div>
						{receivedToken ? (
							<div className="flex flex-col gap-2">
								<p className="text-xs text-muted-foreground">
									{labels.preparedToken}
								</p>
								<div className="flex items-center gap-2">
									<code className="text-xs font-mono bg-muted px-2 py-1 rounded break-all">
										{receivedToken.slice(0, 24)}…
									</code>
									<Button size="sm" variant="outline" onClick={copyToken}>
										{labels.copyToken}
									</Button>
								</div>
							</div>
						) : (
							<Button
								size="sm"
								variant="outline"
								onClick={handlePrepare}
								disabled={pending}
							>
								{labels.prepareBtn}
							</Button>
						)}
					</div>
				</div>
			)}

			{/* Active migration controls */}
			{isActive && (
				<div className="flex items-center gap-2">
					{state.status === "waiting_agents" &&
						state.agents_confirmed >= state.agents_total &&
						state.role === "source" && (
							<Button
								size="sm"
								variant="destructive"
								onClick={handleShutdown}
								disabled={pending}
							>
								{labels.confirmShutdown}
							</Button>
						)}
					<Button
						size="sm"
						variant="outline"
						onClick={handleAbort}
						disabled={pending}
					>
						{labels.abortBtn}
					</Button>
				</div>
			)}
		</div>
	);
}
