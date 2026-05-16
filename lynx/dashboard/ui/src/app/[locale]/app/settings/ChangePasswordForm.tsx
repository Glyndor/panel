"use client";

import { useState, useTransition } from "react";
import { toast } from "sonner";
import { useRouter } from "next/navigation";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { changePassword } from "./profileActions";

interface Labels {
	currentPassword: string;
	newPassword: string;
	btn: string;
	success: string;
	wrong: string;
	error: string;
}

interface Props {
	locale: string;
	labels: Labels;
}

export function ChangePasswordForm({ locale, labels }: Props) {
	const router = useRouter();
	const [current, setCurrent] = useState("");
	const [next, setNext] = useState("");
	const [pending, startTransition] = useTransition();

	const handleSubmit = (e: React.FormEvent) => {
		e.preventDefault();
		if (!current || !next) return;
		startTransition(async () => {
			const r = await changePassword(current, next);
			if (r.ok) {
				toast.success(labels.success);
				// All sessions are invalidated — redirect to login
				setTimeout(() => router.push(`/${locale}/login`), 1500);
			} else if (r.status === 401) {
				toast.error(labels.wrong);
			} else {
				toast.error(labels.error);
			}
		});
	};

	return (
		<form onSubmit={handleSubmit} className="flex flex-col gap-3 max-w-sm">
			<div className="flex flex-col gap-1.5">
				<Label>{labels.currentPassword}</Label>
				<Input
					type="password"
					value={current}
					onChange={(e) => setCurrent(e.target.value)}
					disabled={pending}
					autoComplete="current-password"
				/>
			</div>
			<div className="flex flex-col gap-1.5">
				<Label>{labels.newPassword}</Label>
				<Input
					type="password"
					value={next}
					onChange={(e) => setNext(e.target.value)}
					disabled={pending}
					autoComplete="new-password"
				/>
			</div>
			<Button type="submit" disabled={!current || !next || pending}>
				{labels.btn}
			</Button>
		</form>
	);
}
