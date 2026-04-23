import {useCallback, useEffect, useMemo, useRef, useState} from "react";
import {useMutation, useQuery, useQueryClient} from "@tanstack/react-query";
import {useNavigate, useSearch} from "@tanstack/react-router";
import {
	api,
	type CreateTaskRequest,
	type TaskItem,
	type TaskStatus,
} from "@spacebot/api-client/client";
import {useLiveContext} from "@/hooks/useLiveContext";
import {
	Button,
	Popover,
	SelectPill,
	OptionList,
	OptionListItem,
} from "@spacedrive/primitives";
import {
	TaskList,
	TaskDetail,
	TaskCreateForm,
	type Task,
	type TaskStatus as UiTaskStatus,
	type TaskCreateFormData,
} from "@spacedrive/ai";
import {
	GithubMetadataBadges,
	getGithubReferences,
} from "@/components/TaskUtils";
import {VisibilityChip} from "@/components/VisibilityChip";
import {
	VisibilityFilter,
	type VisibilityFilterValue,
} from "@/components/VisibilityFilter";
import {
	ShareResourceModal,
	type ShareSubmitArgs,
} from "@/components/ShareResourceModal";
import {useTeams} from "@/auth/useMe";

const TASK_LIMIT = 200;

function AgentPicker({
	agents,
	value,
	onChange,
}: {
	agents: {id: string; display_name?: string | null}[];
	value?: string;
	onChange: (id: string) => void;
}) {
	const [open, setOpen] = useState(false);
	const selected = agents.find((a) => a.id === value);

	return (
		<div className="flex items-center gap-2">
			<label className="text-xs text-ink-dull">Create as:</label>
			<Popover.Root open={open} onOpenChange={setOpen}>
				<Popover.Trigger asChild>
					<SelectPill size="sm">
						{selected?.display_name ?? selected?.id ?? "Select agent"}
					</SelectPill>
				</Popover.Trigger>
				<Popover.Content
					align="start"
					sideOffset={4}
					className="min-w-[180px] p-1.5"
				>
					<OptionList>
						{agents.map((agent) => (
							<OptionListItem
								key={agent.id}
								selected={agent.id === value}
								size="sm"
								onClick={() => {
									onChange(agent.id);
									setOpen(false);
								}}
							>
								{agent.display_name ?? agent.id}
							</OptionListItem>
						))}
					</OptionList>
				</Popover.Content>
			</Popover.Root>
		</div>
	);
}

