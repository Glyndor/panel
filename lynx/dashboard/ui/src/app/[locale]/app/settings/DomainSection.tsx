"use client";

import { useState, useTransition } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Globe, ShieldCheck, Lock } from "lucide-react";
import {
	configureDomain,
	verifyDomain,
	setHsts,
	closePort19443,
} from "./actions";

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
	certExpires: string;
}

interface Props {
	initial: DomainConfig;
	labels: Labels;
}

const STATUS_VARIANT: Record<string, "default" | "secondary" | "destructive"> =
	{
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
	const [domain, setDomain] = useState("");
	const [email, setEmail] = useState("");
	const [cfg, setCfg] = useState<DomainConfig>(initial);
	const [dnsResult, setDnsResult] = useState<boolean | null>(null);
	const [setupPending, startSetup] = useTransition();
	const [verifyPending, startVerify] = useTransition();
	const [hstsPending, startHsts] = useTransition();
	const [closePending, startClose] = useTransition();

	const handleSetup = (e: React.FormEvent) => {
		e.preventDefault();
		if (!domain || !email) return;
		startSetup(async () => {
			const r = await configureDomain(domain, email);
			if (r.ok) {
				setCfg((prev) => ({ ...prev, status: "pending", domain }));
				setDomain("");
				setEmail("");
			} else {
				toast.error(labels.setupError);
			}
		});
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
			} else {
				toast.error(labels.hstsError);
			}
		});
	};

	const handleClosePort = () => {
		if (!window.confirm(labels.closePortConfirm)) return;
		startClose(async () => {
			const r = await closePort19443();
			if (r.ok) {
				setCfg((prev) => ({ ...prev, port_19443_open: false }));
				toast.success(labels.closePortSuccess);
			} else {
				toast.error(labels.closePortError);
			}
		});
	};

	const isActive = cfg.status === "active";

	return (
		<div className="flex flex-col gap-4">
			<p className="text-sm text-muted-foreground">{labels.desc}</p>

			{/* Current status */}
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

			{/* Cert info */}
			{cfg.domain && (
				<div className="flex items-center gap-4 text-xs text-muted-foreground flex-wrap">
					<span>
						{labels.cert}{" "}
						<span className="text-foreground font-medium">
							{cfg.cert_type === "lets_encrypt"
								? labels.certLE
								: labels.certSelfSigned}
						</span>
					</span>
					{cfg.cert_expires_at && (
						<span>
							{labels.certExpires}{" "}
							<span className="text-foreground">
								{formatExpiry(cfg.cert_expires_at)}
							</span>
						</span>
					)}
				</div>
			)}

			{/* Error message */}
			{cfg.error_message && (
				<p className="text-sm text-destructive">{cfg.error_message}</p>
			)}

			{/* Setup form */}
			{!isActive && (
				<form onSubmit={handleSetup} className="flex flex-col gap-3">
					<div className="flex flex-col gap-1.5">
						<Label>{labels.input}</Label>
						<Input
							value={domain}
							onChange={(e) => setDomain(e.target.value)}
							placeholder="panel.example.com"
							disabled={setupPending || cfg.status === "pending"}
						/>
					</div>
					<div className="flex flex-col gap-1.5">
						<Label>{labels.email}</Label>
						<Input
							type="email"
							value={email}
							onChange={(e) => setEmail(e.target.value)}
							placeholder="you@example.com"
							disabled={setupPending || cfg.status === "pending"}
						/>
					</div>
					<Button
						type="submit"
						disabled={!domain || !email || setupPending || cfg.status === "pending"}
					>
						{cfg.status === "pending" ? labels.pending : labels.setup}
					</Button>
				</form>
			)}

			{/* Verify DNS button */}
			{cfg.domain && (
				<div className="flex items-center gap-2">
					<Button
						variant="outline"
						size="sm"
						onClick={handleVerify}
						disabled={verifyPending}
					>
						{labels.verify}
					</Button>
					{dnsResult !== null && (
						<span
							className={
								dnsResult ? "text-sm text-green-600" : "text-sm text-destructive"
							}
						>
							{dnsResult ? labels.dnsOk : labels.dnsFail}
						</span>
					)}
				</div>
			)}

			{/* HSTS section */}
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

			{/* Close port 19443 */}
			{isActive && cfg.port_19443_open && (
				<div className="rounded-lg border border-destructive/30 p-3 flex items-start justify-between gap-4">
					<div className="flex flex-col gap-0.5 min-w-0">
						<div className="flex items-center gap-1.5 text-sm font-medium">
							<Lock className="size-3.5" />
							{labels.closePort}
						</div>
						<p className="text-xs text-muted-foreground">{labels.closePortDesc}</p>
					</div>
					<Button
						variant="destructive"
						size="sm"
						onClick={handleClosePort}
						disabled={closePending}
					>
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
		</div>
	);
}
