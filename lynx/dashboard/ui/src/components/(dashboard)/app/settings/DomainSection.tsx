"use client";

import { zodResolver } from "@hookform/resolvers/zod";
import { Globe, Lock, ShieldCheck } from "lucide-react";
import { useState, useTransition } from "react";
import { useForm } from "react-hook-form";
import { toast } from "sonner";
import { closePort19443, configureDomain, setHsts, verifyDomain } from "@/actions/(dashboard)/app/settings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { type DomainSetupInput, domainSetupSchema } from "@/schemas/(dashboard)/app/settings";
import { CertUploadSection } from "./CertUploadSection";

interface DomainConfig {
	cert_expires_at: string | null;
	cert_type: string;
	domain: string | null;
	error_message: string | null;
	hsts_enabled: boolean;
	port_19443_open: boolean;
	status: string;
}

interface Labels {
	active: string;
	cert: string;
	certCloudflare: string;
	certCustom: string;
	certExpires: string;
	certKeyOptional: string;
	certKeyPem: string;
	certKeyPemPlaceholder: string;
	certLE: string;
	certPem: string;
	certPemPlaceholder: string;
	certSelfSigned: string;
	certUpload: string;
	certUploadCloudflare: string;
	certUploadCustom: string;
	certUploadError: string;
	certUploadSuccess: string;
	closePort: string;
	closePortBtn: string;
	closePortConfirm: string;
	closePortDesc: string;
	closePortError: string;
	closePortSuccess: string;
	current: string;
	desc: string;
	dnsFail: string;
	dnsOk: string;
	email: string;
	error: string;
	hsts: string;
	hstsDesc: string;
	hstsDisable: string;
	hstsEnable: string;
	hstsError: string;
	hstsSuccess: string;
	input: string;
	none: string;
	pending: string;
	setup: string;
	setupError: string;
	unconfigured: string;
	verify: string;
	verifyError: string;
}

interface Props {
	initial: DomainConfig;
	labels: Labels;
}

const STATUS_VARIANT: Record<string, "default" | "secondary" | "destructive"> = {
	active: "default",
	error: "destructive",
	pending: "secondary",
	unconfigured: "secondary",
};

function formatExpiry(ts: string | null): string {
	if (!ts) return "—";
	return new Date(ts).toLocaleDateString("en-GB", {
		day: "numeric",
		month: "short",
		year: "numeric",
	});
}

