"use client";

import { useState, useTransition } from "react";
import { toast } from "sonner";
import { toggleSingleSession } from "@/actions/(dashboard)/app/member/profile";
import { Switch } from "@/components/ui/switch";

type Props = {
	initial: boolean;
	labels: {
		label: string;
		desc: string;
		success: string;
		error: string;
	};
};

export function SingleSessionToggle({ initial, labels }: Props) {
	const [enabled, setEnabled] = useState(initial);
	const [, startTransition] = useTransition();

	function handleChange(value: boolean) {
		setEnabled(value);
		startTransition(async () => {
			const result = await toggleSingleSession(value);
			if (result.ok) {
				toast.success(labels.success);
			} else {
				setEnabled(!value);
				toast.error(labels.error);
			}
		});
	}

	return (
		<div className="flex items-center justify-between gap-4">
			<div className="min-w-0">
				<p className="text-sm font-medium">{labels.label}</p>
				<p className="mt-0.5 text-xs text-muted-foreground">{labels.desc}</p>
			</div>
			<Switch checked={enabled} className="cursor-pointer shrink-0" onCheckedChange={handleChange} />
		</div>
	);
}
