// Admin Teams directory page. Two-pane layout: team list on the left,
// member roster for the selected team on the right. Both lists scroll
// independently. Route-level guard via `useRole("SpacebotAdmin")`: a
// non-admin navigating here sees an access-denied panel, not the page
// chrome.
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { UsersThree, WarningCircle } from "@phosphor-icons/react";

import { api } from "@spacebot/api-client/client";
import { useRole } from "@/auth/useMe";

function formatLastSync(value: string | null | undefined): string {
	if (!value) return "Never synced";
	// Display the ISO timestamp as-is. A richer "x minutes ago" is left
	// to a future polish pass; the admin UI values precision over
	// friendliness here.
	return value;
}

function TeamsList({
	selected,
	onSelect,
}: {
	selected: string | null;
	onSelect: (id: string) => void;
}) {
	const { data, isLoading, error } = useQuery({
		queryKey: ["admin", "teams"],
		queryFn: () => api.listAdminTeams(),
		staleTime: 30_000,
	});

	if (isLoading) {
		return (
			<p className="px-3 py-4 text-center text-xs text-ink-dull">Loading…</p>
		);
	}
	if (error) {
		return (
			<div className="px-3 py-6 text-center">
				<p className="text-xs text-red-400">Failed to load teams.</p>
				<p className="mt-1 font-mono text-[10px] text-ink-faint">
					{(error as Error).message}
				</p>
			</div>
		);
	}
	const teams = data?.teams ?? [];
	if (teams.length === 0) {
		return (
			<div className="px-3 py-6 text-center">
				<p className="text-xs text-ink-dull">No teams registered yet.</p>
			</div>
		);
	}

	return (
		<ul className="divide-y divide-app-line/30">
			{teams.map((t) => (
				<li key={t.id}>
					<button
						type="button"
						onClick={() => onSelect(t.id)}
						className={`flex w-full items-center gap-3 px-3 py-2 text-left transition-colors hover:bg-app-hover/40 ${
							selected === t.id ? "bg-app-selected/40" : ""
						}`}
					>
						<div className="min-w-0 flex-1">
							<div className="truncate text-sm font-medium text-ink">
								{t.display_name}
							</div>
							<div className="flex items-center gap-2 text-[11px] text-ink-faint">
								<span>
									{t.member_count} member{t.member_count !== 1 ? "s" : ""}
								</span>
								<span className="text-ink-faint/50">·</span>
								<span className="truncate">{formatLastSync(t.last_sync_at)}</span>
							</div>
						</div>
					</button>
				</li>
			))}
		</ul>
	);
}

function MembersPane({ teamId }: { teamId: string | null }) {
	const { data, isLoading, error } = useQuery({
		queryKey: ["admin", "teams", teamId, "members"],
		queryFn: () => api.listTeamMembers(teamId as string),
		// Gate on a selected team id so the "nothing selected yet"
		// state doesn't fire a `/members` request with an empty string.
		enabled: teamId !== null,
		staleTime: 30_000,
	});

	if (!teamId) {
		return (
			<div className="flex flex-1 flex-col items-center justify-center gap-3 text-ink-dull">
				<UsersThree className="size-10 text-ink-dull/30" weight="thin" />
				<p className="text-sm">Select a team to view members</p>
			</div>
		);
	}
	if (isLoading) {
		return (
			<p className="p-6 text-center text-xs text-ink-dull">Loading members…</p>
		);
	}
	if (error) {
		return (
			<div className="p-6 text-center">
				<p className="text-xs text-red-400">Failed to load members.</p>
				<p className="mt-1 font-mono text-[10px] text-ink-faint">
					{(error as Error).message}
				</p>
			</div>
		);
	}
	const members = data?.members ?? [];
	if (members.length === 0) {
		return (
			<div className="flex flex-1 flex-col items-center justify-center gap-2 text-ink-dull">
				<UsersThree className="size-10 text-ink-dull/30" weight="thin" />
				<p className="text-sm">No members in this team.</p>
			</div>
		);
	}

	return (
		<div className="flex-1 overflow-y-auto">
			<table className="w-full text-sm">
				<thead className="sticky top-0 bg-app-bg text-left text-[11px] uppercase tracking-wide text-ink-faint">
					<tr>
						<th className="px-4 py-2 font-medium">Name</th>
						<th className="px-4 py-2 font-medium">Email</th>
						<th className="px-4 py-2 font-medium">Observed</th>
						<th className="px-4 py-2 font-medium">Source</th>
					</tr>
				</thead>
				<tbody className="divide-y divide-app-line/30">
					{members.map((m) => (
						<tr key={m.principal_key} className="hover:bg-app-hover/40">
							<td className="px-4 py-2 text-ink">
								{m.display_name ?? m.principal_key}
							</td>
							<td className="px-4 py-2 text-ink-dull">
								{m.display_email ?? "—"}
							</td>
							<td className="px-4 py-2 font-mono text-[11px] text-ink-faint">
								{m.observed_at}
							</td>
							<td className="px-4 py-2 text-[11px] text-ink-dull">
								{m.source}
							</td>
						</tr>
					))}
				</tbody>
			</table>
		</div>
	);
}

export function AdminTeams() {
	const isAdmin = useRole("SpacebotAdmin");
	const [selectedTeamId, setSelectedTeamId] = useState<string | null>(null);

	if (!isAdmin) {
		return (
			<div className="flex flex-1 flex-col items-center justify-center gap-3 text-ink-dull">
				<WarningCircle
					className="size-10 text-red-400/60"
					weight="thin"
				/>
				<p className="text-sm">This page requires the SpacebotAdmin role.</p>
			</div>
		);
	}

	return (
		<div className="flex h-full overflow-hidden">
			<div className="flex w-72 shrink-0 flex-col border-r border-app-line/30">
				<div className="flex items-center gap-2 border-b border-app-line/30 px-3 py-3">
					<UsersThree className="size-4 text-ink-dull" weight="bold" />
					<span className="text-sm font-semibold text-ink">Teams</span>
				</div>
				<div className="flex-1 overflow-y-auto">
					<TeamsList selected={selectedTeamId} onSelect={setSelectedTeamId} />
				</div>
			</div>
			<div className="flex flex-1 flex-col">
				<div className="border-b border-app-line/30 px-4 py-3">
					<h2 className="text-sm font-semibold text-ink">Members</h2>
				</div>
				<MembersPane teamId={selectedTeamId} />
			</div>
		</div>
	);
}