export function DomainSection({ initial, labels }: Props) {
	const [cfg, setCfg] = useState<DomainConfig>(initial);
	const [dnsResult, setDnsResult] = useState<boolean | null>(null);
	const [verifyPending, startVerify] = useTransition();
	const [hstsPending, startHsts] = useTransition();
	const [closePending, startClose] = useTransition();

	const {
		register,
		handleSubmit,
		reset,
		formState: { errors, isSubmitting },
	} = useForm<DomainSetupInput>({
		resolver: zodResolver(domainSetupSchema),
	});

	const onSetup = (data: DomainSetupInput) => {
		toast.promise(
			configureDomain(data.domain, data.email).then((r) => {
				if (!r.ok) throw new Error(r.error);
				setCfg((prev) => ({ ...prev, domain: data.domain, status: "pending" }));
				reset();
				return r;
			}),
			{
				error: labels.setupError,
				loading: labels.setup,
				success: labels.pending,
			},
		);
	};

	const handleVerify = () => {
		setDnsResult(null);
		startVerify(async () => {
			const r = await verifyDomain();
			if (!r.ok) {
				toast.error(labels.verifyError);
				return;
			}
			const ok = r.dns_ok ?? false;
			setDnsResult(ok);
			toast[ok ? "success" : "error"](ok ? labels.dnsOk : labels.dnsFail);
		});
	};

	const handleHsts = (enable: boolean) => {
		startHsts(async () => {
			const r = await setHsts(enable);
			if (r.ok) {
				setCfg((prev) => ({ ...prev, hsts_enabled: enable }));
				toast.success(labels.hstsSuccess);
			} else toast.error(labels.hstsError);
		});
	};

	const handleClosePort = () => {
		if (!window.confirm(labels.closePortConfirm)) return;
		startClose(async () => {
			const r = await closePort19443();
			if (r.ok) {
				setCfg((prev) => ({ ...prev, port_19443_open: false }));
				toast.success(labels.closePortSuccess);
			} else toast.error(labels.closePortError);
		});
	};

	const isActive = cfg.status === "active";

	return (
		<div className="flex flex-col gap-4">
			<p className="text-sm text-muted-foreground">{labels.desc}</p>

			<div className="flex items-center gap-2 flex-wrap">
				<Globe className="size-4 text-muted-foreground" />
				<span className="text-sm font-medium">
					{cfg.domain ? (
						<span className="font-mono">{cfg.domain}</span>
					) : (
						<span className="text-muted-foreground">{labels.none}</span>
					)}
				</span>
				<Badge variant={STATUS_VARIANT[cfg.status] ?? "secondary"}>
					{labels[cfg.status as keyof typeof labels] ?? cfg.status}
				</Badge>
			</div>

			{cfg.domain && (
				<div className="flex items-center gap-4 text-xs text-muted-foreground flex-wrap">
					<span>
						{labels.cert}{" "}
						<span className="text-foreground font-medium">
							{cfg.cert_type === "lets_encrypt"
								? labels.certLE
								: cfg.cert_type === "cloudflare"
									? labels.certCloudflare
									: cfg.cert_type === "custom"
										? labels.certCustom
										: labels.certSelfSigned}
						</span>
					</span>
					{cfg.cert_expires_at && (
						<span>
							{labels.certExpires}{" "}
							<span className="text-foreground">{formatExpiry(cfg.cert_expires_at)}</span>
						</span>
					)}
				</div>
			)}

			{cfg.error_message && <p className="text-sm text-destructive">{cfg.error_message}</p>}

			{!isActive && (
				<form className="flex flex-col gap-3" onSubmit={handleSubmit(onSetup)}>
					<Field>
						<FieldLabel htmlFor="domain-input">{labels.input}</FieldLabel>
						<Input
							id="domain-input"
							{...register("domain")}
							disabled={isSubmitting || cfg.status === "pending"}
							placeholder="panel.example.com"
						/>
						<FieldError errors={[errors.domain]} />
					</Field>
					<Field>
						<FieldLabel htmlFor="domain-email">{labels.email}</FieldLabel>
						<Input
							id="domain-email"
							type="email"
							{...register("email")}
							disabled={isSubmitting || cfg.status === "pending"}
							placeholder="you@example.com"
						/>
						<FieldError errors={[errors.email]} />
					</Field>
					<Button disabled={isSubmitting || cfg.status === "pending"} type="submit">
						{cfg.status === "pending" ? labels.pending : labels.setup}
					</Button>
				</form>
			)}

			{cfg.domain && (
				<div className="flex items-center gap-2">
					<Button disabled={verifyPending} onClick={handleVerify} size="sm" variant="outline">
						{labels.verify}
					</Button>
					{dnsResult !== null && (
						<span className={dnsResult ? "text-sm text-green-600" : "text-sm text-destructive"}>
							{dnsResult ? labels.dnsOk : labels.dnsFail}
						</span>
					)}
				</div>
			)}

			{isActive && (
				<div className="rounded-lg border p-3 flex items-start justify-between gap-4">
					<div className="flex flex-col gap-0.5 min-w-0">
						<div className="flex items-center gap-1.5 text-sm font-medium">
							<ShieldCheck className="size-3.5" />
							{labels.hsts}
						</div>
						<p className="text-xs text-muted-foreground">{labels.hstsDesc}</p>
					</div>
					<Button
						disabled={hstsPending}
						onClick={() => handleHsts(!cfg.hsts_enabled)}
						size="sm"
						variant={cfg.hsts_enabled ? "destructive" : "outline"}
					>
						{cfg.hsts_enabled ? labels.hstsDisable : labels.hstsEnable}
					</Button>
				</div>
			)}

			{isActive && cfg.port_19443_open && (
				<div className="rounded-lg border border-destructive/30 p-3 flex items-start justify-between gap-4">
					<div className="flex flex-col gap-0.5 min-w-0">
						<div className="flex items-center gap-1.5 text-sm font-medium">
							<Lock className="size-3.5" />
							{labels.closePort}
						</div>
						<p className="text-xs text-muted-foreground">{labels.closePortDesc}</p>
					</div>
					<Button disabled={closePending} onClick={handleClosePort} size="sm" variant="destructive">
						{labels.closePortBtn}
					</Button>
				</div>
			)}

			{!cfg.port_19443_open && (
				<p className="text-xs text-muted-foreground flex items-center gap-1">
					<Lock className="size-3" />
					Port 19443 is closed
				</p>
			)}

			{isActive && (
				<CertUploadSection
					labels={{
						certPem: labels.certPem,
						certPemPlaceholder: labels.certPemPlaceholder,
						cloudflareTab: labels.certUploadCloudflare,
						customTab: labels.certUploadCustom,
						error: labels.certUploadError,
						keyOptional: labels.certKeyOptional,
						keyPem: labels.certKeyPem,
						keyPemPlaceholder: labels.certKeyPemPlaceholder,
						success: labels.certUploadSuccess,
						title: labels.certUpload,
						upload: labels.certUpload,
					}}
					onSuccess={() => setCfg((prev) => ({ ...prev }))}
				/>
			)}
		</div>
	);
}
