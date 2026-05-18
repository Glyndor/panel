"use client";

import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useState, useTransition } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel, FieldError } from "@/components/ui/field";
import { Globe, ShieldCheck, Lock } from "lucide-react";
import { domainSetupSchema, type DomainSetupInput } from "@/schemas/(dashboard)/app/settings";
import {
	configureDomain,
	verifyDomain,
	setHsts,
	closePort19443,
} from "@/actions/(dashboard)/app/settings";
import { CertUploadSection } from "./CertUploadSection";

interface DomainConfig {
	domain: string | null;
	cert_type: string;
	cert_expires_at: string | null;
	hsts_enabled: boolean;
	port_19443_open: boolean;
	status: string;
	error_message: string | null;
}

interface Labels {
	desc: string;
	current: string;
	none: string;
	input: string;
	email: string;
	setup: string;
	pending: string;
	active: string;
	error: string;
	unconfigured: string;
	verify: string;
	dnsOk: string;
	dnsFail: string;
	verifyError: string;
	setupError: string;
	hsts: string;
	hstsDesc: string;
	hstsEnable: string;
	hstsDisable: string;
	hstsSuccess: string;
	hstsError: string;
	closePort: string;
	closePortDesc: string;
	closePortBtn: string;
	closePortConfirm: string;
	closePortSuccess: string;
	closePortError: string;
	cert: string;
	certSelfSigned: string;
	certLE: string;
	certCloudflare: string;
	certCustom: string;
	certExpires: string;
	certUpload: string;
	certUploadCloudflare: string;
	certUploadCustom: string;
	certPem: string;
	certPemPlaceholder: string;
	certKeyPem: string;
	certKeyPemPlaceholder: string;
	certKeyOptional: string;
	certUploadSuccess: string;
	certUploadError: string;
}

interface Props {
	initial: DomainConfig;
	labels: Labels;
}

const STATUS_VARIANT: Record<string, "default" | "secondary" | "destructive"> = {
	active: "default",
	pending: "secondary",
	error: "destructive",
	unconfigured: "secondary",
};

function formatExpiry(ts: string | null): string {
	if (!ts) return "—";
	return new Date(ts).toLocaleDateString("en-GB", {
		year: "numeric",
		month: "short",
		day: "numeric",
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
				setCfg((prev) => ({ ...prev, status: "pending", domain: data.domain }));
				reset();
				return r;
			}),
			{
				loading: labels.setup,
				success: labels.pending,
				error: labels.setupError,
			},
		);
	};

	const handleVerify = () => {
		setDnsResult(null);
		startVerify(async () => {
			const r = await verifyDomain();
			if (!r.ok) { toast.error(labels.verifyError); return; }
			const ok = r.dns_ok ?? false;
			setDnsResult(ok);
			toast[ok ? "success" : "error"](ok ? labels.dnsOk : labels.dnsFail);
		});
	};

	const handleHsts = (enable: boolean) => {
		startHsts(async () => {
			const r = await setHsts(enable);
			if (r.ok) { setCfg((prev) => ({ ...prev, hsts_enabled: enable })); toast.success(labels.hstsSuccess); }
			else toast.error(labels.hstsError);
		});
	};

	const handleClosePort = () => {
		if (!window.confirm(labels.closePortConfirm)) return;
		startClose(async () => {
			const r = await closePort19443();
			if (r.ok) { setCfg((prev) => ({ ...prev, port_19443_open: false })); toast.success(labels.closePortSuccess); }
			else toast.error(labels.closePortError);
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

			{cfg.error_message && (
				<p className="text-sm text-destructive">{cfg.error_message}</p>
			)}

			{!isActive && (
				<form onSubmit={handleSubmit(onSetup)} className="flex flex-col gap-3">
					<Field>
						<FieldLabel htmlFor="domain-input">{labels.input}</FieldLabel>
						<Input
							id="domain-input"
							{...register("domain")}
							placeholder="panel.example.com"
							disabled={isSubmitting || cfg.status === "pending"}
						/>
						<FieldError errors={[errors.domain]} />
					</Field>
					<Field>
						<FieldLabel htmlFor="domain-email">{labels.email}</FieldLabel>
						<Input
							id="domain-email"
							type="email"
							{...register("email")}
							placeholder="you@example.com"
							disabled={isSubmitting || cfg.status === "pending"}
						/>
						<FieldError errors={[errors.email]} />
					</Field>
					<Button type="submit" disabled={isSubmitting || cfg.status === "pending"}>
						{cfg.status === "pending" ? labels.pending : labels.setup}
					</Button>
				</form>
			)}

			{cfg.domain && (
				<div className="flex items-center gap-2">
					<Button variant="outline" size="sm" onClick={handleVerify} disabled={verifyPending}>
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
						variant={cfg.hsts_enabled ? "destructive" : "outline"}
						size="sm"
						onClick={() => handleHsts(!cfg.hsts_enabled)}
						disabled={hstsPending}
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
					<Button variant="destructive" size="sm" onClick={handleClosePort} disabled={closePending}>
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
						title: labels.certUpload,
						cloudflareTab: labels.certUploadCloudflare,
						customTab: labels.certUploadCustom,
						certPem: labels.certPem,
						certPemPlaceholder: labels.certPemPlaceholder,
						keyPem: labels.certKeyPem,
						keyPemPlaceholder: labels.certKeyPemPlaceholder,
						keyOptional: labels.certKeyOptional,
						upload: labels.certUpload,
						success: labels.certUploadSuccess,
						error: labels.certUploadError,
					}}
					onSuccess={() =>
						setCfg((prev) => ({ ...prev }))
					}
				/>
			)}
		</div>
	);
}
