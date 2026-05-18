"use client";

import { useState, useTransition } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { RotateCcw, Trash2 } from "lucide-react";
import { rebootAgent, deleteAgent } from "@/actions/(dashboard)/app/agents";

interface Labels {
	reboot: string;
	rebootConfirm: string;
	rebootSuccess: string;
	rebootError: string;
	deleteAgent: string;
	deleteConfirm: string;
	deleteError: string;
}

interface Props {
	agentId: string;
	locale: string;
	labels: Labels;
}

export function AgentDetailActions({ agentId, locale, labels }: Props) {
	const [rebootPending, startReboot] = useTransition();
	const [deletePending, startDelete] = useTransition();
	const [deleted, setDeleted] = useState(false);

	const handleReboot = () => {
		if (!window.confirm(labels.rebootConfirm)) return;
		startReboot(async () => {
			const r = await rebootAgent(agentId);
			if (r.ok) toast.success(labels.rebootSuccess);
			else toast.error(labels.rebootError);
		});
	};

	const handleDelete = () => {
		if (!window.confirm(labels.deleteConfirm)) return;
		setDeleted(true);
		startDelete(async () => {
			try {
				await deleteAgent(agentId, locale);
			} catch {
				toast.error(labels.deleteError);
				setDeleted(false);
			}
		});
	};

	return (
		<div className="flex items-center gap-2 flex-wrap">
			<Button
				variant="outline"
				size="sm"
				onClick={handleReboot}
				disabled={rebootPending || deleted}
				className="select-none cursor-pointer"
			>
				<RotateCcw className="size-3.5 mr-1.5" />
				{labels.reboot}
			</Button>
			<Button
				variant="destructive"
				size="sm"
				onClick={handleDelete}
				disabled={deletePending || deleted}
				className="select-none cursor-pointer"
			>
				<Trash2 className="size-3.5 mr-1.5" />
				{labels.deleteAgent}
			</Button>
		</div>
	);
}
