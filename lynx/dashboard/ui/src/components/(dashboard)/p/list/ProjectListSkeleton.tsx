import { Skeleton } from "@/components/ui/skeleton";

export function ProjectListSkeleton() {
	return (
		<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
			{Array.from({ length: 6 }).map((_, i) => (
				// biome-ignore lint/suspicious/noArrayIndexKey: static skeleton, no reorder
				<div className="rounded-lg border p-4 flex flex-col gap-3" key={i}>
					<Skeleton className="h-4 w-3/4" />
					<Skeleton className="h-3 w-1/2" />
					<Skeleton className="h-3 w-2/3" />
					<Skeleton className="h-5 w-20 rounded-full" />
				</div>
			))}
		</div>
	);
}
