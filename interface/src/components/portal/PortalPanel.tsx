import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { usePortal, getPortalSessionId } from "@/hooks/usePortal";
import { useLiveContext } from "@/hooks/useLiveContext";
import { api, type ConversationDefaultsResponse, type ConversationSettings } from "@spacebot/api-client/client";
import type { PortalConversationListItem } from "@spacebot/api-client/types";
import { type VisibilityFilterValue } from "@/components/VisibilityFilter";
import {
	ShareResourceModal,
	type ShareSubmitArgs,
} from "@/components/ShareResourceModal";
import { useTeams } from "@/auth/useMe";
import { PortalHeader } from "./PortalHeader";
import { PortalTimeline } from "./PortalTimeline";
import { PortalComposer } from "./PortalComposer";
import { PortalEmpty } from "./PortalEmpty";

interface PortalPanelProps {
	agentId: string;
}

export function PortalPanel({ agentId }: PortalPanelProps) {
	const queryClient = useQueryClient();
	const [activeConversationId, setActiveConversationId] = useState<string>(
		getPortalSessionId(agentId),
	);
	const { isSending, error, sendMessage } = usePortal(agentId, activeConversationId);
	const { liveStates } = useLiveContext();
	const [input, setInput] = useState("");
	const [showSettings, setShowSettings] = useState(false);
	const [showHistory, setShowHistory] = useState(false);
	const [settings, setSettings] = useState<ConversationSettings>({});
	const [pendingFiles, setPendingFiles] = useState<File[]>([]);
	const [sendCount, setSendCount] = useState(0);
	// Track uploaded attachment IDs keyed by file name+size for deduplication.
	const uploadedIds = useRef<Map<string, string>>(new Map());

	// Visibility filter state persists to URL query params so a reload
	// restores the filter. Same pattern as AgentCron and Wiki.
	const urlSearch = useSearch({ strict: false }) as { visibility?: string };
	const navigate = useNavigate();
	const visibilityFilter: VisibilityFilterValue =
		urlSearch.visibility === "personal" ||
		urlSearch.visibility === "team" ||
		urlSearch.visibility === "org"
			? urlSearch.visibility
			: "all";
	const setVisibilityFilter = (v: VisibilityFilterValue) =>
		navigate({
			to: ".",
			search: (prev) => ({
				...prev,
				visibility: v === "all" ? undefined : v,
			}),
		});

	const [shareTarget, setShareTarget] =
		useState<PortalConversationListItem | null>(null);
	// Lazy-gate the teams fetch until the Share modal opens.
	const teamsQuery = useTeams({ enabled: shareTarget !== null });

	// Fetch conversations list. QueryKey includes the filter so cache
	// entries stay isolated per visibility; the filter itself is applied
	// client-side because the backend does not yet accept a `visibility=`
	// param on this endpoint.
	const { data: conversationsData } = useQuery({
		queryKey: ["portal-conversations", agentId, visibilityFilter],
		queryFn: () => api.listPortalConversations(agentId),
	});

	const allConversations = conversationsData?.conversations ?? [];
	const conversations = useMemo(
		() =>
			visibilityFilter === "all"
				? allConversations
				: allConversations.filter((c) => c.visibility === visibilityFilter),
		[allConversations, visibilityFilter],
	);

	// Auto-select the newest conversation on first load
	useEffect(() => {
		if (conversations.length === 0) return;
		const isPlaceholder = activeConversationId === getPortalSessionId(agentId);
		if (!isPlaceholder) return;
		const newest = conversations[0];
		if (newest) setActiveConversationId(newest.id);
	}, [conversationsData, agentId]);

	// Hydrate settings from cached data when switching conversations. The ref
	// prevents background conversation refetches from clobbering edits the user
	// has made since we last hydrated.
	const hydratedSettingsFor = useRef<string | null>(null);
	useEffect(() => {
		if (hydratedSettingsFor.current === activeConversationId) return;
		const activeConv = conversations.find((c) => c.id === activeConversationId);
		if (!activeConv) return;
		setSettings((activeConv.settings ?? {}) as ConversationSettings);
		setShowSettings(false);
		hydratedSettingsFor.current = activeConversationId;
	}, [activeConversationId, conversations]);

	const {
		data: defaults,
		isLoading: defaultsLoading,
		error: defaultsError,
	} = useQuery<ConversationDefaultsResponse>({
		queryKey: ["conversation-defaults", agentId],
		queryFn: () => api.getConversationDefaults(agentId),
	});

	const { data: projectsData } = useQuery({
		queryKey: ["projects"],
		queryFn: () => api.listProjects("active"),
		staleTime: 30_000,
	});
	const projects = projectsData?.projects ?? [];
	const projectOptions = projects.map((p) => p.name);
	const [selectedProject, setSelectedProject] = useState<string>("");
	useEffect(() => {
		if (!selectedProject && projectOptions.length > 0) {
			setSelectedProject(projectOptions[0]);
		}
	}, [projectOptions, selectedProject]);

	const agentsQuery = useQuery({
		queryKey: ["agents"],
		queryFn: () => api.agents(),
		staleTime: 10_000,
	});
	const agentDisplayName =
		agentsQuery.data?.agents.find((a) => a.id === agentId)?.display_name ?? agentId;

	const liveState = liveStates[activeConversationId];
	const timeline = liveState?.timeline ?? [];
	const isTyping = liveState?.isTyping ?? false;
	const activeWorkers = Object.values(liveState?.workers ?? {});

	const createConversationMutation = useMutation({
		mutationFn: () => api.createPortalConversation(agentId),
		onSuccess: (data) => {
			setActiveConversationId(data.conversation.id);
			queryClient.invalidateQueries({ queryKey: ["portal-conversations", agentId] });
		},
	});

	const deleteConversationMutation = useMutation({
		mutationFn: (id: string) => api.deletePortalConversation(agentId, id),
		onSuccess: (_, deletedId) => {
			if (activeConversationId === deletedId) {
				setActiveConversationId(getPortalSessionId(agentId));
			}
			queryClient.invalidateQueries({ queryKey: ["portal-conversations", agentId] });
		},
	});

	const archiveConversationMutation = useMutation({
		mutationFn: ({ id, archived }: { id: string; archived: boolean }) =>
			api.updatePortalConversation(agentId, id, undefined, archived),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["portal-conversations", agentId] });
		},
	});

	const saveSettingsMutation = useMutation({
		mutationFn: () =>
			api.updatePortalConversation(
				agentId,
				activeConversationId,
				undefined,
				undefined,
				settings,
			),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["portal-conversations", agentId] });
			setShowSettings(false);
		},
	});

	const handleAddFiles = (files: File[]) => {
		setPendingFiles((prev) => [...prev, ...files]);
	};

	const handleRemoveFile = (index: number) => {
		setPendingFiles((prev) => prev.filter((_, i) => i !== index));
	};

	const handleSubmit = async () => {
		const trimmed = input.trim();
		if (!trimmed || isSending) return;
		setInput("");

		// Upload any pending files and collect their IDs.
		let attachmentIds: string[] = [];
		if (pendingFiles.length > 0) {
			const filesToUpload = pendingFiles;
			setPendingFiles([]);

			const ids = await Promise.all(
				filesToUpload.map(async (file) => {
					const key = `${file.name}:${file.size}`;
					const cached = uploadedIds.current.get(key);
					if (cached) return cached;

					try {
						const response = await api.uploadAttachment(agentId, activeConversationId, file);
						if (!response.ok) return null;
						const data: { id: string } = await response.json();
						uploadedIds.current.set(key, data.id);
						return data.id;
					} catch {
						return null;
					}
				}),
			);
			attachmentIds = ids.filter((id): id is string => id !== null);
		}

		setSendCount((n) => n + 1);
		sendMessage(trimmed, attachmentIds.length > 0 ? attachmentIds : undefined);
	};

	const modelLabel = defaults
		? (defaults.available_models.find(
				(m) => m.id === (settings.model || defaults.model),
			)?.name ?? settings.model ?? defaults.model)
		: undefined;

	const isEmpty = timeline.length === 0 && !isTyping;

	return (
		<div className="relative flex h-full w-full min-w-0 flex-col">
				{/* Grid background with accent glow — empty state only */}
				{isEmpty && (
					<>
						<div
							aria-hidden="true"
							className="pointer-events-none absolute inset-0 z-0 opacity-100"
							style={{
								backgroundImage:
									'linear-gradient(to right, color-mix(in srgb, var(--color-app-line) 45%, transparent) 1px, transparent 1px), linear-gradient(to bottom, color-mix(in srgb, var(--color-app-line) 45%, transparent) 1px, transparent 1px)',
								backgroundSize: '28px 28px',
								maskImage:
									'linear-gradient(to bottom, rgba(0,0,0,0.42), rgba(0,0,0,0.08))',
								WebkitMaskImage:
									'linear-gradient(to bottom, rgba(0,0,0,0.42), rgba(0,0,0,0.08))',
							}}
						/>
						<div
							aria-hidden="true"
							className="pointer-events-none absolute inset-0 z-0"
							style={{
								background:
									'radial-gradient(circle at top, color-mix(in srgb, var(--color-accent) 8%, transparent), transparent 42%)',
							}}
						/>
					</>
				)}

				<div className="relative z-10 flex min-h-0 flex-1 flex-col">
				<PortalHeader
					title={agentDisplayName}
					modelLabel={modelLabel}
					responseMode={settings.response_mode}
					activeWorkers={activeWorkers}
					showSettings={showSettings}
					onToggleSettings={setShowSettings}
					defaults={defaults}
					defaultsLoading={defaultsLoading}
					defaultsError={defaultsError as Error | null}
					settings={settings}
					onSettingsChange={setSettings}
					onSaveSettings={() => saveSettingsMutation.mutate()}
					saving={saveSettingsMutation.isPending}
					conversations={conversations}
					activeConversationId={activeConversationId}
					onNewConversation={() => createConversationMutation.mutate()}
					onSelectConversation={setActiveConversationId}
					onDeleteConversation={(id) => deleteConversationMutation.mutate(id)}
					onArchiveConversation={(id, archived) =>
						archiveConversationMutation.mutate({ id, archived })
					}
					onShareConversation={(conv) => setShareTarget(conv)}
					visibilityFilter={visibilityFilter}
					onVisibilityFilterChange={setVisibilityFilter}
					showHistory={showHistory}
					onToggleHistory={setShowHistory}
				/>

				{isEmpty ? (
					<div className="flex flex-1 items-center justify-center py-10">
						<div className="w-full max-w-2xl px-6">
							<PortalEmpty agentName={agentDisplayName} />
							<PortalComposer
								agentName={agentDisplayName}
								draft={input}
								onDraftChange={setInput}
								onSend={() => void handleSubmit()}
								disabled={isSending || isTyping}
								modelOptions={defaults?.available_models ?? []}
								selectedModel={settings.model || defaults?.model || ""}
								onSelectModel={(model) => setSettings((s) => ({ ...s, model }))}
								projectOptions={projectOptions}
								selectedProject={selectedProject}
								onSelectProject={setSelectedProject}
								pendingFiles={pendingFiles}
								onAddFiles={handleAddFiles}
								onRemoveFile={handleRemoveFile}
							/>
						</div>
					</div>
				) : (
					<>
						<PortalTimeline
							agentId={agentId}
							conversationId={activeConversationId}
							timeline={timeline}
							isTyping={isTyping}
							sendCount={sendCount}
						/>

						{error && (
							<div className="mx-4 mb-2 rounded-lg border border-red-500/20 bg-red-500/5 px-4 py-3 text-sm text-red-400">
								{error}
							</div>
						)}

						<div className="absolute inset-x-0 bottom-0 z-10 p-4 bg-gradient-to-t from-app via-app/80 to-transparent pt-8 pointer-events-none">
							<div className="mx-auto w-full max-w-3xl pointer-events-auto">
								<PortalComposer
									agentName={agentDisplayName}
									draft={input}
									onDraftChange={setInput}
									onSend={() => void handleSubmit()}
									disabled={isSending || isTyping}
									modelOptions={defaults?.available_models ?? []}
									selectedModel={settings.model || defaults?.model || ""}
									onSelectModel={(model) => setSettings((s) => ({ ...s, model }))}
									projectOptions={projectOptions}
									selectedProject={selectedProject}
									onSelectProject={setSelectedProject}
									pendingFiles={pendingFiles}
									onAddFiles={handleAddFiles}
									onRemoveFile={handleRemoveFile}
								/>
							</div>
						</div>
					</>
				)}
				</div>

				{shareTarget && (
					<ShareResourceModal
						resourceType="portal_conversation"
						resourceId={shareTarget.id}
						currentVisibility={shareTarget.visibility ?? null}
						teams={(teamsQuery.data ?? []).map((t) => ({
							id: t.id,
							name: t.display_name,
						}))}
						onSubmit={async (args: ShareSubmitArgs) => {
							await api.setResourceVisibility(
								"portal_conversation",
								shareTarget.id,
								args,
							);
							// Rethrow on invalidate failure: the
							// conversations query has no refetchInterval
							// and no SSE-driven invalidation, so a silent
							// swallow would leave stale chip state visible
							// until the user manually reopens the history
							// popover. Matches the Wiki pattern for routes
							// without a refetch backstop.
							try {
								await queryClient.invalidateQueries({
									queryKey: ["portal-conversations", agentId],
								});
							} catch (e) {
								console.error(
									"PortalPanel: failed to invalidate portal-conversations cache after share",
									e,
								);
								throw e;
							}
						}}
						onClose={() => setShareTarget(null)}
					/>
				)}
		</div>
	);
}
