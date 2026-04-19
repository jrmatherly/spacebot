import type {RegistryView} from "@spacebot/api-client/client";

export const REGISTRY_VIEWS: {key: RegistryView; label: string}[] = [
	{key: "all-time", label: "All Time"},
	{key: "trending", label: "Trending"},
	{key: "hot", label: "Hot"},
];