export function GlobalTasks() {
	const queryClient = useQueryClient();
	const {taskEventVersion} = useLiveContext();

	// Visibility filter state persists to URL query params so a reload
	// restores the filter. Same pattern as AgentMemories.
	const search = useSearch({strict: false}) as {visibility?: string};
	const navigate = useNavigate();
	const visibilityFilter: VisibilityFilterValue =
		search.visibility === "personal" ||
		search.visibility === "team" ||
		search.visibility === "org"
			? search.visibility
			: "all";

	const queryKey = useMemo(
		() => ["tasks", visibilityFilter] as const,
		[visibilityFilter],
	);

	// SSE-driven cache invalidation: bust every filter variant.
	const prevVersion = useRef(taskEventVersion);
	useEffect(() => {
		if (taskEventVersion !== prevVersion.current) {
			prevVersion.current = taskEventVersion;
			queryClient.invalidateQueries({queryKey: ["tasks"]});
		}
	}, [taskEventVersion, queryClient]);

	const {data: agentsData} = useQuery({
		queryKey: ["agents"],
		queryFn: api.agents,
		staleTime: 10_000,
	});

	const agents = agentsData?.agents ?? [];
	const [selectedOwnerId, setSelectedOwnerId] = useState<string | undefined>();
	const effectiveOwner = selectedOwnerId ?? agents[0]?.id;

	const agentNameMap = useMemo(() => {
		const map: Record<string, string> = {};
		for (const agent of agents) {
			map[agent.id] = agent.display_name ?? agent.id;
		}
		return map;
	}, [agents]);

	const resolveAgentName = useCallback(
		(agentId: string) => agentNameMap[agentId] ?? agentId,
		[agentNameMap],
	);

	const {data, isLoading, error} = useQuery({
		queryKey,
		queryFn: () => api.listTasks({limit: TASK_LIMIT}),
		refetchInterval: 15_000,
	});

	const rawTasks: TaskItem[] = useMemo(() => data?.tasks ?? [], [data]);
	const filteredTasks: TaskItem[] = useMemo(
		() =>
			visibilityFilter === "all"
				? rawTasks
				: rawTasks.filter((t) => t.visibility === visibilityFilter),
		[rawTasks, visibilityFilter],
	);
	const tasks = filteredTasks as unknown as Task[];

	const [activeTaskId, setActiveTaskId] = useState<string | null>(null);
	const [collapsedGroups, setCollapsedGroups] = useState<Set<UiTaskStatus>>(
		() => new Set(),
	);
	const [createOpen, setCreateOpen] = useState(false);
	const [shareTarget, setShareTarget] = useState<TaskItem | null>(null);

	const teamsQuery = useTeams({enabled: shareTarget !== null});

	const activeTask = tasks.find((t) => t.id === activeTaskId);
	const activeTaskEnriched = activeTaskId
		? rawTasks.find((t) => t.id === activeTaskId)
		: undefined;

	const invalidate = useCallback(
		// Partial-key invalidate so a mutation on one filter variant
		// busts all cached variants.
		() => queryClient.invalidateQueries({queryKey: ["tasks"]}),
		[queryClient],
	);

	const updateMutation = useMutation({
		mutationFn: ({
			taskNumber,
			...req
		}: {
			taskNumber: number;
			status?: TaskStatus;
			complete_subtask?: number;
		}) => api.updateTask(taskNumber, req),
		onSuccess: () => void invalidate(),
	});

	const approveMutation = useMutation({
		mutationFn: (taskNumber: number) => api.approveTask(taskNumber, "human"),
		onSuccess: () => void invalidate(),
	});

	const executeMutation = useMutation({
		mutationFn: (taskNumber: number) => api.executeTask(taskNumber),
		onSuccess: () => void invalidate(),
	});

	const deleteMutation = useMutation({
		mutationFn: (taskNumber: number) => api.deleteTask(taskNumber),
		onSuccess: () => {
			setActiveTaskId(null);
			void invalidate();
		},
	});

	const createMutation = useMutation({
		mutationFn: (req: CreateTaskRequest) => api.createTask(req),
		onSuccess: () => {
			setCreateOpen(false);
			void invalidate();
		},
	});

	const handleStatusChange = useCallback(
		(task: Task, status: UiTaskStatus) => {
			const t = task as unknown as TaskItem;
			if (t.status === "pending_approval" && status === "ready") {
				approveMutation.mutate(t.task_number);
			} else if (t.status === "backlog" && status === "in_progress") {
				executeMutation.mutate(t.task_number);
			} else {
				updateMutation.mutate({taskNumber: t.task_number, status});
			}
		},
		[updateMutation, approveMutation, executeMutation],
	);

	const handleDelete = useCallback(
		(task: Task) => {
			deleteMutation.mutate((task as unknown as TaskItem).task_number);
		},
		[deleteMutation],
	);

	const handleSubtaskToggle = useCallback(
		(task: Task, index: number, _completed: boolean) => {
			updateMutation.mutate({
				taskNumber: (task as unknown as TaskItem).task_number,
				complete_subtask: index,
			});
		},
		[updateMutation],
	);

	const handleToggleGroup = useCallback((status: UiTaskStatus) => {
		setCollapsedGroups((prev) => {
			const next = new Set(prev);
			if (next.has(status)) next.delete(status);
			else next.add(status);
			return next;
		});
	}, []);

	const handleCreate = useCallback(
		(formData: TaskCreateFormData) => {
			if (!effectiveOwner) return;
			createMutation.mutate({
				owner_agent_id: effectiveOwner,
				title: formData.title,
				description: formData.description || undefined,
				priority: formData.priority,
				status: "backlog",
			});
		},
		[createMutation, effectiveOwner],
	);

	return (
		<div className="flex h-full w-full">
			{/* List panel */}
			<div className="flex min-w-0 flex-1 flex-col">
				{/* Toolbar */}
				<div className="flex items-center justify-between border-b border-app-line px-4 py-2">
					<div className="flex items-center gap-3">
						<span className="text-sm text-ink-dull">
							{tasks.length} task{tasks.length !== 1 ? "s" : ""}
						</span>
						{agents.length > 1 && (
							<AgentPicker
								agents={agents}
								value={effectiveOwner}
								onChange={setSelectedOwnerId}
							/>
						)}
						<VisibilityFilter
							value={visibilityFilter}
							onChange={(v) =>
								navigate({
									to: ".",
									search: (prev) => ({
										...prev,
										visibility: v === "all" ? undefined : v,
									}),
								})
							}
						/>
					</div>
					{effectiveOwner && (
						<Button size="md" onClick={() => setCreateOpen(!createOpen)}>
							{createOpen ? "Cancel" : "Create Task"}
						</Button>
					)}
				</div>

				{/* Create form */}
				{createOpen && (
					<div className="border-b border-app-line px-3 py-2">
						<TaskCreateForm
							onSubmit={handleCreate}
							onCancel={() => setCreateOpen(false)}
							isSubmitting={createMutation.isPending}
						/>
					</div>
				)}

				{/* Task list */}
				{isLoading ? (
					<div className="py-8 text-center text-sm text-ink-faint">
						Loading tasks...
					</div>
				) : error ? (
					<div className="py-8 text-center text-sm text-red-400">
						Failed to load tasks.
						<div className="mt-1 font-mono text-[10px] text-ink-faint">
							{(error as Error).message}
						</div>
					</div>
				) : tasks.length === 0 ? (
					<div className="flex flex-1 items-center justify-center">
						<div className="text-center">
							<p className="text-sm text-ink-dull">No tasks yet.</p>
							<p className="mt-1 text-xs text-ink-faint">
								Create one to get started.
							</p>
						</div>
					</div>
				) : (
					<div className="flex-1 overflow-y-auto">
						<TaskList
							tasks={tasks}
							activeTaskId={activeTaskId ?? undefined}
							collapsedGroups={collapsedGroups}
							onToggleGroup={handleToggleGroup}
							onTaskClick={(task) => setActiveTaskId(task.id)}
							onStatusChange={handleStatusChange}
							onDelete={handleDelete}
							resolveAgentName={resolveAgentName}
						/>
					</div>
				)}
			</div>

			{/* Detail panel */}
			{activeTask && (
				<div className="w-[400px] shrink-0 overflow-y-auto border-l border-app-line">
					<TaskDetail
						task={activeTask}
						resolveAgentName={resolveAgentName}
						onStatusChange={handleStatusChange}
						onSubtaskToggle={handleSubtaskToggle}
						onDelete={handleDelete}
						onClose={() => setActiveTaskId(null)}
					/>
					{activeTaskEnriched && (
						<div className="flex items-center gap-2 border-t border-app-line/40 px-4 py-3">
							{activeTaskEnriched.visibility && (
								<VisibilityChip
									visibility={activeTaskEnriched.visibility}
									teamName={activeTaskEnriched.team_name ?? undefined}
								/>
							)}
							<button
								type="button"
								onClick={() => setShareTarget(activeTaskEnriched)}
								className="ml-auto rounded px-2 py-1 text-tiny font-medium text-ink-dull hover:bg-app-hover"
							>
								Share
							</button>
						</div>
					)}
					<GithubSection
						metadata={activeTask.metadata as Record<string, unknown>}
					/>
				</div>
			)}
			{shareTarget && (
				<ShareResourceModal
					resourceType="task"
					resourceId={shareTarget.id}
					currentVisibility={shareTarget.visibility ?? null}
					teams={(teamsQuery.data ?? []).map((t) => ({
						id: t.id,
						name: t.display_name,
					}))}
					onSubmit={async (args: ShareSubmitArgs) => {
						await api.setResourceVisibility("task", shareTarget.id, args);
						try {
							await queryClient.invalidateQueries({queryKey: ["tasks"]});
						} catch (e) {
							console.error(
								"GlobalTasks: failed to invalidate tasks cache after share",
								e,
							);
						}
					}}
					onClose={() => setShareTarget(null)}
				/>
			)}
		</div>
	);
}

function GithubSection({metadata}: {metadata: Record<string, unknown>}) {
	const refs = getGithubReferences(metadata);
	if (refs.length === 0) return null;

	return (
		<div className="border-t border-app-line/40 px-4 py-3">
			<h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-ink-dull">
				GitHub Links
			</h3>
			<GithubMetadataBadges references={refs} />
		</div>
	);
}
