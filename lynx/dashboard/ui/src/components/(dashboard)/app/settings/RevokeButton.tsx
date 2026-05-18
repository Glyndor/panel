"use client";

import { Button } from "@/components/ui/button";
import { useRouter } from "next/navigation";
import { useTransition } from "react";
import { toast } from "sonner";
import { revokeSession } from "@/actions/(dashboard)/app/settings";

type Props = {
	sessionId: string;
	label: string;
	successMsg: string;
	errorMsg: string;
};

export function RevokeButton({
	sessionId,
	label,
	successMsg,
	errorMsg,
}: Props) {
	const router = useRouter();
	const [pending, startTransition] = useTransition();

	function handleRevoke() {
		startTransition(async () => {
			const result = await revokeSession(sessionId);
			if (result.ok) {
				toast.success(successMsg);
				router.refresh();
			} else {
				toast.error(errorMsg);
			}
		});
	}

	return (
		<Button
			variant="outline"
			size="sm"
			disabled={pending}
			onClick={handleRevoke}
		>
			{label}
		</Button>
	);
}
