"use client";

import { useState, useTransition } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { updateBranding } from "./actions";

interface Props {
	initial: {
		company_name: string;
		logo_url: string | null;
		primary_color: string;
		secondary_color: string;
		accent_color: string;
	};
	labels: {
		companyName: string;
		logoUrl: string;
		primaryColor: string;
		secondaryColor: string;
		accentColor: string;
		save: string;
		saved: string;
		error: string;
	};
}

function ColorField({
	id,
	label,
	value,
	onChange,
}: {
	id: string;
	label: string;
	value: string;
	onChange: (v: string) => void;
}) {
	return (
		<div className="flex flex-col gap-1.5">
			<Label htmlFor={id}>{label}</Label>
			<div className="flex items-center gap-2">
				<div
					className="size-7 rounded border shrink-0"
					style={{ background: value }}
				/>
				<Input
					id={id}
					value={value}
					onChange={(e) => onChange(e.target.value)}
					placeholder="#000000"
					maxLength={7}
					className="font-mono text-sm w-32"
				/>
			</div>
		</div>
	);
}

export function BrandingForm({ initial, labels }: Props) {
	const [companyName, setCompanyName] = useState(initial.company_name);
	const [logoUrl, setLogoUrl] = useState(initial.logo_url ?? "");
	const [primaryColor, setPrimaryColor] = useState(initial.primary_color);
	const [secondaryColor, setSecondaryColor] = useState(initial.secondary_color);
	const [accentColor, setAccentColor] = useState(initial.accent_color);
	const [isPending, startTransition] = useTransition();

	function handleSubmit(e: React.FormEvent) {
		e.preventDefault();
		startTransition(async () => {
			const result = await updateBranding({
				company_name: companyName || undefined,
				logo_url: logoUrl || null,
				primary_color: primaryColor || undefined,
				secondary_color: secondaryColor || undefined,
				accent_color: accentColor || undefined,
			});
			if (result.ok) {
				toast.success(labels.saved);
			} else {
				toast.error(labels.error, { description: result.error });
			}
		});
	}

	return (
		<form onSubmit={handleSubmit} className="flex flex-col gap-5">
			<div className="flex flex-col gap-1.5">
				<Label htmlFor="company-name">{labels.companyName}</Label>
				<Input
					id="company-name"
					value={companyName}
					onChange={(e) => setCompanyName(e.target.value)}
					maxLength={80}
				/>
			</div>

			<div className="flex flex-col gap-1.5">
				<Label htmlFor="logo-url">{labels.logoUrl}</Label>
				<Input
					id="logo-url"
					type="url"
					value={logoUrl}
					onChange={(e) => setLogoUrl(e.target.value)}
					placeholder="https://example.com/logo.png"
				/>
			</div>

			<div className="flex flex-wrap gap-6">
				<ColorField
					id="primary-color"
					label={labels.primaryColor}
					value={primaryColor}
					onChange={setPrimaryColor}
				/>
				<ColorField
					id="secondary-color"
					label={labels.secondaryColor}
					value={secondaryColor}
					onChange={setSecondaryColor}
				/>
				<ColorField
					id="accent-color"
					label={labels.accentColor}
					value={accentColor}
					onChange={setAccentColor}
				/>
			</div>

			<div>
				<Button type="submit" disabled={isPending} size="sm">
					{isPending ? "…" : labels.save}
				</Button>
			</div>
		</form>
	);
}
