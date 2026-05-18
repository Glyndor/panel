"use client";

import { Plus, Send, Trash2 } from "lucide-react";
import { useState, useTransition } from "react";
import { toast } from "sonner";
import type { CreateRulePayload, NftRule } from "@/actions/(dashboard)/app/agents/nftables";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";

interface Labels {
	addRule: string;
	create: string;
	createError: string;
	createSuccess: string;
	deleteError: string;
	deleteSuccess: string;
	description: string;
	ipList: string;
	kind: string;
	kindAllowIp: string;
	kindAllowPort: string;
	kindBlockIp: string;
	kindBlockPort: string;
	kindRateLimit: string;
	noRules: string;
	port: string;
	priority: string;
	protoBoth: string;
	protocol: string;
	protoTcp: string;
	protoUdp: string;
	push: string;
	pushError: string;
	pushSuccess: string;
	ratePerMin: string;
}

interface Props {
	initialRules: NftRule[];
	labels: Labels;
	onCreateRule: (payload: CreateRulePayload) => Promise<{ ok: boolean; error?: string }>;
	onDeleteRule: (ruleId: string) => Promise<{ ok: boolean; error?: string }>;
	onPush: () => Promise<{ ok: boolean; error?: string; pushed?: number; failed?: number }>;
}

const KIND_LABELS: Record<string, keyof Labels> = {
	allow_ip: "kindAllowIp",
	allow_port: "kindAllowPort",
	block_ip: "kindBlockIp",
	block_port: "kindBlockPort",
	rate_limit: "kindRateLimit",
};

const PROTO_LABELS: Record<string, keyof Labels> = {
	both: "protoBoth",
	tcp: "protoTcp",
	udp: "protoUdp",
};

const KIND_BADGE: Record<string, "default" | "destructive" | "secondary"> = {
	allow_ip: "default",
	allow_port: "default",
	block_ip: "destructive",
	block_port: "destructive",
	rate_limit: "secondary",
};

const PORT_KINDS = new Set(["allow_port", "block_port", "rate_limit"]);
const IP_KINDS = new Set(["allow_ip", "block_ip"]);

export function NftRulesPanel({ initialRules, labels, onCreateRule, onDeleteRule, onPush }: Props) {
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
			description: description.trim() || undefined,
			kind,
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
								<tr className="hover:bg-muted/20" key={rule.id}>
									<td className="px-3 py-2">
										<Badge
											className="text-xs select-none"
											variant={KIND_BADGE[rule.kind] ?? "secondary"}
										>
											{labels[KIND_LABELS[rule.kind] ?? "kindAllowPort"]}
										</Badge>
									</td>
									<td className="px-3 py-2 font-mono text-xs">
										{rule.port != null ? `:${rule.port}` : ""}
										{rule.protocol
											? ` (${labels[PROTO_LABELS[rule.protocol] ?? "protoBoth"]})`
											: ""}
										{rule.ip_list.length > 0 ? ` ${rule.ip_list.join(", ")}` : ""}
										{rule.rate_per_min != null ? ` ${rule.rate_per_min}/min` : ""}
									</td>
									<td className="px-3 py-2 text-xs text-muted-foreground max-w-[12rem] truncate">
										{rule.description ?? ""}
									</td>
									<td className="px-3 py-2">
										<Button
											className="h-6 w-6 p-0 text-muted-foreground hover:text-destructive cursor-pointer"
											disabled={createPending}
											onClick={() => handleDelete(rule.id)}
											size="sm"
											variant="ghost"
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
					className="select-none cursor-pointer"
					disabled={createPending}
					onClick={() => setShowForm(!showForm)}
					size="sm"
					variant="outline"
				>
					<Plus className="size-3.5 mr-1.5" />
					{labels.addRule}
				</Button>
				<Button
					className="select-none cursor-pointer"
					disabled={pushPending}
					onClick={handlePush}
					size="sm"
					variant="outline"
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
							className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
							onChange={(e) => setKind(e.target.value)}
							value={kind}
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
									max={65535}
									min={1}
									onChange={(e) => setPort(e.target.value)}
									placeholder="80"
									type="number"
									value={port}
								/>
							</Field>
							<Field>
								<FieldLabel>{labels.protocol}</FieldLabel>
								<select
									className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
									onChange={(e) => setProtocol(e.target.value)}
									value={protocol}
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
								onChange={(e) => setIpList(e.target.value)}
								placeholder="0.0.0.0/0, ::/0"
								value={ipList}
							/>
						</Field>
					)}

					{kind === "rate_limit" && (
						<Field>
							<FieldLabel>{labels.ratePerMin}</FieldLabel>
							<Input
								min={1}
								onChange={(e) => setRatePerMin(e.target.value)}
								placeholder="100"
								type="number"
								value={ratePerMin}
							/>
						</Field>
					)}

					<Field>
						<FieldLabel>{labels.description}</FieldLabel>
						<Input
							onChange={(e) => setDescription(e.target.value)}
							placeholder="Allow web traffic"
							value={description}
						/>
					</Field>

					<Button
						className="select-none cursor-pointer w-fit"
						disabled={createPending}
						onClick={handleCreate}
						size="sm"
					>
						{labels.create}
					</Button>
				</div>
			)}
		</div>
	);
}
