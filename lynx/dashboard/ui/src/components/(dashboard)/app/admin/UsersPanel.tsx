"use client";

import { useState, useTransition } from "react";
import { toast } from "sonner";
import {
	addUserRoleAction,
	deleteUserAction,
	forcePasswordChangeAction,
	removeUserRoleAction,
	type RoleRow,
	type UserRow,
} from "@/actions/(dashboard)/app/admin/users";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { MoreHorizontal, ShieldAlert, Trash2, UserPlus } from "lucide-react";

type Props = {
	initial: UserRow[];
	roles: RoleRow[];
	labels: {
		deleteUser: string;
		deleteConfirm: string;
		deleteSuccess: string;
		deleteError: string;
		forcePasswordChange: string;
		forcePasswordChangeSuccess: string;
		forcePasswordChangeError: string;
		addRole: string;
		addRoleSuccess: string;
		addRoleError: string;
		removeRole: string;
		removeRoleSuccess: string;
		removeRoleError: string;
		noRoles: string;
		selectRole: string;
	};
};

export function UsersPanel({ initial, roles, labels }: Props) {
	const [users, setUsers] = useState(initial);
	const [, startTransition] = useTransition();

	function handleDelete(userId: string, username: string) {
		if (!confirm(`${labels.deleteConfirm} "${username}"?`)) return;
		startTransition(async () => {
			const res = await deleteUserAction(userId);
			if (res.success) {
				setUsers((prev) => prev.filter((u) => u.id !== userId));
				toast.success(labels.deleteSuccess);
			} else {
				toast.error(res.error ?? labels.deleteError);
			}
		});
	}

	function handleForcePasswordChange(userId: string) {
		startTransition(async () => {
			const { success } = await forcePasswordChangeAction(userId);
			if (success) {
				setUsers((prev) =>
					prev.map((u) =>
						u.id === userId ? { ...u, force_password_change: true } : u,
					),
				);
				toast.success(labels.forcePasswordChangeSuccess);
			} else {
				toast.error(labels.forcePasswordChangeError);
			}
		});
	}

	function handleAddRole(userId: string, roleId: string) {
		const role = roles.find((r) => r.id === roleId);
		if (!role) return;
		startTransition(async () => {
			const { success } = await addUserRoleAction(userId, roleId);
			if (success) {
				setUsers((prev) =>
					prev.map((u) =>
						u.id === userId
							? { ...u, roles: [...u.roles, { id: role.id, name: role.name }] }
							: u,
					),
				);
				toast.success(labels.addRoleSuccess);
			} else {
				toast.error(labels.addRoleError);
			}
		});
	}

	function handleRemoveRole(userId: string, roleId: string) {
		startTransition(async () => {
			const { success } = await removeUserRoleAction(userId, roleId);
			if (success) {
				setUsers((prev) =>
					prev.map((u) =>
						u.id === userId
							? { ...u, roles: u.roles.filter((r) => r.id !== roleId) }
							: u,
					),
				);
				toast.success(labels.removeRoleSuccess);
			} else {
				toast.error(labels.removeRoleError);
			}
		});
	}

	return (
		<div className="rounded-lg border divide-y">
			{users.map((user) => {
				const assignableRoles = roles.filter(
					(r) => !user.roles.some((ur) => ur.id === r.id),
				);
				return (
					<div className="flex flex-col gap-2 px-4 py-3" key={user.id}>
						<div className="flex items-center gap-2">
							<span className="font-medium text-sm flex-1">{user.username}</span>
							{user.force_password_change && (
								<Badge className="text-[10px]" variant="destructive">
									pw reset
								</Badge>
							)}
							<DropdownMenu>
								<DropdownMenuTrigger asChild>
									<Button
										className="h-7 w-7 cursor-pointer select-none"
										size="icon"
										variant="ghost"
									>
										<MoreHorizontal className="size-4" />
									</Button>
								</DropdownMenuTrigger>
								<DropdownMenuContent align="end">
									<DropdownMenuItem
										className="gap-2 cursor-pointer"
										onClick={() => handleForcePasswordChange(user.id)}
									>
										<ShieldAlert className="size-3.5" />
										{labels.forcePasswordChange}
									</DropdownMenuItem>
									<DropdownMenuSeparator />
									<DropdownMenuItem
										className="gap-2 text-destructive cursor-pointer focus:text-destructive"
										onClick={() => handleDelete(user.id, user.username)}
									>
										<Trash2 className="size-3.5" />
										{labels.deleteUser}
									</DropdownMenuItem>
								</DropdownMenuContent>
							</DropdownMenu>
						</div>

						<div className="flex flex-wrap items-center gap-1.5">
							{user.roles.length === 0 && (
								<span className="text-xs text-muted-foreground">{labels.noRoles}</span>
							)}
							{user.roles.map((role) => (
								<Badge
									className="gap-1 text-xs cursor-pointer select-none"
									key={role.id}
									onClick={() => handleRemoveRole(user.id, role.id)}
									title={labels.removeRole}
									variant="secondary"
								>
									{role.name}
									<span className="opacity-60">×</span>
								</Badge>
							))}
							{assignableRoles.length > 0 && (
								<Select onValueChange={(roleId) => handleAddRole(user.id, roleId)}>
									<SelectTrigger className="h-6 w-auto gap-1 text-xs border-dashed cursor-pointer select-none">
										<UserPlus className="size-3 shrink-0" />
										<SelectValue placeholder={labels.addRole} />
									</SelectTrigger>
									<SelectContent>
										{assignableRoles.map((r) => (
											<SelectItem className="text-xs" key={r.id} value={r.id}>
												{r.name}
											</SelectItem>
										))}
									</SelectContent>
								</Select>
							)}
						</div>
					</div>
				);
			})}
		</div>
	);
}
