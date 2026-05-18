"use client";

import { useState, useTransition } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Field, FieldLabel } from "@/components/ui/field";
import { Plus, Trash2, Send } from "lucide-react";
import {
	type NftRule,
	type CreateRulePayload,
} from "@/actions/(dashboard)/app/agents/nftables";

interface Labels {
	addRule: string;
	kind: string;
	port: string;
	protocol: string;
	ipList: string;
	ratePerMin: string;
	description: string;
	priority: string;
	create: string;
	createSuccess: string;
	createError: string;
	deleteSuccess: string;
	deleteError: string;
	push: string;
	pushSuccess: string;
	pushError: string;
	noRules: string;
	kindAllowPort: string;
	kindBlockPort: string;
	kindAllowIp: string;
	kindBlockIp: string;
	kindRateLimit: string;
	protoTcp: string;
	protoUdp: string;
	protoBoth: string;
}

interface Props {
	initialRules: NftRule[];
	labels: Labels;
	onCreateRule: (payload: CreateRulePayload) => Promise<{ ok: boolean; error?: string }>;
	onDeleteRule: (ruleId: string) => Promise<{ ok: boolean; error?: string }>;
	onPush: () => Promise<{ ok: boolean; error?: string; pushed?: number; failed?: number }>;
}

const KIND_LABELS: Record<string, keyof Labels> = {
	allow_port: "kindAllowPort",
	block_port: "kindBlockPort",
	allow_ip: "kindAllowIp",
	block_ip: "kindBlockIp",
	rate_limit: "kindRateLimit",
};

const PROTO_LABELS: Record<string, keyof Labels> = {
	tcp: "protoTcp",
	udp: "protoUdp",
	both: "protoBoth",
};

const KIND_BADGE: Record<string, "default" | "destructive" | "secondary"> = {
	allow_port: "default",
	allow_ip: "default",
	block_port: "destructive",
	block_ip: "destructive",
	rate_limit: "secondary",
};

const PORT_KINDS = new Set(["allow_port", "block_port", "rate_limit"]);
const IP_KINDS = new Set(["allow_ip", "block_ip"]);

