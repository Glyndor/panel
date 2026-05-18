"use client";

import { Plus, Trash2 } from "lucide-react";
import { useState, useTransition } from "react";
import { toast } from "sonner";
import {
	addRolePermissionAction,
	createRoleAction,
	deleteRoleAction,
	type PermRef,
	type RoleRow,
	removeRolePermissionAction,
} from "@/actions/(dashboard)/app/admin/users";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

type Props = {
	initial: RoleRow[];
	allPerms: PermRef[];
	labels: {
		createRole: string;
		createRoleSuccess: string;
		createRoleError: string;
		deleteRole: string;
		deleteRoleConfirm: string;
		deleteRoleSuccess: string;
		deleteRoleError: string;
		addPermission: string;
		addPermissionSuccess: string;
		addPermissionError: string;
		removePermission: string;
		removePermissionSuccess: string;
		removePermissionError: string;
		roleName: string;
		noPermissions: string;
	};
};

export function RolesPanel({ initial, allPerms, labels }: Props) {
	const [roles, setRoles] = useState(initial);
	const [newName, setNewName] = useState("");
	const [, startTransition] = useTransition();

	function handleCreate() {
		const name = newName.trim();
		if (!name) return;
		startTransition(async () => {
			const res = await createRoleAction(name);
			if (res.success && res.id) {
				setRoles((prev) => [...prev, { id: res.id!, name, permissions: [] }]);
				setNewName("");
				toast.success(labels.createRoleSuccess);
			} else {
				toast.error(res.error ?? labels.createRoleError);
			}
		});
	}

	function handleDelete(roleId: string, roleName: string) {
		if (!confirm(`${labels.deleteRoleConfirm} "${roleName}"?`)) return;
		startTransition(async () => {
			const res = await deleteRoleAction(roleId);
			if (res.success) {
				setRoles((prev) => prev.filter((r) => r.id !== roleId));
				toast.success(labels.deleteRoleSuccess);
			} else {
				toast.error(res.error ?? labels.deleteRoleError);
			}
		});
	}

	function handleAddPerm(roleId: string, permId: string) {
		const perm = allPerms.find((p) => p.id === permId);
		if (!perm) return;
		startTransition(async () => {
			const { success } = await addRolePermissionAction(roleId, permId);
			if (success) {
				setRoles((prev) =>
					prev.map((r) =>
						r.id === roleId ? { ...r, permissions: [...r.permissions, { id: perm.id, key: perm.key }] } : r,
					),
				);
				toast.success(labels.addPermissionSuccess);
			} else {
				toast.error(labels.addPermissionError);
			}
		});
	}

	function handleRemovePerm(roleId: string, permId: string) {
		startTransition(async () => {
			const res = await removeRolePermissionAction(roleId, permId);
			if (res.success) {
				setRoles((prev) =>
					prev.map((r) =>
						r.id === roleId ? { ...r, permissions: r.permissions.filter((p) => p.id !== permId) } : r,
					),
				);
				toast.success(labels.removePermissionSuccess);
			} else {
				toast.error(res.error ?? labels.removePermissionError);
			}
		});
	}

	return (
		<div className="flex flex-col gap-4">
			<div className="flex gap-2">
				<Input
					className="max-w-64 h-8 text-sm"
					onChange={(e) => setNewName(e.target.value)}
					onKeyDown={(e) => e.key === "Enter" && handleCreate()}
					placeholder={labels.roleName}
					value={newName}
				/>
				<Button
					className="h-8 gap-1.5 cursor-pointer select-none"
					disabled={!newName.trim()}
					onClick={handleCreate}
					size="sm"
				>
					<Plus className="size-3.5" />
					{labels.createRole}
				</Button>
			</div>

			<div className="rounded-lg border divide-y">
				{roles.map((role) => {
					const assignablePerms = allPerms.filter((p) => !role.permissions.some((rp) => rp.id === p.id));
					return (
						<div className="flex flex-col gap-2 px-4 py-3" key={role.id}>
							<div className="flex items-center gap-2">
								<span className="font-medium text-sm flex-1">{role.name}</span>
								<Button
									className="h-7 w-7 cursor-pointer select-none text-destructive hover:text-destructive"
									onClick={() => handleDelete(role.id, role.name)}
									size="icon"
									variant="ghost"
								>
									<Trash2 className="size-3.5" />
								</Button>
							</div>

							<div className="flex flex-wrap items-center gap-1.5">
								{role.permissions.length === 0 && (
									<span className="text-xs text-muted-foreground">{labels.noPermissions}</span>
								)}
								{role.permissions.map((perm) => (
									<Badge
										className="gap-1 text-xs font-mono cursor-pointer select-none"
										key={perm.id}
										onClick={() => handleRemovePerm(role.id, perm.id)}
										title={labels.removePermission}
										variant="outline"
									>
										{perm.key}
										<span className="opacity-60">×</span>
									</Badge>
								))}
								{assignablePerms.length > 0 && (
									<Select onValueChange={(permId) => handleAddPerm(role.id, permId)}>
										<SelectTrigger className="h-6 w-auto gap-1 text-xs border-dashed cursor-pointer select-none">
											<Plus className="size-3 shrink-0" />
											<SelectValue placeholder={labels.addPermission} />
										</SelectTrigger>
										<SelectContent>
											{assignablePerms.map((p) => (
												<SelectItem className="text-xs font-mono" key={p.id} value={p.id}>
													{p.key}
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
		</div>
	);
}
