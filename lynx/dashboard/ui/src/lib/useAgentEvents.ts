"use client";

import { useCallback, useEffect, useRef, useState } from "react";

export type AgentEventKind =
	| "connected"
	| "disconnected"
	| "lockdown"
	| "heartbeat_lost"
	| "update_applied"
	| "nftables_divergence"
	| "bootstrap_completed"
	| "conflicting_software_detected";

export interface AgentEvent {
	agent_id: string;
	detail: string | null;
	event: AgentEventKind;
	type: "agent_event";
}

export type EventsStatus = "connecting" | "open" | "closed";

const BACKOFF_BASE = 2000;
const BACKOFF_MAX = 30000;

interface UseAgentEventsOptions {
	onEvent?: (evt: AgentEvent) => void;
}

export function useAgentEvents({ onEvent }: UseAgentEventsOptions = {}) {
	const [status, setStatus] = useState<EventsStatus>("connecting");
	const wsRef = useRef<WebSocket | null>(null);
	const backoffRef = useRef(BACKOFF_BASE);
	const mountedRef = useRef(true);
	const onEventRef = useRef(onEvent);
	onEventRef.current = onEvent;

	const connect = useCallback(() => {
		if (!mountedRef.current) return;
		const ws = new WebSocket("/api/agents/events/ws");
		wsRef.current = ws;

		ws.onopen = () => {
			if (!mountedRef.current) {
				ws.close();
				return;
			}
			backoffRef.current = BACKOFF_BASE;
			setStatus("open");
		};

		ws.onmessage = (e) => {
			try {
				const data = JSON.parse(e.data as string) as AgentEvent;
				if (data.type === "agent_event") {
					onEventRef.current?.(data);
				}
			} catch {
				// ignore malformed frames
			}
		};

		ws.onerror = () => setStatus("closed");

		ws.onclose = () => {
			if (!mountedRef.current) return;
			setStatus("closed");
			const delay = backoffRef.current;
			backoffRef.current = Math.min(backoffRef.current * 2, BACKOFF_MAX);
			setTimeout(connect, delay);
		};
	}, []);

	useEffect(() => {
		mountedRef.current = true;
		connect();
		return () => {
			mountedRef.current = false;
			wsRef.current?.close();
		};
	}, [connect]);

	return { status };
}
