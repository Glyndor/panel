"use client";

import { useEffect, useRef, useState } from "react";

export interface AgentMetrics {
	cpu_percent: number;
	mem_used_mb: number;
	mem_total_mb: number;
	disk_used_gb: number;
	disk_total_gb: number;
	timestamp: number;
}

type ConnectionStatus = "connecting" | "online" | "offline" | "agent_offline";

export function useAgentMetrics(agentId: string) {
	const [metrics, setMetrics] = useState<AgentMetrics | null>(null);
	const [status, setStatus] = useState<ConnectionStatus>("connecting");
	const wsRef = useRef<WebSocket | null>(null);
	const retryRef = useRef<ReturnType<typeof setTimeout> | null>(null);
	const retryDelay = useRef(2000);

	useEffect(() => {
		let cancelled = false;

		function connect() {
			if (cancelled) return;
			setStatus("connecting");

			const proto = window.location.protocol === "https:" ? "wss" : "ws";
			const host = window.location.host;
			// auth_token cookie is sent automatically by the browser (same origin)
			const url = `${proto}://${host}/api/agents/${agentId}/metrics/ws`;

			const ws = new WebSocket(url);
			wsRef.current = ws;

			ws.onmessage = (ev) => {
				try {
					const msg = JSON.parse(ev.data as string) as {
						type: string;
						data?: AgentMetrics;
					};
					if (msg.type === "metrics" && msg.data) {
						setMetrics(msg.data);
						setStatus("online");
						retryDelay.current = 2000;
					} else if (msg.type === "agent_offline") {
						setStatus("agent_offline");
					}
				} catch {
					// ignore malformed frames
				}
			};

			ws.onerror = () => setStatus("offline");

			ws.onclose = () => {
				wsRef.current = null;
				if (cancelled) return;
				setStatus("offline");
				retryRef.current = setTimeout(() => {
					retryDelay.current = Math.min(retryDelay.current * 2, 30_000);
					connect();
				}, retryDelay.current);
			};
		}

		connect();

		return () => {
			cancelled = true;
			if (retryRef.current) clearTimeout(retryRef.current);
			wsRef.current?.close();
		};
	}, [agentId]);

	return { metrics, status };
}
