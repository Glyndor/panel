import { Skeleton } from "@/components/ui/skeleton";
import { Card, CardContent, CardHeader } from "@/components/ui/card";

export function AgentListSkeleton() {
	return (
		<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
			{Array.from({ length: 3 }).map((_, i) => (
				<Card key={i}>
					<CardHeader className="pb-2">
						<div className="flex items-center justify-between gap-2">
							<Skeleton className="h-5 w-32" />
							<Skeleton className="h-5 w-16 rounded-full" />
						</div>
					</CardHeader>
					<CardContent className="space-y-2">
						<Skeleton className="h-4 w-full" />
						<Skeleton className="h-4 w-3/4" />
						<Skeleton className="h-4 w-2/4" />
						<Skeleton className="h-3 w-full opacity-50" />
					</CardContent>
				</Card>
			))}
		</div>
	);
}