export function NftRulesPanel({
	initialRules,
	labels,
	onCreateRule,
	onDeleteRule,
	onPush,
}: Props) {
	const [rules, setRules] = useState<NftRule[]>(initialRules);
	const [showForm, setShowForm] = useState(false);
	const [kind, setKind] = useState("allow_port");
	const [port, setPort] = useState("");
	const [protocol, setProtocol] = useState("tcp");
	const [ipList, setIpList] = useState("");
	const [ratePerMin, setRatePerMin] = useState("");
	const [description, setDescription] = useState("");
	const [createPending, startCreate] = useTransition();
	const [pushPending, startPush] = useTransition();

	const handleCreate = () => {
		const payload: CreateRulePayload = {
			kind,
			description: description.trim() || undefined,
		};
		if (PORT_KINDS.has(kind)) {
			const p = parseInt(port);
			if (!p || p < 1 || p > 65535) {
				toast.error("Invalid port");
				return;
			}
			payload.port = p;
			payload.protocol = protocol;
		}
		if (IP_KINDS.has(kind) || ipList.trim()) {
			payload.ip_list = ipList
				.split(",")
				.map((s) => s.trim())
				.filter(Boolean);
		}
		if (kind === "rate_limit") {
			const r = parseInt(ratePerMin);
			if (!r || r < 1) {
				toast.error("Invalid rate");
				return;
			}
			payload.rate_per_min = r;
		}

		startCreate(async () => {
			const result = await onCreateRule(payload);
			if (result.ok) {
				toast.success(labels.createSuccess);
				setShowForm(false);
				setPort("");
				setIpList("");
				setRatePerMin("");
				setDescription("");
				// Optimistic: refetch happens via revalidatePath on next navigation
			} else {
				toast.error(labels.createError);
			}
		});
	};

	const handleDelete = (ruleId: string) => {
		startCreate(async () => {
			const result = await onDeleteRule(ruleId);
			if (result.ok) {
				setRules((prev) => prev.filter((r) => r.id !== ruleId));
				toast.success(labels.deleteSuccess);
			} else {
				toast.error(labels.deleteError);
			}
		});
	};

	const handlePush = () => {
		startPush(async () => {
			const result = await onPush();
			if (result.ok) {
				toast.success(labels.pushSuccess);
			} else {
				toast.error(labels.pushError);
			}
		});
	};

	return (
		<div className="flex flex-col gap-3">
			{rules.length === 0 ? (
				<p className="text-sm text-muted-foreground">{labels.noRules}</p>
			) : (
				<div className="rounded-lg border overflow-hidden">
					<table className="w-full text-sm">
						<tbody className="divide-y">
							{rules.map((rule) => (
								<tr key={rule.id} className="hover:bg-muted/20">
									<td className="px-3 py-2">
										<Badge variant={KIND_BADGE[rule.kind] ?? "secondary"} className="text-xs select-none">
											{labels[KIND_LABELS[rule.kind] ?? "kindAllowPort"]}
										</Badge>
									</td>
									<td className="px-3 py-2 font-mono text-xs">
										{rule.port != null ? `:${rule.port}` : ""}
										{rule.protocol ? ` (${labels[PROTO_LABELS[rule.protocol] ?? "protoBoth"]})` : ""}
										{rule.ip_list.length > 0 ? ` ${rule.ip_list.join(", ")}` : ""}
										{rule.rate_per_min != null ? ` ${rule.rate_per_min}/min` : ""}
									</td>
									<td className="px-3 py-2 text-xs text-muted-foreground max-w-[12rem] truncate">
										{rule.description ?? ""}
									</td>
									<td className="px-3 py-2">
										<Button
											variant="ghost"
											size="sm"
											className="h-6 w-6 p-0 text-muted-foreground hover:text-destructive cursor-pointer"
											onClick={() => handleDelete(rule.id)}
											disabled={createPending}
										>
											<Trash2 className="size-3.5" />
										</Button>
									</td>
								</tr>
							))}
						</tbody>
					</table>
				</div>
			)}

			<div className="flex items-center gap-2">
				<Button
					variant="outline"
					size="sm"
					onClick={() => setShowForm(!showForm)}
					disabled={createPending}
					className="select-none cursor-pointer"
				>
					<Plus className="size-3.5 mr-1.5" />
					{labels.addRule}
				</Button>
				<Button
					variant="outline"
					size="sm"
					onClick={handlePush}
					disabled={pushPending}
					className="select-none cursor-pointer"
				>
					<Send className="size-3.5 mr-1.5" />
					{labels.push}
				</Button>
			</div>

			{showForm && (
				<div className="rounded-lg border p-3 flex flex-col gap-3">
					<Field>
						<FieldLabel>{labels.kind}</FieldLabel>
						<select
							value={kind}
							onChange={(e) => setKind(e.target.value)}
							className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
						>
							<option value="allow_port">{labels.kindAllowPort}</option>
							<option value="block_port">{labels.kindBlockPort}</option>
							<option value="allow_ip">{labels.kindAllowIp}</option>
							<option value="block_ip">{labels.kindBlockIp}</option>
							<option value="rate_limit">{labels.kindRateLimit}</option>
						</select>
					</Field>

					{PORT_KINDS.has(kind) && (
						<div className="grid grid-cols-2 gap-3">
							<Field>
								<FieldLabel>{labels.port}</FieldLabel>
								<Input
									type="number"
									min={1}
									max={65535}
									value={port}
									onChange={(e) => setPort(e.target.value)}
									placeholder="80"
								/>
							</Field>
							<Field>
								<FieldLabel>{labels.protocol}</FieldLabel>
								<select
									value={protocol}
									onChange={(e) => setProtocol(e.target.value)}
									className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
								>
									<option value="tcp">{labels.protoTcp}</option>
									<option value="udp">{labels.protoUdp}</option>
									<option value="both">{labels.protoBoth}</option>
								</select>
							</Field>
						</div>
					)}

					{(IP_KINDS.has(kind) || PORT_KINDS.has(kind)) && (
						<Field>
							<FieldLabel>{labels.ipList}</FieldLabel>
							<Input
								value={ipList}
								onChange={(e) => setIpList(e.target.value)}
								placeholder="0.0.0.0/0, ::/0"
							/>
						</Field>
					)}

					{kind === "rate_limit" && (
						<Field>
							<FieldLabel>{labels.ratePerMin}</FieldLabel>
							<Input
								type="number"
								min={1}
								value={ratePerMin}
								onChange={(e) => setRatePerMin(e.target.value)}
								placeholder="100"
							/>
						</Field>
					)}

					<Field>
						<FieldLabel>{labels.description}</FieldLabel>
						<Input
							value={description}
							onChange={(e) => setDescription(e.target.value)}
							placeholder="Allow web traffic"
						/>
					</Field>

					<Button
						size="sm"
						onClick={handleCreate}
						disabled={createPending}
						className="select-none cursor-pointer w-fit"
					>
						{labels.create}
					</Button>
				</div>
			)}
		</div>
	);
}
