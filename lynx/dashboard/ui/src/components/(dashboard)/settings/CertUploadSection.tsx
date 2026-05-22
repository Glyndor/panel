"use client";

import { zodResolver } from "@hookform/resolvers/zod";
import { Upload } from "lucide-react";
import { useState } from "react";
import { useForm } from "react-hook-form";
import { toast } from "sonner";
import { uploadCert } from "@/actions/(dashboard)/settings";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { type CertUploadInput, certUploadSchema } from "@/schemas/(dashboard)/settings";

interface Labels {
	certPem: string;
	certPemPlaceholder: string;
	cloudflareTab: string;
	customTab: string;
	error: string;
	keyOptional: string;
	keyPem: string;
	keyPemPlaceholder: string;
	success: string;
	title: string;
	upload: string;
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
		defaultValues: { cert_type: "cloudflare" },
		resolver: zodResolver(certUploadSchema),
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
				onValueChange={(v) => {
					setTab(v as "cloudflare" | "custom");
					reset({ cert_type: v as "cloudflare" | "custom" });
				}}
				value={tab}
			>
				<TabsList className="w-full">
					<TabsTrigger className="flex-1 select-none cursor-pointer" value="cloudflare">
						{labels.cloudflareTab}
					</TabsTrigger>
					<TabsTrigger className="flex-1 select-none cursor-pointer" value="custom">
						{labels.customTab}
					</TabsTrigger>
				</TabsList>

				<form className="flex flex-col gap-3 mt-3" onSubmit={handleSubmit(onSubmit)}>
					<input type="hidden" {...register("cert_type")} value={tab} />

					<TabsContent className={tab !== "cloudflare" ? "hidden" : ""} forceMount value="cloudflare">
						<Field>
							<FieldLabel htmlFor="cf-cert-pem">{labels.certPem}</FieldLabel>
							<Textarea
								id="cf-cert-pem"
								{...register("cert_pem")}
								className="font-mono text-xs resize-y"
								disabled={isSubmitting}
								placeholder={labels.certPemPlaceholder}
								rows={6}
							/>
							<FieldError errors={[errors.cert_pem]} />
						</Field>
						<p className="text-xs text-muted-foreground mt-1">{labels.keyOptional}</p>
					</TabsContent>

					<TabsContent className={tab !== "custom" ? "hidden" : ""} forceMount value="custom">
						<div className="flex flex-col gap-3">
							<Field>
								<FieldLabel htmlFor="custom-cert-pem">{labels.certPem}</FieldLabel>
								<Textarea
									id="custom-cert-pem"
									{...register("cert_pem")}
									className="font-mono text-xs resize-y"
									disabled={isSubmitting}
									placeholder={labels.certPemPlaceholder}
									rows={6}
								/>
								<FieldError errors={[errors.cert_pem]} />
							</Field>
							<Field>
								<FieldLabel htmlFor="custom-key-pem">{labels.keyPem}</FieldLabel>
								<Textarea
									id="custom-key-pem"
									{...register("key_pem")}
									className="font-mono text-xs resize-y"
									disabled={isSubmitting}
									placeholder={labels.keyPemPlaceholder}
									rows={6}
								/>
								<FieldError errors={[errors.key_pem]} />
							</Field>
						</div>
					</TabsContent>

					<Button disabled={isSubmitting} size="sm" type="submit">
						{labels.upload}
					</Button>
				</form>
			</Tabs>
		</div>
	);
}
