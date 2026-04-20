import type {GlobalSettingsResponse} from "@spacebot/api-client/client";

export type SectionId =
	| "instance"
	| "appearance"
	| "providers"
	| "channels"
	| "api-keys"
	| "secrets"
	| "server"
	| "opencode"
	| "worker-logs"
	| "updates"
	| "config-file"
	| "changelog";

export type Platform =
	| "discord"
	| "slack"
	| "telegram"
	| "twitch"
	| "email"
	| "webhook"
	| "mattermost"
	| "signal";

export interface GlobalSettingsSectionProps {
	settings: GlobalSettingsResponse | undefined;
	isLoading: boolean;
}

export interface ChangelogRelease {
	version: string;
	body: string;
}

export interface ProviderCardProps {
	provider: string;
	name: string;
	description: string;
	configured: boolean;
	defaultModel: string;
	onEdit: () => void;
	onRemove: () => void;
	removing: boolean;
	actionLabel?: string;
	showRemove?: boolean;
}

export interface ChatGptOAuthDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	isRequesting: boolean;
	isPolling: boolean;
	message: {text: string; type: "success" | "error"} | null;
	deviceCodeInfo: {userCode: string; verificationUrl: string} | null;
	deviceCodeCopied: boolean;
	onCopyDeviceCode: () => void;
	onOpenDeviceLogin: () => void;
	onRestart: () => void;
}

/** Shape of each entry in the `PROVIDERS` array in `constants.ts`. */
export interface ProviderDefinition {
	id: string;
	name: string;
	description: string;
	placeholder: string;
	envVar: string;
	defaultModel: string;
	/** When true, the Update modal renders a Base URL input. LiteLLM-specific today. */
	requiresBaseUrl?: boolean;
	/** Placeholder text for the Base URL input when requiresBaseUrl is true. */
	defaultBaseUrl?: string;
	/** When true, the Update modal renders optional use_bearer_auth checkbox
	 * and extra_headers key/value list. LiteLLM-specific today. */
	supportsAdvancedHeaders?: boolean;
}
