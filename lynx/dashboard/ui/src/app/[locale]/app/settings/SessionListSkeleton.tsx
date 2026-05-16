import { Skeleton } from "@/components/ui/skeleton";

export function SessionListSkeleton() {
	return (
		<div className="flex flex-col gap-2">
			{[0, 1, 2].map((i) => (
				<div key={i} className="rounded-lg border p-4 flex items-center gap-4">
					<div className="flex-1 space-y-2">
						<Skeleton className="h-4 w-32" />
						<Skeleton className="h-3 w-48" />
					</div>
					<Skeleton className="h-8 w-20" />
				</div>
			))}
		</div>
	);
}
