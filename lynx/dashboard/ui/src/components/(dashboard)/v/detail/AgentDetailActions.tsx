"use client";

import { RotateCcw, Trash2 } from "lucide-react";
import { useState, useTransition } from "react";
import { toast } from "sonner";
import { deleteAgent, rebootAgent } from "@/actions/(dashboard)/v";
import { Button } from "@/components/ui/button";

interface Labels {
	deleteAgent: string;
	deleteConfirm: string;
	deleteError: string;
	reboot: string;
	rebootConfirm: string;
	rebootError: string;
	rebootSuccess: string;
}

interface Props {
	agentId: string;
	labels: Labels;
	locale: string;
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
				className="select-none cursor-pointer"
				disabled={rebootPending || deleted}
				onClick={handleReboot}
				size="sm"
				variant="outline"
			>
				<RotateCcw className="size-3.5 mr-1.5" />
				{labels.reboot}
			</Button>
			<Button
				className="select-none cursor-pointer"
				disabled={deletePending || deleted}
				onClick={handleDelete}
				size="sm"
				variant="destructive"
			>
				<Trash2 className="size-3.5 mr-1.5" />
				{labels.deleteAgent}
			</Button>
		</div>
	);
}
