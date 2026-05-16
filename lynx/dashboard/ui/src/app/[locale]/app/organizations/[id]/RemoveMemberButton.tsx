"use client";

import { useTransition } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { removeMember } from "./actions";

interface Props {
	orgId: string;
	userId: string;
	label: string;
	successMsg: string;
	errorMsg: string;
}

export function RemoveMemberButton({
	orgId,
	userId,
	label,
	successMsg,
	errorMsg,
}: Props) {
	const [isPending, startTransition] = useTransition();

	return (
		<Button
			variant="ghost"
			size="sm"
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
		>
			{label}
		</Button>
	);
}
