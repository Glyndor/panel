"use client";

interface Props {
	ts: string;
	format?: "datetime" | "relative";
}

export function LocalTime({ ts, format = "datetime" }: Props) {
	const date = new Date(ts);

	if (format === "relative") {
		const diff = Math.floor((Date.now() - date.getTime()) / 1000);
		if (diff < 60) return <span>{diff}s ago</span>;
		if (diff < 3600) return <span>{Math.floor(diff / 60)}m ago</span>;
		return <span>{Math.floor(diff / 3600)}h ago</span>;
	}

	return (
		<span>
			{date.toLocaleString(undefined, {
				day: "numeric",
				hour: "2-digit",
				hour12: false,
				minute: "2-digit",
				month: "short",
				second: "2-digit",
				year: "numeric",
			})}
		</span>
	);
}
