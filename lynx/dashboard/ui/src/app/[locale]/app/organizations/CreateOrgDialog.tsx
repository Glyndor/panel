"use client";

import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
	DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { BACKEND_URL } from "@/lib/api";
import { Plus } from "lucide-react";
import { useRouter } from "next/navigation";
import { useState, useTransition } from "react";
import { toast } from "sonner";

type Props = {
	token: string;
	label: string;
	slugConflict: string;
	errorMsg: string;
};

export function CreateOrgDialog({ token, label, slugConflict, errorMsg }: Props) {
	const [open, setOpen] = useState(false);
	const [name, setName] = useState("");
	const [slug, setSlug] = useState("");
	const [pending, start] = useTransition();
	const router = useRouter();

	function derivedSlug(n: string) {
		return n
			.toLowerCase()
			.replace(/[^a-z0-9]+/g, "-")
			.replace(/^-|-$/g, "");
	}

	function handleNameChange(v: string) {
		setName(v);
		setSlug(derivedSlug(v));
	}

	function handleCreate() {
		if (!name.trim() || !slug) return;
		start(async () => {
			try {
				const res = await fetch(`${BACKEND_URL}/organizations`, {
					method: "POST",
					headers: {
						"Content-Type": "application/json",
						Authorization: `Bearer ${token}`,
					},
					body: JSON.stringify({ name: name.trim(), slug }),
				});
				if (res.status === 409) {
					toast.error(slugConflict);
					return;
				}
				if (!res.ok) {
					toast.error(errorMsg);
					return;
				}
				setOpen(false);
				setName("");
				setSlug("");
				router.refresh();
			} catch {
				toast.error(errorMsg);
			}
		});
	}

	return (
		<Dialog open={open} onOpenChange={setOpen}>
			<DialogTrigger asChild>
				<Button size="sm">
					<Plus className="size-4 mr-1" />
					{label}
				</Button>
			</DialogTrigger>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>{label}</DialogTitle>
					<DialogDescription>
						Create an organization to group projects and containers.
					</DialogDescription>
				</DialogHeader>
				<div className="flex flex-col gap-3 py-2">
					<div className="space-y-1.5">
						<Label htmlFor="org-name">Name</Label>
						<Input
							id="org-name"
							placeholder="Acme Corp"
							value={name}
							onChange={(e) => handleNameChange(e.target.value)}
							disabled={pending}
						/>
					</div>
					<div className="space-y-1.5">
						<Label htmlFor="org-slug">Slug</Label>
						<Input
							id="org-slug"
							placeholder="acme-corp"
							value={slug}
							onChange={(e) => setSlug(e.target.value)}
							disabled={pending}
						/>
					</div>
				</div>
				<DialogFooter>
					<Button
						onClick={handleCreate}
						disabled={pending || !name.trim() || !slug}
					>
						{pending ? "Creating…" : "Create"}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
