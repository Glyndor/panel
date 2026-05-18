"use client";

import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Field, FieldLabel, FieldError } from "@/components/ui/field";
import { Upload } from "lucide-react";
import { certUploadSchema, type CertUploadInput } from "@/schemas/(dashboard)/app/settings";
import { uploadCert } from "@/actions/(dashboard)/app/settings";

interface Labels {
	title: string;
	cloudflareTab: string;
	customTab: string;
	certPem: string;
	certPemPlaceholder: string;
	keyPem: string;
	keyPemPlaceholder: string;
	keyOptional: string;
	upload: string;
	success: string;
	error: string;
}

interface Props {
	labels: Labels;
	onSuccess?: () => void;
}

export function CertUploadSection({ labels, onSuccess }: Props) {
	const [tab, setTab] = useState<"cloudflare" | "custom">("cloudflare");

	const {
		register,
		handleSubmit,
		reset,
		formState: { errors, isSubmitting },
	} = useForm<CertUploadInput>({
		resolver: zodResolver(certUploadSchema),
		defaultValues: { cert_type: "cloudflare" },
	});

	const onSubmit = async (data: CertUploadInput) => {
		const r = await uploadCert(tab, data.cert_pem, data.key_pem || undefined);
		if (r.ok) {
			toast.success(labels.success);
			reset();
			onSuccess?.();
		} else {
			toast.error(labels.error);
		}
	};

	return (
		<div className="rounded-lg border p-4 flex flex-col gap-3">
			<div className="flex items-center gap-2 text-sm font-medium">
				<Upload className="size-3.5" />
				{labels.title}
			</div>

			<Tabs
				value={tab}
				onValueChange={(v) => {
					setTab(v as "cloudflare" | "custom");
					reset({ cert_type: v as "cloudflare" | "custom" });
				}}
			>
				<TabsList className="w-full">
					<TabsTrigger value="cloudflare" className="flex-1 select-none cursor-pointer">
						{labels.cloudflareTab}
					</TabsTrigger>
					<TabsTrigger value="custom" className="flex-1 select-none cursor-pointer">
						{labels.customTab}
					</TabsTrigger>
				</TabsList>

				<form onSubmit={handleSubmit(onSubmit)} className="flex flex-col gap-3 mt-3">
					<input type="hidden" {...register("cert_type")} value={tab} />

					<TabsContent value="cloudflare" forceMount className={tab !== "cloudflare" ? "hidden" : ""}>
						<Field>
							<FieldLabel htmlFor="cf-cert-pem">{labels.certPem}</FieldLabel>
							<Textarea
								id="cf-cert-pem"
								{...register("cert_pem")}
								placeholder={labels.certPemPlaceholder}
								rows={6}
								className="font-mono text-xs resize-y"
								disabled={isSubmitting}
							/>
							<FieldError errors={[errors.cert_pem]} />
						</Field>
						<p className="text-xs text-muted-foreground mt-1">{labels.keyOptional}</p>
					</TabsContent>

					<TabsContent value="custom" forceMount className={tab !== "custom" ? "hidden" : ""}>
						<div className="flex flex-col gap-3">
							<Field>
								<FieldLabel htmlFor="custom-cert-pem">{labels.certPem}</FieldLabel>
								<Textarea
									id="custom-cert-pem"
									{...register("cert_pem")}
									placeholder={labels.certPemPlaceholder}
									rows={6}
									className="font-mono text-xs resize-y"
									disabled={isSubmitting}
								/>
								<FieldError errors={[errors.cert_pem]} />
							</Field>
							<Field>
								<FieldLabel htmlFor="custom-key-pem">{labels.keyPem}</FieldLabel>
								<Textarea
									id="custom-key-pem"
									{...register("key_pem")}
									placeholder={labels.keyPemPlaceholder}
									rows={6}
									className="font-mono text-xs resize-y"
									disabled={isSubmitting}
								/>
								<FieldError errors={[errors.key_pem]} />
							</Field>
						</div>
					</TabsContent>

					<Button type="submit" size="sm" disabled={isSubmitting}>
						{labels.upload}
					</Button>
				</form>
			</Tabs>
		</div>
	);
}
