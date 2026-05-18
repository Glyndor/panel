"use client";

import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useRouter } from "next/navigation";
import { useState } from "react";
import { toast } from "sonner";
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
import { Field, FieldLabel, FieldError } from "@/components/ui/field";
import { BACKEND_URL } from "@/lib/api";
import { Plus } from "lucide-react";
import { createOrgSchema, type CreateOrgInput } from "@/schemas/(dashboard)/app/organizations";

type Props = {
	token: string;
	label: string;
	slugConflict: string;
	errorMsg: string;
};

function deriveSlug(n: string) {
	return n.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

export function CreateOrgDialog({ token, label, slugConflict, errorMsg }: Props) {
	const [open, setOpen] = useState(false);
	const router = useRouter();

	const {
		register,
		handleSubmit,
		setValue,
		reset,
		formState: { errors, isSubmitting },
	} = useForm<CreateOrgInput>({
		resolver: zodResolver(createOrgSchema),
	});

	const onSubmit = (data: CreateOrgInput) => {
		toast.promise(
			fetch(`${BACKEND_URL}/organizations`, {
				method: "POST",
				headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}` },
				body: JSON.stringify({ name: data.name, slug: data.slug }),
			}).then(async (res) => {
				if (res.status === 409) throw new Error("conflict");
				if (!res.ok) throw new Error("error");
				setOpen(false);
				reset();
				router.refresh();
			}),
			{
				loading: "Creating…",
				success: label,
				error: (e: Error) => (e.message === "conflict" ? slugConflict : errorMsg),
			},
		);
	};

	return (
		<Dialog open={open} onOpenChange={(v) => { setOpen(v); if (!v) reset(); }}>
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
				<form onSubmit={handleSubmit(onSubmit)} className="flex flex-col gap-3 py-2">
					<Field>
						<FieldLabel htmlFor="org-name">Name</FieldLabel>
						<Input
							id="org-name"
							placeholder="Acme Corp"
							{...register("name", {
								onChange: (e) => setValue("slug", deriveSlug(e.target.value)),
							})}
							disabled={isSubmitting}
						/>
						<FieldError errors={[errors.name]} />
					</Field>
					<Field>
						<FieldLabel htmlFor="org-slug">Slug</FieldLabel>
						<Input
							id="org-slug"
							placeholder="acme-corp"
							{...register("slug")}
							disabled={isSubmitting}
						/>
						<FieldError errors={[errors.slug]} />
					</Field>
					<DialogFooter className="mt-2">
						<Button type="submit" disabled={isSubmitting}>
							{isSubmitting ? "Creating…" : "Create"}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}
