"use client";

import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useWatch } from "react-hook-form";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel, FieldError } from "@/components/ui/field";
import { brandingSchema, type BrandingInput } from "@/schemas/(dashboard)/app/settings";
import { updateBranding } from "@/actions/(dashboard)/app/settings";

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

function ColorPreview({ value }: { value: string | undefined }) {
	return (
		<div
			className="size-7 rounded border shrink-0"
			style={{ background: value || "#000000" }}
		/>
	);
}

export function BrandingForm({ initial, labels }: Props) {
	const {
		register,
		handleSubmit,
		control,
		formState: { errors, isSubmitting },
	} = useForm<BrandingInput>({
		resolver: zodResolver(brandingSchema),
		defaultValues: {
			company_name: initial.company_name,
			logo_url: initial.logo_url ?? "",
			primary_color: initial.primary_color,
			secondary_color: initial.secondary_color,
			accent_color: initial.accent_color,
		},
	});

	const [primary, secondary, accent] = useWatch({
		control,
		name: ["primary_color", "secondary_color", "accent_color"],
	});

	const onSubmit = (data: BrandingInput) => {
		toast.promise(
			updateBranding({
				company_name: data.company_name || undefined,
				logo_url: data.logo_url || null,
				primary_color: data.primary_color || undefined,
				secondary_color: data.secondary_color || undefined,
				accent_color: data.accent_color || undefined,
			}).then((r) => {
				if (!r.ok) throw new Error(r.error);
				return r;
			}),
			{
				loading: labels.save,
				success: labels.saved,
				error: labels.error,
			},
		);
	};

	return (
		<form onSubmit={handleSubmit(onSubmit)} className="flex flex-col gap-5">
			<Field>
				<FieldLabel htmlFor="company_name">{labels.companyName}</FieldLabel>
				<Input
					id="company_name"
					{...register("company_name")}
					maxLength={80}
					disabled={isSubmitting}
				/>
				<FieldError errors={[errors.company_name]} />
			</Field>

			<Field>
				<FieldLabel htmlFor="logo_url">{labels.logoUrl}</FieldLabel>
				<Input
					id="logo_url"
					{...register("logo_url")}
					placeholder="https://example.com/logo.png"
					disabled={isSubmitting}
				/>
				<FieldError errors={[errors.logo_url]} />
			</Field>

			<div className="flex flex-wrap gap-6">
				<Field>
					<FieldLabel htmlFor="primary_color">{labels.primaryColor}</FieldLabel>
					<div className="flex items-center gap-2">
						<ColorPreview value={primary} />
						<Input
							id="primary_color"
							{...register("primary_color")}
							placeholder="#000000"
							maxLength={7}
							className="font-mono text-sm w-32"
							disabled={isSubmitting}
						/>
					</div>
					<FieldError errors={[errors.primary_color]} />
				</Field>

				<Field>
					<FieldLabel htmlFor="secondary_color">{labels.secondaryColor}</FieldLabel>
					<div className="flex items-center gap-2">
						<ColorPreview value={secondary} />
						<Input
							id="secondary_color"
							{...register("secondary_color")}
							placeholder="#000000"
							maxLength={7}
							className="font-mono text-sm w-32"
							disabled={isSubmitting}
						/>
					</div>
					<FieldError errors={[errors.secondary_color]} />
				</Field>

				<Field>
					<FieldLabel htmlFor="accent_color">{labels.accentColor}</FieldLabel>
					<div className="flex items-center gap-2">
						<ColorPreview value={accent} />
						<Input
							id="accent_color"
							{...register("accent_color")}
							placeholder="#000000"
							maxLength={7}
							className="font-mono text-sm w-32"
							disabled={isSubmitting}
						/>
					</div>
					<FieldError errors={[errors.accent_color]} />
				</Field>
			</div>

			<div>
				<Button type="submit" disabled={isSubmitting} size="sm">
					{isSubmitting ? "…" : labels.save}
				</Button>
			</div>
		</form>
	);
}
