"use client";

import { Button } from "@/components/ui/button";
import { useTransition } from "react";
import { toast } from "sonner";
import { rotateKeys } from "@/actions/(dashboard)/app/settings";

type Props = {
	locale: string;
	label: string;
	confirmMsg: string;
};

export function RotateButton({ locale, label, confirmMsg }: Props) {
	const [pending, startTransition] = useTransition();

	function handleRotate() {
		if (!confirm(confirmMsg)) return;
		startTransition(async () => {
			toast.loading(label);
			await rotateKeys(locale);
		});
	}

	return (
		<Button variant="destructive" size="sm" disabled={pending} onClick={handleRotate}>
			{label}
		</Button>
	);
}
