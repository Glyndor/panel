"use client";

import { useState, useTransition } from "react";
import { toast } from "sonner";
import { checkForUpdates, triggerUpdate, type UpdateCheckResult } from "@/actions/(dashboard)/app/settings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

interface Props {
	labels: {
		checkBtn: string;
		current: string;
		latest: string;
		upToDate: string;
		updateAvailable: string;
		triggerBtn: string;
		triggerSuccess: string;
		triggerError: string;
		checkError: string;
	};
}

export function UpdateSection({ labels }: Props) {
	const [result, setResult] = useState<UpdateCheckResult | null>(null);
	const [checking, startCheck] = useTransition();
	const [triggering, startTrigger] = useTransition();

	function handleCheck() {
		startCheck(async () => {
			const res = await checkForUpdates();
			if (res.ok && res.data) {
				setResult(res.data);
			} else {
				toast.error(labels.checkError, { description: res.error });
			}
		});
	}

	function handleTrigger() {
		if (!result) return;
		startTrigger(async () => {
			const res = await triggerUpdate(result.latest_version);
			if (res.ok) {
				toast.success(labels.triggerSuccess);
			} else {
				toast.error(labels.triggerError, { description: res.error });
			}
		});
	}

	return (
		<div className="flex flex-col gap-3">
			<div className="flex items-center gap-2">
				<Button disabled={checking} onClick={handleCheck} size="sm" variant="outline">
					{checking ? "…" : labels.checkBtn}
				</Button>
			</div>

			{result && (
				<div className="rounded-md border p-4 flex flex-col gap-2 text-sm">
					<div className="flex items-center gap-2">
						<span className="text-muted-foreground">{labels.current}</span>
						<Badge className="font-mono" variant="outline">
							{result.current_version}
						</Badge>
					</div>
					<div className="flex items-center gap-2">
						<span className="text-muted-foreground">{labels.latest}</span>
						<Badge className="font-mono" variant={result.update_available ? "default" : "secondary"}>
							{result.latest_version}
						</Badge>
						{result.update_available ? (
							<Badge className="text-xs" variant="destructive">
								{labels.updateAvailable}
							</Badge>
						) : (
							<Badge className="text-xs" variant="secondary">
								{labels.upToDate}
							</Badge>
						)}
					</div>

					{result.update_available && (
						<Button className="mt-1 self-start" disabled={triggering} onClick={handleTrigger} size="sm">
							{triggering ? "…" : labels.triggerBtn}
						</Button>
					)}
				</div>
			)}
		</div>
	);
}
