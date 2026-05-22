"use client";

import { zodResolver } from "@hookform/resolvers/zod";
import { AlertTriangle, ArrowRightLeft } from "lucide-react";
import { useState, useTransition } from "react";
import { useForm } from "react-hook-form";
import { toast } from "sonner";
import {
	abortMigration,
	confirmMigrationShutdown,
	prepareMigration,
	startMigration,
} from "@/actions/(dashboard)/settings/migration";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { type MigrationStartInput, migrationStartSchema } from "@/schemas/(dashboard)/settings";

interface MigrationState {
	agents_confirmed: number;
	agents_total: number;
	error_message: string | null;
	role: string;
	started_at: string | null;
	status: string;
	target_url: string | null;
}

interface Labels {
	abortBtn: string;
	abortError: string;
	abortSuccess: string;
	agentsProgress: string;
	confirmShutdown: string;
	confirmShutdownMsg: string;
	copyToken: string;
	desc: string;
	error: string;
	prepareBtn: string;
	preparedToken: string;
	prepareError: string;
	shutdownError: string;
	sourceDesc: string;
	sourceTitle: string;
	startError: string;
	startMigration: string;
	statusAborted: string;
	statusCompleted: string;
	statusError: string;
	statusIdle: string;
	statusNotifying: string;
	statusPreparing: string;
	statusTransferring: string;
	statusWaiting: string;
	targetDesc: string;
	targetTitle: string;
	targetUrl: string;
	title: string;
	token: string;
}

interface Props {
	initial: MigrationState;
	labels: Labels;
}

const STATUS_VARIANT: Record<string, "default" | "secondary" | "destructive"> = {
	aborted: "secondary",
	completed: "default",
	error: "destructive",
	idle: "secondary",
	notifying_agents: "secondary",
	preparing: "secondary",
	transferring: "secondary",
	waiting_agents: "secondary",
};

export function MigrationSection({ initial, labels }: Props) {
	const [state, setState] = useState<MigrationState>(initial);
	const [receivedToken, setReceivedToken] = useState<string | null>(null);
	const [preparePending, startPrepare] = useTransition();
	const [abortPending, startAbort] = useTransition();
	const [shutdownPending, startShutdown] = useTransition();

	const {
		register,
		handleSubmit,
		reset,
		formState: { errors, isSubmitting },
	} = useForm<MigrationStartInput>({
		resolver: zodResolver(migrationStartSchema),
	});

	const handlePrepare = () => {
		startPrepare(async () => {
			const r = await prepareMigration();
			if (r.ok && r.migration_token) {
				setReceivedToken(r.migration_token);
				setState((prev) => ({ ...prev, role: "target", status: "preparing" }));
			} else {
				toast.error(labels.prepareError);
			}
		});
	};

	const onStart = (data: MigrationStartInput) => {
		toast.promise(
			startMigration(data.target_url, data.migration_token).then((r) => {
				if (!r.ok) throw new Error(r.error);
				setState((prev) => ({
					...prev,
					role: "source",
					status: "transferring",
					target_url: data.target_url,
				}));
				reset();
				return r;
			}),
			{
				error: labels.startError,
				loading: labels.startMigration,
				success: labels.startMigration,
			},
		);
	};

	const handleAbort = () => {
		startAbort(async () => {
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
		startShutdown(async () => {
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

	const isActive = !["idle", "completed", "aborted", "error"].includes(state.status);

	return (
		<div className="flex flex-col gap-4">
			<p className="text-sm text-muted-foreground">{labels.desc}</p>

			<div className="flex items-center gap-2 flex-wrap">
				<ArrowRightLeft className="size-4 text-muted-foreground" />
				<Badge variant={STATUS_VARIANT[state.status] ?? "secondary"}>
					{labels[
						`status${state.status.charAt(0).toUpperCase() + state.status.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase()).slice(1)}` as keyof typeof labels
					] ?? state.status}
				</Badge>
				{state.target_url && (
					<span className="font-mono text-xs text-muted-foreground">→ {state.target_url}</span>
				)}
			</div>

			{(state.status === "waiting_agents" || state.status === "notifying_agents") && (
				<p className="text-sm text-muted-foreground">
					{labels.agentsProgress
						.replace("{confirmed}", String(state.agents_confirmed))
						.replace("{total}", String(state.agents_total))}
				</p>
			)}

			{state.error_message && (
				<div className="flex items-start gap-2 text-sm text-destructive">
					<AlertTriangle className="size-4 shrink-0 mt-0.5" />
					<span>{state.error_message}</span>
				</div>
			)}

			{state.status === "idle" && (
				<div className="grid sm:grid-cols-2 gap-4">
					<div className="rounded-lg border p-4 flex flex-col gap-3">
						<div>
							<p className="text-sm font-medium">{labels.sourceTitle}</p>
							<p className="text-xs text-muted-foreground mt-0.5">{labels.sourceDesc}</p>
						</div>
						<form className="flex flex-col gap-3" onSubmit={handleSubmit(onStart)}>
							<Field>
								<FieldLabel htmlFor="migration-url">{labels.targetUrl}</FieldLabel>
								<Input
									id="migration-url"
									{...register("target_url")}
									disabled={isSubmitting}
									placeholder="https://1.2.3.4:19443"
								/>
								<FieldError errors={[errors.target_url]} />
							</Field>
							<Field>
								<FieldLabel htmlFor="migration-token">{labels.token}</FieldLabel>
								<Input
									id="migration-token"
									{...register("migration_token")}
									disabled={isSubmitting}
									placeholder="migration token from VPS-B"
								/>
								<FieldError errors={[errors.migration_token]} />
							</Field>
							<Button disabled={isSubmitting} size="sm" type="submit">
								{isSubmitting ? "…" : labels.startMigration}
							</Button>
						</form>
					</div>

					<div className="rounded-lg border p-4 flex flex-col gap-3">
						<div>
							<p className="text-sm font-medium">{labels.targetTitle}</p>
							<p className="text-xs text-muted-foreground mt-0.5">{labels.targetDesc}</p>
						</div>
						{receivedToken ? (
							<div className="flex flex-col gap-2">
								<p className="text-xs text-muted-foreground">{labels.preparedToken}</p>
								<div className="flex items-center gap-2">
									<code className="text-xs font-mono bg-muted px-2 py-1 rounded break-all">
										{receivedToken.slice(0, 24)}…
									</code>
									<Button onClick={copyToken} size="sm" variant="outline">
										{labels.copyToken}
									</Button>
								</div>
							</div>
						) : (
							<Button disabled={preparePending} onClick={handlePrepare} size="sm" variant="outline">
								{labels.prepareBtn}
							</Button>
						)}
					</div>
				</div>
			)}

			{isActive && (
				<div className="flex items-center gap-2">
					{state.status === "waiting_agents" &&
						state.agents_confirmed >= state.agents_total &&
						state.role === "source" && (
							<Button disabled={shutdownPending} onClick={handleShutdown} size="sm" variant="destructive">
								{labels.confirmShutdown}
							</Button>
						)}
					<Button disabled={abortPending} onClick={handleAbort} size="sm" variant="outline">
						{labels.abortBtn}
					</Button>
				</div>
			)}
		</div>
	);
}
