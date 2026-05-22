"use client";

import { useTransition } from "react";
import { toast } from "sonner";
import { removeMember } from "@/actions/(dashboard)/o/[id]";
import { Button } from "@/components/ui/button";

interface Props {
	errorMsg: string;
	label: string;
	orgId: string;
	successMsg: string;
	userId: string;
}

export function RemoveMemberButton({ orgId, userId, label, successMsg, errorMsg }: Props) {
	const [isPending, startTransition] = useTransition();

	return (
		<Button
			className="text-destructive hover:text-destructive hover:bg-destructive/10"
			disabled={isPending}
			onClick={() =>
				startTransition(async () => {
					const result = await removeMember(orgId, userId);
					if (result.ok) {
						toast.success(successMsg);
					} else {
						toast.error(errorMsg, { description: result.error });
					}
				})
			}
			size="sm"
			variant="ghost"
		>
			{label}
		</Button>
	);
}
